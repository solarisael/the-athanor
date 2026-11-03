use super::error::{Error, invalid, refusal};
use super::redaction;
use crate::sea::idempotency_digest;
use chrono::{DateTime, Datelike, SecondsFormat, Utc};
use hearth::orihon::{
    CaptureBatch, CaptureRecord, ChildRef, ECHO_CHARACTERS, Kind, MAX_BATCH_RECORDS, MAX_CHILDREN,
    Record,
};
use serde::Serialize;
use std::io::{self, Write};

pub(super) const MAX_TEXT_BYTES: usize = 1024 * 1024;
pub(super) const MAX_BATCH_BYTES: usize = 4 * 1024 * 1024;
const ELISION: &str = "\n[... Orihon echo elided ...]\n";

#[derive(Serialize)]
pub(super) struct PreparedBatch {
    pub harness: String,
    pub source_session: String,
    pub records: Vec<PreparedRecord>,
}

#[derive(Serialize)]
pub(super) struct PreparedRecord {
    pub capture: CaptureRecord,
    pub truncated: bool,
    pub content_digest: String,
    /// The record as it was accepted before captures carried semantic fields. Present only
    /// when this capture carries `foldText` or `children`.
    #[serde(skip)]
    pub original: Option<Original>,
}

/// Facts of a capture without its semantic fields: its redaction flag from the audit text
/// alone and the digest an earlier acceptance of the same source computed.
pub(super) struct Original {
    redacted: bool,
    content_digest: String,
}

pub(super) fn identity(value: &str, field: &str) -> Result<(), Error> {
    if value.trim().is_empty() || value.contains('\0') {
        return Err(invalid(format!(
            "{field} must be nonblank and contain no NUL"
        )));
    }
    Ok(())
}

pub(super) fn timestamp(value: &str) -> Result<String, Error> {
    let instant = DateTime::parse_from_rfc3339(value)
        .map_err(|_| invalid("saidAt and writtenAt must be RFC3339 timestamps"))?
        .with_timezone(&Utc);
    if !(1..=9999).contains(&instant.year()) {
        return Err(invalid("timestamp UTC year must be between 0001 and 9999"));
    }
    Ok(instant.to_rfc3339_opts(SecondsFormat::AutoSi, true))
}

pub(super) fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::User => "user",
        Kind::Talk => "talk",
        Kind::Tool => "tool",
        Kind::Echo => "echo",
        Kind::Work => "work",
        Kind::Note => "note",
    }
}

pub(super) fn parse_kind(value: &str) -> Result<Kind, Error> {
    match value {
        "user" => Ok(Kind::User),
        "talk" => Ok(Kind::Talk),
        "tool" => Ok(Kind::Tool),
        "echo" => Ok(Kind::Echo),
        "work" => Ok(Kind::Work),
        "note" => Ok(Kind::Note),
        _ => Err(refusal("invalid_book", "stored record has an unknown kind")),
    }
}

fn source_shape(
    source_id: &str,
    parent: Option<&str>,
    kind: Kind,
    speaker: &str,
) -> Result<(), Error> {
    identity(source_id, "sourceId")?;
    identity(speaker, "speaker")?;
    match (kind, parent) {
        (Kind::User, None) => Ok(()),
        (Kind::User, Some(_)) => Err(invalid("a user record cannot name parentUserId")),
        (_, Some(parent)) => identity(parent, "parentUserId"),
        // A door-opened session prelude; each store admits it only before the session's first user page.
        (_, None) => Ok(()),
    }
}

fn semantic_shape(
    kind: Kind,
    fold_text: Option<&str>,
    children: Option<&[ChildRef]>,
) -> Result<(), Error> {
    if let Some(text) = fold_text {
        if kind != Kind::Tool || text.is_empty() || text.contains('\0') {
            return Err(invalid(
                "foldText belongs only to tool records and must be nonempty without NUL",
            ));
        }
        if text.len() > MAX_TEXT_BYTES {
            return Err(invalid("foldText exceeds 1 MiB"));
        }
    }
    if let Some(children) = children {
        if kind != Kind::Work || children.is_empty() || children.len() > MAX_CHILDREN {
            return Err(invalid(
                "children belong only to work records and hold 1 through 16 identities",
            ));
        }
        for child in children {
            identity(&child.name, "child name")?;
            identity(&child.session, "child session")?;
        }
    }
    Ok(())
}

fn truncate_echo(text: String) -> (String, bool) {
    let count = text.chars().count();
    if count <= ECHO_CHARACTERS {
        return (text, false);
    }
    let retained = ECHO_CHARACTERS - ELISION.chars().count();
    let head = retained / 2;
    let tail = retained - head;
    let head_end = text
        .char_indices()
        .nth(head)
        .map_or(text.len(), |(offset, _)| offset);
    let tail_start = text
        .char_indices()
        .nth(count - tail)
        .map_or(text.len(), |(offset, _)| offset);
    let mut shortened = String::with_capacity(head_end + ELISION.len() + text.len() - tail_start);
    shortened.push_str(&text[..head_end]);
    shortened.push_str(ELISION);
    shortened.push_str(&text[tail_start..]);
    (shortened, true)
}

fn source_speaker(kind: Kind, speaker: &str) -> &str {
    // Host attribution is fixed on first acceptance, not recomputed on replay.
    match kind {
        Kind::User | Kind::Talk | Kind::Note => "",
        _ => speaker,
    }
}

#[allow(clippy::too_many_arguments)]
fn digest(
    kind: Kind,
    parent: Option<&str>,
    speaker: &str,
    text: &str,
    said_at: &str,
    observed: bool,
    redacted: bool,
    truncated: bool,
    fold_text: Option<&str>,
    children: Option<&[ChildRef]>,
) -> String {
    let children = children
        .map(|children| serde_json::to_string(children).expect("child identities serialize"));
    let mut parts = vec![
        kind_name(kind),
        parent.unwrap_or(""),
        source_speaker(kind, speaker),
        text,
        said_at,
        if observed { "observed" } else { "source" },
        if redacted { "redacted" } else { "unredacted" },
        if truncated { "truncated" } else { "complete" },
    ];
    // Records without semantic fields keep the digest they had before these fields existed.
    if let Some(fold_text) = fold_text {
        parts.extend(["foldText", fold_text]);
    }
    if let Some(children) = children.as_deref() {
        parts.extend(["children", children]);
    }
    idempotency_digest(&parts)
}

fn prepare_record(mut capture: CaptureRecord) -> Result<PreparedRecord, Error> {
    source_shape(
        &capture.source_id,
        capture.parent_user_id.as_deref(),
        capture.kind,
        &capture.speaker,
    )?;
    semantic_shape(
        capture.kind,
        capture.fold_text.as_deref(),
        capture.children.as_deref(),
    )?;
    capture.said_at = timestamp(&capture.said_at)?;
    if capture.text.contains('\0') {
        return Err(invalid("text cannot contain NUL"));
    }
    if capture.kind != Kind::Echo && capture.text.len() > MAX_TEXT_BYTES {
        return Err(invalid("non-echo text exceeds 1 MiB"));
    }
    let (text, redacted) = redaction::redact(capture.text);
    capture.redacted |= redacted;
    let (text, truncated) = if capture.kind == Kind::Echo {
        truncate_echo(text)
    } else {
        (text, false)
    };
    capture.text = text;
    if capture.kind != Kind::Echo && capture.text.len() > MAX_TEXT_BYTES {
        return Err(invalid("redacted non-echo text exceeds 1 MiB"));
    }
    let audit_redacted = capture.redacted;
    if let Some(fold_text) = capture.fold_text.take() {
        let (fold_text, redacted) = redaction::redact(fold_text);
        capture.redacted |= redacted;
        if fold_text.len() > MAX_TEXT_BYTES {
            return Err(invalid("redacted foldText exceeds 1 MiB"));
        }
        capture.fold_text = Some(fold_text);
    }
    let content_digest = digest(
        capture.kind,
        capture.parent_user_id.as_deref(),
        &capture.speaker,
        &capture.text,
        &capture.said_at,
        capture.timestamp_observed,
        capture.redacted,
        truncated,
        capture.fold_text.as_deref(),
        capture.children.as_deref(),
    );
    let original = (capture.fold_text.is_some() || capture.children.is_some()).then(|| Original {
        redacted: audit_redacted,
        content_digest: digest(
            capture.kind,
            capture.parent_user_id.as_deref(),
            &capture.speaker,
            &capture.text,
            &capture.said_at,
            capture.timestamp_observed,
            audit_redacted,
            truncated,
            None,
            None,
        ),
    });
    Ok(PreparedRecord {
        capture,
        truncated,
        content_digest,
        original,
    })
}

pub(super) fn prepare(
    room: &str,
    writer_session: &str,
    batch: CaptureBatch,
) -> Result<PreparedBatch, Error> {
    identity(room, "room")?;
    identity(writer_session, "writer session")?;
    identity(&batch.harness, "harness")?;
    identity(&batch.source_session, "sourceSession")?;
    if batch.records.is_empty() || batch.records.len() > MAX_BATCH_RECORDS {
        return Err(invalid("a capture batch must contain 1 through 64 records"));
    }
    let records = batch
        .records
        .into_iter()
        .map(prepare_record)
        .collect::<Result<_, _>>()?;
    let prepared = PreparedBatch {
        harness: batch.harness,
        source_session: batch.source_session,
        records,
    };
    if json_size(&(room, writer_session, &prepared))? > MAX_BATCH_BYTES {
        return Err(refusal(
            "prepared_batch_too_large",
            "prepared capture batch exceeds 4 MiB",
        ));
    }
    Ok(prepared)
}

/// How a capture relates to the record already stored under its source identity.
pub(super) enum Replay {
    /// The stored record, exactly.
    Same,
    /// The stored record was accepted before captures carried semantic fields, and this
    /// capture differs from it only by adding the named ones. The stored record stands
    /// unchanged; the added fields are not stored.
    Original(Vec<&'static str>),
    /// Different content reusing the identity.
    Different,
}

impl PreparedRecord {
    pub fn replay(&self, stored: &Record) -> Replay {
        if self.facts_match(stored, self.capture.redacted, &self.content_digest)
            && self.capture.fold_text == stored.fold_text
            && self.capture.children == stored.children
        {
            return Replay::Same;
        }
        match &self.original {
            Some(original)
                if stored.fold_text.is_none()
                    && stored.children.is_none()
                    && self.facts_match(stored, original.redacted, &original.content_digest) =>
            {
                let mut added = Vec::with_capacity(2);
                if self.capture.fold_text.is_some() {
                    added.push("foldText");
                }
                if self.capture.children.is_some() {
                    added.push("children");
                }
                Replay::Original(added)
            }
            _ => Replay::Different,
        }
    }

    fn facts_match(&self, stored: &Record, redacted: bool, content_digest: &str) -> bool {
        let source = &self.capture;
        let source_facts = (
            source.source_id.as_str(),
            source.parent_user_id.as_deref(),
            source.kind,
            source_speaker(source.kind, &source.speaker),
            source.text.as_str(),
            source.said_at.as_str(),
            source.timestamp_observed,
            redacted,
            self.truncated,
            content_digest,
        );
        let stored_facts = (
            stored.source_id.as_str(),
            stored.parent_user_id.as_deref(),
            stored.kind,
            source_speaker(stored.kind, &stored.speaker),
            stored.text.as_str(),
            stored.said_at.as_str(),
            stored.timestamp_observed,
            stored.redacted,
            stored.truncated,
            stored.content_digest.as_str(),
        );
        source_facts == stored_facts
    }

    pub fn into_record(
        self,
        room: &str,
        session_id: &str,
        harness: &str,
        source_session: &str,
        seq: i64,
        turn_no: i64,
        written_at: String,
    ) -> Record {
        Record {
            room: room.into(),
            seq,
            turn_no,
            session_id: session_id.into(),
            harness: harness.into(),
            source_session: source_session.into(),
            source_id: self.capture.source_id,
            parent_user_id: self.capture.parent_user_id,
            kind: self.capture.kind,
            speaker: self.capture.speaker,
            text: self.capture.text,
            said_at: self.capture.said_at,
            timestamp_observed: self.capture.timestamp_observed,
            written_at,
            truncated: self.truncated,
            redacted: self.capture.redacted,
            content_digest: self.content_digest,
            fold_text: self.capture.fold_text,
            children: self.capture.children,
        }
    }
}

pub(super) fn validate_stored(record: &Record, room: &str) -> Result<(), Error> {
    identity(&record.room, "room")?;
    identity(&record.session_id, "writer session")?;
    identity(&record.harness, "harness")?;
    identity(&record.source_session, "sourceSession")?;
    if record.room != room || record.seq < 0 || record.turn_no < 0 {
        return Err(invalid(
            "stored record has a foreign room or negative ordinal",
        ));
    }
    source_shape(
        &record.source_id,
        record.parent_user_id.as_deref(),
        record.kind,
        &record.speaker,
    )?;
    semantic_shape(
        record.kind,
        record.fold_text.as_deref(),
        record.children.as_deref(),
    )?;
    if timestamp(&record.said_at)? != record.said_at
        || timestamp(&record.written_at)? != record.written_at
    {
        return Err(invalid("stored timestamps are not normalized RFC3339"));
    }
    if record.text.contains('\0') || record.truncated && record.kind != Kind::Echo {
        return Err(invalid("stored text or truncation flag is invalid"));
    }
    let oversized = match record.kind {
        Kind::Echo => record.text.chars().count() > ECHO_CHARACTERS,
        _ => record.text.len() > MAX_TEXT_BYTES,
    };
    let content_digest = digest(
        record.kind,
        record.parent_user_id.as_deref(),
        &record.speaker,
        &record.text,
        &record.said_at,
        record.timestamp_observed,
        record.redacted,
        record.truncated,
        record.fold_text.as_deref(),
        record.children.as_deref(),
    );
    if oversized || content_digest != record.content_digest {
        return Err(invalid(
            "stored content does not match its bounds or digest",
        ));
    }
    Ok(())
}

struct ByteCount(usize);

impl Write for ByteCount {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .ok_or_else(|| io::Error::other("JSON size overflow"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn json_size(value: &impl Serialize) -> Result<usize, Error> {
    let mut count = ByteCount(0);
    serde_json::to_writer(&mut count, value).map_err(|_| invalid("record cannot be serialized"))?;
    Ok(count.0)
}
