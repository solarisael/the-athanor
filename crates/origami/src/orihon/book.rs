use super::error::{Error, refusal};
use super::prepare::{PreparedBatch, Replay};
use chrono::{SecondsFormat, Utc};
use hearth::orihon::{CaptureReceipt, Kind, Reconciled, Record};
use std::collections::{BTreeMap, HashMap};

#[derive(Default)]
pub(super) struct Book {
    pub records: BTreeMap<i64, Record>,
    sources: HashMap<String, HashMap<String, HashMap<String, i64>>>,
    user_turns: HashMap<i64, i64>,
    /// First user seq per (harness, source session).
    session_users: HashMap<(String, String), i64>,
    /// The single page-less prelude page of a door-opened session.
    prelude_pages: HashMap<(String, String), i64>,
    next_seq: i64,
    next_turn: i64,
}

pub(super) fn advance(value: i64) -> Result<i64, Error> {
    value
        .checked_add(1)
        .ok_or_else(|| refusal("book_full", "the room ordinal is exhausted"))
}

impl Book {
    pub fn source(&self, harness: &str, session: &str, id: &str) -> Option<&Record> {
        let seq = self.sources.get(harness)?.get(session)?.get(id)?;
        self.records.get(seq)
    }

    pub fn insert(&mut self, record: Record) -> Result<(), Error> {
        if self.records.contains_key(&record.seq)
            || self
                .source(&record.harness, &record.source_session, &record.source_id)
                .is_some()
        {
            return Err(refusal(
                "conflicting_record",
                "the book repeats a sequence or source identity",
            ));
        }
        let session = (record.harness.clone(), record.source_session.clone());
        if record.kind == Kind::User {
            if self.user_turns.contains_key(&record.turn_no)
                || self
                    .prelude_pages
                    .values()
                    .any(|page| *page == record.turn_no)
            {
                return Err(refusal(
                    "conflicting_page",
                    "two user records name the same room page",
                ));
            }
            self.next_turn = self.next_turn.max(advance(record.turn_no)?);
            self.user_turns.insert(record.turn_no, record.seq);
            let first = self.session_users.entry(session).or_insert(record.seq);
            *first = (*first).min(record.seq);
        } else if record.parent_user_id.is_none() {
            let page = *self.prelude_pages.entry(session).or_insert(record.turn_no);
            if page != record.turn_no || self.user_turns.contains_key(&record.turn_no) {
                return Err(refusal(
                    "conflicting_page",
                    "a session prelude must keep one page without a user record",
                ));
            }
            self.next_turn = self.next_turn.max(advance(record.turn_no)?);
        }
        self.next_seq = self.next_seq.max(advance(record.seq)?);
        self.sources
            .entry(record.harness.clone())
            .or_default()
            .entry(record.source_session.clone())
            .or_default()
            .insert(record.source_id.clone(), record.seq);
        self.records.insert(record.seq, record);
        Ok(())
    }

    pub fn validate_link(&self, record: &Record) -> Result<(), Error> {
        if record.kind == Kind::User {
            return Ok(());
        }
        let Some(parent_id) = record.parent_user_id.as_deref() else {
            let session = (record.harness.clone(), record.source_session.clone());
            let before_users = self
                .session_users
                .get(&session)
                .is_none_or(|first| *first > record.seq);
            if !before_users || self.prelude_pages.get(&session) != Some(&record.turn_no) {
                return Err(refusal(
                    "missing_parent",
                    "a non-user record must name parentUserId after its session's first user page",
                ));
            }
            return Ok(());
        };
        let parent = self
            .source(&record.harness, &record.source_session, parent_id)
            .ok_or_else(|| {
                refusal(
                    "missing_parent",
                    "parentUserId must identify a stored or earlier user record",
                )
            })?;
        if parent.kind != Kind::User || parent.seq >= record.seq || parent.turn_no != record.turn_no
        {
            return Err(refusal(
                "invalid_parent",
                "the parent must be an earlier user on the same page",
            ));
        }
        Ok(())
    }

    fn complete(&self) -> Result<(), Error> {
        let dense_sequences = self.records.keys().copied().eq(0..self.next_seq);
        let dense_pages =
            (self.user_turns.len() + self.prelude_pages.len()) as i64 == self.next_turn;
        if !dense_sequences || !dense_pages {
            return Err(refusal(
                "incomplete_book",
                "repair missing exported records before using the book as authority",
            ));
        }
        Ok(())
    }

    /// Selected seqs, appended seqs, and replays reconciled to their stored originals.
    pub fn plan(
        &mut self,
        room: &str,
        writer_session: &str,
        batch: PreparedBatch,
    ) -> Result<(Vec<i64>, Vec<i64>, Vec<Reconciled>), Error> {
        self.complete()?;
        let mut selected = Vec::with_capacity(batch.records.len());
        let mut appended = Vec::new();
        let mut reconciled = Vec::new();
        for prepared in batch.records {
            if let Some(stored) = self.source(
                &batch.harness,
                &batch.source_session,
                &prepared.capture.source_id,
            ) {
                match prepared.replay(stored) {
                    Replay::Same => {}
                    Replay::Original(added) => reconciled.push(Reconciled {
                        source_id: stored.source_id.clone(),
                        seq: stored.seq,
                        content_digest: stored.content_digest.clone(),
                        added,
                    }),
                    Replay::Different => {
                        return Err(refusal(
                            "idempotency_reuse",
                            "source identity was reused with different normalized content",
                        ));
                    }
                }
                selected.push(stored.seq);
                continue;
            }
            let session = (batch.harness.clone(), batch.source_session.clone());
            let turn_no = match (
                prepared.capture.kind,
                prepared.capture.parent_user_id.as_deref(),
            ) {
                (Kind::User, _) => self.next_turn,
                (_, None) => {
                    if self.session_users.contains_key(&session) {
                        return Err(refusal(
                            "missing_parent",
                            "a non-user record must name parentUserId after its session's first user page",
                        ));
                    }
                    self.prelude_pages
                        .get(&session)
                        .copied()
                        .unwrap_or(self.next_turn)
                }
                (_, Some(id)) => {
                    let parent = self
                        .source(&batch.harness, &batch.source_session, id)
                        .ok_or_else(|| {
                            refusal(
                                "missing_parent",
                                "parentUserId must identify a stored or earlier user record",
                            )
                        })?;
                    if parent.kind != Kind::User {
                        return Err(refusal(
                            "invalid_parent",
                            "parentUserId identifies a non-user record",
                        ));
                    }
                    parent.turn_no
                }
            };
            let seq = self.next_seq;
            let record = prepared.into_record(
                room,
                writer_session,
                &batch.harness,
                &batch.source_session,
                seq,
                turn_no,
                Utc::now().to_rfc3339_opts(SecondsFormat::AutoSi, true),
            );
            self.insert(record)?;
            selected.push(seq);
            appended.push(seq);
        }
        Ok((selected, appended, reconciled))
    }

    pub fn receipt(
        mut self,
        selected: Vec<i64>,
        appended: usize,
        reconciled: Vec<Reconciled>,
    ) -> Result<CaptureReceipt, Error> {
        let replayed = selected.len() - appended;
        let mut accepted_source_ids = Vec::with_capacity(selected.len());
        let mut accepted_records: Vec<Record> = Vec::with_capacity(selected.len());
        let mut positions: HashMap<i64, usize> = HashMap::new();
        for seq in selected {
            let record = if let Some(&position) = positions.get(&seq) {
                accepted_records[position].clone()
            } else {
                self.records
                    .remove(&seq)
                    .ok_or_else(|| refusal("invalid_book", "an accepted record is absent"))?
            };
            accepted_source_ids.push(record.source_id.clone());
            positions.insert(seq, accepted_records.len());
            accepted_records.push(record);
        }
        Ok(CaptureReceipt {
            accepted_source_ids,
            appended,
            replayed,
            reconciled,
            accepted_records,
        })
    }
}
