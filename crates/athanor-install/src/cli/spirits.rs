//! The doors of the spirit record: `athanor summon`, `athanor retire`,
//! `athanor rename`, and `athanor spirits`. The record is table `spirits`; a
//! running Host opens and closes rooms when it changes. A new room keeps one
//! folder in the installation folder, `<dataRoot>/rooms/<room>`, with its
//! Host state in `host/`. These doors write nothing under `roomsRoot`.

use crate::{app::installed_runtime, layout::InstallLayout};
use akasha::spirits::{self, PendingSpirit, SpiritRecord, SpiritWrite};
use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use serde_json::{Map, Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const MARKER: &str = ".athanor-room.json";
const ROOM_STATE: &str = ".omp/runtime/athanor-house-state.json";
/// The Host state folder inside an installation room folder.
pub(crate) const HOST_STATE: &str = "host";
const SUMMONED_BY: &str = "cli:summon";

/// The folders of this install that the doors use.
struct Places {
    /// `InstallLayout::rooms()`: the installation folder of each room.
    installed: PathBuf,
    /// `roomsRoot`: read only, to recognise a legacy room.
    legacy: PathBuf,
}

enum Door {
    Summon(SpiritRecord),
    Retire(String),
    Rename {
        old: String,
        new: String,
        spirit: Option<String>,
    },
    List,
}

pub fn run(mode: &str, arguments: &[String]) -> Result<ExitCode> {
    let door = parse(mode, arguments)?;
    let layout = InstallLayout::from_environment()?;
    let (config, secrets) = installed_runtime(&layout)?;
    let places = Places {
        installed: layout.rooms(),
        legacy: config.rooms_root.clone(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build the database runtime")?;
    let answer = runtime.block_on(async {
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&secrets.database_url())
            .await
            .context("connect to the AKASHA database")?;

        match door {
            Door::Summon(record) => written(summon(&pool, &places, &record).await?),
            Door::Retire(room) => written(spirits::retire_spirit(&pool, &room).await?),
            Door::Rename { old, new, spirit } => {
                written(rename(&pool, &places, &old, &new, spirit.as_deref()).await?)
            }
            Door::List => Ok(json!({"ok": true, "spirits": spirits::active_spirits(&pool).await?})),
        }
    })?;

    println!("{answer}");
    Ok(ExitCode::SUCCESS)
}

fn parse(mode: &str, arguments: &[String]) -> Result<Door> {
    let flag = match mode {
        "summon" => Some("--operator"),
        "rename" => Some("--spirit"),
        _ => None,
    };
    let mut positional = Vec::new();
    let mut value = None;
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        if Some(argument.as_str()) == flag {
            value = Some(
                rest.next()
                    .with_context(|| format!("{argument} requires a value"))?
                    .clone(),
            );
        } else if argument.starts_with("--") {
            bail!("unknown option {argument}; run athanor help");
        } else {
            positional.push(argument.clone());
        }
    }

    match (mode, positional.as_slice(), value) {
        ("summon", [room, spirit], Some(operator)) => Ok(Door::Summon(SpiritRecord {
            room: room.clone(),
            spirit: spirit.clone(),
            operator,
        })),
        ("retire", [room], None) => Ok(Door::Retire(room.clone())),
        ("rename", [old, new], spirit) => Ok(Door::Rename {
            old: old.clone(),
            new: new.clone(),
            spirit,
        }),
        ("spirits", [], None) => Ok(Door::List),
        ("summon", ..) => bail!("usage: athanor summon ROOM SPIRIT --operator NAME"),
        ("retire", ..) => bail!("usage: athanor retire ROOM"),
        ("rename", ..) => bail!("usage: athanor rename OLD NEW [--spirit NAME]"),
        _ => bail!("usage: athanor spirits"),
    }
}

fn written(write: SpiritWrite) -> Result<Value> {
    Ok(json!({"ok": true, "change": write.change, "spirit": write.spirit}))
}

/// Writes the record and, before COMMIT, the missing files of the
/// installation folder. The Host opens the room on the NOTIFY that COMMIT
/// sends, so the files must exist first. A room with a legacy folder under
/// `roomsRoot` gets no installation folder.
async fn summon(pool: &PgPool, places: &Places, record: &SpiritRecord) -> Result<SpiritWrite> {
    record.validate()?;
    let folder = places.installed.join(&record.room);
    let legacy = !folder.is_dir() && places.legacy.join(&record.room).is_dir();
    if !legacy {
        check_names(&folder, &record.room)?;
    }

    let pending = spirits::begin_summon(pool, record, SUMMONED_BY).await?;
    let mut journal = Journal::default();
    if !legacy && let Err(error) = room_files(&mut journal, &folder, record) {
        return Err(undone(error, journal));
    }

    commit(pending, journal).await
}

/// Moves the record and the rows of a room that never woke, and, before
/// COMMIT, its installation folder, which also holds its Host state. A room
/// that lives only in a legacy folder under `roomsRoot` is refused.
async fn rename(
    pool: &PgPool,
    places: &Places,
    old: &str,
    new: &str,
    spirit: Option<&str>,
) -> Result<SpiritWrite> {
    for room in [old, new] {
        if !protocol::is_safe_room_key(room) {
            bail!("room {room:?} must be lowercase words joined by single hyphens, and not house");
        }
    }
    let (old_dir, new_dir) = (places.installed.join(old), places.installed.join(new));

    if !old_dir.is_dir() && places.legacy.join(old).is_dir() {
        bail!(
            "room {old} lives in the legacy folder {}; rename moves only a room in {}",
            places.legacy.join(old).display(),
            places.installed.display()
        );
    }
    if new_dir.exists() {
        bail!("{} already exists", new_dir.display());
    }
    check_names(&old_dir, old)?;

    let pending = spirits::begin_rename(pool, old, new, spirit).await?;
    let record = pending.write().spirit.clone();
    let mut journal = Journal::default();
    if let Err(error) = move_room(&mut journal, &old_dir, &new_dir, &record, spirit.is_some()) {
        return Err(undone(error, journal));
    }

    commit(pending, journal).await
}

fn move_room(
    journal: &mut Journal,
    old_dir: &Path,
    new_dir: &Path,
    record: &SpiritRecord,
    respirited: bool,
) -> Result<()> {
    if old_dir.is_dir() {
        journal.rename(old_dir, new_dir)?;
        journal.rewrite_json(&new_dir.join(MARKER), |marker| {
            marker.insert("room".into(), json!(record.room));
            marker.insert("trueName".into(), json!(record.spirit));
        })?;
        journal.rewrite_json(&new_dir.join(ROOM_STATE), |state| {
            state.insert("room".into(), json!(record.room));
            if respirited {
                state.insert("agentName".into(), json!(record.spirit));
                state.insert("embodiedSpirit".into(), json!(record.spirit));
            }
        })?;
    }

    room_files(journal, new_dir, record)
}

async fn commit(pending: PendingSpirit, journal: Journal) -> Result<SpiritWrite> {
    match pending.commit().await {
        Ok(write) => Ok(write),
        Err(error) => Err(undone(anyhow!("the database COMMIT failed: {error}"), journal)),
    }
}

fn undone(error: anyhow::Error, journal: Journal) -> anyhow::Error {
    match journal.undo() {
        Ok(()) => anyhow!("{error:#}; the file changes of this run were undone"),
        Err(failures) => anyhow!("{error:#}; the undo of the file changes also failed: {failures}"),
    }
}

/// A marker or room-state file of another room refuses the door before any write.
fn check_names(folder: &Path, room: &str) -> Result<()> {
    for file in [MARKER, ROOM_STATE] {
        let path = folder.join(file);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
        };
        let named = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|document| document.get("room").and_then(Value::as_str).map(str::to_owned));

        if named.as_deref() != Some(room) {
            bail!("{} names room {named:?}, not {room}", path.display());
        }
    }

    Ok(())
}

/// Writes each missing file and folder of the installation folder: the
/// marker, the room-state file, and the Host state folder. Existing files stay.
fn room_files(journal: &mut Journal, folder: &Path, record: &SpiritRecord) -> Result<()> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Marker<'a> {
        version: u8,
        room: &'a str,
        true_name: &'a str,
    }

    let marker = folder.join(MARKER);
    let state = folder.join(ROOM_STATE);
    journal.create_dirs(state.parent().context("the room-state path has no folder")?)?;
    journal.create_dirs(&folder.join(HOST_STATE))?;
    if !marker.exists() {
        journal.create_file(
            &marker,
            &pretty(&Marker {
                version: 1,
                room: &record.room,
                true_name: &record.spirit,
            })?,
        )?;
    }
    if !state.exists() {
        journal.create_file(
            &state,
            &pretty(&host::fresh_room_state(&record.room, &record.spirit, &record.operator))?,
        )?;
    }

    Ok(())
}

fn pretty(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;

    bytes.push(b'\n');
    Ok(bytes)
}

/// Each file change of one door, so that a failed COMMIT can undo exactly
/// what this run did and nothing that existed before it.
#[derive(Default)]
struct Journal {
    steps: Vec<Step>,
}

enum Step {
    CreatedDir(PathBuf),
    CreatedFile(PathBuf),
    Rewrote { path: PathBuf, bytes: Vec<u8> },
    Moved { from: PathBuf, to: PathBuf },
}

impl Journal {
    fn create_dirs(&mut self, path: &Path) -> Result<()> {
        let missing: Vec<PathBuf> = path
            .ancestors()
            .take_while(|folder| !folder.exists())
            .map(Path::to_path_buf)
            .collect();

        for folder in missing.into_iter().rev() {
            fs::create_dir(&folder).with_context(|| format!("create {}", folder.display()))?;
            self.steps.push(Step::CreatedDir(folder));
        }
        Ok(())
    }

    fn create_file(&mut self, path: &Path, bytes: &[u8]) -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| format!("create {}", path.display()))?;

        self.steps.push(Step::CreatedFile(path.to_owned()));
        file.write_all(bytes)
            .with_context(|| format!("write {}", path.display()))
    }

    /// A missing file stays missing here; [`room_files`] writes a fresh one.
    fn rewrite_json(
        &mut self,
        path: &Path,
        change: impl FnOnce(&mut Map<String, Value>),
    ) -> Result<()> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
        };
        let mut document: Map<String, Value> = serde_json::from_slice(&bytes)
            .with_context(|| format!("{} is not a JSON object", path.display()))?;

        change(&mut document);
        let rewritten = pretty(&document)?;
        self.steps.push(Step::Rewrote {
            path: path.to_owned(),
            bytes,
        });
        fs::write(path, rewritten).with_context(|| format!("write {}", path.display()))
    }

    fn rename(&mut self, from: &Path, to: &Path) -> Result<()> {
        fs::rename(from, to)
            .with_context(|| format!("move {} to {}", from.display(), to.display()))?;
        self.steps.push(Step::Moved {
            from: from.to_owned(),
            to: to.to_owned(),
        });
        Ok(())
    }

    /// Undoes the steps in reverse order and names the path of each failure.
    fn undo(self) -> Result<(), String> {
        let failures: Vec<String> = self
            .steps
            .into_iter()
            .rev()
            .filter_map(|step| {
                let (path, result) = match &step {
                    Step::CreatedDir(path) => (path.clone(), fs::remove_dir(path)),
                    Step::CreatedFile(path) => (path.clone(), fs::remove_file(path)),
                    Step::Rewrote { path, bytes } => (path.clone(), fs::write(path, bytes)),
                    Step::Moved { from, to } => (to.clone(), fs::rename(to, from)),
                };
                result
                    .err()
                    .map(|error| format!("{}: {error}", path.display()))
            })
            .collect();

        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Kills: a summon that writes under roomsRoot or rewrites a marker on a
    // second run, and a failed COMMIT that leaves the room folder moved.
    #[tokio::test]
    #[ignore = "requires ATHANOR_SUBSTRATE_TEST_DATABASE_URL; creates and drops one isolated schema"]
    async fn a_failed_rename_commit_puts_the_room_folder_back() {
        let url = std::env::var("ATHANOR_SUBSTRATE_TEST_DATABASE_URL")
            .expect("the rename proof requires a dedicated PostgreSQL URL");
        assert!(
            !url.contains("solarisael_memory") && !url.contains("solarisael-house"),
            "refusing a live or production-looking database"
        );
        let schema = format!("athanor_rename_{}", uuid::Uuid::new_v4().simple());
        let pool = PgPoolOptions::new().max_connections(1).connect(&url).await.unwrap();
        for statement in [
            format!("CREATE SCHEMA {schema}"),
            format!("SET search_path TO {schema}, public"),
        ] {
            sqlx::query(&statement).execute(&pool).await.unwrap();
        }
        akasha::migrations::run_migrations(&pool).await.unwrap();
        let root = tempfile::tempdir().unwrap();
        let places = Places {
            installed: root.path().join("rooms"),
            legacy: root.path().join("vault"),
        };
        let record = SpiritRecord {
            room: "hugo".into(),
            spirit: "Hugo".into(),
            operator: "sol".into(),
        };

        let first = summon(&pool, &places, &record).await.unwrap();
        let marker = fs::read(places.installed.join("hugo").join(MARKER)).unwrap();
        let again = summon(&pool, &places, &record).await.unwrap();
        assert_eq!(
            (first.change, again.change),
            (spirits::SpiritChange::Summoned, spirits::SpiritChange::Unchanged)
        );
        assert_eq!(
            String::from_utf8(marker.clone()).unwrap(),
            "{\n  \"version\": 1,\n  \"room\": \"hugo\",\n  \"trueName\": \"Hugo\"\n}\n"
        );
        assert!(places.installed.join("hugo").join(HOST_STATE).is_dir());
        assert!(!places.legacy.exists(), "nothing is written under roomsRoot");
        sqlx::raw_sql(
            "CREATE FUNCTION refuse_at_commit() RETURNS trigger LANGUAGE plpgsql AS $$
             BEGIN RAISE EXCEPTION 'forced COMMIT failure'; END $$;
             CREATE CONSTRAINT TRIGGER refuse_at_commit AFTER UPDATE ON spirits
                 DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION refuse_at_commit();",
        )
        .execute(&pool)
        .await
        .unwrap();

        let error = rename(&pool, &places, "hugo", "hati", Some("Hati"))
            .await
            .unwrap_err();

        let error = format!("{error:#}");
        assert!(error.contains("COMMIT failed") && error.contains("were undone"), "{error}");
        assert_eq!(fs::read(places.installed.join("hugo").join(MARKER)).unwrap(), marker);
        assert!(places.installed.join("hugo").join(HOST_STATE).is_dir());
        assert!(!places.installed.join("hati").exists());
        let rooms: Vec<String> = sqlx::query_scalar("SELECT room FROM spirits")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(rooms, ["hugo"]);

        sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
            .execute(&pool)
            .await
            .unwrap();
    }
}
