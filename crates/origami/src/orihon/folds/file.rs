use super::{Fold, identity, replay, validate};
use crate::orihon::error::{Error, io, malformed, refusal};
use hearth::orihon::ReadRequest;
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::Path,
};

type Key = (String, i16, i64, String, String);

fn key(fold: &Fold) -> Key {
    (
        fold.room.clone(),
        fold.level,
        fold.index,
        fold.source_digest.clone(),
        fold.recipe_digest.clone(),
    )
}

fn check_children(fold: &Fold, folds: &HashMap<Key, Fold>) -> Result<(), Error> {
    for child in &fold.children {
        let key = (
            fold.room.clone(),
            child.level,
            child.index,
            child.source_digest.clone(),
            child.recipe_digest.clone(),
        );
        if !folds.contains_key(&key) {
            return Err(refusal(
                "invalid_fold_children",
                "fold references a missing exact child revision",
            ));
        }
    }
    Ok(())
}

fn lock(directory: &Path, exclusive: bool) -> Result<File, Error> {
    let path = directory.join(".orihon-folds.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| io(&path, error))?;

    if exclusive {
        file.lock().map_err(|error| io(&path, error))?;
    } else {
        file.lock_shared().map_err(|error| io(&path, error))?;
    }

    Ok(file)
}

// enough: file operations scan both books; add an index when measured file I/O dominates a page fold.
fn load(directory: &Path, room: &str, ceiling: Option<usize>) -> Result<HashMap<Key, Fold>, Error> {
    let path = directory.join("folds.jsonl");
    let file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(error) => return Err(io(&path, error)),
    };
    let mut reader = BufReader::new(file);
    let mut bytes = Vec::new();
    let mut folds = HashMap::new();
    let mut line = 0;

    loop {
        bytes.clear();
        let count = (&mut reader)
            .take(1024 * 1024 + 1)
            .read_until(b'\n', &mut bytes)
            .map_err(|error| io(&path, error))?;

        if count == 0 {
            break;
        }
        line += 1;

        // A torn final line must never be joined to the next accepted revision.
        if bytes.len() > 1024 * 1024 || bytes.last() != Some(&b'\n') {
            return Err(malformed(&path, line));
        }
        let fold: Fold = serde_json::from_slice(&bytes).map_err(|_| malformed(&path, line))?;
        validate(&fold).map_err(|_| malformed(&path, line))?;

        check_children(&fold, &folds).map_err(|_| malformed(&path, line))?;
        if fold.room != room || folds.insert(key(&fold), fold).is_some() {
            return Err(malformed(&path, line));
        }
        if ceiling.is_some_and(|limit| folds.len() > limit) {
            return Err(refusal(
                "fold_snapshot_limit",
                "fold snapshot exceeds 200000 folds",
            ));
        }
    }

    Ok(folds)
}

fn check_sources(directory: &Path, fold: &Fold) -> Result<(), Error> {
    let through = fold
        .sources
        .last()
        .expect("validated sources are nonempty")
        .seq;
    let mut after = if fold.level == 0 {
        fold.index.checked_sub(1).filter(|seq| *seq >= 0)
    } else {
        None
    };
    let mut matched = 0;

    loop {
        let receipt = crate::orihon::read_file(
            directory,
            &fold.room,
            ReadRequest {
                after_seq: after,
                until_seq: Some(through),
                turn_no: (fold.level == 1).then_some(fold.index),
                day: None,
                limit: 200,
            },
        )?;

        for record in &receipt.rows {
            let expected = fold.sources.get(matched);

            if !expected.is_some_and(|source| {
                source.seq == record.seq && source.content_digest == record.content_digest
            }) {
                return Err(refusal(
                    "invalid_fold_sources",
                    "fold sources differ from the complete source frontier",
                ));
            }
            matched += 1;
        }

        if !receipt.has_more {
            break;
        }
        after = receipt.next_after;
    }

    if matched != fold.sources.len() {
        return Err(refusal(
            "invalid_fold_sources",
            "fold references missing Orihon sources",
        ));
    }

    Ok(())
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> Result<(), Error> {
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|error| io(directory, error))
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> Result<(), Error> {
    // Windows flushes the file; the standard library cannot flush a directory handle.
    Ok(())
}

pub fn read_file(
    directory: &Path,
    room: &str,
    level: i16,
    index: i64,
    source_digest: &str,
    recipe_digest: &str,
) -> Result<Option<Fold>, Error> {
    identity(room, level, index, source_digest, recipe_digest)?;
    let _lock = lock(directory, false)?;
    let mut folds = load(directory, room, None)?;
    let found = folds.remove(&(
        room.to_owned(),
        level,
        index,
        source_digest.to_owned(),
        recipe_digest.to_owned(),
    ));

    if let Some(fold) = &found {
        if fold.level < 2 {
            check_sources(directory, fold)?;
        }
    }

    Ok(found)
}

pub fn list_file(directory: &Path, room: &str) -> Result<Vec<Fold>, Error> {
    super::prepare::identity(room, "room")?;
    let _lock = lock(directory, false)?;
    let folds = load(directory, room, Some(super::MAX_FOLDS))?;
    if folds.is_empty() {
        return Ok(Vec::new());
    }
    let book = crate::orihon::file::source_snapshot(directory, room)?;
    let mut pages: HashMap<i64, Vec<_>> = HashMap::new();
    for record in book.records.values() {
        pages.entry(record.turn_no).or_default().push(record);
    }
    for fold in folds.values().filter(|fold| fold.level < 2) {
        let valid = if fold.level == 0 {
            book.records
                .get(&fold.index)
                .is_some_and(|record| record.content_digest == fold.sources[0].content_digest)
        } else {
            let through = fold
                .sources
                .last()
                .expect("validated sources are nonempty")
                .seq;
            pages.get(&fold.index).is_some_and(|page| {
                let count = page.partition_point(|record| record.seq <= through);
                count == fold.sources.len()
                    && page[..count]
                        .iter()
                        .zip(&fold.sources)
                        .all(|(record, source)| {
                            record.seq == source.seq
                                && record.content_digest == source.content_digest
                        })
            })
        };
        if !valid {
            return Err(refusal(
                "invalid_fold_sources",
                "fold sources differ from the complete source frontier",
            ));
        }
    }
    let mut folds: Vec<_> = folds.into_values().collect();
    super::sort_folds(&mut folds);
    Ok(folds)
}

pub fn append_file(directory: &Path, fold: &Fold) -> Result<Fold, Error> {
    validate(fold)?;
    let _lock = lock(directory, true)?;
    let mut folds = load(directory, &fold.room, None)?;
    check_children(fold, &folds)?;
    if fold.level < 2 {
        check_sources(directory, fold)?;
    }
    let path = directory.join("folds.jsonl");

    if let Some(stored) = folds.remove(&key(fold)) {
        let accepted = replay(stored, fold)?;
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .and_then(|file| file.sync_all())
            .map_err(|error| io(&path, error))?;
        sync_directory(directory)?;

        return Ok(accepted);
    }

    let mut bytes =
        serde_json::to_vec(fold).map_err(|_| super::invalid("fold cannot be serialized"))?;
    bytes.push(b'\n');
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
        .map_err(|error| io(&path, error))?;
    file.write_all(&bytes).map_err(|error| io(&path, error))?;
    file.sync_all().map_err(|error| io(&path, error))?;
    sync_directory(directory)?;

    Ok(fold.clone())
}
