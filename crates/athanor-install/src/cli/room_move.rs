//! The door `athanor move ROOM [--file NAME]... [--offline]`. It moves a room
//! out of its vault folder `<roomsRoot>/<room>` into its installation folder
//! `<dataRoot>/rooms/<room>`: the marker, `.omp/runtime/`, the room door
//! lines, each named file, and the Host state `<dataRoot>/state/host/<room>`
//! as `host/`. Everything else stays in the vault as the library of the room.
//!
//! The files go to a staging folder first. One rename then puts the staging
//! folder in place, so that a Host never opens a half-moved room. The COMMIT
//! of the spirit record sends the notification that makes a running Host
//! rebuild the room on its new folders. A failure undoes each change of the run.

use crate::cli::journal::{Journal, undone};
use crate::cli::spirits::{HOST_STATE, MARKER};
use crate::cli::status::host_answer;
use crate::{app::installed_runtime, layout::InstallLayout};
use akasha::spirits;
use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

const RUNTIME: &str = ".omp/runtime";
/// The room page of the boat door lines, which the adapter reads.
const DOOR_LINES: &str = "handoff-door.md";
const INSTRUCTIONS: &str = "AGENTS.md";
/// The marker key that names the library; the Host opens it as an `@` root.
const LIBRARY: &str = "library";
const USAGE: &str = "usage: athanor move ROOM [--file NAME]... [--offline]";

/// The folders and the Host of one move.
pub(crate) struct Places {
    /// `InstallLayout::rooms()`.
    pub(crate) rooms: PathBuf,
    /// `roomsRoot`: the vault, which keeps the library of each room.
    pub(crate) vault: PathBuf,
    /// `InstallLayout::host_state()`: the Host state of each vault room.
    pub(crate) host_state: PathBuf,
    pub(crate) host_port: u16,
    pub(crate) host_token: String,
}

pub(crate) struct Request {
    pub(crate) room: String,
    pub(crate) files: Vec<PathBuf>,
    pub(crate) offline: bool,
}

pub fn run(arguments: &[String]) -> Result<ExitCode> {
    let request = parse(arguments)?;
    let layout = InstallLayout::from_environment()?;
    let (config, secrets) = installed_runtime(&layout)?;
    let places = Places {
        rooms: layout.rooms(),
        vault: config.rooms_root.clone(),
        host_state: layout.host_state(),
        host_port: config.host_port,
        host_token: secrets.host_token.clone(),
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
        move_room(&pool, &places, &request).await
    })?;

    println!("{answer}");
    Ok(ExitCode::SUCCESS)
}

fn parse(arguments: &[String]) -> Result<Request> {
    let (mut room, mut files, mut offline) = (None, Vec::new(), false);
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--file" => files.push(PathBuf::from(rest.next().context("--file requires a value")?)),
            "--offline" => offline = true,
            flag if flag.starts_with("--") => bail!("unknown option {flag}; {USAGE}"),
            _ if room.is_none() => room = Some(argument.clone()),
            _ => bail!(USAGE),
        }
    }

    Ok(Request {
        room: room.context(USAGE)?,
        files,
        offline,
    })
}

pub(crate) async fn move_room(pool: &PgPool, places: &Places, request: &Request) -> Result<Value> {
    let room = request.room.as_str();
    if !protocol::is_safe_room_key(room) {
        bail!("room {room:?} must be lowercase words joined by single hyphens, and not house");
    }
    let (library, target) = (places.vault.join(room), places.rooms.join(room));
    if fs::symlink_metadata(&target).is_ok() {
        return unchanged(&library, &target, room);
    }
    named(&library.join(MARKER), room)?;
    let names = carried(&library, &request.files)?;
    let staging = places.rooms.join(format!(".{room}.moving"));
    if fs::symlink_metadata(&staging).is_ok() {
        bail!(
            "{} remains from an earlier move; compare it with {} and remove it by hand",
            staging.display(),
            library.display()
        );
    }
    let agents = instructions(&library, &names)?;
    no_live_session(places, room, request.offline)?;

    let pending = spirits::begin_move(pool, room).await?;
    let mut journal = Journal::default();
    let moved = match place(&mut journal, places, room, &names, agents.as_ref()) {
        Ok(moved) => moved,
        Err(error) => return Err(undone(error, journal)),
    };
    if let Err(error) = no_live_session(places, room, request.offline) {
        return Err(undone(error, journal));
    }
    if let Err(error) = pending.commit().await {
        return Err(undone(anyhow!("the database COMMIT failed: {error}"), journal));
    }

    let (rewrites, appended) = agents.map_or((Vec::new(), None), |agents| (agents.rewrites, Some(agents.appended)));
    Ok(json!({
        "ok": true,
        "change": "moved",
        "room": room,
        "folder": target,
        "library": library,
        "moved": moved,
        "rewrites": rewrites,
        "appended": appended,
    }))
}

/// A second run finds the room in place and its marker gone from the vault.
fn unchanged(library: &Path, target: &Path, room: &str) -> Result<Value> {
    named(&target.join(MARKER), room)?;
    if fs::symlink_metadata(library.join(MARKER)).is_ok() {
        bail!(
            "{} and {} both hold a marker of room {room}; move refuses to choose",
            target.display(),
            library.display()
        );
    }

    Ok(json!({"ok": true, "change": "unchanged", "room": room, "folder": target}))
}

/// Refuses a marker that is missing or names another room.
fn named(marker: &Path, room: &str) -> Result<()> {
    let bytes = match fs::read(marker) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            bail!("{} is missing; move takes only a marked room", marker.display())
        }
        Err(error) => return Err(error).with_context(|| format!("read {}", marker.display())),
    };
    let document: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("{} is not JSON", marker.display()))?;
    let named = document.get("room").and_then(Value::as_str);
    if named != Some(room) {
        bail!("{} names room {named:?}, not {room}", marker.display());
    }

    Ok(())
}

/// The relative names that the move carries: the marker, `.omp/runtime`, the
/// room door lines when present, and each `--file`. A `--file` is a relative
/// path that exists, and it neither holds nor sits inside another name.
fn carried(library: &Path, files: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut names = vec![PathBuf::from(MARKER), PathBuf::from(RUNTIME)];
    if fs::symlink_metadata(library.join(DOOR_LINES)).is_ok() {
        names.push(DOOR_LINES.into());
    }
    for file in files {
        let relative = !file.as_os_str().is_empty()
            && file.components().all(|part| matches!(part, Component::Normal(_)));
        if !relative || file == Path::new(HOST_STATE) {
            bail!(
                "--file {} must be a relative path inside the room folder, and not {HOST_STATE}",
                file.display()
            );
        }
        if let Some(name) = names.iter().find(|name| name.starts_with(file) || file.starts_with(name)) {
            bail!("--file {} overlaps {}, which the move carries", file.display(), name.display());
        }
        fs::symlink_metadata(library.join(file))
            .with_context(|| format!("--file {} names no file of {}", file.display(), library.display()))?;
        names.push(file.clone());
    }

    Ok(names)
}

/// The moved `AGENTS.md`, planned before any write.
struct Instructions {
    text: String,
    rewrites: Vec<Value>,
    appended: String,
}

/// Each `@` reference whose file stays in the vault names its absolute vault
/// path; a reference to a moved file keeps its relative path. One appended
/// line tells the spirit where its library stays.
fn instructions(library: &Path, names: &[PathBuf]) -> Result<Option<Instructions>> {
    if !names.iter().any(|name| name == Path::new(INSTRUCTIONS)) {
        return Ok(None);
    }
    let path = library.join(INSTRUCTIONS);
    let source =
        fs::read_to_string(&path).with_context(|| format!("read {} as UTF-8", path.display()))?;
    let mut text = String::with_capacity(source.len() + 128);
    let mut rewrites = Vec::new();
    for (index, line) in source.split_inclusive('\n').enumerate() {
        let Some(span) = host::agents_reference_span(line) else {
            text.push_str(line);
            continue;
        };
        let reference = &line[span.clone()];
        let resolved = lexical(&library.join(reference));
        let moves = resolved
            .strip_prefix(library)
            .is_ok_and(|inside| names.iter().any(|name| inside.starts_with(name)));
        if moves || Path::new(reference).is_absolute() {
            text.push_str(line);
            continue;
        }
        let absolute = resolved.display().to_string();
        let quoted = line[..span.start].ends_with(['"', '\'']);
        let written = if !quoted && absolute.contains(char::is_whitespace) {
            format!("\"{absolute}\"")
        } else {
            absolute.clone()
        };
        text.push_str(&line[..span.start]);
        text.push_str(&written);
        text.push_str(&line[span.end..]);
        rewrites.push(json!({"line": index + 1, "from": reference, "to": absolute}));
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    let appended = format!("The library of this room stays in {}/.", library.display());
    text.push_str(&format!("\n{appended}\n"));

    Ok(Some(Instructions {
        text,
        rewrites,
        appended,
    }))
}

/// `..` and `.` resolved by name only: the vault path that the line meant.
fn lexical(path: &Path) -> PathBuf {
    let mut resolved = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                resolved.pop();
            }
            Component::CurDir => {}
            other => resolved.push(other.as_os_str()),
        }
    }
    resolved
}

/// Moves each name and the Host state into the staging folder, writes the
/// library into the marker and the new `AGENTS.md`, and puts the staging
/// folder in place with one rename.
fn place(
    journal: &mut Journal,
    places: &Places,
    room: &str,
    names: &[PathBuf],
    agents: Option<&Instructions>,
) -> Result<Vec<Value>> {
    let (library, target) = (places.vault.join(room), places.rooms.join(room));
    let staging = places.rooms.join(format!(".{room}.moving"));
    let mut moved = Vec::new();
    journal.create_dirs(&staging)?;
    for name in names {
        let from = library.join(name);
        // Only `.omp/runtime` can be absent here; each other name was found.
        if fs::symlink_metadata(&from).is_err() {
            continue;
        }
        let to = staging.join(name);
        journal.create_dirs(to.parent().context("a carried name has no folder")?)?;
        journal.move_path(&from, &to)?;
        moved.push(json!({"from": from, "to": target.join(name)}));
    }
    let state = places.host_state.join(room);
    if fs::symlink_metadata(&state).is_ok() {
        journal.move_path(&state, &staging.join(HOST_STATE))?;
        moved.push(json!({"from": state, "to": target.join(HOST_STATE)}));
    } else {
        journal.create_dirs(&staging.join(HOST_STATE))?;
    }
    journal.rewrite_json(&staging.join(MARKER), |marker| {
        marker.insert(LIBRARY.into(), json!(library));
    })?;
    if let Some(agents) = agents {
        journal.rewrite(&staging.join(INSTRUCTIONS), agents.text.as_bytes())?;
    }
    journal.move_path(&staging, &target)?;

    Ok(moved)
}

/// Refuses while the Host registers a live session for the room. A Host that
/// does not answer refuses too, unless `--offline` states that it is stopped.
fn no_live_session(places: &Places, room: &str, offline: bool) -> Result<()> {
    let path = format!("/room/{room}/session");
    let (status, body) = match host_answer(places.host_port, "POST", &path, &places.host_token, r#"{"action":"status"}"#) {
        Ok(answer) => answer,
        // The operator states that the Host is stopped, so no session can live.
        Err(_) if offline => return Ok(()),
        Err(error) => bail!(
            "the Host on port {} does not answer ({error:#}); start it, or use --offline only while it is stopped",
            places.host_port
        ),
    };
    if status == 404 && body.contains("unknown_room") {
        return Ok(());
    }
    if status != 200 {
        bail!("the Host answered {status} to the session status of room {room}: {body}");
    }
    let answer: Value =
        serde_json::from_str(&body).context("the Host session status is not JSON")?;
    let live: Vec<&Value> = answer["sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|session| session["connected"] == true)
        .collect();
    if !live.is_empty() {
        bail!(
            "room {room} has {} live session(s) on the Host; close them first: {}",
            live.len(),
            Value::from(live.into_iter().cloned().collect::<Vec<_>>())
        );
    }

    Ok(())
}

#[cfg(test)]
#[path = "room_move_tests.rs"]
mod tests;
