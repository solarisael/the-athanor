//! Lesson trigger compiling and matching.
//!
//! A lesson becomes a trigger when it carries `condition` (regex) or
//! `ast_condition` (ast-grep pattern) rows. PostgreSQL is the only store; this
//! module owns every decision made about those columns: what a valid trigger is
//! at write time, which surface a trigger is allowed to watch, whether a
//! pattern fires, and whether the repeat policy lets it fire again.
//!
//! Nothing here touches the database or the clock. The substrate reads rows,
//! reads the firing ledger, and hands both in.

use regex::Regex;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use ast_grep_core::Pattern;
use ast_grep_core::tree_sitter::LanguageExt;
use ast_grep_language::SupportLang;

/// Bound on how many patterns one lesson may carry per axis. A trigger row is
/// hand-written by a spirit; this is a sanity ceiling, not a tuning knob.
pub const MAX_TRIGGER_PATTERNS: usize = 32;

// enough: the v1 AST language set is the five grammars this crate enables in
// ast-grep-language 0.45.1 — the languages the House's own sources are written
// in. Upgrade path: enable another `tree-sitter-*` feature in
// crates/house-core/Cargo.toml and add its extensions to `ast_language`.
const AST_LANGUAGES: [SupportLang; 5] = [
    SupportLang::Rust,
    SupportLang::TypeScript,
    SupportLang::Tsx,
    SupportLang::JavaScript,
    SupportLang::Python,
];

// enough: `.sql` is a v1 trigger extension with no tree-sitter grammar —
// ast-grep-language 0.45.1's builtin-parser set ships no SQL at all. Regex
// conditions still cover .sql surfaces; ast conditions skip with a warning
// rather than pretending. Upgrade path: depend on `tree-sitter-sequel` and
// implement `LanguageExt` for a House `Sql` language, then map it here.
const GRAMMARLESS_EXTENSIONS: [&str; 1] = ["sql"];

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

    fn admits(&self, surface: &Surface<'_>) -> bool {
        match (self, surface.kind) {
            (Self::Text, SurfaceKind::Prose) => true,
            (Self::AnyTool, SurfaceKind::Tool) => true,
            (Self::NamedTool(name), SurfaceKind::Tool) => surface.tool == Some(name.as_str()),
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
}

impl LessonTriggerSpec {
    pub fn is_empty(&self) -> bool {
        self.condition.is_empty()
            && self.ast_condition.is_empty()
            && self.trigger_scope.is_empty()
            && self.interrupt_mode.is_none()
            && self.repeat_cooldown_secs.is_none()
    }

    /// Write-time semantics for the fields actually present. Every failure
    /// here is a refusal on the write, not a warning: a trigger that cannot
    /// compile can never fire, and a lesson that can never fire is a lie in
    /// the store.
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
        for pattern in &self.condition {
            if pattern.trim().is_empty() {
                return Err("condition must not contain empty patterns".to_owned());
            }
            if let Err(error) = Regex::new(pattern) {
                return Err(format!("condition is not a valid regex: {pattern}: {error}"));
            }
        }
        for pattern in &self.ast_condition {
            if pattern.trim().is_empty() {
                return Err("astCondition must not contain empty patterns".to_owned());
            }
            if !ast_pattern_parses(pattern) {
                return Err(format!(
                    "astCondition does not parse for any supported language: {pattern}"
                ));
            }
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

/// True when an ast-grep pattern parses under at least one supported grammar.
pub fn ast_pattern_parses(pattern: &str) -> bool {
    AST_LANGUAGES
        .iter()
        .any(|lang| Pattern::try_new(pattern, *lang).is_ok_and(|parsed| !parsed.has_error()))
}

/// The grammar for a surface path, or the warning that says why there is none.
pub fn ast_language(path: Option<&str>) -> Result<SupportLang, String> {
    let Some(path) = path.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err("ast conditions skipped: surface carries no path".to_owned());
    };
    let extension = path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "rs" => Ok(SupportLang::Rust),
        "ts" => Ok(SupportLang::TypeScript),
        "tsx" => Ok(SupportLang::Tsx),
        "js" | "jsx" => Ok(SupportLang::JavaScript),
        "py" => Ok(SupportLang::Python),
        other if GRAMMARLESS_EXTENSIONS.contains(&other) => Err(format!(
            "ast conditions skipped for {path}: {other} has no ast grammar in ast-grep-language 0.45.1; regex conditions still apply"
        )),
        "" => Err(format!("ast conditions skipped for {path}: no extension")),
        other => Err(format!(
            "ast conditions skipped for {path}: unsupported extension {other}"
        )),
    }
}

/// One trigger-bearing lesson row, typed by family and id — a fired lesson is
/// never a flattened row.
#[derive(Clone, Debug)]
pub struct TriggerRow {
    pub family: String,
    pub id: i64,
    pub title: String,
    pub lesson: String,
    pub proof_pattern: Option<String>,
    pub spec: LessonTriggerSpec,
}

/// A lesson's triggers, compiled once.
#[derive(Debug)]
pub struct CompiledTrigger {
    pub family: String,
    pub id: i64,
    pub title: String,
    pub lesson: String,
    pub proof_pattern: Option<String>,
    pub urgency: Urgency,
    pub cooldown: Cooldown,
    scopes: Vec<ScopeToken>,
    regexes: Vec<(String, Regex)>,
    ast_patterns: Vec<String>,
}

impl CompiledTrigger {
    fn admits(&self, surface: &Surface<'_>) -> bool {
        if self.scopes.is_empty() {
            // Empty scope: regex watches prose and tool text alike, ast watches
            // tool surfaces only. The ast half is enforced where it parses.
            return true;
        }
        self.scopes.iter().any(|scope| scope.admits(surface))
    }

    fn has_ast(&self) -> bool {
        !self.ast_patterns.is_empty()
    }
}

/// Every trigger visible to one room, compiled against one fingerprint.
#[derive(Debug)]
pub struct CompiledTriggerSet {
    fingerprint: String,
    triggers: Vec<CompiledTrigger>,
    warnings: Vec<String>,
}

impl CompiledTriggerSet {
    /// Rows are validated at write time, so a pattern that fails to compile
    /// here is store drift: the pattern is dropped and named, the rest of the
    /// lesson still fires.
    pub fn compile(fingerprint: impl Into<String>, rows: &[TriggerRow]) -> Self {
        let mut triggers = Vec::with_capacity(rows.len());
        let mut warnings = Vec::new();
        for row in rows {
            let urgency = match Urgency::from_column(row.spec.interrupt_mode.as_deref()) {
                Ok(urgency) => urgency,
                Err(error) => {
                    warnings.push(format!("{}#{}: {error}", row.family, row.id));
                    continue;
                }
            };
            let cooldown = match Cooldown::from_column(row.spec.repeat_cooldown_secs) {
                Ok(cooldown) => cooldown,
                Err(error) => {
                    warnings.push(format!("{}#{}: {error}", row.family, row.id));
                    continue;
                }
            };
            let mut scopes = Vec::with_capacity(row.spec.trigger_scope.len());
            for token in &row.spec.trigger_scope {
                match ScopeToken::parse(token) {
                    Ok(scope) => scopes.push(scope),
                    Err(error) => warnings.push(format!("{}#{}: {error}", row.family, row.id)),
                }
            }
            let mut regexes = Vec::with_capacity(row.spec.condition.len());
            for pattern in &row.spec.condition {
                match Regex::new(pattern) {
                    Ok(compiled) => regexes.push((pattern.clone(), compiled)),
                    Err(error) => warnings.push(format!(
                        "{}#{}: condition dropped, not a valid regex: {pattern}: {error}",
                        row.family, row.id
                    )),
                }
            }
            if regexes.is_empty() && row.spec.ast_condition.is_empty() {
                continue;
            }
            triggers.push(CompiledTrigger {
                family: row.family.clone(),
                id: row.id,
                title: row.title.clone(),
                lesson: row.lesson.clone(),
                proof_pattern: row.proof_pattern.clone(),
                urgency,
                cooldown,
                scopes,
                regexes,
                ast_patterns: row.spec.ast_condition.clone(),
            });
        }
        Self {
            fingerprint: fingerprint.into(),
            triggers,
            warnings,
        }
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn triggers(&self) -> &[CompiledTrigger] {
        &self.triggers
    }

    pub fn is_empty(&self) -> bool {
        self.triggers.is_empty()
    }
}

/// One payload offered for matching.
#[derive(Clone, Copy, Debug)]
pub struct Surface<'a> {
    pub kind: SurfaceKind,
    pub tool: Option<&'a str>,
    pub path: Option<&'a str>,
    pub text: &'a str,
}

/// One lesson matched on one surface, before the repeat policy is consulted.
#[derive(Clone, Debug)]
pub struct TriggerHit {
    /// Index into [`CompiledTriggerSet::triggers`].
    pub trigger: usize,
    pub surface: SurfaceKind,
    pub tool: Option<String>,
    pub path: Option<String>,
    pub pattern_kind: PatternKind,
    pub pattern: String,
}

#[derive(Clone, Debug, Default)]
pub struct MatchOutcome {
    pub hits: Vec<TriggerHit>,
    pub warnings: Vec<String>,
}

/// Match every surface against every trigger. At most one hit per lesson: a
/// lesson fires once per call, on the first surface and pattern that catches
/// it, so the ledger never records the same lesson twice for one turn.
pub fn match_surfaces(set: &CompiledTriggerSet, surfaces: &[Surface<'_>]) -> MatchOutcome {
    let mut outcome = MatchOutcome {
        hits: Vec::new(),
        warnings: set.warnings.clone(),
    };
    let mut fired = vec![false; set.triggers.len()];
    for surface in surfaces {
        // enough: ast patterns are compiled per surface, not cached per
        // (pattern, language). A turn carries a handful of surfaces and the
        // parse dominates. Upgrade path: memoize Pattern by (pattern, lang) in
        // the compiled set once a profile shows it matters.
        let mut parsed: Option<ast_grep_core::AstGrep<_>> = None;
        let mut language: Option<SupportLang> = None;
        let mut language_reported = false;
        for (index, trigger) in set.triggers.iter().enumerate() {
            if fired[index] || !trigger.admits(surface) {
                continue;
            }
            if let Some((pattern, _)) = trigger
                .regexes
                .iter()
                .find(|(_, regex)| regex.is_match(surface.text))
            {
                fired[index] = true;
                outcome.hits.push(TriggerHit {
                    trigger: index,
                    surface: surface.kind,
                    tool: surface.tool.map(str::to_owned),
                    path: surface.path.map(str::to_owned),
                    pattern_kind: PatternKind::Regex,
                    pattern: pattern.clone(),
                });
                continue;
            }
            // AST conditions are inherently a tool-surface matcher: prose has
            // no path and therefore no grammar.
            if !trigger.has_ast() || surface.kind != SurfaceKind::Tool {
                continue;
            }
            if parsed.is_none() {
                match ast_language(surface.path) {
                    Ok(lang) => {
                        language = Some(lang);
                        parsed = Some(lang.ast_grep(surface.text));
                    }
                    Err(warning) => {
                        if !language_reported {
                            language_reported = true;
                            outcome.warnings.push(warning);
                        }
                        continue;
                    }
                }
            }
            let (Some(root), Some(lang)) = (parsed.as_ref(), language) else {
                continue;
            };
            for pattern in &trigger.ast_patterns {
                let compiled = match Pattern::try_new(pattern, lang) {
                    Ok(compiled) if !compiled.has_error() => compiled,
                    Ok(_) => {
                        outcome.warnings.push(format!(
                            "{}#{}: astCondition dropped for {lang}: {pattern}: pattern parses with errors",
                            trigger.family, trigger.id
                        ));
                        continue;
                    }
                    Err(error) => {
                        outcome.warnings.push(format!(
                            "{}#{}: astCondition dropped for {lang}: {pattern}: {error}",
                            trigger.family, trigger.id
                        ));
                        continue;
                    }
                };
                if root.root().find(&compiled).is_some() {
                    fired[index] = true;
                    outcome.hits.push(TriggerHit {
                        trigger: index,
                        surface: surface.kind,
                        tool: surface.tool.map(str::to_owned),
                        path: surface.path.map(str::to_owned),
                        pattern_kind: PatternKind::Ast,
                        pattern: pattern.clone(),
                    });
                    break;
                }
            }
        }
    }
    outcome
}

// enough: the compile cache is one entry per room, replaced whenever the
// caller's fingerprint (trigger-bearing row count + max updated_at) changes.
// It is bounded by the number of rooms a substrate process serves. Upgrade
// path: an LRU with a size ceiling if a host ever serves unbounded rooms.
static CACHE: OnceLock<Mutex<HashMap<String, Arc<CompiledTriggerSet>>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, Arc<CompiledTriggerSet>>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A poisoned cache is recoverable: the map holds no invariant a panic could
/// have broken, so we take the inner map rather than propagating the panic.
fn locked() -> std::sync::MutexGuard<'static, HashMap<String, Arc<CompiledTriggerSet>>> {
    cache().lock().unwrap_or_else(|error| error.into_inner())
}

/// The cached set for a room, when it was compiled from the same fingerprint.
pub fn cached_set(room: &str, fingerprint: &str) -> Option<Arc<CompiledTriggerSet>> {
    locked()
        .get(room)
        .filter(|set| set.fingerprint == fingerprint)
        .map(Arc::clone)
}

/// Install a freshly compiled set as the room's cache entry.
pub fn store_set(room: &str, set: CompiledTriggerSet) -> Arc<CompiledTriggerSet> {
    let set = Arc::new(set);
    locked().insert(room.to_owned(), Arc::clone(&set));
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, spec: LessonTriggerSpec) -> TriggerRow {
        TriggerRow {
            family: "coding".into(),
            id,
            title: format!("lesson {id}"),
            lesson: "hold the rope".into(),
            proof_pattern: None,
            spec,
        }
    }

    fn tool<'a>(name: &'a str, path: &'a str, text: &'a str) -> Surface<'a> {
        Surface {
            kind: SurfaceKind::Tool,
            tool: Some(name),
            path: Some(path),
            text,
        }
    }

    fn prose(text: &str) -> Surface<'_> {
        Surface {
            kind: SurfaceKind::Prose,
            tool: None,
            path: None,
            text,
        }
    }

    #[test]
    fn empty_scope_watches_prose_and_tools_but_named_scope_pins_one_tool() {
        let set = CompiledTriggerSet::compile(
            "f1",
            &[
                row(
                    1,
                    LessonTriggerSpec {
                        condition: vec!["unwrap\\(\\)".into()],
                        ..Default::default()
                    },
                ),
                row(
                    2,
                    LessonTriggerSpec {
                        condition: vec!["unwrap\\(\\)".into()],
                        trigger_scope: vec!["tool:write".into()],
                        ..Default::default()
                    },
                ),
                row(
                    3,
                    LessonTriggerSpec {
                        condition: vec!["unwrap\\(\\)".into()],
                        trigger_scope: vec!["text".into()],
                        ..Default::default()
                    },
                ),
            ],
        );
        assert_eq!(set.triggers().len(), 3);

        let on_edit = match_surfaces(&set, &[tool("edit", "src/a.rs", "x.unwrap()")]);
        let fired: Vec<i64> = on_edit
            .hits
            .iter()
            .map(|hit| set.triggers()[hit.trigger].id)
            .collect();
        assert_eq!(fired, vec![1], "named tool and text scopes must not fire");

        let on_write = match_surfaces(&set, &[tool("write", "src/a.rs", "x.unwrap()")]);
        let fired: Vec<i64> = on_write
            .hits
            .iter()
            .map(|hit| set.triggers()[hit.trigger].id)
            .collect();
        assert_eq!(fired, vec![1, 2]);

        let on_prose = match_surfaces(&set, &[prose("I will just unwrap() it")]);
        let fired: Vec<i64> = on_prose
            .hits
            .iter()
            .map(|hit| set.triggers()[hit.trigger].id)
            .collect();
        assert_eq!(fired, vec![1, 3], "tool-scoped lesson must not read prose");
    }

    #[test]
    fn a_lesson_fires_once_per_call_even_across_many_surfaces() {
        let set = CompiledTriggerSet::compile(
            "f2",
            &[row(
                7,
                LessonTriggerSpec {
                    condition: vec!["TODO".into()],
                    ..Default::default()
                },
            )],
        );
        let outcome = match_surfaces(
            &set,
            &[
                tool("edit", "a.rs", "// TODO one"),
                tool("edit", "b.rs", "// TODO two"),
            ],
        );
        assert_eq!(outcome.hits.len(), 1);
        assert_eq!(outcome.hits[0].path.as_deref(), Some("a.rs"));
    }

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
    fn language_inference_covers_the_v1_set_and_names_what_it_refuses() {
        assert_eq!(ast_language(Some("src/lib.rs")), Ok(SupportLang::Rust));
        assert_eq!(ast_language(Some("a/b/index.ts")), Ok(SupportLang::TypeScript));
        assert_eq!(ast_language(Some("App.TSX")), Ok(SupportLang::Tsx));
        assert_eq!(ast_language(Some("a.js")), Ok(SupportLang::JavaScript));
        assert_eq!(ast_language(Some("a.jsx")), Ok(SupportLang::JavaScript));
        assert_eq!(ast_language(Some("s.py")), Ok(SupportLang::Python));
        let sql = ast_language(Some("migrations/0019.sql")).unwrap_err();
        assert!(sql.contains("no ast grammar"), "{sql}");
        let unknown = ast_language(Some("notes.md")).unwrap_err();
        assert!(unknown.contains("unsupported extension md"), "{unknown}");
        assert!(ast_language(Some("Makefile")).is_err());
        assert!(ast_language(None).is_err());
    }

    #[test]
    fn ast_condition_fires_on_structure_and_ignores_unrelated_code() {
        let set = CompiledTriggerSet::compile(
            "f3",
            &[row(
                11,
                LessonTriggerSpec {
                    ast_condition: vec!["$A.unwrap()".into()],
                    ..Default::default()
                },
            )],
        );
        let fire = match_surfaces(&set, &[tool("edit", "src/a.rs", "fn f() { g().unwrap(); }")]);
        assert_eq!(fire.hits.len(), 1);
        assert_eq!(fire.hits[0].pattern_kind, PatternKind::Ast);
        assert_eq!(fire.hits[0].pattern, "$A.unwrap()");

        let quiet = match_surfaces(
            &set,
            &[tool("edit", "src/a.rs", "fn f() { let _ = g()?; }")],
        );
        assert!(quiet.hits.is_empty());
        assert!(quiet.warnings.is_empty());
    }

    #[test]
    fn ast_conditions_never_read_prose_and_skip_grammarless_paths_with_a_warning() {
        let set = CompiledTriggerSet::compile(
            "f4",
            &[row(
                12,
                LessonTriggerSpec {
                    ast_condition: vec!["$A.unwrap()".into()],
                    trigger_scope: vec!["text".into(), "tool".into()],
                    ..Default::default()
                },
            )],
        );
        let on_prose = match_surfaces(&set, &[prose("g().unwrap()")]);
        assert!(on_prose.hits.is_empty());
        assert!(on_prose.warnings.is_empty(), "prose is not a skipped parse");

        let on_sql = match_surfaces(&set, &[tool("write", "db/0019.sql", "SELECT unwrap();")]);
        assert!(on_sql.hits.is_empty());
        assert_eq!(on_sql.warnings.len(), 1);
        assert!(on_sql.warnings[0].contains("0019.sql"));
    }

    #[test]
    fn write_validation_refuses_what_can_never_fire() {
        let good = LessonTriggerSpec {
            condition: vec!["\\bunwrap\\(\\)".into()],
            ast_condition: vec!["$A.unwrap()".into()],
            trigger_scope: vec!["tool:edit".into(), "text".into()],
            interrupt_mode: Some("remind".into()),
            repeat_cooldown_secs: Some(600),
        };
        assert_eq!(good.validate(), Ok(()));

        let bad_regex = LessonTriggerSpec {
            condition: vec!["unwrap(".into()],
            ..Default::default()
        };
        assert!(
            bad_regex
                .validate()
                .unwrap_err()
                .starts_with("condition is not a valid regex:")
        );

        let bad_ast = LessonTriggerSpec {
            ast_condition: vec!["((((".into()],
            ..Default::default()
        };
        assert!(
            bad_ast
                .validate()
                .unwrap_err()
                .starts_with("astCondition does not parse for any supported language:")
        );

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

    #[test]
    fn compiling_drops_a_broken_pattern_and_names_it_instead_of_failing_the_room() {
        let set = CompiledTriggerSet::compile(
            "f5",
            &[row(
                13,
                LessonTriggerSpec {
                    condition: vec!["unwrap(".into(), "expect\\(".into()],
                    ..Default::default()
                },
            )],
        );
        assert_eq!(set.triggers().len(), 1);
        assert_eq!(set.warnings.len(), 1);
        let outcome = match_surfaces(&set, &[tool("edit", "a.rs", "x.expect(\"y\")")]);
        assert_eq!(outcome.hits.len(), 1);
        assert_eq!(outcome.warnings.len(), 1);
    }

    #[test]
    fn the_cache_answers_only_its_own_fingerprint() {
        let room = "kodo-lesson-trigger-cache-test";
        assert!(cached_set(room, "v1").is_none());
        store_set(
            room,
            CompiledTriggerSet::compile(
                "v1",
                &[row(
                    21,
                    LessonTriggerSpec {
                        condition: vec!["x".into()],
                        ..Default::default()
                    },
                )],
            ),
        );
        assert!(cached_set(room, "v1").is_some());
        assert!(
            cached_set(room, "v2").is_none(),
            "a changed fingerprint must recompile"
        );
    }
}
