use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct VaultRecallRequest {
    pub room_dir: PathBuf,
    pub room: String,
    pub query: String,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct VaultCandidate {
    pub source_path: String,
    pub title: String,
    pub heading_path: String,
    pub sources: Vec<String>,
    pub score: f64,
    pub term_coverage: f64,
    pub matched_terms: Vec<String>,
    pub missing_terms: Vec<String>,
    pub reasons: Vec<String>,
    pub excerpt: String,
}
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VaultTaxonomy {
    pub memory_types: Vec<String>,
    pub thread_keys: Vec<String>,
    pub named_entities: Vec<String>,
    pub file_types: Vec<String>,
}
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VaultRecallResult {
    pub ok: bool,
    pub query: String,
    pub found: bool,
    pub source: String,
    pub authority: String,
    pub roots: Vec<String>,
    pub scanned_files: usize,
    pub indexed_documents: usize,
    pub retrieval_candidates: Vec<VaultCandidate>,
    pub canon_matches: Vec<Value>,
    pub semantic_chunks: Vec<Value>,
    pub content_chunks: Vec<Value>,
    pub date_matches: Vec<Value>,
    pub query_dates: Vec<String>,
    pub taxonomy: VaultTaxonomy,
    pub warnings: Vec<String>,
}
