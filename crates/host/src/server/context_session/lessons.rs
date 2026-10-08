use super::*;
use futures_util::FutureExt;
use serde::{Deserialize, Serialize};

type SieveDecision = (Option<std::collections::HashSet<u64>>, Value);
type SieveFuture =
    futures_util::future::Shared<futures_util::future::BoxFuture<'static, SieveDecision>>;
pub(super) type SieveCache = std::collections::VecDeque<(String, SieveFuture)>;

pub(super) fn sieve_once(
    cache: &mut SieveCache,
    state: &AppState,
    request: &ContextPrepareRequest,
    plan: &LessonPlan,
    cancellation: CancellationToken,
) -> SieveFuture {
    let ids = plan
        .baseline
        .iter()
        .map(|lesson| lesson.id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let key = format!(
        "{}\0{:x}\0{ids}",
        request.turn_id,
        Sha256::digest(request.prompt.as_bytes())
    );
    if let Some((_, decision)) = cache.iter().find(|(existing, _)| existing == &key) {
        return decision.clone();
    }
    let owned_state = state.clone();
    let query = request.prompt.clone();
    let credential = request.credential.clone();
    let plan = plan.clone();
    let task = state.tasks.spawn(async move {
        tokio::select! {
            _ = cancellation.cancelled() => (None, json!({"status":"refused","reason":"cancelled","fallbackUsed":true})),
            _ = owned_state.cancellation.cancelled() => (None, json!({"status":"refused","reason":"cancelled","fallbackUsed":true})),
            decision = score_sieve(&owned_state, &query, &credential, &plan) => decision,
        }
    });
    let decision = async move {
        task.await.unwrap_or_else(|_| {
            (
                None,
                json!({"status":"failed","reason":"sieve-failed","fallbackUsed":true}),
            )
        })
    }
    .boxed()
    .shared();
    cache.push_back((key, decision.clone()));
    if cache.len() > 64 {
        cache.pop_front();
    }
    decision
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Lesson {
    pub id: u64,
    pub body: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LessonPlan {
    pub token: String,
    pub turn_id: String,
    pub rules: Vec<Value>,
    pub lessons: Vec<Lesson>,
    pub baseline: Vec<Lesson>,
    pub warnings: Vec<String>,
    pub deadline: u64,
    pub requires_credential: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(super) struct LessonPlanRequest {
    pub turn_id: String,
    pub active_project: Option<String>,
    pub manager_available: bool,
    pub deadline: u64,
    pub native_user: bool,
    pub capabilities: Capabilities,
}

pub(super) async fn select(
    state: &AppState,
    meta: &CommandMeta,
    request: LessonPlanRequest,
) -> Result<LessonPlan, String> {
    let deadline = request.deadline.min(now_ms() + CONTEXT_BUDGET_MS);
    let requires_credential = requires_credential(state, &request).await;
    let mut queries = vec![json!({"type":"coding","alwaysOn":true,"limit":50})];
    for family in ["coding", "writing", "design", "audio"] {
        queries.push(json!({"type":family,"tag":"ttsr-approved","triggerOnly":true,"limit":50}));
    }
    if let Some(project) = request
        .active_project
        .as_deref()
        .filter(|project| !project.is_empty())
    {
        queries.push(json!({"type":"project","project":project,"tag":"ttsr-approved","triggerOnly":true,"limit":50}));
    }
    let results = futures_util::future::join_all(
        queries
            .iter()
            .map(|query| organ(state, meta, OrganOperation::LessonQuery, query.clone())),
    )
    .await;
    let mut warnings = Vec::new();
    let mut all_rows = Vec::new();
    let mut baseline = Vec::new();
    let mut baseline_rows = Vec::new();
    for (index, result) in results.into_iter().enumerate() {
        let rows = result
            .ok()
            .filter(|result| result["ok"] == true)
            .map(|result| array(&result["lessons"]).to_vec())
            .unwrap_or_default();
        if rows.len() == 50 {
            let family = text(&queries[index]["type"]);
            warnings.push(if index == 0 {
                "coding baseline query reached the 50-row ceiling".into()
            } else {
                format!("{family} trigger query reached the 50-row ceiling")
            });
        }
        if index == 0 {
            baseline_rows = rows.clone();
            baseline = rows
                .iter()
                .filter(|row| row["type"] == "coding" && row["alwaysOn"] == true)
                .filter_map(lesson)
                .collect();
        }
        if index == 0 {
            continue;
        }
        all_rows.extend(rows);
    }
    all_rows.extend(baseline_rows);
    let mut rules = Vec::new();
    let mut lessons = Vec::new();
    let mut ordered = Vec::<Value>::new();
    let mut positions = HashMap::new();
    for row in all_rows {
        let key = format!("{}:{}", text(&row["type"]), row["id"]);
        if let Some(index) = positions.get(&key) {
            ordered[*index] = row;
        } else {
            positions.insert(key, ordered.len());
            ordered.push(row);
        }
    }
    for row in ordered {
        if let Some(rule) = rule(&row, request.active_project.as_deref()) {
            rules.push(json!({"rule":rule,"block":row["interruptMode"] == "block"}));
            if request.manager_available {
                if let Some(lesson) = lesson(&row) {
                    lessons.push(lesson);
                }
            }
        }
    }
    if !request.manager_available {
        warnings.push("native OMP TTSR manager unavailable".into());
    }
    Ok(LessonPlan {
        token: new_id(),
        turn_id: request.turn_id,
        rules,
        lessons,
        baseline,
        warnings,
        deadline,
        requires_credential,
    })
}

async fn requires_credential(state: &AppState, request: &LessonPlanRequest) -> bool {
    let mut policies = Vec::new();
    if request.capabilities.automatic_recall {
        policies.push(JudgmentRequest::RecallPolicy {
            grant: RecallGrantKind::Recall,
        });
    }
    if request.capabilities.top_level {
        policies.push(JudgmentRequest::RecallPolicy {
            grant: RecallGrantKind::Lessons,
        });
    }
    if request.native_user {
        policies.push(JudgmentRequest::ModePolicy);
        policies.push(JudgmentRequest::VerdictPolicy);
    }
    futures_util::future::join_all(policies.into_iter().map(|request| async move {
        let typesafe_only = matches!(&request, JudgmentRequest::RecallPolicy { .. });
        match judge(state, request).await {
            Ok(policy) => {
                policy["approved"] == true && (typesafe_only || policy["provider"] == "typesafe")
            }
            Err(_) => false,
        }
    }))
    .await
    .into_iter()
    .any(|required| required)
}

fn lesson(row: &Value) -> Option<Lesson> {
    Some(Lesson {
        id: row["id"].as_u64()?,
        body: text(&row["lesson"]).into(),
    })
}

fn normalized_project(project: &str) -> String {
    project
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
}

fn strings(value: &Value) -> Vec<String> {
    array(value)
        .iter()
        .filter_map(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn scoped(row: &Value, active_project: Option<&str>) -> Option<Vec<String>> {
    let language_bound = !array(&row["languageKeys"]).is_empty();
    let project_bound = row["type"] == "project" && !text(&row["project"]).is_empty();
    let same_project = normalized_project(text(&row["project"]))
        == normalized_project(active_project.unwrap_or_default());
    let mut scope = strings(&row["triggerScope"]);
    if language_bound || (project_bound && !same_project) {
        scope.retain(|scope| scope != "text");
    }
    if (language_bound || project_bound) && scope.is_empty() {
        scope.push("tool".into());
    }
    if !scope.is_empty()
        && !scope
            .iter()
            .any(|scope| scope == "text" || scope == "tool" || scope.starts_with("tool:"))
    {
        return None;
    }
    Some(scope)
}

fn rule(row: &Value, active_project: Option<&str>) -> Option<Value> {
    if !array(&row["tags"]).iter().any(|tag| tag == "ttsr-approved") {
        return None;
    }
    let condition = strings(&row["condition"]);
    let ast_condition = strings(&row["astCondition"]);
    if condition.is_empty() && ast_condition.is_empty() {
        return None;
    }
    let scope = scoped(row, active_project)?;
    let mut identity = vec![
        format!("\"condition\":{}", json!(condition)),
        format!("\"astCondition\":{}", json!(ast_condition)),
        format!("\"scope\":{}", json!(scope)),
    ];
    for (name, key) in [
        ("project", "project"),
        ("languageKeys", "languageKeys"),
        ("mode", "interruptMode"),
        ("body", "lesson"),
    ] {
        if let Some(value) = row.get(key) {
            identity.push(format!("\"{name}\":{value}"));
        }
    }
    let digest = format!(
        "{:x}",
        Sha256::digest(format!("{{{}}}", identity.join(",")).as_bytes())
    );
    let path = format!("athanor://lessons/{}/{}", text(&row["type"]), row["id"]);
    let mut content = vec![
        text(&row["title"]).to_owned(),
        text(&row["lesson"]).to_owned(),
    ];
    if !text(&row["proofPattern"]).is_empty() {
        content.push(format!("Proof pattern: {}", text(&row["proofPattern"])));
    }
    content.retain(|part| !part.is_empty());
    let mut rule = json!({"name":format!("athanor-{}-{}-{}",text(&row["type"]),row["id"],&digest[..12]),"path":path,"content":content.join("\n\n"),"description":row["title"],"interruptMode":if row["interruptMode"] == "remind" {"never"} else {"always"},"_source":{"provider":"athanor-lessons","providerName":"The Athanor","path":path,"level":"native"}});
    for (key, values) in [
        ("condition", condition),
        ("astCondition", ast_condition),
        ("scope", scope),
        ("globs", globs(row)),
    ] {
        if !values.is_empty() {
            rule[key] = json!(values);
        }
    }
    Some(rule)
}

fn globs(row: &Value) -> Vec<String> {
    let mut extensions = Vec::new();
    for key in strings(&row["languageKeys"]) {
        let family = key.trim().to_lowercase();
        let mut family = family.as_str();
        while let Some((base, version)) = family.rsplit_once('-') {
            if version.is_empty() || !version.bytes().all(|byte| byte.is_ascii_digit()) {
                break;
            }
            family = base;
        }
        for extension in language_extensions(family) {
            if !extensions.contains(extension) {
                extensions.push(*extension);
            }
        }
    }
    let project = normalized_project(text(&row["project"]));
    let prefix = if project.is_empty() {
        "**".into()
    } else {
        format!("**/{project}/**")
    };
    if extensions.is_empty() {
        return if project.is_empty() {
            Vec::new()
        } else {
            vec![prefix]
        };
    }
    extensions
        .into_iter()
        .map(|extension| format!("{prefix}/*.{extension}"))
        .collect()
}

fn language_extensions(language: &str) -> &'static [&'static str] {
    const EXTENSIONS: &[(&str, &[&str])] = &[
        ("rust", &["rs"]),
        ("typescript", &["ts", "tsx", "mts", "cts"]),
        ("javascript", &["js", "jsx", "mjs", "cjs"]),
        ("python", &["py"]),
        ("powershell", &["ps1", "psm1", "psd1"]),
        ("shell", &["sh", "bash"]),
        ("sql", &["sql"]),
        ("css", &["css", "scss", "sass", "less"]),
        ("html", &["html", "htm"]),
        ("markdown", &["md", "mdx"]),
        ("go", &["go"]),
        ("ruby", &["rb"]),
        ("java", &["java"]),
        ("c", &["c", "h"]),
        ("cpp", &["cc", "cpp", "hpp"]),
        ("csharp", &["cs"]),
        ("lua", &["lua"]),
        ("zig", &["zig"]),
        ("gdscript", &["gd"]),
        ("glsl", &["glsl", "vert", "frag"]),
        ("wgsl", &["wgsl"]),
        ("bend", &["bend"]),
    ];
    EXTENSIONS
        .iter()
        .find(|(key, _)| *key == language)
        .map(|(_, extensions)| *extensions)
        .unwrap_or_default()
}

async fn score_sieve(
    state: &AppState,
    query: &str,
    credential: &Option<String>,
    plan: &LessonPlan,
) -> SieveDecision {
    let fallback = || {
        (
            None,
            json!({"status":"failed","reason":"sieve-failed","fallbackUsed":true}),
        )
    };
    let policy = judge(
        state,
        JudgmentRequest::RecallPolicy {
            grant: RecallGrantKind::Lessons,
        },
    )
    .await;
    let Ok(policy) = policy else {
        return fallback();
    };
    let deadline = judge_deadline(plan.deadline);
    if policy["approved"] == true && deadline.is_none() {
        return (
            None,
            json!({"status":"refused","reason":"deadline","fallbackUsed":true}),
        );
    }
    let candidates = plan.baseline.iter().map(|lesson| json!(lesson)).collect();
    let result = judge(
        state,
        JudgmentRequest::RecallRerank {
            grant: RecallGrantKind::Lessons,
            query: query.into(),
            retrieval_candidates: candidates,
            rerank_candidates: None,
            credential: credential.clone(),
            deadline,
            exact_candidate_indexes: Vec::new(),
        },
    )
    .await;
    let Ok(result) = result else {
        return fallback();
    };
    if result["receipt"]["status"] != "active" {
        return (None, result["receipt"].clone());
    }
    let keep: std::collections::HashSet<u64> = array(&result["retrievalCandidates"])
        .iter()
        .filter_map(|lesson| lesson["id"].as_u64())
        .collect();
    (Some(keep), result["receipt"].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unapproved_and_language_bound_triggers_do_not_gain_text_scope() {
        let mut row = json!({"id":1,"type":"coding","title":"Guard","lesson":"Body","condition":["reject"],"triggerScope":["text","tool:edit"],"languageKeys":["rust"]});
        assert!(rule(&row, None).is_none());
        row["tags"] = json!(["ttsr-approved"]);
        let prepared = rule(&row, None).unwrap();
        assert_eq!(prepared["scope"], json!(["tool:edit"]));
        assert_eq!(prepared["globs"], json!(["**/*.rs"]));
        row["languageKeys"] = json!(["bend-2"]);
        assert_eq!(rule(&row, None).unwrap()["globs"], json!(["**/*.bend"]));
        row["languageKeys"] = json!(["bend2"]);
        assert!(rule(&row, None).unwrap().get("globs").is_none());
    }

    #[test]
    fn project_trigger_text_scope_requires_the_observed_project() {
        let row = json!({"id":2,"type":"project","project":"target","title":"Guard","lesson":"Body",
            "condition":["reject"],"triggerScope":["text","tool:write"],"tags":["ttsr-approved"]});
        assert_eq!(
            rule(&row, Some("other")).unwrap()["scope"],
            json!(["tool:write"])
        );
        assert_eq!(
            rule(&row, Some("target")).unwrap()["scope"],
            json!(["text", "tool:write"])
        );
    }
}
