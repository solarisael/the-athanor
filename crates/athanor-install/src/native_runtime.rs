use crate::{boundaries::RuntimeControl, installer::CurrentRelease, layout::InstallLayout};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Secrets {
    postgres_password: String,
    external_database_url: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    database_mode: String,
}

pub struct NativeRuntimeControl {
    pub layout: InstallLayout,
}
impl NativeRuntimeControl {
    fn current_root(&self) -> Result<PathBuf> {
        let current: CurrentRelease = serde_json::from_slice(&fs::read(self.layout.current())?)?;
        Ok(self.layout.version(&current.version))
    }
    fn maintenance_root(&self) -> Result<PathBuf> {
        // During install/update the pre-upgrade backup must run BEFORE the new
        // version activates, but the currently installed substrate only knows
        // migrations up to its own release. A database migrated ahead of the
        // installed binaries (developer migration, live proof) would make the
        // upgrade permanently impossible. main sets this process-local staging
        // path for the install command only; the staged substrate and bundled
        // PostgreSQL tools always come from the same release root.
        if let Some(staged) = std::env::var_os("ATHANOR_INSTALL_STAGING_BIN") {
            let bin = PathBuf::from(staged);
            if bin.join("athanor-substrate.exe").exists() {
                return bin
                    .parent()
                    .map(Path::to_path_buf)
                    .context("staged Athanor bin directory has no release root");
            }
        }
        self.current_root()
    }
    fn secrets(&self) -> Result<Secrets> {
        Ok(serde_json::from_slice(
            &fs::read(self.layout.secrets()).context("read runtime secrets")?,
        )?)
    }
    fn config(&self) -> Result<Config> {
        Ok(serde_json::from_slice(
            &fs::read(self.layout.config()).context("read runtime configuration")?,
        )?)
    }
    fn database_url(&self) -> Result<String> {
        let secrets = self.secrets()?;
        Ok(secrets.external_database_url.unwrap_or_else(|| {
            crate::endpoints::managed_database_url(&secrets.postgres_password)
        }))
    }
    fn run(&self, arguments: &[&str]) -> Result<String> {
        let root = self.maintenance_root()?;
        let output = Command::new(root.join("bin/athanor-substrate.exe"))
            .args(arguments)
            .env("DATABASE_URL", self.database_url()?)
            .env("PG_BIN_DIR", root.join("runtime/postgresql/bin"))
            .stdin(Stdio::null())
            .output()
            .context("run substrate maintenance command")?;
        if !output.status.success() {
            bail!(
                "substrate maintenance failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8(output.stdout)?.trim().into())
    }
    fn postgres_tool(&self, name: &str) -> Result<PathBuf> {
        Ok(self
            .current_root()?
            .join("runtime/postgresql/bin")
            .join(format!("{name}.exe")))
    }
    fn postgres(&self, arguments: &[&str]) -> Result<()> {
        let output = Command::new(self.postgres_tool("pg_ctl")?)
            .args(arguments)
            .stdin(Stdio::null())
            .output()?;
        if !output.status.success() {
            bail!(
                "managed PostgreSQL control failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(())
    }
    fn initialize_postgres(&self) -> Result<bool> {
        if self.layout.postgres_data().join("PG_VERSION").exists() {
            return Ok(false);
        }
        fs::create_dir_all(self.layout.postgres_data())?;
        let password_file = self.layout.data.join("secrets/.postgres-init-password");
        fs::write(&password_file, self.secrets()?.postgres_password)?;
        let output = Command::new(self.postgres_tool("initdb")?)
            .args([
                "-D",
                &self.layout.postgres_data().to_string_lossy(),
                "--username",
                "athanor",
                "--pwfile",
                &password_file.to_string_lossy(),
                "--auth-host",
                "scram-sha-256",
                "--auth-local",
                "scram-sha-256",
                "--encoding",
                "UTF8",
            ])
            .stdin(Stdio::null())
            .output();
        let _ = fs::remove_file(&password_file);
        let output = output?;
        if !output.status.success() {
            bail!(
                "managed PostgreSQL initialization failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(true)
    }
    fn with_database<T>(&self, action: impl FnOnce() -> Result<T>) -> Result<T> {
        if self.config()?.database_mode == "external" {
            return action();
        }
        let initialized = self.initialize_postgres()?;
        let log = self.layout.logs().join("postgresql-maintenance.log");
        self.postgres(&[
            "-D",
            &self.layout.postgres_data().to_string_lossy(),
            "-l",
            &log.to_string_lossy(),
            "-w",
            "-t",
            "90",
            "-o",
            "-h 127.0.0.1 -p 5432",
            "start",
        ])?;
        if initialized {
            let output = Command::new(self.postgres_tool("createdb")?)
                .args([
                    "--host",
                    "127.0.0.1",
                    "--port",
                    "5432",
                    "--username",
                    "athanor",
                    "athanor",
                ])
                .env("PGPASSWORD", self.secrets()?.postgres_password)
                .stdin(Stdio::null())
                .output()?;
            if !output.status.success() {
                let _ = self.postgres(&[
                    "-D",
                    &self.layout.postgres_data().to_string_lossy(),
                    "-w",
                    "-t",
                    "30",
                    "-m",
                    "fast",
                    "stop",
                ]);
                bail!(
                    "managed database creation failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }
        }
        let result = action();
        let stop = self.postgres(&[
            "-D",
            &self.layout.postgres_data().to_string_lossy(),
            "-w",
            "-t",
            "30",
            "-m",
            "fast",
            "stop",
        ]);
        match (result, stop) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }
}
impl RuntimeControl for NativeRuntimeControl {
    fn backup_database(&self, backup_dir: &Path) -> Result<Option<PathBuf>> {
        self.run(&[
            "backup",
            "--output-dir",
            &backup_dir.to_string_lossy(),
            "--keep",
            "3",
        ])?;
        let newest = fs::read_dir(backup_dir)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|part| part.to_str()) == Some("json"))
            .filter_map(|path| {
                fs::metadata(&path)
                    .ok()
                    .and_then(|meta| meta.modified().ok())
                    .map(|time| (time, path))
            })
            .max_by_key(|(time, _)| *time)
            .map(|(_, path)| path);
        newest
            .map(Some)
            .context("backup command did not produce a manifest")
    }
    fn import_legacy(&self, source: &Path, backup_dir: &Path) -> Result<()> {
        fn copy_bounded(source: &Path, destination: &Path, depth: u8) -> Result<()> {
            if depth > 8 {
                bail!("legacy import exceeded bounded directory depth");
            }
            fs::create_dir_all(destination)?;
            for entry in fs::read_dir(source)?.take(10_000) {
                let entry = entry?;
                let name = entry.file_name();
                if matches!(
                    name.to_str(),
                    Some("node_modules" | "target" | ".venv" | "__pycache__")
                ) {
                    continue;
                }
                let target = destination.join(&name);
                if entry.file_type()?.is_dir() {
                    copy_bounded(&entry.path(), &target, depth + 1)?;
                } else if entry.file_type()?.is_file() {
                    fs::copy(entry.path(), target)?;
                }
            }
            Ok(())
        }
        copy_bounded(
            source,
            &backup_dir.join(source.file_name().context("legacy path has no name")?),
            0,
        )
    }
    fn migrate_database(&self) -> Result<()> {
        self.with_database(|| self.run(&["migrations"]).map(|_| ()))
    }
    fn restore_database(&self, backup: &Path) -> Result<()> {
        self.with_database(|| {
            let url = self.database_url()?;
            let database = url
                .rsplit('/')
                .next()
                .context("DATABASE_URL has no database name")?
                .split('?')
                .next()
                .unwrap_or("athanor");
            self.run(&[
                "restore",
                "--manifest",
                &backup.to_string_lossy(),
                "--confirm-database",
                database,
            ])
            .map(|_| ())
        })
    }
    fn wait_ready(&self) -> Result<()> {
        self.run(&["health", "--skip-embedding"]).map(|_| ())
    }
}
