use crate::config::ROOM_MARKER;
use crate::error::VaultError;
use crate::model::{VaultRecallRequest, VaultRecallResult};
use crate::rank::{EXCERPT_CHARS, MAX_RESULTS};
use crate::recall::recall;
use crate::walk::normalized_path;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    room: PathBuf,
    alpha: PathBuf,
    beta: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn fixture() -> Fixture {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "athanor-vault-{}-{stamp}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let room = root.join("work-room");
    let alpha = root.join("alpha-project");
    let beta = root.join("beta-project");
    fs::create_dir_all(&room).unwrap();
    fs::create_dir_all(&alpha).unwrap();
    fs::create_dir_all(&beta).unwrap();
    fs::write(room.join(ROOM_MARKER), r#"{"version":1,"room":"work-room","vaultRoots":["../alpha-project","../beta-project"],"vaultIgnore":["private/**"]}"#).unwrap();
    fs::write(alpha.join(".gitignore"), "ignored.md\n").unwrap();
    fs::write(alpha.join("README.md"), "---\ntags: [furnace, retrieval]\n---\n# Architecture\nThe exact bridge identifier is HINGE-PROTOCOL-77.\n\n## Failure behavior\nLexical recall remains available when embeddings disappear.\n").unwrap();
    fs::write(
        alpha.join("ignored.md"),
        "HINGE-PROTOCOL-77 must never surface",
    )
    .unwrap();
    fs::write(alpha.join(".env"), "HINGE-PROTOCOL-77=secret").unwrap();
    fs::create_dir(alpha.join("node_modules")).unwrap();
    fs::write(
        alpha.join("node_modules/noise.json"),
        r#"{"hinge":"HINGE-PROTOCOL-77"}"#,
    )
    .unwrap();
    fs::create_dir(alpha.join("private")).unwrap();
    fs::write(alpha.join("private/notes.md"), "HINGE-PROTOCOL-77 hidden").unwrap();
    fs::write(beta.join("projects.json"), r#"{"atlas":{"owner":"Dino","sharedLibrary":"cross-project-capsule"},"leo":{"owner":"Leo","status":"evaluating"}}"#).unwrap();
    fs::write(beta.join("events.jsonl"), "{\"type\":\"decision\",\"project\":\"atlas\",\"value\":\"cold models need attributed evidence\"}\n{ malformed\n{\"type\":\"receipt\",\"project\":\"atlas\",\"value\":\"vault-search-live\"}").unwrap();
    Fixture {
        root,
        room,
        alpha,
        beta,
    }
}
fn search(fixture: &Fixture, query: &str) -> VaultRecallResult {
    recall(VaultRecallRequest {
        room_dir: fixture.room.clone(),
        room: "work-room".into(),
        query: query.into(),
    })
    .unwrap()
}
#[test]
fn exact_markdown_recall_is_attributed_and_ignored_paths_stay_absent() {
    let fixture = fixture();
    let result = search(&fixture, "HINGE-PROTOCOL-77");
    assert!(result.found);
    let first = &result.retrieval_candidates[0];
    assert_eq!(
        first.source_path,
        normalized_path(&fixture.alpha.join("README.md"))
    );
    assert_eq!(first.heading_path, "Architecture");
    assert!(first.matched_terms.contains(&"hinge-protocol-77".into()));
    assert!(
        first
            .reasons
            .iter()
            .any(|reason| reason.contains("exact content fields: body"))
    );
    assert!(result.retrieval_candidates.iter().all(|candidate| {
        !candidate.source_path.contains("ignored.md")
            && !candidate.source_path.contains("node_modules")
            && !candidate.source_path.contains("private/")
    }));
}
#[test]
fn structured_records_and_malformed_line_receipts_match_the_file_contract() {
    let fixture = fixture();
    let json = search(&fixture, "cross-project-capsule Dino");
    assert_eq!(
        json.retrieval_candidates[0].source_path,
        normalized_path(&fixture.beta.join("projects.json"))
    );
    assert_eq!(json.retrieval_candidates[0].heading_path, "/atlas");
    let jsonl = search(&fixture, "vault-search-live");
    assert!(
        jsonl.retrieval_candidates[0]
            .heading_path
            .contains("line:3")
    );
    assert!(
        jsonl
            .warnings
            .iter()
            .any(|warning| warning.contains("skipped 1 malformed JSONL record"))
    );
}
#[test]
fn multi_term_paraphrase_and_source_ties_are_deterministic() {
    let fixture = fixture();
    let paraphrase = search(&fixture, "embeddings disappear lexical retrieval");
    assert_eq!(
        paraphrase.retrieval_candidates[0].source_path,
        normalized_path(&fixture.alpha.join("README.md"))
    );
    fs::write(fixture.beta.join("a.md"), "# Receipt\nTIE-MARKER-88").unwrap();
    fs::write(fixture.beta.join("b.md"), "# Receipt\nTIE-MARKER-88").unwrap();
    let ties = search(&fixture, "TIE-MARKER-88");
    let paths = ties
        .retrieval_candidates
        .iter()
        .map(|candidate| candidate.source_path.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        vec![
            normalized_path(&fixture.beta.join("a.md")),
            normalized_path(&fixture.beta.join("b.md"))
        ]
    );
}
#[test]
fn rejects_room_mismatch_and_refuses_symlink_escape() {
    let fixture = fixture();
    let mismatch = recall(VaultRecallRequest {
        room_dir: fixture.room.clone(),
        room: "another-room".into(),
        query: "hinge".into(),
    })
    .unwrap_err();
    assert!(matches!(mismatch, VaultError::RoomMismatch { .. }));
    let outside = fixture.root.join("outside.md");
    fs::write(&outside, "ESCAPE-MARKER-42").unwrap();
    let link = fixture.alpha.join("escape.md");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&outside, &link).unwrap();
    let result = search(&fixture, "ESCAPE-MARKER-42");
    assert!(!result.found);
    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.contains("refused symbolic link"))
    );
}
#[test]
fn chunks_results_and_formats_are_bounded() {
    let fixture = fixture();
    let baseline_documents = search(&fixture, "absent-baseline").indexed_documents;
    let mut body = "padding ".repeat(2_000);
    body.push_str(" BOUNDED-MARKER-99");
    fs::write(fixture.beta.join("long.txt"), body).unwrap();
    fs::write(fixture.beta.join("not-authority.csv"), "BOUNDED-MARKER-99").unwrap();
    for index in 0..10 {
        fs::write(
            fixture.beta.join(format!("bounded-{index:02}.md")),
            format!("# Bounded\nBOUNDED-MARKER-99 receipt {index}"),
        )
        .unwrap();
    }
    let result = search(&fixture, "BOUNDED-MARKER-99");
    assert_eq!(result.indexed_documents, baseline_documents + 13);
    assert_eq!(result.retrieval_candidates.len(), MAX_RESULTS);
    assert!(
        result
            .retrieval_candidates
            .iter()
            .all(|candidate| candidate.excerpt.chars().count() <= EXCERPT_CHARS + 2)
    );
    assert!(
        result
            .retrieval_candidates
            .iter()
            .all(|candidate| !candidate.source_path.ends_with(".csv"))
    );
}
