use super::book::Book;
use super::error::{Error, invalid, io, malformed, refusal};
use super::prepare::{self, MAX_BATCH_BYTES};
use super::read::{ReadBounds, read_bounds};
use chrono::{Datelike, NaiveDate};
use hearth::orihon::{
    CaptureBatch, CaptureReceipt, MAX_BATCH_RECORDS, ReadReceipt, ReadRequest, Record,
};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

const MAX_LINE_BYTES: usize = MAX_BATCH_BYTES + 4096;
const LOCK_FILE: &str = ".orihon.lock";

fn lock(directory: &Path, exclusive: bool) -> Result<File, Error> {
    let path = directory.join(LOCK_FILE);
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

fn daily_name(name: &str) -> bool {
    let Some(day) = name.strip_suffix(".jsonl") else {
        return false;
    };
    NaiveDate::parse_from_str(day, "%Y-%m-%d").is_ok_and(|date| {
        (1..=9999).contains(&date.year()) && date.format("%Y-%m-%d").to_string() == day
    })
}

fn daily_paths(directory: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory).map_err(|error| io(directory, error))? {
        let entry = entry.map_err(|error| io(directory, error))?;
        if !entry.file_name().to_str().is_some_and(daily_name) {
            continue;
        }
        let path = entry.path();
        if !entry
            .file_type()
            .map_err(|error| io(&path, error))?
            .is_file()
        {
            return Err(refusal(
                "invalid_book",
                format!("{} is not a regular daily file", path.display()),
            ));
        }
        paths.push(path);
    }
    paths.sort();
    Ok(paths)
}

fn load_day(
    book: &mut Book,
    locations: &mut HashMap<i64, (PathBuf, usize)>,
    path: &Path,
    room: &str,
) -> Result<(), Error> {
    let file = File::open(path).map_err(|error| io(path, error))?;
    let mut reader = BufReader::new(file);
    let day = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| refusal("invalid_book", "daily file has an invalid name"))?;
    let mut bytes = Vec::new();
    let mut line = 0;
    loop {
        bytes.clear();
        let count = (&mut reader)
            .take((MAX_LINE_BYTES + 1) as u64)
            .read_until(b'\n', &mut bytes)
            .map_err(|error| io(path, error))?;
        if count == 0 {
            break;
        }
        line += 1;
        if bytes.len() > MAX_LINE_BYTES {
            return Err(malformed(path, line));
        }
        let record: Record = serde_json::from_slice(&bytes).map_err(|_| malformed(path, line))?;
        prepare::validate_stored(&record, room).map_err(|_| malformed(path, line))?;
        if !record.said_at.starts_with(day) {
            return Err(malformed(path, line));
        }
        let seq = record.seq;
        book.insert(record).map_err(|_| malformed(path, line))?;
        locations.insert(seq, (path.to_owned(), line));
    }
    Ok(())
}

fn load(directory: &Path, room: &str) -> Result<Book, Error> {
    let mut book = Book::default();
    let mut locations = HashMap::new();
    // ponytail: each operation scans the room book; add an index when measured capture latency exceeds the Host deadline.
    for path in daily_paths(directory)? {
        load_day(&mut book, &mut locations, &path, room)?;
    }
    for record in book.records.values() {
        if book.validate_link(record).is_err() {
            let (path, line) = &locations[&record.seq];
            return Err(malformed(path, *line));
        }
    }
    Ok(book)
}

fn record_path(directory: &Path, record: &Record) -> PathBuf {
    directory.join(format!("{}.jsonl", &record.said_at[..10]))
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> Result<(), Error> {
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|error| io(directory, error))
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> Result<(), Error> {
    // Windows flushes each accepted file through File::sync_all. Opening a
    // directory for FlushFileBuffers is not supported by the standard library.
    Ok(())
}

fn append_record(directory: &Path, record: &Record) -> Result<(), Error> {
    let path = record_path(directory, record);
    let mut bytes =
        serde_json::to_vec(record).map_err(|_| invalid("record cannot be serialized"))?;
    bytes.push(b'\n');
    if bytes.len() > MAX_LINE_BYTES {
        return Err(invalid("stored record exceeds the daily line bound"));
    }
    let mut file = OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(&path)
        .map_err(|error| io(&path, error))?;
    let length = file.metadata().map_err(|error| io(&path, error))?.len();
    if length != 0 {
        file.seek(SeekFrom::End(-1))
            .map_err(|error| io(&path, error))?;
        let mut last = [0];
        file.read_exact(&mut last)
            .map_err(|error| io(&path, error))?;
        if last[0] != b'\n' {
            file.write_all(b"\n").map_err(|error| io(&path, error))?;
            file.sync_all().map_err(|error| io(&path, error))?;
        }
    }
    file.write_all(&bytes).map_err(|error| io(&path, error))?;
    file.sync_all().map_err(|error| io(&path, error))?;
    if length == 0 {
        sync_directory(directory)?;
    }
    Ok(())
}

fn sync_accepted(directory: &Path, book: &Book, selected: &[i64]) -> Result<(), Error> {
    let mut paths: Vec<PathBuf> = selected
        .iter()
        .map(|seq| record_path(directory, &book.records[seq]))
        .collect();
    paths.sort();
    paths.dedup();
    for path in paths {
        // A retry can find complete bytes from a failed fsync. Acceptance still
        // needs a successful flush, even when no new row is necessary.
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .and_then(|file| file.sync_all())
            .map_err(|error| io(&path, error))?;
    }
    sync_directory(directory)
}

fn writable_directory(directory: &Path) -> Result<(), Error> {
    fs::create_dir_all(directory).map_err(|error| io(directory, error))
}

pub fn append_file(
    directory: &Path,
    room: &str,
    writer_session: &str,
    batch: CaptureBatch,
) -> Result<CaptureReceipt, Error> {
    let prepared = prepare::prepare(room, writer_session, batch)?;
    writable_directory(directory)?;
    let _lock = lock(directory, true)?;
    let mut book = load(directory, room)?;
    let (selected, appended) = book.plan(room, writer_session, prepared)?;
    for seq in &appended {
        append_record(directory, &book.records[seq])?;
    }
    sync_accepted(directory, &book, &selected)?;
    book.receipt(selected, appended.len())
}

fn read_book(directory: &Path, room: &str, bounds: ReadBounds) -> Result<ReadReceipt, Error> {
    match fs::metadata(directory) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(bounds.receipt(Vec::new()));
        }
        Err(error) => return Err(io(directory, error)),
    }
    let _lock = lock(directory, false)?;
    let book = load(directory, room)?;
    let rows = book
        .records
        .into_values()
        .filter(|record| bounds.includes(record))
        .take(bounds.request.limit + 1)
        .collect();
    Ok(bounds.receipt(rows))
}

pub fn read_file(directory: &Path, room: &str, request: ReadRequest) -> Result<ReadReceipt, Error> {
    read_book(directory, room, read_bounds(room, request)?)
}

pub fn export_file(directory: &Path, room: &str, records: &[Record]) -> Result<(), Error> {
    prepare::identity(room, "room")?;
    if records.len() > MAX_BATCH_RECORDS {
        return Err(invalid("an export batch cannot exceed 64 records"));
    }
    for record in records {
        prepare::validate_stored(record, room)?;
        if prepare::json_size(record)? + 1 > MAX_LINE_BYTES {
            return Err(invalid("exported record exceeds the daily line bound"));
        }
    }
    if records.is_empty() {
        return Ok(());
    }
    writable_directory(directory)?;
    let _lock = lock(directory, true)?;
    let mut book = load(directory, room)?;
    let mut appended = Vec::new();
    let mut selected = Vec::with_capacity(records.len());
    for record in records {
        if let Some(stored) =
            book.source(&record.harness, &record.source_session, &record.source_id)
        {
            if stored != record {
                return Err(refusal(
                    "conflicting_export",
                    "the exported record differs from the accepted database record",
                ));
            }
        } else {
            book.validate_link(record)?;
            book.insert(record.clone())?;
            appended.push(record.seq);
        }
        selected.push(record.seq);
    }
    for seq in appended {
        append_record(directory, &book.records[&seq])?;
    }
    sync_accepted(directory, &book, &selected)
}
