mod delete;
mod receipt;
pub(crate) mod update;

pub use delete::{LessonDeleteParams, lesson_delete};
pub use receipt::{LessonMutationKind, LessonMutationReceipt};
pub use update::{LessonUpdateParams, lesson_update};
