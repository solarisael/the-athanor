mod query;
mod write;

pub use query::{
    DesignDocument, DesignDocumentFilters, DesignDocumentQueryParams, DesignDocumentQueryResult,
    DesignDocumentTaxonomy, design_document_query,
};
pub use write::{DesignDocumentWriteParams, DesignDocumentWriteReceipt, design_document_write};

fn valid_doc_type(value: &str) -> bool {
    matches!(value, "token" | "component" | "contract" | "guideline")
}
