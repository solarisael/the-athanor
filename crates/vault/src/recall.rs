use crate::config::load_config;
use crate::error::VaultError;
use crate::index::build_index;
use crate::model::{VaultRecallRequest, VaultRecallResult, VaultTaxonomy};
use crate::rank::rank;
use crate::walk::normalize_lexical;

pub fn recall(request: VaultRecallRequest) -> Result<VaultRecallResult, VaultError> {
    if request.query.trim().is_empty() {
        return Err(VaultError::EmptyQuery);
    }
    if !request.room_dir.is_absolute() {
        return Err(VaultError::InvalidRoomDirectory(
            "Vault room directory must be absolute".into(),
        ));
    }
    let room_dir = normalize_lexical(&request.room_dir);
    let actual_room = room_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if actual_room != request.room {
        return Err(VaultError::RoomMismatch {
            requested: request.room,
            actual: actual_room.to_owned(),
        });
    }
    let config = load_config(&room_dir)?;
    let index = build_index(&config);
    let retrieval_candidates = rank(&index, &request.query);
    Ok(VaultRecallResult {
        ok: true,
        query: request.query,
        found: !retrieval_candidates.is_empty(),
        source: "vault-files".into(),
        authority: "vault-files".into(),
        roots: index.roots,
        scanned_files: index.scanned_files,
        indexed_documents: index.documents.len(),
        retrieval_candidates,
        canon_matches: Vec::new(),
        semantic_chunks: Vec::new(),
        content_chunks: Vec::new(),
        date_matches: Vec::new(),
        query_dates: Vec::new(),
        taxonomy: VaultTaxonomy {
            memory_types: vec!["vault-file".into()],
            thread_keys: Vec::new(),
            named_entities: Vec::new(),
            file_types: vec![
                "markdown".into(),
                "json".into(),
                "jsonl".into(),
                "text".into(),
            ],
        },
        warnings: index.warnings,
    })
}
