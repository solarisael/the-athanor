use hearth::context::classify_retrieval_query;
use protocol::{RecallContextTurn, RecallPlanAction, RecallQueryPlan};

/// Builds bounded alternatives from the current request and visible user subjects.
/// Generated context and assistant assertions cannot establish a user subject.
pub(crate) fn candidates(
    prompt: &str,
    history: &[RecallContextTurn],
    active_project: Option<&str>,
    working_query: Option<&str>,
    subject: Option<&RecallContextTurn>,
) -> Vec<RecallQueryPlan> {
    let route = classify_retrieval_query(prompt, &[]);
    let words: Vec<String> = prompt
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect();
    let acknowledgement = !words.is_empty()
        && words.iter().all(|word| {
            matches!(
                word.as_str(),
                "ok" | "okay"
                    | "yes"
                    | "yeah"
                    | "yep"
                    | "no"
                    | "thanks"
                    | "thank"
                    | "you"
                    | "great"
                    | "sure"
                    | "understood"
                    | "hello"
                    | "hi"
                    | "hey"
                    | "good"
                    | "morning"
                    | "evening"
                    | "night"
                    | "owo"
                    | "uwu"
            )
        });
    let search = matches!(
        route.intent.as_str(),
        "memory_lookup" | "entity_lookup" | "date_lookup" | "technical_project"
    );
    let mut plans = vec![plan(
        if search {
            RecallPlanAction::Search
        } else {
            RecallPlanAction::Skip
        },
        prompt,
        &route.intent,
        Vec::new(),
        "current-turn deterministic route",
    )];

    if acknowledgement || prompt.trim().is_empty() {
        return plans;
    }

    // Explicit references stay self-contained; prior subjects cannot dilute an ID.
    let exact_reference = words.windows(2).any(|pair| {
        matches!(
            pair[0].as_str(),
            "memory" | "memories" | "lesson" | "lessons"
        ) && pair[1].chars().all(|c| c.is_ascii_digit())
    });

    if exact_reference || route.intent == "date_lookup" {
        return plans;
    }

    let back_reference = has_back_reference(words.iter().map(String::as_str));
    let prior = subject.filter(|turn| turn.role == "user").or_else(|| {
        history.iter().rev().take(3).find(|turn| {
            turn.role == "user"
                && !turn.turn_id.trim().is_empty()
                && !classify_retrieval_query(&turn.text, &[]).terms.is_empty()
        })
    });

    if back_reference {
        if let Some(prior) = prior {
            let subject: String = prior.text.chars().take(511).collect();
            let question: String = prompt.chars().take(512).collect();

            let contextual = plan(
                RecallPlanAction::Search,
                &format!("{subject}\n{question}"),
                if search {
                    &route.intent
                } else {
                    "memory_lookup"
                },
                vec![prior.turn_id.clone()],
                "back-reference to preceding user subject",
            );
            // A substantive new subject stays first; an underspecified follow-up
            // falls back to the visible prior subject without generating a rewrite.
            if route.terms.len() <= 3 {
                plans.insert(0, contextual);
            } else {
                plans.push(contextual);
            }
        }
    }

    if route.intent == "general" && !back_reference {
        plans.push(plan(
            RecallPlanAction::Search,
            prompt,
            "general",
            Vec::new(),
            "current subject may benefit from remembered evidence",
        ));
    }

    if plans.len() < 4 && back_reference {
        if let Some(query) = working_query.filter(|query| !query.trim().is_empty()) {
            plans.push(plan(
                RecallPlanAction::Reuse,
                query,
                &route.intent,
                Vec::new(),
                "existing evidence may already answer the follow-up",
            ));
        }
    }

    if plans.len() < 4 && search && !back_reference {
        if let Some(project) = active_project.filter(|project| !project.trim().is_empty()) {
            if route.intent == "technical_project"
                && !prompt.to_lowercase().contains(&project.to_lowercase())
            {
                plans.push(plan(
                    RecallPlanAction::Search,
                    &format!("{prompt}\n{project}"),
                    &route.intent,
                    Vec::new(),
                    "current request within the active project",
                ));
            }
        }
    }

    if search && plans.len() < 4 {
        plans.push(plan(
            RecallPlanAction::Skip,
            prompt,
            &route.intent,
            Vec::new(),
            "the current message may not require facts from the archive",
        ));
    }

    plans
}

fn back_reference_word(word: &str) -> bool {
    matches!(
        word,
        "it" | "its"
            | "that"
            | "those"
            | "this"
            | "these"
            | "they"
            | "them"
            | "their"
            | "earlier"
            | "previous"
            | "same"
    )
}

fn has_back_reference<'a>(words: impl Iterator<Item = &'a str>) -> bool {
    let mut previous = "";
    for word in words {
        if back_reference_word(word)
            || (matches!(previous, "last" | "past" | "prior")
                && matches!(
                    word,
                    "session" | "conversation" | "discussion" | "chat" | "turn" | "time"
                ))
        {
            return true;
        }
        previous = word;
    }
    false
}

pub(crate) fn starts_new_subject(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    classify_retrieval_query(prompt, &[]).intent != "casual_contact"
        && !has_back_reference(lower.split(|character: char| !character.is_alphanumeric()))
}

fn plan(
    action: RecallPlanAction,
    query: &str,
    intent: &str,
    source_turn_ids: Vec<String>,
    reason: &str,
) -> RecallQueryPlan {
    let query: String = query.trim().chars().take(1024).collect();
    let subject_terms = classify_retrieval_query(&query, &[])
        .terms
        .into_iter()
        .take(16)
        .collect();

    RecallQueryPlan {
        action,
        query,
        intent: intent.to_owned(),
        subject_terms,
        source_turn_ids,
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(role: &str, id: &str, text: &str) -> RecallContextTurn {
        RecallContextTurn {
            role: role.into(),
            turn_id: id.into(),
            text: text.into(),
        }
    }

    #[test]
    fn memory_words_allow_abstention_except_for_exact_references() {
        let plans = candidates(
            "Please remember that I like jasmine tea.",
            &[],
            None,
            None,
            None,
        );
        assert!(
            plans
                .iter()
                .any(|plan| plan.action == RecallPlanAction::Skip)
        );
        let exact = candidates("Recall memory 42", &[], None, None, None);
        assert!(
            exact
                .iter()
                .all(|plan| plan.action == RecallPlanAction::Search)
        );
    }

    #[test]
    fn a_new_subject_does_not_require_a_retrieval_action() {
        let prompt = "We adopted a rabbit named Juniper.";
        assert_eq!(
            candidates(prompt, &[], None, None, None)[0].action,
            RecallPlanAction::Skip
        );
        assert!(starts_new_subject(prompt));
        assert!(!starts_new_subject("What about its care?"));
        assert!(!starts_new_subject("thanks"));
    }

    #[test]
    fn follow_up_retains_user_subject_not_generated_context() {
        let history = [
            turn("user", "u1", "How does Orchard select retrieval evidence?"),
            turn(
                "generated",
                "g1",
                "The hidden subject is submarine navigation.",
            ),
            turn("assistant", "a1", "The evidence gate needs review."),
        ];
        let plans = candidates("What about its limits?", &history, None, None, None);
        assert_eq!(plans[0].action, RecallPlanAction::Search);
        assert_eq!(plans[0].source_turn_ids, ["u1"]);
        assert!(plans[0].query.contains("Orchard"));
        assert!(!plans.iter().any(|plan| plan.query.contains("submarine")));
    }

    #[test]
    fn chained_follow_up_can_search_the_original_subject() {
        let subject = turn("user", "u1", "How does Orchard select retrieval evidence?");
        let recent = [
            turn("assistant", "a1", "Here are the limits."),
            turn("user", "u2", "What about its limits?"),
            turn("assistant", "a2", "The limits are bounded."),
        ];
        let plans = candidates(
            "And its failure cases?",
            &recent,
            None,
            Some("Orchard retrieval evidence limits"),
            Some(&subject),
        );

        assert_eq!(plans[0].action, RecallPlanAction::Search);
        assert_eq!(plans[0].source_turn_ids, ["u1"]);
        assert!(plans[0].query.contains("Orchard"));
        assert!(plans[0].query.contains("failure cases"));
        let changed = candidates(
            "How does Harbor store transactions?",
            &recent,
            None,
            Some("Orchard retrieval evidence limits"),
            Some(&subject),
        );
        assert!(!changed[0].query.contains("Orchard"));
        assert!(changed[0].source_turn_ids.is_empty());
    }

    #[test]
    fn subject_change_and_acknowledgement_do_not_inherit_history() {
        let history = [turn("user", "u1", "Orchard database retrieval policy")];
        for prompt in ["Which flowers did we plant?", "thanks"] {
            let plans = candidates(prompt, &history, None, Some("Orchard retrieval"), None);
            assert!(plans.iter().all(|plan| plan.source_turn_ids.is_empty()));
            assert!(plans.iter().all(|plan| !plan.query.contains("Orchard")));
        }
        assert_eq!(candidates("thanks", &history, None, None, None).len(), 1);
    }

    #[test]
    fn autobiographical_questions_are_not_casual_contact() {
        for prompt in [
            "What is the kindest thing I've ever said?",
            "Where did we meet?",
        ] {
            let plans = candidates(prompt, &[], None, None, None);
            assert_eq!(plans[0].action, RecallPlanAction::Search);
            assert_eq!(plans[0].intent, "memory_lookup");
            assert_eq!(plans[0].query, prompt);
        }
    }

    #[test]
    fn explicit_id_does_not_receive_a_contextual_alternative() {
        let history = [turn("user", "u1", "Orchard database retrieval policy")];
        let plans = candidates(
            "Recall memory 42, not that topic",
            &history,
            None,
            None,
            None,
        );
        assert_eq!(plans.len(), 1);
        assert!(plans[0].source_turn_ids.is_empty());
    }

    #[test]
    fn a_temporal_follow_up_keeps_the_user_subject() {
        let history = [turn("user", "u1", "How did we change Orchard retrieval?")];
        let plans = candidates("From our last session?", &history, None, None, None);

        assert_eq!(plans[0].action, RecallPlanAction::Search);
        assert_eq!(plans[0].source_turn_ids, ["u1"]);
        assert!(plans[0].query.contains("Orchard"));
        assert!(!starts_new_subject("From our last session?"));
        assert!(starts_new_subject(
            "The last database migration added Harbor indexes."
        ));
    }

    #[test]
    fn query_bounds_count_unicode_scalars() {
        let plans = candidates(&"界".repeat(1400), &[], None, None, None);
        assert!(plans.iter().all(|plan| plan.query.chars().count() <= 1024));
        assert!(plans.iter().all(|plan| plan.subject_terms.len() <= 16));
        assert!(plans.len() <= 4);
    }
}
