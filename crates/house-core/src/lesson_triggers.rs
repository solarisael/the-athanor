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

/// One file extension the House knows: the language slug a lesson's
/// `language_keys` names, and the grammar an ast pattern parses under.
struct ExtensionLanguage {
    ext: &'static str,
    slug: &'static str,
    /// `None` when regex is the only matcher available for this extension.
    grammar: Option<SupportLang>,
}

// enough: one table, so the ast grammar and the language fence can never
// drift. Grammarless rows (`grammar: None`) fence regex conditions only —
// ast-grep-language 0.45.1 ships no parser for them; ast conditions skip
// with a warning. The slug column speaks the lesson registry's live
// `language_keys` vocabulary. Upgrade path: enable another `tree-sitter-*`
// feature in crates/house-core/Cargo.toml and fill in the grammar, or add a
// row when the registry grows a new slug.
const EXTENSION_LANGUAGES: [ExtensionLanguage; 19] = [
    ExtensionLanguage {
        ext: "rs",
        slug: "rust",
        grammar: Some(SupportLang::Rust),
    },
    ExtensionLanguage {
        ext: "ts",
        slug: "typescript",
        grammar: Some(SupportLang::TypeScript),
    },
    ExtensionLanguage {
        ext: "tsx",
        slug: "typescript",
        grammar: Some(SupportLang::Tsx),
    },
    ExtensionLanguage {
        ext: "js",
        slug: "javascript",
        grammar: Some(SupportLang::JavaScript),
    },
    ExtensionLanguage {
        ext: "jsx",
        slug: "javascript",
        grammar: Some(SupportLang::JavaScript),
    },
    ExtensionLanguage {
        ext: "py",
        slug: "python",
        grammar: Some(SupportLang::Python),
    },
    ExtensionLanguage {
        ext: "sql",
        slug: "sql",
        grammar: None,
    },
    ExtensionLanguage {
        ext: "go",
        slug: "go",
        grammar: None,
    },
    ExtensionLanguage {
        ext: "css",
        slug: "css",
        grammar: None,
    },
    ExtensionLanguage {
        ext: "scss",
        slug: "css",
        grammar: None,
    },
    ExtensionLanguage {
        ext: "html",
        slug: "html",
        grammar: None,
    },
    ExtensionLanguage {
        ext: "htm",
        slug: "html",
        grammar: None,
    },
    ExtensionLanguage {
        ext: "jinja",
        slug: "html",
        grammar: None,
    },
    ExtensionLanguage {
        ext: "j2",
        slug: "html",
        grammar: None,
    },
    ExtensionLanguage {
        ext: "gd",
        slug: "gdscript",
        grammar: None,
    },
    ExtensionLanguage {
        ext: "glsl",
        slug: "glsl",
        grammar: None,
    },
    ExtensionLanguage {
        ext: "md",
        slug: "markdown",
        grammar: None,
    },
    ExtensionLanguage {
        ext: "php",
        slug: "php",
        grammar: None,
    },
    ExtensionLanguage {
        ext: "lean",
        slug: "lean",
        grammar: None,
    },
];

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
                return Err(format!(
                    "condition is not a valid regex: {pattern}: {error}"
                ));
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
    EXTENSION_LANGUAGES
        .iter()
        .filter_map(|entry| entry.grammar)
        .any(|lang| Pattern::try_new(pattern, lang).is_ok_and(|parsed| !parsed.has_error()))
}

/// The extension of a surface path, empty when the file name carries none.
fn path_extension(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .rsplit_once('.')
        .map_or("", |(_, extension)| extension)
}

fn extension_language(extension: &str) -> Option<&'static ExtensionLanguage> {
    EXTENSION_LANGUAGES
        .iter()
        .find(|entry| entry.ext.eq_ignore_ascii_case(extension))
}

/// The language a surface path is written in, as a lesson's `language_keys`
/// names it. The fence and [`ast_language`] read the same table, so they can
/// never disagree about what a `.tsx` file is.
pub fn language_slug(path: Option<&str>) -> Option<&'static str> {
    let path = path.map(str::trim).filter(|value| !value.is_empty())?;
    extension_language(path_extension(path)).map(|entry| entry.slug)
}

/// The grammar for a surface path, or the warning that says why there is none.
pub fn ast_language(path: Option<&str>) -> Result<SupportLang, String> {
    let Some(path) = path.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err("ast conditions skipped: surface carries no path".to_owned());
    };
    let extension = path_extension(path);
    if extension.is_empty() {
        return Err(format!("ast conditions skipped for {path}: no extension"));
    }
    match extension_language(extension) {
        Some(entry) => entry.grammar.ok_or_else(|| {
            format!(
                "ast conditions skipped for {path}: {extension} has no ast grammar in ast-grep-language 0.45.1; regex conditions still apply"
            )
        }),
        None => Err(format!(
            "ast conditions skipped for {path}: unsupported extension {extension}"
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
    /// Lowercase language slugs; empty means the lesson watches every surface.
    language_keys: Vec<String>,
    regexes: Vec<(String, Regex)>,
    ast_patterns: Vec<String>,
}

impl CompiledTrigger {
    fn admits(&self, surface: &Surface<'_>) -> bool {
        if !self.language_keys.is_empty() && !self.language_admits(surface) {
            return false;
        }
        if self.scopes.is_empty() {
            // Empty scope: regex watches prose and tool text alike, ast watches
            // tool surfaces only. The ast half is enforced where it parses.
            return true;
        }
        self.scopes.iter().any(|scope| scope.admits(surface))
    }

    /// A language-keyed lesson watches source, not talk: it needs a tool
    /// surface whose path names one of the languages it was written for.
    fn language_admits(&self, surface: &Surface<'_>) -> bool {
        surface.kind == SurfaceKind::Tool
            && language_slug(surface.path)
                .is_some_and(|slug| self.language_keys.iter().any(|key| key == slug))
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
            let language_keys: Vec<String> = row
                .spec
                .language_keys
                .iter()
                .filter_map(|key| {
                    let key = key.trim().to_ascii_lowercase();
                    (!key.is_empty()).then_some(key)
                })
                .collect();
            if regexes.is_empty() && row.spec.ast_condition.is_empty() {
                continue;
            }
            if !language_keys.is_empty() && scopes.contains(&ScopeToken::Text) {
                warnings.push(format!(
                    "{}#{}: trigger_scope includes text but language_keys are set; prose carries no path, so only this lesson's tool scopes can fire",
                    row.family, row.id
                ));
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
                language_keys,
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
    pub surface_index: usize,
    pub surface: SurfaceKind,
    pub tool: Option<String>,
    pub path: Option<String>,
    pub pattern_kind: PatternKind,
    pub pattern: String,
    pub match_start: Option<usize>,
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
    for (surface_index, surface) in surfaces.iter().enumerate() {
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
            if let Some((pattern, matched)) = trigger.regexes.iter().find_map(|(pattern, regex)| {
                regex.find(surface.text).map(|matched| (pattern, matched))
            }) {
                fired[index] = true;
                outcome.hits.push(TriggerHit {
                    trigger: index,
                    surface_index,
                    surface: surface.kind,
                    tool: surface.tool.map(str::to_owned),
                    path: surface.path.map(str::to_owned),
                    pattern_kind: PatternKind::Regex,
                    pattern: pattern.clone(),
                    match_start: Some(matched.start()),
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
                        surface_index,
                        surface: surface.kind,
                        tool: surface.tool.map(str::to_owned),
                        path: surface.path.map(str::to_owned),
                        pattern_kind: PatternKind::Ast,
                        pattern: pattern.clone(),
                        match_start: None,
                    });
                    break;
                }
            }
        }
    }
    outcome
}

// enough: the compile cache is one entry per caller-built key (room, or
// room\0project when the caller stands in one), replaced whenever that
// fence's fingerprint (trigger-bearing row count + max updated_at) changes.
// It is bounded by rooms × active projects a substrate process serves.
// Upgrade path: an LRU with a size ceiling if a host ever serves unbounded
// fences.
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
    fn regex_hit_records_surface_index_and_byte_offset() {
        let set = CompiledTriggerSet::compile(
            "f2-offsets",
            &[row(
                8,
                LessonTriggerSpec {
                    condition: vec!["TODO".into()],
                    ..Default::default()
                },
            )],
        );
        let outcome = match_surfaces(
            &set,
            &[
                tool("edit", "a.rs", "nothing here"),
                tool("edit", "b.rs", "π TODO"),
            ],
        );

        assert_eq!(outcome.hits.len(), 1);
        assert_eq!(outcome.hits[0].surface_index, 1);
        assert_eq!(outcome.hits[0].match_start, Some(3));
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
        assert_eq!(
            ast_language(Some("a/b/index.ts")),
            Ok(SupportLang::TypeScript)
        );
        assert_eq!(ast_language(Some("App.TSX")), Ok(SupportLang::Tsx));
        assert_eq!(ast_language(Some("a.js")), Ok(SupportLang::JavaScript));
        assert_eq!(ast_language(Some("a.jsx")), Ok(SupportLang::JavaScript));
        assert_eq!(ast_language(Some("s.py")), Ok(SupportLang::Python));
        let sql = ast_language(Some("migrations/0019.sql")).unwrap_err();
        assert!(sql.contains("no ast grammar"), "{sql}");
        let markdown = ast_language(Some("notes.md")).unwrap_err();
        assert!(markdown.contains("no ast grammar"), "{markdown}");
        let unknown = ast_language(Some("data.toml")).unwrap_err();
        assert!(unknown.contains("unsupported extension toml"), "{unknown}");
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
        let fire = match_surfaces(
            &set,
            &[tool("edit", "src/a.rs", "fn f() { g().unwrap(); }")],
        );
        assert_eq!(fire.hits.len(), 1);
        assert_eq!(fire.hits[0].pattern_kind, PatternKind::Ast);
        assert_eq!(fire.hits[0].pattern, "$A.unwrap()");
        assert_eq!(fire.hits[0].match_start, None);

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
            language_keys: vec![],
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

    #[test]
    fn language_keys_admit_only_their_own_language_and_leave_unkeyed_lessons_alone() {
        let set = CompiledTriggerSet::compile(
            "f6",
            &[
                row(
                    31,
                    LessonTriggerSpec {
                        condition: vec!["print\\(".into()],
                        language_keys: vec!["python".into()],
                        ..Default::default()
                    },
                ),
                row(
                    32,
                    LessonTriggerSpec {
                        condition: vec!["print\\(".into()],
                        ..Default::default()
                    },
                ),
                row(
                    33,
                    LessonTriggerSpec {
                        condition: vec!["print\\(".into()],
                        language_keys: vec!["sql".into()],
                        ..Default::default()
                    },
                ),
            ],
        );
        let fired = |surface: Surface<'_>| -> Vec<i64> {
            match_surfaces(&set, &[surface])
                .hits
                .iter()
                .map(|hit| set.triggers()[hit.trigger].id)
                .collect()
        };

        assert_eq!(
            fired(tool("edit", "src/a.rs", "print(")),
            vec![32],
            "a python-keyed lesson must not read rust"
        );
        assert_eq!(fired(tool("edit", "app.py", "print(")), vec![31, 32]);
        assert_eq!(
            fired(tool("write", "db/0019.sql", "print(")),
            vec![32, 33],
            "sql has no ast grammar but stays regex-capable behind the fence"
        );
        assert_eq!(
            fired(tool("edit", "notes.md", "print(")),
            vec![32],
            "an extension outside the map admits no keyed lesson"
        );
        assert_eq!(
            fired(Surface {
                kind: SurfaceKind::Tool,
                tool: Some("bash"),
                path: None,
                text: "print(",
            }),
            vec![32],
            "a pathless tool surface names no language"
        );
        assert_eq!(
            fired(prose("I will print( it")),
            vec![32],
            "a keyed lesson never watches prose"
        );
    }

    #[test]
    fn a_language_keyed_lesson_watching_text_is_named_as_a_contradiction() {
        let set = CompiledTriggerSet::compile(
            "f7",
            &[
                row(
                    34,
                    LessonTriggerSpec {
                        condition: vec!["print\\(".into()],
                        trigger_scope: vec!["text".into(), "tool:edit".into()],
                        language_keys: vec!["Python".into()],
                        ..Default::default()
                    },
                ),
                row(
                    35,
                    LessonTriggerSpec {
                        condition: vec!["print\\(".into()],
                        trigger_scope: vec!["tool:edit".into()],
                        language_keys: vec!["python".into()],
                        ..Default::default()
                    },
                ),
            ],
        );
        assert_eq!(set.triggers().len(), 2, "the contradiction still compiles");
        assert_eq!(set.warnings.len(), 1);
        assert!(
            set.warnings[0].starts_with("coding#34:")
                && set.warnings[0].contains("text")
                && set.warnings[0].contains("language_keys"),
            "{:?}",
            set.warnings
        );

        let on_prose = match_surfaces(&set, &[prose("print(")]);
        assert!(on_prose.hits.is_empty(), "prose carries no language");
        let on_python = match_surfaces(&set, &[tool("edit", "app.py", "print(")]);
        let ids: Vec<i64> = on_python
            .hits
            .iter()
            .map(|hit| set.triggers()[hit.trigger].id)
            .collect();
        assert_eq!(
            ids,
            vec![34, 35],
            "the tool scopes still fire; keys fold case"
        );
    }

    #[test]
    fn the_slug_map_and_the_ast_grammars_are_one_table() {
        assert_eq!(language_slug(Some("src/lib.rs")), Some("rust"));
        assert_eq!(language_slug(Some("a/b/index.ts")), Some("typescript"));
        assert_eq!(language_slug(Some("App.TSX")), Some("typescript"));
        assert_eq!(language_slug(Some("a.js")), Some("javascript"));
        assert_eq!(language_slug(Some("a.jsx")), Some("javascript"));
        assert_eq!(language_slug(Some("s.py")), Some("python"));
        assert_eq!(language_slug(Some("db/0019.sql")), Some("sql"));
        assert_eq!(language_slug(Some("main.go")), Some("go"));
        assert_eq!(language_slug(Some("styles/app.CSS")), Some("css"));
        assert_eq!(language_slug(Some("page.jinja")), Some("html"));
        assert_eq!(language_slug(Some("notes.md")), Some("markdown"));
        assert_eq!(language_slug(Some("data.toml")), None);
        assert_eq!(language_slug(Some("Makefile")), None);
        assert_eq!(language_slug(None), None);

        // Drift guard: one table answers both questions, so every extension the
        // ast side knows carries a fence slug and vice versa.
        for entry in EXTENSION_LANGUAGES {
            let path = format!("probe.{}", entry.ext);
            assert_eq!(language_slug(Some(&path)), Some(entry.slug));
            match entry.grammar {
                Some(grammar) => assert_eq!(ast_language(Some(&path)), Ok(grammar)),
                None => assert!(ast_language(Some(&path)).is_err(), "{path}"),
            }
        }
    }
}
