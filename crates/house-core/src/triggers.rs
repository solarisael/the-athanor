//! Process-trigger rules.
//!
//! Which lessons a matched process trigger braids into a turn, and how that
//! braid is worded, are House rules. The adapter only performs the lesson query
//! this module asks for and splices back the text it returns.

use serde::{Deserialize, Serialize};

/// The lesson query a matched process trigger requires.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LessonQueryPlan {
    pub trigger: String,
    #[serde(rename = "type")]
    pub family: &'static str,
    pub shape: &'static str,
    pub limit: u32,
}

/// No trigger, no braid. A matched trigger always braids process-shape coding
/// lessons, bounded to what a single turn can carry.
pub fn process_lesson_plan(trigger: Option<&str>) -> Option<LessonQueryPlan> {
    let trigger = trigger.map(str::trim).filter(|value| !value.is_empty())?;
    Some(LessonQueryPlan {
        trigger: trigger.to_owned(),
        family: "coding",
        shape: "process",
        limit: 12,
    })
}

/// One lesson row as the substrate returns it. Unknown columns are ignored on
/// purpose: the braid depends on these fields only.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcessLesson {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub lesson: String,
    #[serde(default)]
    pub proof_pattern: Option<String>,
    #[serde(default)]
    pub trigger_context: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessLessonReminder {
    pub trigger: String,
    pub lessons: usize,
    pub text: String,
}

fn lesson_lines(lesson: &ProcessLesson) -> String {
    let mut lines = vec![
        format!("#{} {}", lesson.id, lesson.title),
        lesson.lesson.clone(),
    ];
    if let Some(proof) = lesson
        .proof_pattern
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("Proof pattern: {proof}"));
    }
    if let Some(context) = lesson
        .trigger_context
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("Trigger context: {context}"));
    }
    lines.join("\n")
}

/// The hidden braid for a matched process trigger, or nothing when no lesson
/// answered.
pub fn process_lesson_reminder(
    trigger: Option<&str>,
    lessons: &[ProcessLesson],
) -> Option<ProcessLessonReminder> {
    let plan = process_lesson_plan(trigger)?;
    if lessons.is_empty() {
        return None;
    }
    let banner = lessons
        .iter()
        .map(lesson_lines)
        .collect::<Vec<_>>()
        .join("\n\n");
    Some(ProcessLessonReminder {
        trigger: plan.trigger,
        lessons: lessons.len(),
        text: [
            "<system-reminder>",
            "Solarisael process-shape lessons matched this user turn.",
            "Use this as hidden reasoning context before advising on the matched process. Do not render this banner verbatim unless the operator asks.",
            "",
            banner.as_str(),
            "</system-reminder>",
        ]
        .join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unmatched_turn_never_queries_or_braids() {
        assert!(process_lesson_plan(None).is_none());
        assert!(process_lesson_plan(Some("   ")).is_none());
        assert!(process_lesson_reminder(None, &[ProcessLesson::default()]).is_none());
    }

    #[test]
    fn a_matched_trigger_braids_every_answering_lesson() {
        let plan = process_lesson_plan(Some("deploy")).expect("a matched trigger plans a query");
        assert_eq!(
            (plan.family, plan.shape, plan.limit),
            ("coding", "process", 12)
        );

        assert!(process_lesson_reminder(Some("deploy"), &[]).is_none());

        let reminder = process_lesson_reminder(
            Some("deploy"),
            &[ProcessLesson {
                id: 42,
                title: "Verify the deploy".into(),
                lesson: "Read the staged set first.".into(),
                proof_pattern: Some("git status --short".into()),
                trigger_context: Some("   ".into()),
            }],
        )
        .expect("an answered trigger braids");

        assert_eq!(reminder.lessons, 1);
        assert!(reminder.text.contains("#42 Verify the deploy"));
        assert!(reminder.text.contains("Proof pattern: git status --short"));
        assert!(!reminder.text.contains("Trigger context:"));
    }
}
