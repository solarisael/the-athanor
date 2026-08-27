use crate::authority::Authority;
use crate::error::DomainError;
use crate::lesson_triggers::LessonTriggerSpec;
use crate::room::RoomKey;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RememberKind {
    Memory,
    CodingLesson,
    ProjectLesson,
    WritingLesson,
    DesignLesson,
    AudioLesson,
}

impl RememberKind {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "memory" => Ok(Self::Memory),
            "coding-lesson" => Ok(Self::CodingLesson),
            "project-lesson" => Ok(Self::ProjectLesson),
            "writing-lesson" => Ok(Self::WritingLesson),
            "design-lesson" => Ok(Self::DesignLesson),
            "audio-lesson" => Ok(Self::AudioLesson),
            other => Err(DomainError::UnsupportedKind(other.to_owned())),
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::CodingLesson => "coding-lesson",
            Self::ProjectLesson => "project-lesson",
            Self::WritingLesson => "writing-lesson",
            Self::DesignLesson => "design-lesson",
            Self::AudioLesson => "audio-lesson",
        }
    }
    pub const fn is_lesson(self) -> bool {
        !matches!(self, Self::Memory)
    }
}

pub(crate) const MAX_ARRAY_VALUES: usize = 64;

pub(crate) fn normalize_eligibility_keys(
    field: &str,
    values: Vec<String>,
) -> Result<Vec<String>, DomainError> {
    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        let value = value.trim();
        let valid = !value.is_empty()
            && !value.starts_with('-')
            && !value.ends_with('-')
            && value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            && !value.contains("--");
        if !valid {
            return Err(DomainError::InvalidField {
                field: field.into(),
                kind: "eligibility-key".into(),
            });
        }
        if !normalized.iter().any(|entry| entry == value) {
            normalized.push(value.to_owned());
        }
    }
    Ok(normalized)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadContinuation {
    pub thread: String,
    pub previous_memory_id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RememberMemoryDetails {
    pub source_path: Option<String>,
    pub threads: Vec<String>,
    pub continues: Vec<ThreadContinuation>,
    pub supersedes: Vec<u64>,
    pub backup: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RememberLessonDetails {
    pub backup: bool,
    pub source_memory_path: Option<String>,
    pub shape: Option<String>,
    pub voice: Option<String>,
    pub register: Vec<String>,
    pub scope: Option<String>,
    pub project: Option<String>,
    pub proof_pattern: Option<String>,
    pub trigger_context: Option<String>,
    pub example_text: Option<String>,
    pub language_keys: Vec<String>,
    pub technology_keys: Vec<String>,
    pub thread_keys: Vec<String>,
    pub tags: Vec<String>,
    /// The lesson's trigger columns. Empty means the lesson never fires on its
    /// own; the write path still validates whatever is here.
    pub triggers: LessonTriggerSpec,
}

enum RememberDetails {
    Memory(RememberMemoryDetails),
    Lesson(RememberLessonDetails),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RememberRequest {
    room: RoomKey,
    kind: RememberKind,
    title: String,
    body: String,
    source_path: Option<String>,
    source_memory_path: Option<String>,
    threads: Vec<String>,
    continues: Vec<ThreadContinuation>,
    supersedes: Vec<u64>,
    backup: bool,
    shape: Option<String>,
    voice: Option<String>,
    register: Vec<String>,
    scope: Option<String>,
    project: Option<String>,
    proof_pattern: Option<String>,
    trigger_context: Option<String>,
    example_text: Option<String>,
    language_keys: Vec<String>,
    technology_keys: Vec<String>,
    thread_keys: Vec<String>,
    tags: Vec<String>,
    triggers: LessonTriggerSpec,
}

impl RememberRequest {
    pub fn new_memory(
        room: RoomKey,
        title: String,
        body: String,
        details: RememberMemoryDetails,
    ) -> Result<Self, DomainError> {
        Self::build(
            room,
            RememberKind::Memory,
            title,
            body,
            RememberDetails::Memory(details),
        )
    }

    pub fn new_lesson(
        room: RoomKey,
        kind: RememberKind,
        title: String,
        body: String,
        details: RememberLessonDetails,
    ) -> Result<Self, DomainError> {
        Self::build(room, kind, title, body, RememberDetails::Lesson(details))
    }

    fn build(
        room: RoomKey,
        kind: RememberKind,
        title: String,
        body: String,
        details: RememberDetails,
    ) -> Result<Self, DomainError> {
        if title.trim().is_empty() {
            return Err(DomainError::EmptyTitle);
        }
        if body.trim().is_empty() {
            return Err(DomainError::EmptyBody);
        }
        let (
            source_path,
            source_memory_path,
            threads,
            continues,
            supersedes,
            backup,
            shape,
            voice,
            register,
            scope,
            project,
            proof_pattern,
            trigger_context,
            example_text,
            language_keys,
            technology_keys,
            thread_keys,
            tags,
            triggers,
        ) = match details {
            RememberDetails::Memory(details) => {
                if kind.is_lesson() {
                    return Err(DomainError::InvalidField {
                        field: "memory fields".into(),
                        kind: kind.as_str().into(),
                    });
                }
                (
                    details.source_path,
                    None,
                    details.threads,
                    details.continues,
                    details.supersedes,
                    details.backup,
                    None,
                    None,
                    Vec::new(),
                    None,
                    None,
                    None,
                    None,
                    None,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    LessonTriggerSpec::default(),
                )
            }
            RememberDetails::Lesson(details) => {
                if !kind.is_lesson() {
                    return Err(DomainError::InvalidField {
                        field: "lesson fields".into(),
                        kind: kind.as_str().into(),
                    });
                }
                (
                    None,
                    details.source_memory_path,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    details.backup,
                    details.shape,
                    details.voice,
                    details.register,
                    details.scope,
                    details.project,
                    details.proof_pattern,
                    details.trigger_context,
                    details.example_text,
                    details.language_keys,
                    details.technology_keys,
                    details.thread_keys,
                    details.tags,
                    details.triggers,
                )
            }
        };
        if threads.len() > MAX_ARRAY_VALUES
            || continues.len() > MAX_ARRAY_VALUES
            || supersedes.len() > MAX_ARRAY_VALUES
            || register.len() > MAX_ARRAY_VALUES
            || language_keys.len() > MAX_ARRAY_VALUES
            || technology_keys.len() > MAX_ARRAY_VALUES
            || thread_keys.len() > MAX_ARRAY_VALUES
            || tags.len() > MAX_ARRAY_VALUES
        {
            return Err(DomainError::TooManyValues {
                field: "array".into(),
            });
        }
        if supersedes.contains(&0) {
            return Err(DomainError::InvalidSupersedes);
        }
        if matches!(kind, RememberKind::ProjectLesson)
            && project.as_deref().is_none_or(|p| p.trim().is_empty())
        {
            return Err(DomainError::MissingProject);
        }
        if matches!(kind, RememberKind::WritingLesson)
            && (scope.is_some() || project.is_some() || proof_pattern.is_some())
        {
            return Err(DomainError::InvalidField {
                field: "scope/project/proof_pattern".into(),
                kind: kind.as_str().into(),
            });
        }
        if matches!(kind, RememberKind::DesignLesson) && (scope.is_some() || project.is_some()) {
            return Err(DomainError::InvalidField {
                field: "scope/project".into(),
                kind: kind.as_str().into(),
            });
        }
        if matches!(kind, RememberKind::AudioLesson)
            && (voice.is_some()
                || !register.is_empty()
                || scope.is_some()
                || project.is_some()
                || proof_pattern.is_some())
        {
            return Err(DomainError::InvalidField {
                field: "voice/register/scope/project/proof_pattern".into(),
                kind: kind.as_str().into(),
            });
        }
        if matches!(kind, RememberKind::ProjectLesson)
            && (voice.is_some() || !register.is_empty() || scope.is_some())
        {
            return Err(DomainError::InvalidField {
                field: "voice/register/scope".into(),
                kind: kind.as_str().into(),
            });
        }
        if matches!(kind, RememberKind::CodingLesson) && !register.is_empty() {
            return Err(DomainError::InvalidField {
                field: "register".into(),
                kind: kind.as_str().into(),
            });
        }
        if !matches!(
            kind,
            RememberKind::CodingLesson | RememberKind::ProjectLesson
        ) && (!language_keys.is_empty() || !technology_keys.is_empty())
        {
            return Err(DomainError::InvalidField {
                field: "language_keys/technology_keys".into(),
                kind: kind.as_str().into(),
            });
        }
        let language_keys = normalize_eligibility_keys("language_keys", language_keys)?;
        let technology_keys = normalize_eligibility_keys("technology_keys", technology_keys)?;
        let thread_keys = normalize_eligibility_keys("thread_keys", thread_keys)?;
        let mut normalized_register = Vec::with_capacity(register.len());
        for value in register {
            let value = value.trim();
            if !value.is_empty() && !normalized_register.iter().any(|entry| entry == value) {
                normalized_register.push(value.to_owned());
            }
        }
        let register = normalized_register;
        let mut normalized_threads = Vec::with_capacity(threads.len());
        for thread in threads {
            let thread = thread.trim();
            if !thread.is_empty() && !normalized_threads.iter().any(|entry| entry == thread) {
                normalized_threads.push(thread.to_owned());
            }
        }
        let threads = normalized_threads;
        let source_path = source_path.and_then(|path| (!path.trim().is_empty()).then_some(path));
        let source_memory_path =
            source_memory_path.and_then(|path| (!path.trim().is_empty()).then_some(path));
        let mut normalized_continues = Vec::with_capacity(continues.len());
        for continuation in continues {
            let thread = continuation.thread.trim();
            if thread.is_empty() || continuation.previous_memory_id == 0 {
                return Err(DomainError::InvalidContinuation);
            }
            if normalized_continues
                .iter()
                .any(|entry: &ThreadContinuation| entry.thread == thread)
            {
                return Err(DomainError::DuplicateContinuationThread(thread.into()));
            }
            if !threads.iter().any(|candidate| candidate.trim() == thread) {
                return Err(DomainError::ContinuationThreadNotMember(thread.into()));
            }
            normalized_continues.push(ThreadContinuation {
                thread: thread.into(),
                previous_memory_id: continuation.previous_memory_id,
            });
        }
        let mut unique_supersedes = Vec::with_capacity(supersedes.len());
        for id in supersedes {
            if !unique_supersedes.contains(&id) {
                unique_supersedes.push(id);
            }
        }
        triggers
            .validate()
            .map_err(DomainError::InvalidLessonTrigger)?;
        Ok(Self {
            room,
            kind,
            title,
            body,
            source_path,
            source_memory_path,
            threads,
            continues: normalized_continues,
            supersedes: unique_supersedes,
            backup,
            shape,
            voice,
            register,
            scope,
            project,
            proof_pattern,
            trigger_context,
            example_text,
            language_keys,
            technology_keys,
            thread_keys,
            tags,
            triggers,
        })
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn kind(&self) -> RememberKind {
        self.kind
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
    }
    pub fn source_memory_path(&self) -> Option<&str> {
        self.source_memory_path.as_deref()
    }
    pub fn threads(&self) -> &[String] {
        &self.threads
    }
    pub fn continues(&self) -> &[ThreadContinuation] {
        &self.continues
    }
    pub fn supersedes(&self) -> &[u64] {
        &self.supersedes
    }
    pub const fn backup(&self) -> bool {
        self.backup
    }
    pub fn shape(&self) -> Option<&str> {
        self.shape.as_deref()
    }
    pub fn voice(&self) -> Option<&str> {
        self.voice.as_deref()
    }
    pub fn register(&self) -> &[String] {
        &self.register
    }
    pub fn scope(&self) -> Option<&str> {
        self.scope.as_deref()
    }
    pub fn project(&self) -> Option<&str> {
        self.project.as_deref()
    }
    pub fn proof_pattern(&self) -> Option<&str> {
        self.proof_pattern.as_deref()
    }
    pub fn trigger_context(&self) -> Option<&str> {
        self.trigger_context.as_deref()
    }
    pub fn example_text(&self) -> Option<&str> {
        self.example_text.as_deref()
    }
    pub fn language_keys(&self) -> &[String] {
        &self.language_keys
    }
    pub fn technology_keys(&self) -> &[String] {
        &self.technology_keys
    }
    pub fn thread_keys(&self) -> &[String] {
        &self.thread_keys
    }
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
    pub fn triggers(&self) -> &LessonTriggerSpec {
        &self.triggers
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RememberReceipt {
    memory_id: Option<u64>,
    lesson_id: Option<u64>,
    kind: RememberKind,
    room: RoomKey,
    source_path: Option<String>,
    warnings: Vec<String>,
}

impl RememberReceipt {
    pub fn committed(
        memory_id: u64,
        room: RoomKey,
        source_path: String,
        warnings: Vec<String>,
    ) -> Result<Self, DomainError> {
        if source_path.trim().is_empty() {
            return Err(DomainError::EmptySourcePath);
        }
        Ok(Self {
            memory_id: Some(memory_id),
            lesson_id: None,
            kind: RememberKind::Memory,
            room,
            source_path: Some(source_path),
            warnings,
        })
    }
    pub fn committed_lesson(
        lesson_id: u64,
        kind: RememberKind,
        room: RoomKey,
        warnings: Vec<String>,
    ) -> Result<Self, DomainError> {
        if !kind.is_lesson() {
            return Err(DomainError::UnsupportedKind(kind.as_str().into()));
        }
        Ok(Self {
            memory_id: None,
            lesson_id: Some(lesson_id),
            kind,
            room,
            source_path: None,
            warnings,
        })
    }
    pub fn memory_id(&self) -> u64 {
        self.memory_id.unwrap_or(0)
    }
    pub fn lesson_id(&self) -> u64 {
        self.lesson_id.unwrap_or(0)
    }
    pub const fn kind(&self) -> RememberKind {
        self.kind
    }
    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn source_path(&self) -> &str {
        self.source_path.as_deref().unwrap_or("")
    }
    pub const fn durable(&self) -> bool {
        true
    }
    pub const fn authority(&self) -> Authority {
        Authority::Full
    }
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_receipt_requires_source_path_and_is_postgres_durable() {
        let room = RoomKey::new("lab").unwrap();
        assert!(RememberReceipt::committed(1, room.clone(), " ".into(), vec![]).is_err());
        let receipt = RememberReceipt::committed(1, room, "memory.md".into(), vec![]).unwrap();
        assert_eq!(receipt.source_path(), "memory.md");
        assert!(receipt.durable());
        assert_eq!(receipt.authority(), Authority::Full);
    }

    #[test]
    fn validates_memory_request_invariants() {
        let room = RoomKey::new("lab").unwrap();
        assert_eq!(
            RememberRequest::new_memory(
                room.clone(),
                " ".into(),
                "body".into(),
                RememberMemoryDetails {
                    source_path: None,
                    threads: vec![],
                    continues: vec![],
                    supersedes: vec![],
                    backup: true,
                },
            ),
            Err(DomainError::EmptyTitle)
        );
        assert_eq!(
            RememberRequest::new_memory(
                room,
                "title".into(),
                "\n".into(),
                RememberMemoryDetails {
                    source_path: None,
                    threads: vec![],
                    continues: vec![],
                    supersedes: vec![],
                    backup: true,
                },
            ),
            Err(DomainError::EmptyBody)
        );
    }

    #[test]
    fn design_lessons_round_trip_and_reject_cross_family_fields() {
        let kind = RememberKind::parse("design-lesson").unwrap();
        assert_eq!(kind, RememberKind::DesignLesson);
        assert_eq!(kind.as_str(), "design-lesson");
        assert!(kind.is_lesson());

        let details = || RememberLessonDetails {
            backup: true,
            source_memory_path: Some("memory/design.md".into()),
            shape: Some("component-contract".into()),
            voice: Some("solarisael".into()),
            register: vec!["general".into()],
            scope: None,
            project: None,
            proof_pattern: Some("Verify keyboard navigation.".into()),
            trigger_context: Some("Before introducing a component.".into()),
            example_text: Some("Use the token, not a one-off value.".into()),
            language_keys: vec![],
            technology_keys: vec![],
            thread_keys: vec![],
            tags: vec!["accessibility".into()],
            triggers: LessonTriggerSpec::default(),
        };
        let request = RememberRequest::new_lesson(
            RoomKey::new("lab").unwrap(),
            kind,
            "Accessibility floor".into(),
            "Components retain their keyboard contract.".into(),
            details(),
        )
        .unwrap();
        assert_eq!(
            request.example_text(),
            Some("Use the token, not a one-off value.")
        );
        assert_eq!(request.source_memory_path(), Some("memory/design.md"));

        let mut invalid = details();
        invalid.scope = Some("house".into());
        assert_eq!(
            RememberRequest::new_lesson(
                RoomKey::new("lab").unwrap(),
                kind,
                "Accessibility floor".into(),
                "Components retain their keyboard contract.".into(),
                invalid,
            ),
            Err(DomainError::InvalidField {
                field: "scope/project".into(),
                kind: "design-lesson".into(),
            })
        );
    }

    #[test]
    fn lesson_eligibility_keys_normalize_and_reject_wrong_families() {
        let details = RememberLessonDetails {
            backup: false,
            source_memory_path: None,
            shape: Some("process".into()),
            voice: None,
            register: vec![],
            scope: Some("house".into()),
            project: None,
            proof_pattern: Some("The exact path passes.".into()),
            trigger_context: Some("When editing Rust database code.".into()),
            example_text: None,
            language_keys: vec![" rust ".into(), "rust".into()],
            technology_keys: vec!["postgresql".into()],
            thread_keys: vec!["subagent-dispatch".into()],
            tags: vec![],
            triggers: LessonTriggerSpec::default(),
        };
        let request = RememberRequest::new_lesson(
            RoomKey::new("lab").unwrap(),
            RememberKind::CodingLesson,
            "Keyed database lesson".into(),
            "Apply only in eligible contexts.".into(),
            details,
        )
        .unwrap();
        assert_eq!(request.language_keys(), &["rust"]);
        assert_eq!(request.technology_keys(), &["postgresql"]);
        assert_eq!(request.thread_keys(), &["subagent-dispatch"]);

        let mut invalid = RememberLessonDetails {
            backup: false,
            source_memory_path: None,
            shape: None,
            voice: Some("general".into()),
            register: vec![],
            scope: None,
            project: None,
            proof_pattern: None,
            trigger_context: None,
            example_text: None,
            language_keys: vec!["rust".into()],
            technology_keys: vec![],
            thread_keys: vec![],
            tags: vec![],
            triggers: LessonTriggerSpec::default(),
        };
        assert!(
            RememberRequest::new_lesson(
                RoomKey::new("lab").unwrap(),
                RememberKind::WritingLesson,
                "Wrong family".into(),
                "Must refuse keys.".into(),
                invalid.clone(),
            )
            .is_err()
        );
        invalid.language_keys = vec!["Rust".into()];
        assert!(
            RememberRequest::new_lesson(
                RoomKey::new("lab").unwrap(),
                RememberKind::CodingLesson,
                "Bad slug".into(),
                "Must refuse malformed keys.".into(),
                invalid,
            )
            .is_err()
        );
    }

    #[test]
    fn validates_memory_continuations_per_thread() {
        let accepted = RememberRequest::new_memory(
            RoomKey::new("lab").unwrap(),
            "decision".into(),
            "new decision".into(),
            RememberMemoryDetails {
                source_path: None,
                threads: vec![" work / page ".into()],
                continues: vec![ThreadContinuation {
                    thread: "work / page".into(),
                    previous_memory_id: 41,
                }],
                supersedes: vec![],
                backup: false,
            },
        )
        .unwrap();
        assert_eq!(accepted.threads(), &["work / page"]);
        assert_eq!(
            accepted.continues(),
            &[ThreadContinuation {
                thread: "work / page".into(),
                previous_memory_id: 41,
            }]
        );

        let missing_membership = RememberRequest::new_memory(
            RoomKey::new("lab").unwrap(),
            "decision".into(),
            "new decision".into(),
            RememberMemoryDetails {
                source_path: None,
                threads: vec!["other".into()],
                continues: vec![ThreadContinuation {
                    thread: "work / page".into(),
                    previous_memory_id: 41,
                }],
                supersedes: vec![],
                backup: false,
            },
        );
        assert_eq!(
            missing_membership,
            Err(DomainError::ContinuationThreadNotMember(
                "work / page".into()
            ))
        );

        let duplicate = RememberRequest::new_memory(
            RoomKey::new("lab").unwrap(),
            "decision".into(),
            "new decision".into(),
            RememberMemoryDetails {
                source_path: None,
                threads: vec!["work / page".into()],
                continues: vec![
                    ThreadContinuation {
                        thread: "work / page".into(),
                        previous_memory_id: 41,
                    },
                    ThreadContinuation {
                        thread: " work / page ".into(),
                        previous_memory_id: 42,
                    },
                ],
                supersedes: vec![],
                backup: false,
            },
        );
        assert_eq!(
            duplicate,
            Err(DomainError::DuplicateContinuationThread(
                "work / page".into()
            ))
        );
    }
}
