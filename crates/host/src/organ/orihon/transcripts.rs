use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

pub(crate) fn migrate_and_lock(room: &Path) -> Result<File, String> {
    let directory = room.join(hearth::orihon::DIRECTORY);
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join("transcripts.lock"))
        .map_err(|error| error.to_string())?;
    lock.lock().map_err(|error| error.to_string())?;
    for entry in fs::read_dir(room).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(date) = name
            .strip_prefix("conversation_log_")
            .and_then(|name| name.strip_suffix(".md"))
        else {
            continue;
        };
        if date.len() != 10 || chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
            continue;
        }
        if !entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            return Err(format!(
                "transcript migration refuses non-file {}",
                entry.path().display()
            ));
        }
        let destination = PathBuf::from(hearth::conversation::transcript_path(
            &room.to_string_lossy(),
            date,
        ));
        if destination
            .try_exists()
            .map_err(|error| error.to_string())?
        {
            return Err(format!(
                "transcript migration preserves both files: {} already exists",
                destination.display()
            ));
        }
        fs::rename(entry.path(), &destination).map_err(|error| {
            format!(
                "cannot move transcript to {}: {error}",
                destination.display()
            )
        })?;
        OpenOptions::new()
            .write(true)
            .open(&destination)
            .and_then(|file| file.sync_all())
            .map_err(|error| error.to_string())?;
    }
    Ok(lock)
}
