use hearth::orihon::{CaptureBatch, CaptureRecord, Kind, ReadRequest, Record};
use origami::orihon::{Error, append_file, export_file, read_file};
use std::fs;
use std::path::{Path, PathBuf};

struct BookDirectory(PathBuf);

impl BookDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("orihon-export-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for BookDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove owned export fixture");
    }
}

fn captured(id: &str, kind: Kind, parent: Option<&str>) -> CaptureRecord {
    CaptureRecord {
        source_id: id.into(),
        parent_user_id: parent.map(str::to_owned),
        kind,
        speaker: "Operator".into(),
        text: format!("synthetic {id}"),
        said_at: "2026-10-24T00:00:00Z".into(),
        timestamp_observed: false,
        redacted: false,
        fold_text: None,
        children: None,
    }
}

fn records(directory: &Path) -> Vec<Record> {
    read_file(
        directory,
        "room",
        ReadRequest {
            after_seq: None,
            until_seq: None,
            turn_no: None,
            day: None,
            limit: 64,
        },
    )
    .unwrap()
    .rows
}

#[test]
fn accepted_prelude_exports_then_replays_after_the_first_user_page() {
    let source = BookDirectory::new();
    let mirror = BookDirectory::new();
    append_file(
        &source.0,
        "room",
        "writer",
        CaptureBatch {
            harness: "omp".into(),
            source_session: "session".into(),
            records: vec![
                captured("shell:tool", Kind::Tool, None),
                captured("shell:echo", Kind::Echo, None),
                captured("first:user", Kind::User, None),
                captured("reply:talk", Kind::Talk, Some("first:user")),
            ],
        },
    )
    .unwrap();
    let accepted = records(&source.0);

    export_file(&mirror.0, "room", &accepted[..2]).expect("export pre-user shell records");
    export_file(&mirror.0, "room", &accepted[2..]).expect("export user page and reply");
    export_file(&mirror.0, "room", &accepted).expect("replay after the user page exists");
    assert_eq!(records(&mirror.0), accepted);

    let mut late = accepted[0].clone();
    late.seq = accepted.last().unwrap().seq + 1;
    late.source_id = "late:tool".into();
    assert!(matches!(export_file(&mirror.0, "room", &[late]),
        Err(Error::Refusal { code, .. }) if code == "missing_parent"));
    assert_eq!(
        records(&mirror.0),
        accepted,
        "a refused export must preserve the file book"
    );
}
