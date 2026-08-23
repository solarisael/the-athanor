use serde::{Serialize, Serializer, ser::SerializeStruct};
use super::delete::LessonDeleteParams;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LessonMutationKind {
    Update,
    Delete,
}

#[derive(Debug)]
pub enum LessonMutationReceipt {
    Updated {
        kind: String,
        id: i64,
        title: String,
        always_on: bool,
        project: Option<String>,
    },
    Deleted {
        kind: String,
        id: i64,
        title: String,
    },
    Refused {
        mutation: LessonMutationKind,
        kind: String,
        id: i64,
        actual_title: Option<String>,
        error: String,
    },
}

impl Serialize for LessonMutationReceipt {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Updated {
                kind,
                id,
                title,
                always_on,
                project,
            } => {
                let mut receipt = serializer.serialize_struct("LessonMutationReceipt", 7)?;
                receipt.serialize_field("ok", &true)?;
                receipt.serialize_field("kind", kind)?;
                receipt.serialize_field("id", id)?;
                receipt.serialize_field("title", title)?;
                receipt.serialize_field("updated", &true)?;
                receipt.serialize_field("alwaysOn", always_on)?;
                receipt.serialize_field("project", project)?;
                receipt.end()
            }
            Self::Deleted { kind, id, title } => {
                let mut receipt = serializer.serialize_struct("LessonMutationReceipt", 5)?;
                receipt.serialize_field("ok", &true)?;
                receipt.serialize_field("kind", kind)?;
                receipt.serialize_field("id", id)?;
                receipt.serialize_field("title", title)?;
                receipt.serialize_field("deleted", &true)?;
                receipt.end()
            }
            Self::Refused {
                mutation,
                kind,
                id,
                actual_title,
                error,
            } => {
                let field_count = if actual_title.is_some() { 6 } else { 5 };
                let mut receipt =
                    serializer.serialize_struct("LessonMutationReceipt", field_count)?;
                receipt.serialize_field("ok", &false)?;
                receipt.serialize_field("kind", kind)?;
                receipt.serialize_field("id", id)?;
                if let Some(actual_title) = actual_title {
                    receipt.serialize_field("actualTitle", actual_title)?;
                }
                match mutation {
                    LessonMutationKind::Update => receipt.serialize_field("updated", &false)?,
                    LessonMutationKind::Delete => receipt.serialize_field("deleted", &false)?,
                }
                receipt.serialize_field("error", error)?;
                receipt.end()
            }
        }
    }
}

pub(super) fn lesson_key(kind: &str) -> Option<&'static str> {
    match kind {
        "coding-lesson" => Some("coding"),
        "project-lesson" => Some("project"),
        "writing-lesson" => Some("writing"),
        "design-lesson" => Some("design"),
        _ => None,
    }
}

pub(super) fn mutation_refusal(
    p: &LessonDeleteParams,
    error: impl Into<String>,
    actual_title: Option<String>,
    mutation: LessonMutationKind,
) -> LessonMutationReceipt {
    LessonMutationReceipt::Refused {
        mutation,
        kind: p.kind.clone(),
        id: p.id,
        actual_title,
        error: error.into(),
    }
}
