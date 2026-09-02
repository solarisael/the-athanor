mod coerce;
mod defaults;
mod design;
mod registry;
#[cfg(test)]
mod tests;

pub use design::{
    DesignDocument, DesignDocumentFilters, DesignDocumentQueryParams, DesignDocumentQueryResult,
    DesignDocumentTaxonomy, DesignDocumentWriteParams, DesignDocumentWriteReceipt,
    design_document_query, design_document_write,
};
pub(crate) use registry::validate_patterns;
pub use registry::{
    LessonContextFilters, LessonContextMatch, LessonContextParams, LessonContextRecord,
    LessonContextResult, LessonDeleteParams, LessonFamily, LessonFilters, LessonMutationKind,
    LessonMutationReceipt, LessonQueryParams, LessonQueryResult, LessonRecord, LessonTaxonomy,
    LessonTriggerFired, LessonTriggerMatchParams, LessonTriggerMatchResult, LessonTriggerSurface,
    LessonUpdateParams, lesson_context, lesson_delete, lesson_query, lesson_trigger_match,
    lesson_update,
};
