//! Role lessons for the House's named familiars.
//!
//! Each named kitten and dragonling works only with the complete bodies of
//! the coding lessons tagged for it: the shared tag, plus its role tag when it
//! has one. The roster binds exact agent definition names to those tags,
//! never spellbook lanes; matching ignores ASCII letter case and surrounding
//! space. A store loads the tagged rows on every wake of a familiar, resume
//! included, and [`role_lessons`] delivers all matching bodies whole or
//! refuses the wake. Every other agent keeps the ordinary wake. The tags live
//! on existing coding lessons; there is no role store.

use super::FAMILIAR_LESSON_LIMIT_BYTES;
use super::engine::{LifecycleError, refused};
use serde_json::{Map, Value};

/// A coding-lesson tag that a named familiar requires.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoleTag {
    Shared,
    Scout,
    Executor,
    Tester,
    Verifier,
    Builder,
}

impl RoleTag {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "familiar-role:shared",
            Self::Scout => "familiar-role:scout",
            Self::Executor => "familiar-role:executor",
            Self::Tester => "familiar-role:tester",
            Self::Verifier => "familiar-role:verifier",
            Self::Builder => "familiar-role:builder",
        }
    }

    const fn missing(self) -> LifecycleError {
        refused(
            "familiar_lessons_missing",
            match self {
                Self::Shared => {
                    "no complete coding lesson carries the required tag familiar-role:shared"
                }
                Self::Scout => {
                    "no complete coding lesson carries the required tag familiar-role:scout"
                }
                Self::Executor => {
                    "no complete coding lesson carries the required tag familiar-role:executor"
                }
                Self::Tester => {
                    "no complete coding lesson carries the required tag familiar-role:tester"
                }
                Self::Verifier => {
                    "no complete coding lesson carries the required tag familiar-role:verifier"
                }
                Self::Builder => {
                    "no complete coding lesson carries the required tag familiar-role:builder"
                }
            },
        )
    }
}

/// The sixteen named familiars and the role tag each needs beside
/// [`RoleTag::Shared`]. The two generic room kittens need the shared tag only.
const ROSTER: [(&str, Option<RoleTag>); 16] = [
    ("cisma-kitten", Some(RoleTag::Scout)),
    ("faro-dragonling", Some(RoleTag::Scout)),
    ("caliper-kitten", Some(RoleTag::Scout)),
    ("sutura-kitten", Some(RoleTag::Executor)),
    ("talha-dragonling", Some(RoleTag::Executor)),
    ("kerf-kitten", Some(RoleTag::Executor)),
    ("tempera-kitten", Some(RoleTag::Tester)),
    ("crisol-dragonling", Some(RoleTag::Tester)),
    ("burr-kitten", Some(RoleTag::Tester)),
    ("prumo-kitten", Some(RoleTag::Verifier)),
    ("fiel-dragonling", Some(RoleTag::Verifier)),
    ("plumb-kitten", Some(RoleTag::Verifier)),
    ("forja-kitten", Some(RoleTag::Builder)),
    ("lavra-dragonling", Some(RoleTag::Builder)),
    ("kintsu-kitten", None),
    ("kodo-kitten", None),
];

const UNAVAILABLE: LifecycleError = refused(
    "familiar_lessons_unavailable",
    "a named familiar works only with role lessons from the AKASHA PostgreSQL lesson registry, which this Host does not run",
);

const INCOMPLETE: LifecycleError = refused(
    "familiar_lesson_incomplete",
    "a coding lesson carrying a required familiar-role tag has no valid id, title, or body",
);

const TOO_LARGE: LifecycleError = refused(
    "familiar_lessons_too_large",
    "the role lessons exceed the familiar delivery cap; they are delivered whole or not at all",
);

/// One named familiar from the roster.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Familiar {
    role: Option<RoleTag>,
}

impl Familiar {
    /// The named familiar an agent definition name selects. Letter case and
    /// surrounding space cannot hide a familiar from its door.
    pub fn named(agent_name: &str) -> Option<Self> {
        let agent_name = agent_name.trim();
        ROSTER
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(agent_name))
            .map(|&(_, role)| Self { role })
    }

    /// The exact tags whose lessons this familiar must receive, shared first.
    pub fn tags(self) -> impl Iterator<Item = RoleTag> {
        std::iter::once(RoleTag::Shared).chain(self.role)
    }
}

/// One coding lesson that a store loaded for [`Familiar::tags`].
#[derive(Clone, Debug, PartialEq)]
pub struct TaggedLesson {
    pub id: i64,
    pub title: String,
    pub lesson: String,
    pub tags: Vec<String>,
}

impl TaggedLesson {
    fn carries(&self, tag: RoleTag) -> bool {
        self.tags.iter().any(|carried| carried == tag.as_str())
    }

    fn is_complete(&self) -> bool {
        self.id > 0 && !self.title.trim().is_empty() && !self.lesson.trim().is_empty()
    }

    fn into_json(self) -> Value {
        let mut lesson = Map::new();
        lesson.insert("id".into(), self.id.into());
        lesson.insert("title".into(), self.title.into());
        lesson.insert("lesson".into(), self.lesson.into());
        Value::Object(lesson)
    }
}

/// The `familiarLessons` receipt: required tags, delivered ids, and every
/// matching body whole and once. `None` means the store has no registry.
pub(super) fn role_lessons(
    familiar: Familiar,
    lessons: Option<Vec<TaggedLesson>>,
) -> Result<Value, LifecycleError> {
    let mut lessons = lessons.ok_or(UNAVAILABLE)?;
    lessons.retain(|lesson| familiar.tags().any(|tag| lesson.carries(tag)));
    if !lessons.iter().all(TaggedLesson::is_complete) {
        return Err(INCOMPLETE);
    }
    lessons.sort_unstable_by_key(|lesson| lesson.id);
    lessons.dedup_by_key(|lesson| lesson.id);
    if let Some(tag) = familiar
        .tags()
        .find(|&tag| !lessons.iter().any(|lesson| lesson.carries(tag)))
    {
        return Err(tag.missing());
    }
    let bytes: usize = lessons
        .iter()
        .map(|lesson| lesson.title.len() + lesson.lesson.len())
        .sum();
    if bytes > FAMILIAR_LESSON_LIMIT_BYTES {
        return Err(TOO_LARGE);
    }
    let mut receipt = Map::new();
    receipt.insert(
        "tags".into(),
        familiar.tags().map(RoleTag::as_str).collect(),
    );
    receipt.insert(
        "ids".into(),
        lessons.iter().map(|lesson| lesson.id).collect(),
    );
    receipt.insert(
        "lessons".into(),
        lessons.into_iter().map(TaggedLesson::into_json).collect(),
    );
    Ok(Value::Object(receipt))
}
