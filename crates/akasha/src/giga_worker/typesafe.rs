use super::failure::{WorkerFailure, WorkerFailureKind};
use crate::{
    AppError, Config,
    room_marker::read_room_marker_async,
    typesafe::{
        TYPESAFE_ENDPOINT, TYPESAFE_MAX_PACKET_BYTES, TYPESAFE_MODEL, is_secret,
        jev_giga_policy_from_marker, request_provider,
    },
};
use hearth::OperatorKey;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    collections::BTreeMap,
    env,
    path::{Path, PathBuf},
    sync::Arc,
};

pub(super) const GIGA_DEADLINE: Duration = Duration::from_secs(5);

const MAX_CREDENTIAL_BYTES: usize = 4096;
const MAX_POLICY_REVISION_BYTES: usize = 256;
const MAX_MODEL_BYTES: usize = 256;

#[derive(Clone, PartialEq, Eq)]
pub struct GigaClassifierConfig {
    credential: Arc<str>,
    policy_revision: String,
    room_dir: PathBuf,
    room: String,
    /// The trusted Host operator. `None` is the local House: Recall reads
    /// exactly as the Host's own Recall does without an operator.
    operator: Option<OperatorKey>,
}

impl GigaClassifierConfig {
    pub fn new(
        credential: String,
        policy_revision: String,
        room_dir: PathBuf,
        room: String,
    ) -> Result<Self, AppError> {
        if !valid_text(&credential, MAX_CREDENTIAL_BYTES)
            || !valid_text(&policy_revision, MAX_POLICY_REVISION_BYTES)
            || !valid_text(&room, 256)
            || room_dir.as_os_str().is_empty()
        {
            return Err(AppError::Config(
                "GIGA classifier configuration is invalid".into(),
            ));
        }
        Ok(Self {
            credential: Arc::from(credential),
            policy_revision,
            room_dir,
            room,
            operator: None,
        })
    }

    /// Bind the operator named by trusted Host configuration. Never derive it
    /// from a display name or transcript text.
    pub fn with_operator(mut self, operator: Option<OperatorKey>) -> Self {
        self.operator = operator;
        self
    }

    pub fn from_env(config: &Config) -> Result<Self, AppError> {
        if env::var("ATHANOR_HIPPOCAMPUS_REMOTE_CONSENT")
            .ok()
            .as_deref()
            != Some("1")
        {
            return Err(AppError::Config(
                "standalone GIGA classifier requires explicit remote consent".into(),
            ));
        }
        let credential = env::var("TYPESAFE_API_KEY").map_err(|_| {
            AppError::Config("standalone GIGA classifier requires TYPESAFE_API_KEY".into())
        })?;
        let policy_revision = env::var("ATHANOR_GIGA_POLICY_REVISION").map_err(|_| {
            AppError::Config(
                "standalone GIGA classifier requires ATHANOR_GIGA_POLICY_REVISION".into(),
            )
        })?;
        let room = config.giga_source_room.clone().ok_or_else(|| {
            AppError::Config("standalone GIGA classifier requires a source room".into())
        })?;
        let ledger_dir = config.giga_source_ledger_dir.as_deref().ok_or_else(|| {
            AppError::Config("standalone GIGA classifier requires a source ledger".into())
        })?;
        let room_dir = room_dir_from_ledger(ledger_dir).ok_or_else(|| {
            AppError::Config("standalone GIGA classifier source ledger path is invalid".into())
        })?;
        let marker = crate::room_marker::read_room_marker(&room_dir);
        if marker.is_null() {
            return Err(AppError::Config(
                "standalone GIGA classifier requires a room grant".into(),
            ));
        }
        if jev_giga_policy_from_marker(&marker, &room).as_deref() != Some(policy_revision.as_str())
        {
            return Err(AppError::Config(
                "standalone GIGA classifier room grant is not approved".into(),
            ));
        }
        Self::new(credential, policy_revision, room_dir, room)
    }
    pub(super) fn credential(&self) -> &str {
        &self.credential
    }

    pub fn policy_revision(&self) -> &str {
        &self.policy_revision
    }

    pub(super) fn room(&self) -> &str {
        &self.room
    }

    pub(super) fn operator(&self) -> Option<&OperatorKey> {
        self.operator.as_ref()
    }

    /// The effective room marker that holds this room's grants.
    pub(super) async fn marker(&self) -> Value {
        read_room_marker_async(&self.room_dir).await
    }

    pub(super) async fn grant_is_current(&self) -> bool {
        self.grants_are_current(None).await
    }

    /// One marker read decides both grants, so a revocation observed by one
    /// check cannot pass the other.
    async fn grants_are_current(&self, recall_revision: Option<&str>) -> bool {
        let marker = self.marker().await;
        jev_giga_policy_from_marker(&marker, &self.room).as_deref()
            == Some(self.policy_revision.as_str())
            && recall_revision.is_none_or(|revision| {
                recall_grant_revision(&marker, &self.room).as_deref() == Some(revision)
            })
    }
}

/// The existing Recall private-packet grant (`jevRecall`, as the Host reads it):
/// the policy revision when recalled private content may reach TypeSafe.
pub(super) fn recall_grant_revision(marker: &Value, room: &str) -> Option<String> {
    let grant = marker.get("jevRecall")?;
    let approved = marker.get("room").and_then(Value::as_str) == Some(room)
        && matches!(
            grant.get("mode").and_then(Value::as_str),
            Some("active" | "shadow")
        )
        && grant.get("provider").and_then(Value::as_str) == Some("typesafe")
        && grant.pointer("/grant/purpose").and_then(Value::as_str) == Some("recall-rerank")
        && grant
            .pointer("/grant/allowPrivateRecallPackets")
            .and_then(Value::as_bool)
            == Some(true);
    if !approved {
        return None;
    }
    grant
        .pointer("/grant/policyRevision")
        .and_then(Value::as_str)
        .filter(|revision| !revision.is_empty())
        .map(str::to_owned)
}

fn room_dir_from_ledger(ledger_dir: &Path) -> Option<PathBuf> {
    if !ledger_dir.is_absolute() {
        return None;
    }
    (ledger_dir.file_name().and_then(|name| name.to_str()) == Some("giga-sources")
        && ledger_dir
            .parent()?
            .file_name()
            .and_then(|name| name.to_str())
            == Some("runtime")
        && ledger_dir
            .parent()?
            .parent()?
            .file_name()
            .and_then(|name| name.to_str())
            == Some(".omp"))
    .then_some(ledger_dir.parent()?.parent()?.parent()?.to_owned())
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

/// The questions one judgment asks about one target source.
#[derive(Clone, Debug)]
pub(super) struct JudgmentPlan {
    pub(super) source_id: String,
    /// Ask for novelty only when a completed Recall lookup stands behind it.
    pub(super) novelty: bool,
    /// Follow-up lookup options, each exact conversation text. Empty asks none.
    pub(super) lookup: Vec<String>,
}

#[derive(Clone, Debug)]
pub(super) struct JevJudgment {
    pub(super) model: String,
    pub(super) kind: String,
    pub(super) priority: f64,
    pub(super) durability: f64,
    pub(super) confidence: f64,
    pub(super) novelty: Option<f64>,
    /// Index into the plan's lookup options; `None` when no follow-up was chosen.
    pub(super) lookup: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct JevResponse {
    model: String,
    answers: Map<String, Value>,
}

fn choice_answer<'a>(
    answers: &'a Map<String, Value>,
    id: &str,
) -> Result<&'a Value, WorkerFailure> {
    let answer = answers
        .get(id)
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::ClassifierOutput))?;
    if answer.get("type").and_then(Value::as_str) != Some("choice") {
        return Err(WorkerFailure::new(WorkerFailureKind::ClassifierOutput));
    }
    Ok(answer)
}

fn score_answer<'a>(answers: &'a Map<String, Value>, id: &str) -> Result<&'a Value, WorkerFailure> {
    let answer = answers
        .get(id)
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::ClassifierOutput))?;
    if answer.get("type").and_then(Value::as_str) != Some("score") {
        return Err(WorkerFailure::new(WorkerFailureKind::ClassifierOutput));
    }
    Ok(answer)
}

fn probability_distribution(answer: &Value, keys: &[&str]) -> Result<(), WorkerFailure> {
    let probabilities = answer
        .get("probabilities")
        .and_then(Value::as_object)
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::ClassifierOutput))?;
    if probabilities.len() != keys.len() || keys.iter().any(|key| !probabilities.contains_key(*key))
    {
        return Err(WorkerFailure::new(WorkerFailureKind::ClassifierOutput));
    }
    let mut total = 0.0;
    for key in keys {
        let value = probabilities
            .get(*key)
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
            .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::ClassifierOutput))?;
        total += value;
    }
    if (total - 1.0).abs() > 0.01 {
        return Err(WorkerFailure::new(WorkerFailureKind::ClassifierOutput));
    }
    Ok(())
}

fn parse_choice(
    answers: &Map<String, Value>,
    id: &str,
    allowed: &[String],
) -> Result<(String, f64), WorkerFailure> {
    let answer = choice_answer(answers, id)?;
    let choice = answer
        .get("choice")
        .and_then(Value::as_str)
        .filter(|choice| allowed.iter().any(|allowed| allowed == choice))
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::ClassifierOutput))?;
    let confidence = answer
        .get("confidence")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::ClassifierOutput))?;
    let keys = allowed.iter().map(String::as_str).collect::<Vec<_>>();
    probability_distribution(answer, &keys)?;
    let probabilities = answer["probabilities"]
        .as_object()
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::ClassifierOutput))?;
    let top = probabilities
        .values()
        .filter_map(Value::as_f64)
        .max_by(|left, right| left.total_cmp(right))
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::ClassifierOutput))?;
    let selected = probabilities[choice]
        .as_f64()
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::ClassifierOutput))?;
    if selected + 0.000001 < top {
        return Err(WorkerFailure::new(WorkerFailureKind::ClassifierOutput));
    }
    Ok((choice.to_owned(), confidence))
}

fn parse_score(answers: &Map<String, Value>, id: &str) -> Result<f64, WorkerFailure> {
    let answer = score_answer(answers, id)?;
    let score = answer
        .get("score")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::ClassifierOutput))?;
    let _confidence = answer
        .get("confidence")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::ClassifierOutput))?;
    probability_distribution(answer, &["0", "1"])?;
    let probabilities = answer["probabilities"]
        .as_object()
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::ClassifierOutput))?;
    let expected = probabilities["1"]
        .as_f64()
        .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::ClassifierOutput))?;
    if (score - expected).abs() > 0.05 {
        return Err(WorkerFailure::new(WorkerFailureKind::ClassifierOutput));
    }
    Ok(score)
}

fn kind_criteria() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("none".into(), "No durable, source-grounded change is present.".into()),
        ("memory".into(), "A durable decision, commitment, consent boundary, preference, relationship meaning, or current-state change. Simple facts stay suggestions for later human review.".into()),
        ("coding_lesson".into(), "A transferable engineering rule. The exact source must state a reusable rule and observed proof.".into()),
        ("project_lesson".into(), "A stable rule bound to exactly one supplied project key. The exact source must state a reusable rule and observed proof.".into()),
        ("correction".into(), "A direct correction of an earlier interpretation. The exact source must state the earlier reading and its source-grounded replacement.".into()),
        ("supersession".into(), "An explicit older state, newer replacement, and replacement intent. The exact source must state all three.".into()),
    ])
}

fn choice_question(instructions: String, criteria: BTreeMap<String, String>) -> Value {
    json!({
        "type": "choice",
        "instructions": instructions,
        "criteria": criteria
    })
}

fn score_question(instructions: String) -> Value {
    json!({
        "type": "score",
        "instructions": instructions,
        "criteria": [
            "The exact source does not support this dimension strongly.",
            "The exact source strongly supports this dimension."
        ]
    })
}

fn lookup_criteria(options: &[String]) -> BTreeMap<String, String> {
    let mut criteria = BTreeMap::from([(
        "none".to_owned(),
        "No follow-up lookup. memory_context already covers what this exact source depends on, \
         or nothing it depends on would be in memory."
            .to_owned(),
    )]);
    for (index, option) in options.iter().enumerate() {
        criteria.insert(
            format!("query_{}", index + 1),
            format!("Look up memory for this exact conversation text: {option}"),
        );
    }
    criteria
}

fn judgment_questions(plan: &JudgmentPlan) -> Map<String, Value> {
    let token = format!("source token `{}`", plan.source_id);
    let mut questions = Map::new();
    questions.insert(
        "kind_0".into(),
        choice_question(
            format!(
                "For {token}, choose one kind or none. Judge only this exact source; \
                 nearby_turns and memory_context only help you read it. The selected kind \
                 criteria define the evidence this exact source must state. Do not write prose."
            ),
            kind_criteria(),
        ),
    );
    questions.insert(
        "priority_0".into(),
        score_question(format!(
            "For {token}, how strongly should this exact source matter for later human review?"
        )),
    );
    questions.insert(
        "durability_0".into(),
        score_question(format!(
            "For {token}, how likely is this exact source to remain meaningful after the exchange?"
        )),
    );
    if plan.novelty {
        questions.insert(
            "novelty_0".into(),
            score_question(format!(
                "For {token}, how new is its durable content relative to memory_context? \
                 Near-duplicates of recalled canon or memories score low; content they lack \
                 scores high. Recalled entries are context only and are never citable."
            )),
        );
    }
    if !plan.lookup.is_empty() {
        questions.insert(
            "lookup_0".into(),
            choice_question(
                format!(
                    "For {token}, choose at most one follow-up memory lookup. Choose a query \
                     only when an earlier decision, state, or name that this exact source \
                     depends on is missing from memory_context."
                ),
                lookup_criteria(&plan.lookup),
            ),
        );
    }
    questions
}

pub(super) fn value_contains_secret(value: &Value) -> bool {
    match value {
        Value::String(text) => is_secret(text),
        Value::Array(values) => values.iter().any(value_contains_secret),
        Value::Object(values) => values
            .iter()
            .any(|(key, value)| is_secret(key) || value_contains_secret(value)),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
}

async fn request(
    config: &GigaClassifierConfig,
    body: String,
    recall_revision: Option<&str>,
) -> Result<String, WorkerFailure> {
    if is_secret(&body) {
        return Err(WorkerFailure::new(WorkerFailureKind::PrivacyRefused));
    }
    if body.len() > TYPESAFE_MAX_PACKET_BYTES {
        return Err(WorkerFailure::new(WorkerFailureKind::ClassifierRequest));
    }
    if !config.grants_are_current(recall_revision).await {
        return Err(WorkerFailure::new(WorkerFailureKind::TypesafeConfiguration));
    }
    let deadline = now_ms().saturating_add(GIGA_DEADLINE.as_millis() as u64);
    let (status, raw) =
        request_provider(TYPESAFE_ENDPOINT, Some(config.credential()), body, deadline)
            .await
            .map_err(|_| WorkerFailure::new(WorkerFailureKind::TypesafeTransport))?;
    if !status.is_success() || raw.is_empty() {
        return Err(WorkerFailure::new(WorkerFailureKind::TypesafeResponse));
    }
    Ok(raw)
}

/// Serialize one judgment request. Secret-like state is refused, never sent.
pub(super) fn judgment_body(
    config: &GigaClassifierConfig,
    event_state: &Value,
    plan: &JudgmentPlan,
) -> Result<String, WorkerFailure> {
    if value_contains_secret(event_state)
        || is_secret(&plan.source_id)
        || plan.lookup.iter().any(|option| is_secret(option))
    {
        return Err(WorkerFailure::new(WorkerFailureKind::PrivacyRefused));
    }
    serde_json::to_string(&json!({
        "model": TYPESAFE_MODEL,
        "state": {
            "policy_revision": config.policy_revision(),
            "event": event_state
        },
        "questions": judgment_questions(plan)
    }))
    .map_err(|_| WorkerFailure::new(WorkerFailureKind::ClassifierRequest))
}

/// Send one judgment. `recall_revision` names the Recall grant that the
/// packet's recalled content was gathered under; it must still hold at send time.
pub(super) async fn judge(
    config: &GigaClassifierConfig,
    body: String,
    plan: &JudgmentPlan,
    recall_revision: Option<&str>,
) -> Result<JevJudgment, WorkerFailure> {
    let raw = request(config, body, recall_revision).await?;
    parse_judgment(&raw, plan)
}

pub(super) fn parse_judgment(raw: &str, plan: &JudgmentPlan) -> Result<JevJudgment, WorkerFailure> {
    let response: JevResponse = serde_json::from_str(raw)
        .map_err(|_| WorkerFailure::new(WorkerFailureKind::ClassifierOutput))?;
    let mut expected = vec!["kind_0", "priority_0", "durability_0"];
    if plan.novelty {
        expected.push("novelty_0");
    }
    if !plan.lookup.is_empty() {
        expected.push("lookup_0");
    }
    if !valid_text(&response.model, MAX_MODEL_BYTES)
        || response.answers.len() != expected.len()
        || expected
            .iter()
            .any(|key| !response.answers.contains_key(*key))
    {
        return Err(WorkerFailure::new(WorkerFailureKind::ClassifierOutput));
    }
    let kind_allowed = kind_criteria().into_keys().collect::<Vec<_>>();
    let (kind, confidence) = parse_choice(&response.answers, "kind_0", &kind_allowed)?;
    let novelty = if plan.novelty {
        Some(parse_score(&response.answers, "novelty_0")?)
    } else {
        None
    };
    let lookup = if plan.lookup.is_empty() {
        None
    } else {
        let allowed = lookup_criteria(&plan.lookup)
            .into_keys()
            .collect::<Vec<_>>();
        let (choice, _) = parse_choice(&response.answers, "lookup_0", &allowed)?;
        choice
            .strip_prefix("query_")
            .and_then(|number| number.parse::<usize>().ok())
            .and_then(|number| number.checked_sub(1))
    };
    Ok(JevJudgment {
        priority: parse_score(&response.answers, "priority_0")?,
        durability: parse_score(&response.answers, "durability_0")?,
        model: response.model,
        kind,
        confidence,
        novelty,
        lookup,
    })
}

fn resonance_questions(candidate_ids: &[String]) -> Map<String, Value> {
    let mut questions = Map::new();
    for (index, candidate_id) in candidate_ids.iter().enumerate() {
        questions.insert(
            format!("resonance_{index}"),
            json!({
                "type": "noul",
                "instructions": format!(
                    "For exact parked candidate token `{candidate_id}`, \
                     assess whether the NEW event evidence materially develops or revises \
                     that parked source. Do not reward mere topic overlap. This answer grants \
                     no durable authority."
                )
            }),
        );
    }
    questions
}

pub(super) async fn resonance(
    config: &GigaClassifierConfig,
    state: Value,
    candidate_ids: &[String],
) -> Result<(String, Vec<(String, f64)>), WorkerFailure> {
    if candidate_ids.is_empty()
        || value_contains_secret(&state)
        || candidate_ids
            .iter()
            .any(|candidate_id| is_secret(candidate_id))
    {
        return Err(WorkerFailure::new(WorkerFailureKind::ClassifierRequest));
    }
    let body = serde_json::to_string(&json!({
        "model": TYPESAFE_MODEL,
        "state": {
            "policy_revision": config.policy_revision(),
            "event": state
        },
        "questions": resonance_questions(candidate_ids)
    }))
    .map_err(|_| WorkerFailure::new(WorkerFailureKind::ClassifierRequest))?;
    let raw = request(config, body, None).await?;
    let response: JevResponse = serde_json::from_str(&raw)
        .map_err(|_| WorkerFailure::new(WorkerFailureKind::ClassifierOutput))?;
    if !valid_text(&response.model, MAX_MODEL_BYTES)
        || response.answers.len() != candidate_ids.len()
    {
        return Err(WorkerFailure::new(WorkerFailureKind::ClassifierOutput));
    }
    let mut scores = Vec::with_capacity(candidate_ids.len());
    for (index, candidate_id) in candidate_ids.iter().enumerate() {
        let key = format!("resonance_{index}");
        let answer = response
            .answers
            .get(&key)
            .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::ClassifierOutput))?;
        if answer.get("type").and_then(Value::as_str) != Some("noul") {
            return Err(WorkerFailure::new(WorkerFailureKind::ClassifierOutput));
        }
        let score = answer
            .get("noul")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
            .ok_or_else(|| WorkerFailure::new(WorkerFailureKind::ClassifierOutput))?;
        scores.push((candidate_id.clone(), score));
    }
    if response.answers.keys().any(|key| {
        !candidate_ids
            .iter()
            .enumerate()
            .any(|(index, _)| key == &format!("resonance_{index}"))
    }) {
        return Err(WorkerFailure::new(WorkerFailureKind::ClassifierOutput));
    }
    Ok((response.model, scores))
}
