use crate::config::VaultConfig;
use crate::documents::{VaultDocument, parse_file};
use crate::walk::{collect_files, normalized_path};

pub(crate) struct VaultIndex {
    pub(crate) roots: Vec<String>,
    pub(crate) documents: Vec<VaultDocument>,
    pub(crate) scanned_files: usize,
    pub(crate) warnings: Vec<String>,
}

pub(crate) fn build_index(config: &VaultConfig) -> VaultIndex {
    let (files, mut warnings) = collect_files(config);
    let scanned_files = files.len();
    let mut documents = Vec::new();
    for (root, absolute) in files {
        documents.extend(parse_file(&root, &absolute, config, &mut warnings));
    }
    VaultIndex {
        roots: config
            .roots
            .iter()
            .map(|root| normalized_path(root))
            .collect(),
        documents,
        scanned_files,
        warnings,
    }
}
