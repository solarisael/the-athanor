use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LessonFamily {
    Coding,
    Project,
    Writing,
    Design,
    Audio,
}
impl LessonFamily {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Coding => "coding",
            Self::Project => "project",
            Self::Writing => "writing",
            Self::Design => "design",
            Self::Audio => "audio",
        }
    }
}
