use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QuestTask {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub task: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SettledQuest {
    #[serde(default)]
    pub index: Option<usize>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub stderr: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i64>,
    #[serde(default)]
    pub aborted: bool,
    #[serde(default)]
    pub abort_reason: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QuestBatch {
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub tasks: Vec<QuestTask>,
    #[serde(default)]
    pub results: Vec<SettledQuest>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestMemory {
    pub result_id: String,
    /// Write-once key for this quest memory. Dedupe is a lineage rule, so the
    /// key is issued here rather than recomputed by whoever performs the write.
    pub idempotency_key: String,
    pub title: String,
    pub body: String,
    pub threads: [String; 2],
}

fn compact_line(value: Option<&str>) -> String {
    value
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn target_section(task: &str) -> String {
    let mut in_target = false;
    for line in task.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            let heading = trimmed.trim_start_matches('#').trim();
            if heading.eq_ignore_ascii_case("target") {
                in_target = true;
                continue;
            }
            if in_target {
                break;
            }
        }
        if in_target {
            let candidate = trimmed.trim_start_matches(['-', '*']).trim();
            if !candidate.is_empty() {
                return candidate.into();
            }
        }
    }

    task.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("general")
        .into()
}

fn path_tail(value: &str) -> String {
    let bytes = value.as_bytes();
    let windows_path = bytes.len() > 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\');
    if !windows_path {
        return value.into();
    }

    value
        .split(['/', '\\'])
        .filter(|part| !part.is_empty())
        .rev()
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join(" ")
}

fn slug(value: &str, fallback: &str) -> String {
    let mut output = String::new();
    let mut separator = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            if separator && !output.is_empty() {
                output.push('-');
            }
            output.push(character);
            separator = false;
        } else {
            separator = true;
        }
        if output.len() >= 63 {
            break;
        }
    }
    while output.ends_with('-') {
        output.pop();
    }
    if output.is_empty() {
        fallback.into()
    } else {
        output
    }
}

pub fn quest_domain(task: &str) -> String {
    let target = target_section(task)
        .replace(['`', '*', '_'], "")
        .replace("../", " ")
        .replace("..\\", " ")
        .replace("./", " ")
        .replace(".\\", " ");
    slug(&path_tail(&target), "general")
}

fn report_text(result: &SettledQuest) -> String {
    for candidate in [
        result.output.as_deref(),
        result.error.as_deref(),
        result.abort_reason.as_deref(),
        result.stderr.as_deref(),
    ] {
        if let Some(value) = candidate.map(str::trim).filter(|value| !value.is_empty()) {
            return value.into();
        }
    }

    if result.exit_code == Some(0) {
        "Quest completed without a textual report.".into()
    } else {
        "Quest ended without a textual report.".into()
    }
}

/// Terminal quest statuses. A quest that has not settled leaves no lineage.
pub const TERMINAL_QUEST_STATUSES: [&str; 3] = ["completed", "failed", "aborted"];

/// The lineage dedupe key: one memory per parent tool call and result.
pub fn quest_idempotency_key(tool_call_id: Option<&str>, result_id: &str) -> String {
    format!(
        "{}:{}",
        compact_line(tool_call_id),
        compact_line(Some(result_id))
    )
}

/// The terminal report a kitten leaves beside its OMP child session.
pub fn quest_report_path(session_file: &str) -> Option<String> {
    let source = session_file.trim();
    if source.is_empty() {
        return None;
    }
    let jsonl = source.len() >= 6 && source[source.len() - 6..].eq_ignore_ascii_case(".jsonl");
    Some(if jsonl {
        format!("{}.md", &source[..source.len() - 6])
    } else {
        format!("{source}.md")
    })
}

/// A single settled subagent lifecycle event, joined with what its progress
/// event reported. The status vocabulary and its terminal semantics are House
/// rules, not adapter bookkeeping.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QuestLifecycle {
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub session_file: Option<String>,
}

impl QuestLifecycle {
    pub fn is_terminal(&self) -> bool {
        self.status
            .as_deref()
            .map(str::trim)
            .is_some_and(|status| TERMINAL_QUEST_STATUSES.contains(&status))
    }

    pub fn report_path(&self) -> Option<String> {
        self.session_file.as_deref().and_then(quest_report_path)
    }
}

/// Normalize one settled quest into its lineage memory. A non-terminal or
/// empty quest leaves nothing behind.
pub fn normalize_lifecycle_memory(lifecycle: QuestLifecycle, report: &str) -> Option<QuestMemory> {
    if !lifecycle.is_terminal() {
        return None;
    }
    let status = lifecycle.status.as_deref().unwrap_or_default().trim();
    let aborted = status == "aborted";
    let report = report.trim();
    let batch = QuestBatch {
        tool_call_id: lifecycle.tool_call_id.clone(),
        tasks: vec![QuestTask {
            name: lifecycle.id.clone(),
            agent: lifecycle.agent.clone(),
            task: lifecycle.task.clone(),
        }],
        results: vec![SettledQuest {
            index: Some(0),
            id: lifecycle.id,
            agent: lifecycle.agent,
            task: lifecycle.task,
            output: (!report.is_empty()).then(|| report.to_owned()),
            stderr: None,
            exit_code: Some(if status == "completed" { 0 } else { 1 }),
            aborted,
            abort_reason: aborted.then(|| "Quest aborted.".into()),
            error: (status == "failed").then(|| "Quest failed.".into()),
        }],
    };
    normalize_quest_memories(batch).into_iter().next()
}

pub fn normalize_quest_memories(batch: QuestBatch) -> Vec<QuestMemory> {
    batch
        .results
        .iter()
        .enumerate()
        .filter_map(|(position, result)| {
            let exit_code = result.exit_code?;
            let index = result.index.unwrap_or(position);
            let requested = batch.tasks.get(index).cloned().unwrap_or_default();
            let kitten = compact_line(requested.name.as_deref().or(result.id.as_deref()));
            let kitten = if kitten.is_empty() {
                format!("kitten-{}", index + 1)
            } else {
                kitten
            };
            let quest = result
                .task
                .as_deref()
                .or(requested.task.as_deref())
                .unwrap_or_default()
                .trim();
            if quest.is_empty() {
                return None;
            }

            let status = if result.aborted {
                "aborted"
            } else if exit_code == 0 {
                "completed"
            } else {
                "failed"
            };
            let agent = compact_line(result.agent.as_deref().or(requested.agent.as_deref()));
            let outcome = if agent.is_empty() {
                format!("Outcome: {status}.")
            } else {
                format!("Outcome: {status}; role: {agent}.")
            };
            let body = [
                "Quest".into(),
                quest.into(),
                String::new(),
                "Report".into(),
                report_text(result),
                String::new(),
                outcome,
            ]
            .join("\n");
            let target = compact_line(Some(&target_section(quest)));
            let title_target = target.chars().take(120).collect::<String>();

            let result_id = compact_line(result.id.as_deref())
                .is_empty()
                .then(|| index.to_string())
                .unwrap_or_else(|| compact_line(result.id.as_deref()));

            Some(QuestMemory {
                idempotency_key: quest_idempotency_key(batch.tool_call_id.as_deref(), &result_id),
                result_id,
                title: format!("{kitten} quest: {title_target}"),
                body,
                threads: [
                    format!("kitten:{}", slug(&kitten, "unknown")),
                    format!("domain:{}", quest_domain(quest)),
                ],
            })
        })
        .collect()
}
