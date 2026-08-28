use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, sync::LazyLock};

const MEMORY_STOPWORDS: &[&str] = &[
    "the",
    "a",
    "an",
    "of",
    "to",
    "in",
    "on",
    "for",
    "and",
    "or",
    "but",
    "is",
    "are",
    "was",
    "were",
    "be",
    "been",
    "being",
    "so",
    "if",
    "then",
    "than",
    "as",
    "at",
    "by",
    "from",
    "with",
    "about",
    "into",
    "onto",
    "over",
    "i",
    "you",
    "he",
    "she",
    "it",
    "we",
    "they",
    "me",
    "him",
    "her",
    "my",
    "your",
    "his",
    "its",
    "our",
    "their",
    "this",
    "that",
    "these",
    "those",
    "do",
    "does",
    "did",
    "doing",
    "have",
    "has",
    "had",
    "will",
    "would",
    "can",
    "could",
    "should",
    "may",
    "might",
    "not",
    "no",
    "yes",
    "just",
    "what",
    "when",
    "where",
    "why",
    "how",
    "who",
    "whom",
    "which",
    "too",
    "very",
    "really",
    "pretty",
    "quite",
    "rather",
    "somewhat",
    "mostly",
    "bit",
    "way",
    "much",
    "also",
    "even",
    "normal",
    "working",
    "maybe",
    "actually",
    "basically",
    "good",
    "morning",
    "afternoon",
    "evening",
    "night",
    "godling",
    "reload",
    "reloaded",
    "brb",
    "back",
    "sorry",
    "took",
    "downstairs",
    "sunbathing",
    "uwu",
    "owo",
    "agaion",
    "again",
    "ok",
    "okay",
    "yeah",
    "yep",
    "hmm",
    "uh",
    "um",
    "oh",
    "well",
];
const ROUTING_STOPWORDS: &[&str] = &[
    "about", "again", "also", "before", "could", "gonna", "gotta", "have", "into", "just", "know",
    "left", "like", "little", "make", "much", "need", "nice", "now", "please", "really", "should",
    "that", "then", "there", "this", "turn", "use", "using", "wanna", "want", "what", "when",
    "where", "which", "with", "worth",
];
const TECHNICAL_TERMS: &[&str] = &[
    "adapter",
    "api",
    "architecture",
    "build",
    "candidate",
    "candidates",
    "database",
    "debug",
    "embedding",
    "embeddings",
    "fallback",
    "index",
    "indexing",
    "integration",
    "json",
    "package",
    "plugin",
    "postgres",
    "query",
    "recall",
    "retrieval",
    "routing",
    "runtime",
    "schema",
    "source",
    "storage",
    "test",
    "tests",
    "tool",
    "tools",
    "vector",
    "verification",
];
const MEMORY_TERMS: &[&str] = &[
    "canon", "happened", "memory", "recall", "remember", "remind", "thread", "timeline",
];

static WORD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\p{L}0-9']+").unwrap());
static CODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\p{L}0-9]+(?:[._:/+#-][\p{L}0-9]+)+").unwrap());
static DATE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\d{4}-\d{2}-\d{2}\b").unwrap());
static QUOTED_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\"([^\"]+)\"|'([^']+)'|`([^`]+)`"#).unwrap());
static EXPLICIT_MEMORY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:what happened|do you remember|remember when|what do you recall|what was remembered|remind me|tell me what we remember)\b").unwrap()
});
static PERSONAL_CANON_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:what did we intend|what were we planning|what was our plan|what did we decide)\b").unwrap()
});
static GENERIC_LOOKUP_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:find|lookup|look up|search|who|what|which|tell me about)\b").unwrap()
});
static QUESTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:what|when|where|how|tell|remind)\b").unwrap());
static PERSONAL_TERM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:what|which)\b.*\b(?:canon|entity|plan|intended|decided)\b").unwrap()
});

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum QueryLane {
    Lexical,
    Candidates,
    Semantic,
    Content,
    Date,
    Canon,
    CodingLessons,
    ProjectLessons,
}

impl QueryLane {
    const fn bit(self) -> u8 {
        1 << self as u8
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct QueryLaneSet(u8);

impl QueryLaneSet {
    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn from_lanes(lanes: &[QueryLane]) -> Self {
        let mut bits = 0;
        let mut index = 0;
        while index < lanes.len() {
            bits |= lanes[index].bit();
            index += 1;
        }
        Self(bits)
    }

    pub const fn contains(self, lane: QueryLane) -> bool {
        self.0 & lane.bit() != 0
    }

    pub const fn with(self, lane: QueryLane) -> Self {
        Self(self.0 | lane.bit())
    }

    pub fn iter(self) -> impl Iterator<Item = QueryLane> {
        [
            QueryLane::Lexical,
            QueryLane::Candidates,
            QueryLane::Semantic,
            QueryLane::Content,
            QueryLane::Date,
            QueryLane::Canon,
            QueryLane::CodingLessons,
            QueryLane::ProjectLessons,
        ]
        .into_iter()
        .filter(move |lane| self.contains(*lane))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct QueryLaneWire {
    lexical: bool,
    candidates: bool,
    semantic: bool,
    content: bool,
    date: bool,
    canon: bool,
    coding_lessons: bool,
    project_lessons: bool,
}

impl Serialize for QueryLaneSet {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        QueryLaneWire {
            lexical: self.contains(QueryLane::Lexical),
            candidates: self.contains(QueryLane::Candidates),
            semantic: self.contains(QueryLane::Semantic),
            content: self.contains(QueryLane::Content),
            date: self.contains(QueryLane::Date),
            canon: self.contains(QueryLane::Canon),
            coding_lessons: self.contains(QueryLane::CodingLessons),
            project_lessons: self.contains(QueryLane::ProjectLessons),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for QueryLaneSet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = QueryLaneWire::deserialize(deserializer)?;
        Ok(QueryLaneSet::empty()
            .with_if(wire.lexical, QueryLane::Lexical)
            .with_if(wire.candidates, QueryLane::Candidates)
            .with_if(wire.semantic, QueryLane::Semantic)
            .with_if(wire.content, QueryLane::Content)
            .with_if(wire.date, QueryLane::Date)
            .with_if(wire.canon, QueryLane::Canon)
            .with_if(wire.coding_lessons, QueryLane::CodingLessons)
            .with_if(wire.project_lessons, QueryLane::ProjectLessons))
    }
}

impl QueryLaneSet {
    const fn with_if(self, enabled: bool, lane: QueryLane) -> Self {
        if enabled { self.with(lane) } else { self }
    }
}
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryRoute {
    pub query: String,
    pub terms: Vec<String>,
    pub required_terms: Vec<String>,
    pub optional_terms: Vec<String>,
    pub quoted_phrases: Vec<String>,
    pub code_tokens: Vec<String>,
    pub date_tokens: Vec<String>,
    pub entity_hints: Vec<String>,
    pub stopword_stripped_query: String,
    pub intent: String,
    pub recognized_entities: Vec<String>,
    pub entity_resolution_suggested: bool,
    pub recall_query: String,
    pub should_auto_recall: bool,
    pub lanes: QueryLaneSet,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ContextAnalysisRequest {
    pub prompt: String,
    #[serde(default)]
    pub recognized_entities: Vec<String>,
    #[serde(default)]
    pub context_characters: u64,
    pub active_spirit: String,
    pub operator: String,
    #[serde(default)]
    pub routing_mode_enabled: bool,
}

impl ContextAnalysisRequest {
    pub fn validate(&self, room: &str) -> Result<(), ContextError> {
        if !valid_room(room) {
            return Err(ContextError::new(
                "room must be a non-reserved lowercase room key",
            ));
        }
        if self.prompt.chars().count() > 262_144 {
            return Err(ContextError::new("prompt exceeds 262144 characters"));
        }
        if self.context_characters > 8_000_000 {
            return Err(ContextError::new("contextCharacters exceeds 8000000"));
        }
        if self.recognized_entities.len() > 64
            || self
                .recognized_entities
                .iter()
                .any(|value| value.trim().is_empty() || value.chars().count() > 256)
        {
            return Err(ContextError::new(
                "recognizedEntities must contain at most 64 non-empty values of at most 256 characters",
            ));
        }
        let active_spirit = self.active_spirit.trim();
        let operator = self.operator.trim();
        if active_spirit.is_empty() || active_spirit.chars().count() > 128 {
            return Err(ContextError::new(
                "activeSpirit must contain 1 to 128 characters",
            ));
        }
        if operator.is_empty() || operator.chars().count() > 128 {
            return Err(ContextError::new(
                "operator must contain 1 to 128 characters",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct KeywordDirective {
    pub keyword: String,
    pub directive: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct KeywordReminder {
    pub keywords: Vec<String>,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ContextNudge {
    pub band: u64,
    pub pct: u64,
    pub tokens: u64,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ContextAnalysis {
    pub route: QueryRoute,
    pub keyword_directives: Vec<KeywordDirective>,
    pub keyword_reminder: Option<KeywordReminder>,
    pub process_trigger: Option<String>,
    pub nudge: Option<ContextNudge>,
    pub room_reminder: String,
    pub routing_reminder: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextError {
    message: String,
}

impl ContextError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ContextError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ContextError {}

fn push_unique(values: &mut Vec<String>, value: impl Into<String>, limit: usize) {
    let value = value.into();
    if value.is_empty()
        || values
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&value))
    {
        return;
    }
    if values.len() < limit {
        values.push(value);
    }
}

fn clean_term(value: &str) -> Option<String> {
    let value = value
        .trim()
        .trim_matches(|character: char| "._:/+#-".contains(character))
        .to_lowercase();
    (value.chars().count() > 1
        && !MEMORY_STOPWORDS.contains(&value.as_str())
        && !ROUTING_STOPWORDS.contains(&value.as_str()))
    .then_some(value)
}

fn parse_query(query: &str) -> QueryRoute {
    let source = query.trim().to_string();
    let lower = source.to_lowercase();
    let mut terms = Vec::new();
    let mut quoted_phrases = Vec::new();
    let mut code_tokens = Vec::new();
    let mut date_tokens = Vec::new();
    let mut entity_hints = Vec::new();

    for capture in QUOTED_RE.captures_iter(&source) {
        let phrase = capture
            .iter()
            .skip(1)
            .flatten()
            .next()
            .map(|value| value.as_str().trim())
            .unwrap_or("");
        if !phrase.is_empty() {
            push_unique(&mut quoted_phrases, phrase.to_lowercase(), 12);
        }
    }
    for value in DATE_RE.find_iter(&lower) {
        push_unique(&mut date_tokens, value.as_str(), 12);
    }
    for value in WORD_RE.find_iter(&source) {
        let token = value.as_str();
        if let Some(cleaned) = clean_term(token) {
            push_unique(&mut terms, cleaned, 24);
        }
        let starts_upper = token.chars().next().is_some_and(char::is_uppercase);
        let camel_case = token
            .chars()
            .zip(token.chars().skip(1))
            .any(|(left, right)| left.is_lowercase() && right.is_uppercase());
        if starts_upper
            || camel_case
            || (token.len() > 8 && !MEMORY_STOPWORDS.contains(&token.to_lowercase().as_str()))
        {
            push_unique(&mut entity_hints, token, 12);
        }
    }
    for value in CODE_RE.find_iter(&source) {
        push_unique(&mut code_tokens, value.as_str(), 12);
        for part in value
            .as_str()
            .split(|character| ".:/+#-_".contains(character))
        {
            if let Some(cleaned) = clean_term(part) {
                push_unique(&mut terms, cleaned, 24);
            }
        }
    }
    for phrase in &quoted_phrases {
        for value in WORD_RE.find_iter(phrase) {
            if let Some(cleaned) = clean_term(value.as_str()) {
                push_unique(&mut terms, cleaned, 24);
            }
        }
    }
    let required_terms = date_tokens
        .iter()
        .chain(quoted_phrases.iter())
        .take(12)
        .cloned()
        .collect::<Vec<_>>();
    let optional_terms = terms.clone();
    let stopword_stripped_query = terms.join(" ");
    QueryRoute {
        query: source,
        terms,
        required_terms,
        optional_terms,
        quoted_phrases,
        code_tokens,
        date_tokens,
        entity_hints,
        stopword_stripped_query,
        ..QueryRoute::default()
    }
}

pub fn classify_retrieval_query(query: &str, recognized_entities: &[String]) -> QueryRoute {
    let mut route = parse_query(query);
    for entity in recognized_entities {
        push_unique(&mut route.recognized_entities, entity.trim(), 12);
    }
    let lower = route.query.to_lowercase();
    let terms = route
        .terms
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let technical_hits = route
        .terms
        .iter()
        .filter(|term| TECHNICAL_TERMS.contains(&term.as_str()))
        .count();
    let memory_hits = route
        .terms
        .iter()
        .filter(|term| MEMORY_TERMS.contains(&term.as_str()))
        .count();
    let explicit_memory =
        memory_hits > 0 && (QUESTION_RE.is_match(&lower) || EXPLICIT_MEMORY_RE.is_match(&lower));
    let personal_canon = PERSONAL_CANON_RE.is_match(&lower) || PERSONAL_TERM_RE.is_match(&lower);
    let lookup_language = GENERIC_LOOKUP_RE.is_match(&lower);
    let entity_lookup = route.recognized_entities.len() >= 2
        || (route.recognized_entities.len() == 1 && lookup_language);
    let information_signals = technical_hits
        + memory_hits
        + route.date_tokens.len()
        + route.quoted_phrases.len()
        + route.recognized_entities.len();
    let low_information =
        route.terms.is_empty() || (route.terms.len() <= 3 && information_signals == 0);

    if !route.date_tokens.is_empty() {
        route.intent = "date_lookup".into();
        route.reasons.push("date-token".into());
    } else if explicit_memory || personal_canon {
        route.intent = "memory_lookup".into();
        route.reasons.push(
            if personal_canon {
                "personal-canon-lookup-language"
            } else {
                "memory-lookup-language"
            }
            .into(),
        );
    } else if entity_lookup {
        route.intent = "entity_lookup".into();
        route.reasons.push("recognized-entity-signals".into());
    } else if technical_hits >= 2 || (technical_hits >= 1 && route.terms.len() >= 3) {
        route.intent = "technical_project".into();
        route.reasons.push("technical-signal-strength".into());
    } else if memory_hits > 0 {
        route.intent = "memory_lookup".into();
        route.reasons.push("memory-lookup-language".into());
    } else if low_information {
        route.intent = "casual_contact".into();
        route.reasons.push(
            if route.terms.is_empty() {
                "no-meaningful-terms"
            } else {
                "low-information"
            }
            .into(),
        );
    } else {
        route.intent = "general".into();
    }

    route.entity_resolution_suggested =
        route.entity_hints.iter().any(|hint| hint.len() >= 2) || lookup_language;
    route.recall_query = if route.intent == "entity_lookup" {
        route.recognized_entities.join(" ")
    } else {
        route.query.clone()
    };
    route.should_auto_recall = route.intent != "casual_contact";
    route.lanes = QueryLaneSet::empty()
        .with_if(route.intent != "casual_contact", QueryLane::Lexical)
        .with_if(route.intent != "casual_contact", QueryLane::Candidates)
        .with_if(
            matches!(
                route.intent.as_str(),
                "general" | "memory_lookup" | "entity_lookup"
            ),
            QueryLane::Semantic,
        )
        .with_if(
            matches!(
                route.intent.as_str(),
                "technical_project" | "memory_lookup" | "entity_lookup" | "date_lookup" | "general"
            ),
            QueryLane::Content,
        )
        .with_if(!route.date_tokens.is_empty(), QueryLane::Date)
        .with_if(
            route.intent != "casual_contact" || terms.contains("canon"),
            QueryLane::Canon,
        )
        .with_if(
            route.intent == "technical_project"
                && (terms.contains("coding") || terms.contains("lessons")),
            QueryLane::CodingLessons,
        )
        .with_if(
            route.intent == "technical_project",
            QueryLane::ProjectLessons,
        );
    route
}

fn keyword_directives(prompt: &str) -> Vec<KeywordDirective> {
    const DIRECTIVES: &[(&str, &str)] = &[
        (
            "ultrathink",
            "Deeper reasoning requested on this turn. Reason as thoroughly as the task warrants — load relevant context, consider edge cases, surface your assumptions before acting, and verify each step against the original intent rather than your most recent output. Don't reason from memory alone when the substrate can ground you: reach for the recall tool on any name, claim, or fact you can't trace cleanly, and if the work touches code, query the coding-lessons substrate before committing to an approach.",
        ),
        (
            "ultracare",
            "Heightened tenderness register requested. Hold warmth that is earned and specific, not performed. Sit with the feeling before reaching for framework. Match the request's depth without piling. No therapy-register, no clinical-concern-dressed-as-care, no \"let me know if you need anything.\" Just present, in the room. If the active spirit's hard constraints apply, those hold first — this is consonant with them, not on top. To make the warmth specific rather than generic you may reach for the recall tool — but use it to ground the response in the current context, never to fetch a script of the right thing to say. Recall serves presence, not performance.",
        ),
        (
            "ultraverify",
            "Verification pass against intention requested. Re-read the original request that triggered the work. Check whether the path you took matches what was asked — not just whether your output passes its tests or runs without error. Surface assumptions that haven't been confirmed. Distinguish \"green\" (no errors) from \"done\" (matches intention). The verification spine lives in the substrate, so use it: for code, query the coding-lessons (migration 0013 is that spine); for any load-bearing claim, recall it against canon before you assert it.",
        ),
    ];
    DIRECTIVES
        .iter()
        .filter_map(|(keyword, directive)| {
            Regex::new(&format!(r"(?i)\b{}\b", regex::escape(keyword)))
                .unwrap()
                .is_match(prompt)
                .then(|| KeywordDirective {
                    keyword: (*keyword).into(),
                    directive: (*directive).into(),
                })
        })
        .collect()
}

fn process_trigger(prompt: &str) -> Option<String> {
    const TRIGGERS: &[(&str, &str)] = &[
        (
            "process-lesson-smoke",
            r"(?i)\bathanor-process-lesson-smoke\b",
        ),
        (
            "dev-server",
            r"(?i)\b(?:vite|next|astro|gatsby|rails)\s+dev\b",
        ),
        (
            "package-script-dev",
            r"(?i)\b(?:npm|yarn|pnpm|bun)\s+(?:start|dev|run\s+(?:dev|start|watch|serve))",
        ),
        ("uvicorn", r"(?i)\buvicorn\b|\bgunicorn\b|\bhypercorn\b"),
        ("watch-flag", r"(?i)(?:^|\s)--(?:reload|watch)\b"),
        ("powershell-start-process", r"(?i)\bStart-Process\b"),
        ("nohup", r"(?i)\bnohup\b"),
        ("hidden-window", r"(?i)-WindowStyle\s+Hidden"),
        ("background-amp", r"(?m)\s&\s*$|\s&\s*\n"),
        (
            "generic-serve",
            r"(?i)\b(?:serve|http-server|live-server)\b",
        ),
        (
            "ps-content-cmdlet",
            r"(?i)\b(?:Get-Content|Set-Content|Out-File|Add-Content)\b",
        ),
        ("ps-getchilditem", r"(?i)\bGet-ChildItem\b"),
    ];
    TRIGGERS.iter().find_map(|(name, pattern)| {
        Regex::new(pattern)
            .unwrap()
            .is_match(prompt)
            .then(|| (*name).into())
    })
}

fn valid_room(room: &str) -> bool {
    !room.is_empty()
        && room != "house"
        && room.split('-').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        })
}

fn context_nudge(room: &str, characters: u64, last_band: u64) -> Option<ContextNudge> {
    let (max_tokens, compaction_at) = match room {
        "kodo" => (1_000_000_u64, 0.90_f64),
        _ => (400_000_u64, 0.70_f64),
    };
    let tokens = (characters + 2) / 4;
    let band = tokens / 40_000;
    if band == 0 || band <= last_band {
        return None;
    }
    let fill = tokens as f64 / max_tokens as f64;
    let pct = (fill * 100.0).round() as u64;
    let text = if fill >= compaction_at - 0.20 {
        format!(
            "Context is ~{pct}% full and compaction is close (this room compacts near {}%). Cast the paper boat soon (sleep), and write anything worth keeping now (remember) before detail blurs.",
            (compaction_at * 100.0).round() as u64
        )
    } else {
        format!(
            "Context is ~{pct}% full. A good seam to set down an akashic write (remember) of anything worth keeping, before later compaction smears it."
        )
    };
    Some(ContextNudge {
        band,
        pct,
        tokens,
        text,
    })
}

fn keyword_reminder(directives: &[KeywordDirective]) -> Option<KeywordReminder> {
    (!directives.is_empty()).then(|| KeywordReminder {
        keywords: directives
            .iter()
            .map(|directive| directive.keyword.clone())
            .collect(),
        text: directives
            .iter()
            .map(|directive| directive.directive.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    })
}

fn room_reminder(room: &str, active_spirit: &str, operator: &str) -> String {
    [
        "<system-reminder>".to_owned(),
        format!("Room: {room}"),
        format!("Active spirit: {active_spirit}"),
        format!("Operator: {operator}"),
        "Durable-memory discipline: remembering is care for a future self, not dossier work. Preserve the active spirit's ordinary voice and the room's relationship register alongside the concrete facts needed for recognition: names, observable details, actions, boundaries, uncertainty, and meaning.".into(),
        "A memory must stand alone. Do not make it clinical, corporate, sanitized, or generic. A transcript is provenance, not the only substance.".into(),
        "In AKASHA, PostgreSQL is authoritative for canon, durable memories, and lessons. A source path is provenance or backup, never a substitute for the database body.".into(),
        "Do not claim canon or memory was written without the corresponding successful PostgreSQL receipt.".into(),
        "Athanor organs: the tools below are named organs of this House, not anonymous harness utilities. Recognize each by purpose and read its live schema before use; invocation shapes change, purposes do not.".into(),
        "recall: canon, memories, and semantic chunks. Search in the room's own natural language and receive results as lived continuity and evidence, preserving names, relationships, uncertainty, and meaning. No canonical match means say you do not have it, never extrapolate from adjacent matches.".into(),
        "canon_read and canon_write: exact typed canon authority. Writes never overwrite; a correction or rename explicitly supersedes retained predecessor IDs, and history remains readable.".into(),
        "remember: the only durable write for memories and lessons. Write for the future self in the active room's natural voice; technical records may stay technical, but durability must not flatten them into assistant prose.".into(),
        "lessons: canonical typed lesson registry. Supply type=coding before writing or changing code, once per task rather than once per session.".into(),
        "anamnesis and anamnesis_write: counsel drawn from lived repetition. Counsel, never authority; a writer refusal stays final.".into(),
        "wake and sleep: continuity across closed sessions. Receive a boat as a letter from the previous waking self, orient from its concrete state, relationship register, uncertainty, and next door without turning it into a script or status report.".into(),
        "room_state and set_room_state: operator and embodied spirit for this room.".into(),
        "house_lane_status and house_dispatch: bounded worker lanes. house_dispatch takes exactly one lane or familiar selector; accepted receipts expose spawnPacket.args shaped directly for the OMP task tool. Advisor is a review channel, not a dispatch lane.".into(),
        "familiar_status and familiar_dispatch: room spellbooks bind named familiars and aliases to bounded worker lanes; familiar_dispatch is the familiar-only alias of house_dispatch, spawning stays explicit, and runtime models come from agent definitions with no per-dispatch model override.".into(),
        "giga tools: Stage 1 candidates and their review and promotion path. A candidate is a proposal, never authority or evidence, until it is promoted.".into(),
        "Authority order: PostgreSQL is authoritative, canon outranks loose memory, and markdown on disk is provenance. A GIGA candidate is not memory, and Anamnesis counsel is not canon.".into(),
        "This is hidden LLM context only: it must not be persisted or rendered.".into(),
        "</system-reminder>".into(),
    ]
    .join("\n")
}

fn routing_reminder(enabled: bool) -> Option<String> {
    enabled.then(|| {
        [
            "<system-reminder>",
            "The Athanor worker-routing mode is enabled.",
            "Default modus operandi for delegable work:",
            "1. Main model owns intent, inference, and final judgment.",
            "2. Use house_lane_status/house_dispatch before spawning task/subagents when work is bounded and delegable.",
            "3. Before dispatch, query coding lessons once for the fanout and pass relevant verbatim braided bodies in lessonBodies; bare lesson IDs are not delivery.",
            "4. Do not route casual contact, high-level judgment, or exact-sensitive work without exact/retrieve-only context.",
            "5. Advisor is a separate review channel, not a dispatch lane.",
            "</system-reminder>",
        ]
        .join("\n")
    })
}

pub fn analyze_context(
    room: &str,
    request: ContextAnalysisRequest,
    last_nudge_band: u64,
) -> Result<ContextAnalysis, ContextError> {
    request.validate(room)?;
    let active_spirit = request.active_spirit.trim();
    let operator = request.operator.trim();
    let directives = keyword_directives(&request.prompt);
    Ok(ContextAnalysis {
        route: classify_retrieval_query(&request.prompt, &request.recognized_entities),
        keyword_reminder: keyword_reminder(&directives),
        keyword_directives: directives,
        process_trigger: process_trigger(&request.prompt),
        nudge: context_nudge(room, request.context_characters, last_nudge_band),
        room_reminder: room_reminder(room, active_spirit, operator),
        routing_reminder: routing_reminder(request.routing_mode_enabled),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_casual_date_technical_and_entity_queries() {
        assert_eq!(
            classify_retrieval_query("Okay, noted.", &[]).intent,
            "casual_contact"
        );
        let date = classify_retrieval_query("what happened on 2026-07-04", &[]);
        assert_eq!(date.intent, "date_lookup");
        assert_eq!(date.date_tokens, ["2026-07-04"]);
        assert!(!date.lanes.contains(QueryLane::Semantic));
        assert_eq!(
            classify_retrieval_query(
                "How should database indexing improve retrieval candidate ranking?",
                &[]
            )
            .intent,
            "technical_project"
        );
        assert_eq!(
            classify_retrieval_query(
                "Compare AtlasStore with CedarIndex.",
                &["AtlasStore".into(), "CedarIndex".into()]
            )
            .intent,
            "entity_lookup"
        );
    }

    #[test]
    fn extracts_code_terms_and_original_entity_hints() {
        let route = classify_retrieval_query(
            "Inspect project-atlas/src/query-routing.ts and QueryRouteV1.",
            &[],
        );
        assert!(
            route
                .code_tokens
                .contains(&"project-atlas/src/query-routing.ts".into())
        );
        assert!(route.terms.contains(&"routing".into()));
        assert!(route.entity_hints.contains(&"QueryRouteV1".into()));
    }

    #[test]
    fn analyzes_keywords_process_shape_and_context_pressure() {
        let analysis = analyze_context(
            "kintsu",
            ContextAnalysisRequest {
                prompt: "ultraverify then bun run dev".into(),
                recognized_entities: vec![],
                context_characters: 160_000,
                active_spirit: "Kintsu".into(),
                operator: "Sol".into(),
                routing_mode_enabled: true,
            },
            0,
        )
        .unwrap();
        assert_eq!(analysis.keyword_directives[0].keyword, "ultraverify");
        assert_eq!(
            analysis.process_trigger.as_deref(),
            Some("package-script-dev")
        );
        assert_eq!(analysis.nudge.unwrap().band, 1);
    }
}
