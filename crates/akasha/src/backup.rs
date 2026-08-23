use chrono::{DateTime, Utc};
use protocol::{
    DiagnosticCategory, DiagnosticDetails, DiagnosticEvidence, DiagnosticExecution,
    DiagnosticNextCheck, DiagnosticOwner, DiagnosticRetry, DiagnosticStage, DiagnosticTarget,
    DiagnosticTargetKind, DiagnosticWriteOutcome,
};
use percent_encoding::percent_decode_str;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::{
    env, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use uuid::Uuid;

const LEGACY_MIGRATIONS: &[&str] = &[
    "0001_create_memories",
    "0002_memory_threads_pivot",
    "0003_named_entities",
    "0004_coding_lessons",
    "0005_discord_chat",
    "0006_channel_summaries",
    "0007_continuity_rails",
    "0008_coding_lessons_voice_negation",
    "0009_bot_decision_rows",
    "0009_memories_dates_array",
    "0010_gym_walk_ledger",
    "0011_wake_triggers",
    "0012_project_lessons",
    "0013_coding_lessons_intention_alignment",
    "0014_coding_lessons_long_running_processes",
    "0015_coding_lessons_powershell_encoding",
    "0016_pgvector_and_chunks_8b",
    "0017_memory_chunks_4b",
    "0018_coding_lessons_semantic_duplication",
    "0019_writing_lessons",
    "0020_anamnesis_cabinet",
    "0021_coding_lessons_always_on",
    "0022_memory_clusters_live_space",
    "0023_memory_clusters_centroid",
    "0024_memory_erasure",
    "0025_nemotron_2048",
];

fn known_migration_lineage(versions: &[String]) -> bool {
    if versions.is_empty() {
        return false;
    }
    let prefix_of = |lineage: &[String]| {
        versions.len() <= lineage.len()
            && versions
                .iter()
                .zip(lineage)
                .all(|(actual, expected)| actual == expected)
    };
    let consolidated = crate::migrations::consolidated_version_labels();
    let legacy: Vec<String> = LEGACY_MIGRATIONS.iter().map(|s| (*s).to_owned()).collect();
    prefix_of(&consolidated) || prefix_of(&legacy)
}

#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("backup configuration: {0}")]
    Config(String),
    /// The Athanor state root is unresolved, so there is no default place for
    /// dumps. Treated as a configuration fault, because that is what it is.
    #[error("backup configuration: {0}")]
    State(#[from] crate::state::StateRootError),
    #[error("backup io: {0}")]
    Io(#[from] io::Error),
    #[error("backup command failed: {0}")]
    Command(String),
    #[error("backup manifest: {0}")]
    Manifest(String),
}
impl BackupError {
    pub fn diagnostics(&self, operation: &str) -> DiagnosticDetails {
        let (failure, retry, write_outcome, target) = match self {
            Self::Config(_) | Self::State(_) => (
                "configuration_invalid",
                DiagnosticRetry::AfterChange,
                DiagnosticWriteOutcome::NotStarted,
                DiagnosticTarget::new(DiagnosticTargetKind::File, "src/backup.rs"),
            ),
            Self::Io(_) => (
                "filesystem_error",
                DiagnosticRetry::ReconcileFirst,
                DiagnosticWriteOutcome::Unknown,
                DiagnosticTarget::new(DiagnosticTargetKind::File, "backup output directory"),
            ),
            Self::Command(_) => (
                "postgres_command_failed",
                DiagnosticRetry::ReconcileFirst,
                DiagnosticWriteOutcome::Unknown,
                DiagnosticTarget::new(DiagnosticTargetKind::Service, "pg_dump or pg_restore"),
            ),
            Self::Manifest(_) => (
                "manifest_invalid",
                DiagnosticRetry::AfterChange,
                DiagnosticWriteOutcome::NotStarted,
                DiagnosticTarget::new(DiagnosticTargetKind::File, "backup manifest"),
            ),
        };
        let observed = match self {
            Self::Io(error) => serde_json::json!({
                "failure": failure,
                "io_error_kind": error.kind().to_string(),
            }),
            _ => serde_json::json!({"failure": failure}),
        };
        DiagnosticDetails::new(DiagnosticCategory::Backup, DiagnosticStage::Backup)
            .operation(operation)
            .owner(
                DiagnosticOwner::new("athanor-substrate")
                    .path("src/backup.rs")
                    .symbol(match operation {
                        "restore" => "restore_checked",
                        _ => "backup_with_migrations",
                    }),
            )
            .expected(match operation {
                "restore" => serde_json::json!({
                    "restore": "validated manifest and confirmed target database",
                }),
                _ => serde_json::json!({
                    "backup": "durable custom-format dump and manifest",
                }),
            })
            .observed(observed.clone())
            .evidence(
                DiagnosticEvidence::new("backup_failure")
                    .summary("Backup diagnostics omit command stderr, database URLs, and passwords")
                    .data(observed),
            )
            .target(DiagnosticTarget::new(
                DiagnosticTargetKind::File,
                "src/backup.rs",
            ))
            .target(target.clone())
            .next_check(
                DiagnosticNextCheck::new("inspect_backup_target")
                    .target(target)
                    .expected(serde_json::json!({"failure_resolved": failure})),
            )
            .next_check(
                DiagnosticNextCheck::new(if retry == DiagnosticRetry::ReconcileFirst {
                    "reconcile_backup_or_restore"
                } else {
                    "retry_backup_or_restore"
                })
                .expected(serde_json::json!({"safe_retry": retry == DiagnosticRetry::SafeNow})),
            )
            .execution(DiagnosticExecution::new(
                operation == "restore",
                write_outcome,
                retry,
            ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub database: String,
    pub created_at: String,
    pub size: u64,
    pub sha256: String,
    pub format: String,
    pub schema_migrations: Vec<String>,
    pub pg_dump_version: String,
    pub dump: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupHealth {
    pub ok: bool,
    pub directory: PathBuf,
    pub newest: Option<String>,
    pub age_hours: Option<f64>,
    pub bytes: Option<u64>,
    pub error: Option<String>,
}

fn use_wsl_pg() -> bool {
    cfg!(windows) && env::var("SOLARISAEL_PG_WSL").as_deref() == Ok("1")
}

fn pg_command(name: &str) -> Command {
    if use_wsl_pg() {
        let mut command = Command::new("wsl.exe");
        let mut wslenv = env::var("WSLENV").unwrap_or_default();
        if !wslenv.split(':').any(|entry| entry == "PGPASSWORD/u") {
            if !wslenv.is_empty() {
                wslenv.push(':');
            }
            wslenv.push_str("PGPASSWORD/u");
        }
        command.env("WSLENV", wslenv);
        command.args(["--exec", name]);
        return command;
    }
    let executable = env::var_os("PG_BIN_DIR")
        .map(PathBuf::from)
        .map(|dir| {
            dir.join(if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.into()
            })
        })
        .unwrap_or_else(|| {
            PathBuf::from(if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.into()
            })
        });
    Command::new(executable)
}

fn pg_path(path: &Path) -> Result<String, BackupError> {
    if !use_wsl_pg() {
        return Ok(path.to_string_lossy().into_owned());
    }
    let output = Command::new("wsl.exe")
        .args(["--exec", "wslpath", "-a"])
        .arg(path)
        .output()
        .map_err(BackupError::Io)?;
    if !output.status.success() {
        return Err(BackupError::Command(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
fn db_parts(raw: &str) -> Result<(String, Option<String>, String), BackupError> {
    let u =
        Url::parse(raw).map_err(|e| BackupError::Config(format!("invalid database URL: {e}")))?;
    let forbidden = [
        "dbname", "database", "service", "host", "hostaddr", "port", "user", "username", "password",
    ];
    for (k, _) in u.query_pairs() {
        if forbidden.iter().any(|x| k.eq_ignore_ascii_case(x)) {
            return Err(BackupError::Config(format!(
                "database URL query key overrides identity: {k}"
            )));
        }
    }
    let db = u.path().trim_matches('/').to_string();
    if db.is_empty() {
        return Err(BackupError::Config(
            "database URL has no database name".into(),
        ));
    }
    let password = u
        .password()
        .map(|p| percent_decode_str(p).decode_utf8_lossy().into_owned());
    let mut safe = u.clone();
    if password.is_some() {
        safe.set_password(None)
            .map_err(|_| BackupError::Config("invalid database URL password".into()))?;
    }
    Ok((db, password, safe.to_string()))
}
fn run(mut c: Command) -> Result<std::process::Output, BackupError> {
    c.stdin(Stdio::null())
        .output()
        .map_err(BackupError::Io)
        .and_then(|o| {
            if o.status.success() {
                Ok(o)
            } else {
                Err(BackupError::Command(
                    String::from_utf8_lossy(&o.stderr).trim().to_string(),
                ))
            }
        })
}
fn version(name: &str) -> String {
    pg_command(name)
        .arg("--version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}
fn hash(path: &Path) -> Result<(u64, String), BackupError> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut n = 0;
    let mut b = [0; 8192];
    loop {
        let k = f.read(&mut b)?;
        if k == 0 {
            break;
        }
        n += k as u64;
        h.update(&b[..k]);
    }
    Ok((n, format!("{:x}", h.finalize())))
}
/// Extensions the restoring role is expected to find already installed. A
/// non-superuser owner cannot drop or recreate cluster extensions it does not
/// own, so their TOC entries are excluded from the restore entirely.
const PRESERVED_EXTENSIONS: &[&str] = &["vector", "pg_trgm"];

/// Reads the archive table of contents. Doubles as the dump validity check:
/// `pg_restore --list` fails on a truncated or corrupt custom-format dump.
fn dump_toc(path: &Path, url: &str, password: Option<&str>) -> Result<String, BackupError> {
    let mut c = pg_command("pg_restore");
    c.args(["--list"])
        .arg(pg_path(path)?)
        .args(["--dbname"])
        .arg(url);
    if let Some(p) = password {
        c.env("PGPASSWORD", p);
    }
    let out = run(c)?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A TOC entry line is `<dumpId>; <catalogOid> <oid> <DESC> <schema> <tag…>`.
/// Only the entries that create, comment on, or (under `--clean`) drop a
/// preserved extension match; every other object shape is left alone.
fn is_preserved_extension_entry(entry: &str) -> bool {
    let Some((id, rest)) = entry.split_once(';') else {
        return false;
    };
    let id = id.trim();
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    // Skip the catalog oid and object oid, then read the entry description.
    let mut fields = rest.split_whitespace().skip(2);
    let desc = fields.next();
    let _schema = fields.next();
    match desc {
        Some("EXTENSION") => {
            matches!(fields.next(), Some(tag) if PRESERVED_EXTENSIONS.contains(&tag))
        }
        Some("COMMENT") => {
            fields.next() == Some("EXTENSION")
                && matches!(fields.next(), Some(tag) if PRESERVED_EXTENSIONS.contains(&tag))
        }
        _ => false,
    }
}

/// Pure filter over `pg_restore --list` output. Comment and blank lines (the
/// archive header) pass through untouched; entry lines survive unless they
/// belong to a preserved extension. A TOC with no entries, or one that filters
/// down to nothing, is an error rather than a restore that quietly omits
/// everything.
fn filter_extension_toc(list: &str) -> Result<String, BackupError> {
    let mut kept: Vec<&str> = Vec::new();
    let mut entries = 0usize;
    let mut retained = 0usize;
    for line in list.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') {
            kept.push(line);
            continue;
        }
        entries += 1;
        if is_preserved_extension_entry(trimmed) {
            continue;
        }
        retained += 1;
        kept.push(line);
    }
    if entries == 0 {
        return Err(BackupError::Manifest(
            "dump table of contents has no restorable entries".into(),
        ));
    }
    if retained == 0 {
        return Err(BackupError::Manifest(
            "extension filtering left no restorable entries".into(),
        ));
    }
    let mut out = kept.join("\n");
    out.push('\n');
    Ok(out)
}

/// Owns a scratch restore list so it is removed whether `pg_restore` succeeds
/// or fails, including on an early return between creation and the run.
struct TempList(PathBuf);

impl Drop for TempList {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Writes the filtered list next to the manifest, falling back to the OS
/// temporary directory. The name is unpredictable so a shared backup directory
/// cannot be used to pre-seed or hijack the list.
fn write_temp_list(beside: &Path, body: &str) -> Result<TempList, BackupError> {
    let name = format!(".athanor-restore-{}.list", Uuid::new_v4());
    let mut last: Option<io::Error> = None;
    for dir in [beside.to_path_buf(), env::temp_dir()] {
        let path = dir.join(&name);
        match fs::File::create(&path).and_then(|mut f| {
            f.write_all(body.as_bytes())?;
            f.sync_all()
        }) {
            Ok(()) => return Ok(TempList(path)),
            Err(e) => {
                let _ = fs::remove_file(&path);
                last = Some(e);
            }
        }
    }
    Err(BackupError::Io(last.unwrap_or_else(|| {
        io::Error::other("no writable location for restore list")
    })))
}
fn rotate(dir: &Path, db: &str, keep: usize) -> Result<(), BackupError> {
    let mut pairs = fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let p = e.path();
            let n = p.file_name()?.to_str()?.to_owned();
            if n.starts_with(&format!("{db}-")) && n.ends_with(".manifest.json") {
                let m: Manifest = serde_json::from_slice(&fs::read(&p).ok()?).ok()?;
                let t = DateTime::parse_from_rfc3339(&m.created_at)
                    .ok()?
                    .with_timezone(&Utc);
                Some((p, m, t))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    pairs.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| b.0.cmp(&a.0)));
    for (mp, m, _) in pairs.into_iter().skip(keep) {
        let dp = mp.parent().unwrap_or(dir).join(m.dump);
        if dp.exists() {
            fs::remove_file(dp)?;
        }
        fs::remove_file(mp)?;
    }
    Ok(())
}

pub fn backup_with_migrations(
    database_url: &str,
    output_dir: &Path,
    keep: usize,
    source: Vec<String>,
) -> Result<Manifest, BackupError> {
    if keep == 0 {
        return Err(BackupError::Config("keep must be at least 1".into()));
    }
    if !known_migration_lineage(&source) {
        return Err(BackupError::Manifest(format!(
            "database schema migrations are unsupported: {}",
            source.join(", ")
        )));
    }
    fs::create_dir_all(output_dir)?;
    let (db, password, safe) = db_parts(database_url)?;
    let stem = format!("{db}-{}", Uuid::new_v4());
    let dump_name = format!("{stem}.dump");
    let dump = output_dir.join(&dump_name);
    let tmp = output_dir.join(format!(".{stem}.tmp"));
    let mut c = pg_command("pg_dump");
    c.args(["--format=custom", "--no-owner", "--no-acl", "--file"])
        .arg(pg_path(&tmp)?)
        .args(["--dbname"])
        .arg(&safe);
    if let Some(p) = password.as_deref() {
        c.env("PGPASSWORD", p);
    }
    if let Err(e) = run(c) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    let f = fs::OpenOptions::new().write(true).open(&tmp)?;
    f.sync_all()?;
    drop(f);
    dump_toc(&tmp, &safe, password.as_deref())?;
    fs::rename(&tmp, &dump)?;
    let (size, sha) = hash(&dump)?;
    let manifest = Manifest {
        database: db.clone(),
        created_at: Utc::now().to_rfc3339(),
        size,
        sha256: sha,
        format: "custom".into(),
        schema_migrations: source,
        pg_dump_version: version("pg_dump"),
        dump: dump_name,
    };
    let mp = output_dir.join(format!("{stem}.manifest.json"));
    let mt = output_dir.join(format!(".{stem}.manifest.tmp"));
    let data =
        serde_json::to_vec_pretty(&manifest).map_err(|e| BackupError::Manifest(e.to_string()))?;
    {
        let mut f = fs::File::create(&mt)?;
        f.write_all(&data)?;
        f.sync_all()?;
    }
    fs::rename(&mt, &mp)?;
    rotate(output_dir, &db, keep)?;
    Ok(manifest)
}
pub fn backup(database_url: &str, output_dir: &Path, keep: usize) -> Result<Manifest, BackupError> {
    backup_with_migrations(
        database_url,
        output_dir,
        keep,
        crate::migrations::consolidated_version_labels(),
    )
}

fn normalize_migration_order(mut versions: Vec<String>) -> Vec<String> {
    if versions
        .iter()
        .all(|version| version.parse::<u64>().is_ok())
    {
        versions.sort_by_key(|version| version.parse::<u64>().unwrap_or(u64::MAX));
    } else {
        versions.sort();
    }
    versions
}

pub async fn source_migrations(pool: &PgPool) -> Result<Vec<String>, BackupError> {
    let rows = sqlx::query("SELECT version::text FROM schema_migrations")
        .fetch_all(pool)
        .await
        .map_err(|e| BackupError::Command(format!("migration query: {e}")))?;
    Ok(normalize_migration_order(
        rows.into_iter().map(|r| r.get::<String, _>(0)).collect(),
    ))
}
pub async fn restore_checked(
    pool: &PgPool,
    database_url: &str,
    manifest_path: &Path,
    confirm: &str,
) -> Result<(), BackupError> {
    let shape:Option<String>=sqlx::query_scalar("SELECT format_type(a.atttypid,a.atttypmod) FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid WHERE c.relname='memory_chunks' AND a.attname='body_embedding' AND NOT a.attisdropped").fetch_optional(pool).await.map_err(|e|BackupError::Command(format!("schema preflight: {e}")))?;
    if shape.as_deref() != Some("vector(2048)") {
        return Err(BackupError::Config(format!(
            "incompatible embedding schema: {}",
            shape.unwrap_or_else(|| "missing".into())
        )));
    }
    let versions = source_migrations(pool).await?;
    if !known_migration_lineage(&versions) {
        return Err(BackupError::Config(
            "schema migration versions are incompatible".into(),
        ));
    }
    let manifest: Manifest = serde_json::from_slice(&fs::read(manifest_path)?)
        .map_err(|error| BackupError::Manifest(error.to_string()))?;
    if normalize_migration_order(manifest.schema_migrations) != versions {
        return Err(BackupError::Manifest(
            "restore manifest schema does not match the target database authority".into(),
        ));
    }
    restore(database_url, manifest_path, confirm)
}
pub fn restore(database_url: &str, manifest_path: &Path, confirm: &str) -> Result<(), BackupError> {
    let (db, password, safe) = db_parts(database_url)?;
    if confirm != db {
        return Err(BackupError::Config(
            "database confirmation does not match target database".into(),
        ));
    }
    let m: Manifest = serde_json::from_slice(&fs::read(manifest_path)?)
        .map_err(|e| BackupError::Manifest(e.to_string()))?;
    if m.database != db || m.format != "custom" || !known_migration_lineage(&m.schema_migrations) {
        return Err(BackupError::Manifest(
            "manifest database, format, or migrations mismatch".into(),
        ));
    }
    if Path::new(&m.dump).file_name().and_then(|x| x.to_str()) != Some(m.dump.as_str()) {
        return Err(BackupError::Manifest("manifest dump path is unsafe".into()));
    }
    let dump = manifest_path
        .parent()
        .unwrap_or(Path::new("."))
        .join(&m.dump);
    if !dump.exists() {
        return Err(BackupError::Manifest("dump is missing".into()));
    }
    let (size, sha) = hash(&dump)?;
    if size != m.size || sha != m.sha256 {
        return Err(BackupError::Manifest(
            "dump checksum or size mismatch".into(),
        ));
    }
    let toc = dump_toc(&dump, &safe, password.as_deref())?;
    let list = filter_extension_toc(&toc)?;
    let list = write_temp_list(manifest_path.parent().unwrap_or(Path::new(".")), &list)?;
    let mut c = pg_command("pg_restore");
    c.args([
        "--clean",
        "--if-exists",
        "--no-owner",
        "--no-acl",
        "--single-transaction",
        "--use-list",
    ])
    .arg(pg_path(&list.0)?)
    .args(["--dbname"])
    .arg(&safe)
    .arg(pg_path(&dump)?);
    if let Some(p) = password {
        c.env("PGPASSWORD", p);
    }
    run(c).map(|_| ())
}
/// PostgreSQL dumps are mutable state, so they land under the Athanor state
/// directory unless `SOLARISAEL_BACKUP_DIR` names somewhere else. With neither
/// available there is no safe place to write, and that is an error rather than
/// a dump dropped into a guessed directory.
pub fn default_backup_dir() -> Result<PathBuf, BackupError> {
    if let Some(dir) = env::var_os("SOLARISAEL_BACKUP_DIR").filter(|dir| !dir.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    Ok(crate::state::substrate_state_dir()?.join("backups"))
}

pub fn backup_health(max_age_hours: f64) -> Result<BackupHealth, BackupError> {
    backup_health_in(default_backup_dir()?, max_age_hours)
}

pub fn backup_health_in(
    directory: PathBuf,
    max_age_hours: f64,
) -> Result<BackupHealth, BackupError> {
    if !max_age_hours.is_finite() || max_age_hours <= 0.0 {
        return Err(BackupError::Config(
            "maximum backup age must be a positive finite number".into(),
        ));
    }
    if !directory.is_dir() {
        return Ok(BackupHealth {
            ok: false,
            directory,
            newest: None,
            age_hours: None,
            bytes: None,
            error: Some("backup directory does not exist".into()),
        });
    }
    let newest = fs::read_dir(&directory)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension().and_then(|value| value.to_str()) == Some("dump"))
                .then(|| entry.metadata().ok().map(|metadata| (path, metadata)))
                .flatten()
        })
        .max_by_key(|(_, metadata)| metadata.modified().ok());
    let Some((dump, metadata)) = newest else {
        return Ok(BackupHealth {
            ok: false,
            directory,
            newest: None,
            age_hours: None,
            bytes: None,
            error: Some("no dump files present".into()),
        });
    };
    let modified = metadata.modified()?;
    let age_hours = modified.elapsed().unwrap_or_default().as_secs_f64() / 3600.0;
    let mut header = [0_u8; 5];
    let header_ok = fs::File::open(&dump)
        .and_then(|mut file| file.read_exact(&mut header))
        .is_ok()
        && &header == b"PGDMP";
    let mut problems = Vec::new();
    if !header_ok {
        problems.push("newest dump is not a pg_dump custom-format archive".to_owned());
    }
    if age_hours > max_age_hours {
        problems.push(format!(
            "newest dump is {age_hours:.1}h old, past the {max_age_hours:.0}h bound"
        ));
    }
    Ok(BackupHealth {
        ok: problems.is_empty(),
        directory,
        newest: dump
            .file_name()
            .map(|value| value.to_string_lossy().into_owned()),
        age_hours: Some((age_hours * 100.0).round() / 100.0),
        bytes: Some(metadata.len()),
        error: (!problems.is_empty()).then(|| problems.join("; ")),
    })
}
pub async fn run_post_write(pool: &PgPool, database_url: &str) -> Result<(), BackupError> {
    let keep = env::var("SOLARISAEL_BACKUP_KEEP")
        .ok()
        .and_then(|x| x.parse().ok())
        .unwrap_or(3);
    let source = source_migrations(pool).await?;
    backup_with_migrations(database_url, &default_backup_dir()?, keep, source).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encoded_password() {
        let (_, p, s) = db_parts("postgres://u:p%40ss%2Fword@host/db").unwrap();
        assert_eq!(p.as_deref(), Some("p@ss/word"));
        assert!(!s.contains("p%40"));
    }
    #[test]
    fn query_identity_rejected() {
        assert!(db_parts("postgres://host/db?dbname=other").is_err());
    }
    #[test]
    fn accepts_known_migration_lineage_prefixes_only() {
        let strings = |lineage: &[&str]| {
            lineage
                .iter()
                .map(|version| (*version).to_owned())
                .collect::<Vec<_>>()
        };
        let consolidated = crate::migrations::consolidated_version_labels();
        assert!(known_migration_lineage(&consolidated));
        assert!(
            consolidated.contains(&"18".to_owned()),
            "allowlist must track the registry"
        );
        assert!(known_migration_lineage(&strings(LEGACY_MIGRATIONS)));
        let mut previous_consolidated = consolidated.clone();
        previous_consolidated.pop();
        assert!(known_migration_lineage(&previous_consolidated));
        assert!(!known_migration_lineage(&[]));
        assert!(!known_migration_lineage(&["0001".into(), "0002".into()]));
        assert!(!known_migration_lineage(&["1".into(), "3".into()]));
    }

    #[test]
    fn migration_order_is_numeric_for_consolidated_and_lexical_for_legacy_versions() {
        assert_eq!(
            normalize_migration_order(vec!["1".into(), "10".into(), "2".into()]),
            ["1", "2", "10"]
        );
        assert_eq!(
            normalize_migration_order(vec![
                "0002_memory_threads_pivot".into(),
                "0001_create_memories".into(),
            ]),
            ["0001_create_memories", "0002_memory_threads_pivot"]
        );
    }

    #[test]
    fn keep_rejects_zero() {
        assert!(backup("postgres://host/db", Path::new("target/nope"), 0).is_err());
    }

    #[test]
    fn backup_health_requires_a_custom_format_dump() {
        let dir = env::temp_dir().join(format!("athanor-health-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("db.dump"), b"not-a-dump").unwrap();
        let health = backup_health_in(dir.clone(), 24.0).unwrap();
        assert!(!health.ok);
        assert!(health.error.unwrap().contains("custom-format"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn restore_refuses_wrong_or_ambiguous_database_authority_before_commands() {
        let missing = Path::new("missing.manifest.json");
        assert!(matches!(
            restore("postgres://host/owned", missing, "other"),
            Err(BackupError::Config(_))
        ));

        let dir = env::temp_dir().join(format!("athanor-restore-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let manifest = dir.join("ambiguous.manifest.json");
        fs::write(
            &manifest,
            serde_json::to_vec(&Manifest {
                database: "other".into(),
                created_at: "2026-08-10T00:00:00Z".into(),
                size: 0,
                sha256: String::new(),
                format: "custom".into(),
                schema_migrations: crate::migrations::consolidated_version_labels(),
                pg_dump_version: "16".into(),
                dump: "other.dump".into(),
            })
            .unwrap(),
        )
        .unwrap();
        assert!(matches!(
            restore("postgres://host/owned", &manifest, "owned"),
            Err(BackupError::Manifest(_))
        ));
        fs::remove_dir_all(dir).unwrap();
    }
    const TOC: &str = "\
;
; Archive created at 2026-08-09 11:04:12 UTC
;     dbname: athanor
;     TOC Entries: 9
;     Compression: gzip
;     Dump Version: 1.15-0
;     Format: CUSTOM
;     Dumped by pg_dump version: 16.3
;
;
; Selected TOC Entries:
;
5; 2615 2200 SCHEMA - public solarisael
4216; 0 0 COMMENT - SCHEMA public solarisael
2; 3079 16389 EXTENSION - vector 
4217; 0 0 COMMENT - EXTENSION vector 
3; 3079 16548 EXTENSION - pg_trgm 
4218; 0 0 COMMENT - EXTENSION pg_trgm 
4; 3079 16700 EXTENSION - btree_gin 
4219; 0 0 COMMENT - EXTENSION btree_gin 
216; 1259 16800 TABLE public memories solarisael
4205; 0 16800 TABLE DATA public memories solarisael
229; 1259 16912 TABLE public vector solarisael
3999; 2606 16815 CONSTRAINT public memories memories_pkey solarisael
";

    #[test]
    fn toc_filter_drops_only_preserved_extensions_and_their_comments() {
        let out = filter_extension_toc(TOC).unwrap();
        for dropped in [
            "EXTENSION - vector",
            "COMMENT - EXTENSION vector",
            "EXTENSION - pg_trgm",
            "COMMENT - EXTENSION pg_trgm",
        ] {
            assert!(!out.contains(dropped), "still present: {dropped}");
        }
        assert_eq!(out.lines().count(), TOC.lines().count() - 4);
    }

    #[test]
    fn toc_filter_keeps_headers_unrelated_objects_and_lookalike_tags() {
        let out = filter_extension_toc(TOC).unwrap();
        for kept in [
            "; Archive created at 2026-08-09 11:04:12 UTC",
            ";     dbname: athanor",
            "; Selected TOC Entries:",
            "5; 2615 2200 SCHEMA - public solarisael",
            "4216; 0 0 COMMENT - SCHEMA public solarisael",
            "4; 3079 16700 EXTENSION - btree_gin",
            "4219; 0 0 COMMENT - EXTENSION btree_gin",
            "216; 1259 16800 TABLE public memories solarisael",
            "4205; 0 16800 TABLE DATA public memories solarisael",
            // A table literally named `vector` must survive.
            "229; 1259 16912 TABLE public vector solarisael",
            "3999; 2606 16815 CONSTRAINT public memories memories_pkey solarisael",
        ] {
            assert!(out.contains(kept), "missing: {kept}");
        }
    }

    #[test]
    fn toc_filter_rejects_a_toc_with_no_entries() {
        assert!(filter_extension_toc("").is_err());
        assert!(filter_extension_toc(";\n; Selected TOC Entries:\n;\n").is_err());
    }

    #[test]
    fn toc_filter_rejects_a_toc_that_filters_down_to_nothing() {
        assert!(
            filter_extension_toc(
                ";\n2; 3079 16389 EXTENSION - vector \n3; 3079 16548 EXTENSION - pg_trgm \n"
            )
            .is_err()
        );
    }

    #[test]
    fn toc_filter_ignores_lines_without_a_dump_id() {
        let garbage = "not a toc line at all\n216; 1259 16800 TABLE public memories solarisael\n";
        assert_eq!(filter_extension_toc(garbage).unwrap(), garbage);
    }

    #[test]
    fn temp_list_is_written_beside_the_manifest_and_removed_on_drop() {
        let dir = env::temp_dir().join(format!("athanor-list-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = {
            let list = write_temp_list(&dir, "body\n").unwrap();
            assert_eq!(fs::read_to_string(&list.0).unwrap(), "body\n");
            assert_eq!(list.0.parent(), Some(dir.as_path()));
            list.0.clone()
        };
        assert!(!path.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn temp_list_falls_back_to_the_os_temp_dir() {
        let missing = env::temp_dir().join(format!("athanor-absent-{}", Uuid::new_v4()));
        let list = write_temp_list(&missing, "body\n").unwrap();
        assert_eq!(list.0.parent(), Some(env::temp_dir().as_path()));
        assert!(list.0.exists());
    }
}
