use hearth::orihon::{CaptureBatch, CaptureRecord, Kind, ReadRequest, Record};
use origami::orihon::{
    self,
    folds::{self, Fold, SourceRef},
};
use std::{fs, io::Write, path::PathBuf};

struct Book(PathBuf);

impl Book {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("orihon-fold-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn append(&self, id: &str, parent: Option<&str>, text: &str) {
        orihon::append_file(
            &self.0,
            "proof",
            "writer",
            CaptureBatch {
                harness: "test".into(),
                source_session: "session".into(),
                records: vec![CaptureRecord {
                    source_id: id.into(),
                    parent_user_id: parent.map(str::to_owned),
                    kind: if parent.is_none() {
                        Kind::User
                    } else {
                        Kind::Talk
                    },
                    speaker: if parent.is_none() {
                        "Operator"
                    } else {
                        "Spirit"
                    }
                    .into(),
                    text: text.into(),
                    said_at: "2026-10-22T00:00:00Z".into(),
                    timestamp_observed: false,
                    redacted: false,
                }],
            },
        )
        .unwrap();
    }

    fn page(&self) -> Vec<Record> {
        orihon::read_file(
            &self.0,
            "proof",
            ReadRequest {
                after_seq: None,
                until_seq: None,
                turn_no: Some(0),
                day: None,
                limit: 200,
            },
        )
        .unwrap()
        .rows
    }
}

impl Drop for Book {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove owned test book");
    }
}

fn page_fold(records: &[Record]) -> Fold {
    Fold {
        room: "proof".into(),
        level: 1,
        index: 0,
        source_digest: folds::source_digest(records),
        recipe_digest: "a".repeat(64),
        sources: records
            .iter()
            .map(|row| SourceRef {
                seq: row.seq,
                content_digest: row.content_digest.clone(),
            })
            .collect(),
        text: "Operator keeps the migration paused; Spirit agrees.".into(),
        model: Some("test-model".into()),
        tries: 1,
        built_at: "2026-10-22T00:00:00Z".into(),
    }
}

#[test]
fn late_replay_creates_a_new_snapshot_without_overwriting_the_old_fold() {
    let book = Book::new();
    book.append("first", None, "Keep the migration paused.");
    book.append("answer", Some("first"), "Agreed.");
    let old = folds::append_file(&book.0, &page_fold(&book.page())).unwrap();
    let mut retry = old.clone();
    retry.built_at = "2026-10-23T00:00:00Z".into();
    assert_eq!(folds::append_file(&book.0, &retry).unwrap(), old);
    retry.text = "Migration completed.".into();
    assert!(folds::append_file(&book.0, &retry).is_err());

    book.append("next", None, "Another topic.");
    book.append("late", Some("first"), "The migration is still paused.");
    let records = book.page();
    assert_eq!(
        records.iter().map(|row| row.seq).collect::<Vec<_>>(),
        [0, 1, 3]
    );
    let mut incomplete = records.clone();
    incomplete.remove(1);
    assert!(folds::append_file(&book.0, &page_fold(&incomplete)).is_err());
    let new = folds::append_file(&book.0, &page_fold(&records)).unwrap();
    assert_ne!(old.source_digest, new.source_digest);
    assert_eq!(
        folds::read_file(
            &book.0,
            "proof",
            1,
            0,
            &old.source_digest,
            &old.recipe_digest
        )
        .unwrap(),
        Some(old)
    );
    assert_eq!(
        folds::read_file(
            &book.0,
            "proof",
            1,
            0,
            &new.source_digest,
            &new.recipe_digest
        )
        .unwrap(),
        Some(new)
    );
}

#[test]
fn malformed_fold_tail_refuses_and_preserves_every_byte() {
    let book = Book::new();
    book.append("first", None, "Keep this source.");
    let fold = folds::append_file(&book.0, &page_fold(&book.page())).unwrap();
    let path = book.0.join("folds.jsonl");
    fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"{\"torn\":")
        .unwrap();
    let before = fs::read(&path).unwrap();
    assert!(folds::append_file(&book.0, &fold).is_err());
    assert!(
        folds::read_file(
            &book.0,
            "proof",
            1,
            0,
            &fold.source_digest,
            &fold.recipe_digest
        )
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), before);
}
