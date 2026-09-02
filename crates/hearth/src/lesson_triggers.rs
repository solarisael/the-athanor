//! Lesson trigger vocabulary.
//!
//! A lesson becomes a trigger when it carries `condition` (regex) or
//! `ast_condition` (ast-grep pattern) rows. This module names what those
//! columns mean: which surface a token watches, how loudly a fire interrupts,
//! when the repeat policy lets it fire again, and which rows are well-formed.
//!
//! Whether a pattern compiles is the engine's question; AKASHA owns the
//! compilers and refuses an uncompilable pattern before the write.

/// Bound on how many patterns one lesson may carry per axis. A trigger row is
/// hand-written by a spirit; this is a sanity ceiling, not a tuning knob.
pub const MAX_TRIGGER_PATTERNS: usize = 32;

/// Which kind of surface a payload came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfaceKind {
    Tool,
    Prose,
}

impl SurfaceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tool => "tool",
            Self::Prose => "prose",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "tool" => Ok(Self::Tool),
            "prose" => Ok(Self::Prose),
            other => Err(format!("surface kind must be tool or prose: {other}")),
        }
    }
}

/// Which matcher produced a fire. Kept typed so the ledger never guesses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatternKind {
    Regex,
    Ast,
}

impl PatternKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Regex => "regex",
            Self::Ast => "ast",
        }
    }
}

/// How loudly a fired lesson interrupts. NULL in the column means block:
/// always-by-default, demotion is an explicit UPDATE.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Urgency {
    Block,
    Remind,
}

impl Urgency {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Remind => "remind",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "block" => Ok(Self::Block),
            "remind" => Ok(Self::Remind),
            _ => Err("interruptMode must be block or remind".to_owned()),
        }
    }

    /// The stored column, where absence is the loud default.
    pub fn from_column(value: Option<&str>) -> Result<Self, String> {
        value.map_or(Ok(Self::Block), Self::parse)
    }
}

/// The repeat policy read off `repeat_cooldown_secs`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cooldown {
    /// NULL cooldown: one fire per (room, session), ever.
    OncePerSession,
    After(i64),
}

impl Cooldown {
    pub fn from_column(seconds: Option<i32>) -> Result<Self, String> {
        match seconds {
            None => Ok(Self::OncePerSession),
            Some(value) if value > 0 => Ok(Self::After(i64::from(value))),
            Some(value) => Err(format!(
                "repeatCooldownSecs must be a positive number of seconds: {value}"
            )),
        }
    }

    /// `age` is the seconds elapsed since this lesson's latest fire in the same
    /// (room, session), or None when it has never fired there.
    pub const fn allows(self, age: Option<i64>) -> bool {
        match (self, age) {
            (_, None) => true,
            (Self::OncePerSession, Some(_)) => false,
            (Self::After(cooldown), Some(age)) => age >= cooldown,
        }
    }
}

/// A `trigger_scope` token: which surfaces a lesson's conditions watch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScopeToken {
    Text,
    AnyTool,
    NamedTool(String),
}

impl ScopeToken {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let token = raw.trim();
        match token {
            "text" => Ok(Self::Text),
            "tool" => Ok(Self::AnyTool),
            _ => match token.strip_prefix("tool:").map(str::trim) {
                Some(name) if !name.is_empty() => Ok(Self::NamedTool(name.to_owned())),
                _ => Err(format!("triggerScope token is not valid: {raw}")),
            },
        }
    }

    /// Whether a surface of this kind, produced by `tool`, is one the token
    /// watches.
    pub fn admits(&self, kind: SurfaceKind, tool: Option<&str>) -> bool {
        match (self, kind) {
            (Self::Text, SurfaceKind::Prose) => true,
            (Self::AnyTool, SurfaceKind::Tool) => true,
            (Self::NamedTool(name), SurfaceKind::Tool) => tool == Some(name.as_str()),
            _ => false,
        }
    }
}

/// The trigger columns of one lesson, exactly as a write hands them over.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LessonTriggerSpec {
    pub condition: Vec<String>,
    pub ast_condition: Vec<String>,
    pub trigger_scope: Vec<String>,
    pub interrupt_mode: Option<String>,
    pub repeat_cooldown_secs: Option<i32>,
    /// The lesson's `language_keys` column, read as a surface fence: a keyed
    /// lesson only watches source written in one of its languages. The keys
    /// are validated with the rest of the lesson's eligibility, not with the
    /// trigger columns, so write paths carry them on the row and leave this
    /// empty.
    pub language_keys: Vec<String>,
}

impl LessonTriggerSpec {
    pub fn is_empty(&self) -> bool {
        self.condition.is_empty()
            && self.ast_condition.is_empty()
            && self.trigger_scope.is_empty()
            && self.interrupt_mode.is_none()
            && self.repeat_cooldown_secs.is_none()
    }

    /// Write-time shape for the fields actually present: bounded counts, no
    /// empty patterns, and policy columns that read as the vocabulary above.
    /// Every failure here is a refusal on the write, not a warning.
    ///
    /// A partial update (one patched column against stored siblings) can only
    /// be judged field by field; [`Self::validate`] adds the whole-row rule.
    pub fn validate_fields(&self) -> Result<(), String> {
        if self.condition.len() > MAX_TRIGGER_PATTERNS
            || self.ast_condition.len() > MAX_TRIGGER_PATTERNS
            || self.trigger_scope.len() > MAX_TRIGGER_PATTERNS
        {
            return Err(format!(
                "a lesson may carry at most {MAX_TRIGGER_PATTERNS} trigger patterns per field"
            ));
        }
        if self.condition.iter().any(|pattern| pattern.trim().is_empty()) {
            return Err("condition must not contain empty patterns".to_owned());
        }
        if self.ast_condition.iter().any(|pattern| pattern.trim().is_empty()) {
            return Err("astCondition must not contain empty patterns".to_owned());
        }
        for token in &self.trigger_scope {
            ScopeToken::parse(token)?;
        }
        if let Some(mode) = self.interrupt_mode.as_deref() {
            Urgency::parse(mode)?;
        }
        Cooldown::from_column(self.repeat_cooldown_secs)?;
        Ok(())
    }

    /// The whole-row rule on top of [`Self::validate_fields`]: policy columns
    /// without a pattern would be dead weight nobody can read as intent.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_fields()?;
        if self.condition.is_empty()
            && self.ast_condition.is_empty()
            && (!self.trigger_scope.is_empty()
                || self.interrupt_mode.is_some()
                || self.repeat_cooldown_secs.is_some())
        {
            return Err(
                "triggerScope, interruptMode and repeatCooldownSecs require condition or astCondition"
                    .to_owned(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_policy_separates_once_per_session_from_a_cooldown() {
        assert!(Cooldown::OncePerSession.allows(None));
        assert!(!Cooldown::OncePerSession.allows(Some(86_400)));
        assert!(Cooldown::After(300).allows(None));
        assert!(!Cooldown::After(300).allows(Some(299)));
        assert!(Cooldown::After(300).allows(Some(300)));
        assert_eq!(Cooldown::from_column(None), Ok(Cooldown::OncePerSession));
        assert_eq!(Cooldown::from_column(Some(60)), Ok(Cooldown::After(60)));
        assert!(Cooldown::from_column(Some(0)).is_err());
        assert!(Cooldown::from_column(Some(-1)).is_err());
    }

    #[test]
    fn missing_interrupt_mode_means_block() {
        assert_eq!(Urgency::from_column(None), Ok(Urgency::Block));
        assert_eq!(Urgency::from_column(Some("remind")), Ok(Urgency::Remind));
        assert!(Urgency::from_column(Some("nag")).is_err());
    }

    #[test]
    fn scope_tokens_pin_the_surfaces_they_name() {
        assert_eq!(ScopeToken::parse(" tool: write "), Ok(ScopeToken::NamedTool("write".into())));
        assert!(ScopeToken::parse("tool:").is_err());
        assert!(ScopeToken::parse("toolbar").is_err());

        let text = ScopeToken::Text;
        let any = ScopeToken::AnyTool;
        let write = ScopeToken::NamedTool("write".into());
        assert!(text.admits(SurfaceKind::Prose, None));
        assert!(!text.admits(SurfaceKind::Tool, Some("edit")));
        assert!(any.admits(SurfaceKind::Tool, Some("edit")));
        assert!(!any.admits(SurfaceKind::Prose, None));
        assert!(write.admits(SurfaceKind::Tool, Some("write")));
        assert!(!write.admits(SurfaceKind::Tool, Some("edit")));
        assert!(!write.admits(SurfaceKind::Prose, None));
    }

    #[test]
    fn write_validation_refuses_malformed_rows() {
        let good = LessonTriggerSpec {
            condition: vec!["\\bunwrap\\(\\)".into()],
            ast_condition: vec!["$A.unwrap()".into()],
            trigger_scope: vec!["tool:edit".into(), "text".into()],
            interrupt_mode: Some("remind".into()),
            repeat_cooldown_secs: Some(600),
            language_keys: vec![],
        };
        assert_eq!(good.validate(), Ok(()));

        let empty_pattern = LessonTriggerSpec {
            condition: vec!["  ".into()],
            ..Default::default()
        };
        assert_eq!(
            empty_pattern.validate(),
            Err("condition must not contain empty patterns".to_owned())
        );

        let too_many = LessonTriggerSpec {
            condition: vec!["x".into(); MAX_TRIGGER_PATTERNS + 1],
            ..Default::default()
        };
        assert!(too_many.validate().is_err());

        let bad_scope = LessonTriggerSpec {
            condition: vec!["x".into()],
            trigger_scope: vec!["toolbar".into()],
            ..Default::default()
        };
        assert!(
            bad_scope
                .validate()
                .unwrap_err()
                .starts_with("triggerScope token is not valid:")
        );

        let bad_mode = LessonTriggerSpec {
            condition: vec!["x".into()],
            interrupt_mode: Some("shout".into()),
            ..Default::default()
        };
        assert_eq!(
            bad_mode.validate(),
            Err("interruptMode must be block or remind".to_owned())
        );

        let orphan = LessonTriggerSpec {
            interrupt_mode: Some("block".into()),
            ..Default::default()
        };
        assert!(orphan.validate().is_err(), "policy without a pattern");
        assert_eq!(LessonTriggerSpec::default().validate(), Ok(()));
    }
}
