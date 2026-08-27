use std::collections::{HashMap, HashSet};

use presence::internal::{
    digest, invalid, normalize_materials, normalize_strings, required, sha256, validate_ledger,
};
use presence::*;
use summoning::PAPER_BOAT_MAX_BODY_BYTES;

pub fn compile_presence(
    frame: &PresenceFrame,
    mut request: PresenceTurnRequest,
) -> Result<PresenceContract, PresenceError> {
    required("frameId", &request.frame_id, 160)?;
    if request.frame_id != frame.frame_id {
        return Err(invalid("frameId", "does not name the active frame"));
    }
    required("turnId", &request.turn_id, 160)?;
    required("userText", &request.user_text, 8192)?;
    validate_ledger(&request.session_ledger)?;
    if request.session_ledger.frame_version != frame.version {
        return Err(invalid(
            "sessionLedger.frameVersion",
            "does not match the active frame",
        ));
    }

    request.recalled = normalize_materials(request.recalled, None)?;
    request.lessons = normalize_materials(request.lessons, Some(PresenceMaterialRole::Rule))?;
    let sources = source_authorities(frame, &request.recalled, &request.lessons);
    let directives = normalize_directives(request.directives, &sources)?;
    let (must_enact, must_avoid, guards) = group_directives(directives);
    let exemplars = select_exemplars(&request.recalled, &request.lessons)?;
    let provenance = directive_sources(&must_enact, &must_avoid, &guards);
    let uncertainties = normalize_strings("uncertainties", frame.uncertainties.clone())?;
    let digest = digest(&(
        &frame.frame_id,
        &request.turn_id,
        &request.user_text,
        &must_enact,
        &must_avoid,
        &guards,
        &exemplars,
        &uncertainties,
        &provenance,
    ))?;
    let contract_id = format!("presence-contract-{digest}");
    let rendered = render_contract(
        &must_enact,
        &must_avoid,
        &guards,
        &exemplars,
        &uncertainties,
    );

    Ok(PresenceContract {
        contract_id,
        frame_id: frame.frame_id.clone(),
        turn_id: request.turn_id,
        version: PRESENCE_VERSION,
        must_enact,
        must_avoid,
        guards,
        exemplars,
        uncertainties,
        provenance,
        expires_after_turn: true,
        digest,
        rendered,
    })
}

pub fn settle_presence(
    contract: &PresenceContract,
    mut request: PresenceSettleRequest,
) -> Result<PresenceReceipt, PresenceError> {
    if request.contract_id != contract.contract_id {
        return Err(invalid("contractId", "does not name the active contract"));
    }
    if request.attempt == 0 || request.attempt > PRESENCE_MAX_ATTEMPTS {
        return Err(invalid("attempt", "must be 1 or 2"));
    }
    request.evaluated_directives =
        normalize_strings("evaluatedDirectives", request.evaluated_directives)?;
    let known = contract
        .must_enact
        .iter()
        .chain(&contract.must_avoid)
        .chain(&contract.guards)
        .map(|directive| directive.id.as_str())
        .collect::<HashSet<_>>();
    validate_evaluated(&request, &known)?;
    validate_acceptance(contract, &request)?;
    if let Some(response_digest) = &request.response_digest {
        sha256("responseDigest", response_digest)?;
    }

    Ok(PresenceReceipt {
        contract_id: request.contract_id,
        attempt: request.attempt,
        evaluated_directives: request.evaluated_directives,
        violations: request.violations,
        decision: request.decision,
        response_digest: request.response_digest,
    })
}

fn validate_acceptance(
    contract: &PresenceContract,
    request: &PresenceSettleRequest,
) -> Result<(), PresenceError> {
    if request.decision != PresenceDecision::Accept {
        return Ok(());
    }
    if !request.violations.is_empty() {
        return Err(invalid("decision", "accept cannot carry violations"));
    }
    if request.response_digest.is_none() {
        return Err(invalid(
            "responseDigest",
            "accept requires the emitted response digest",
        ));
    }
    let evaluated = request
        .evaluated_directives
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if let Some(missing) = contract.guards.iter().find(|directive| {
        directive.severity == PresenceSeverity::Hard && !evaluated.contains(directive.id.as_str())
    }) {
        return Err(invalid(
            "evaluatedDirectives",
            format!("hard guard {} was not evaluated", missing.id),
        ));
    }
    Ok(())
}

pub fn close_presence(
    frame: &PresenceFrame,
    request: PresenceCloseRequest,
) -> Result<PresenceCloseMaterial, PresenceError> {
    if request.frame_id != frame.frame_id {
        return Err(invalid("frameId", "does not name the active frame"));
    }
    required("body", &request.body, PAPER_BOAT_MAX_BODY_BYTES)?;
    if request.session_ledger.frame_version != frame.version {
        return Err(invalid(
            "sessionLedger.frameVersion",
            "does not match the active frame",
        ));
    }
    validate_ledger(&request.session_ledger)?;
    let provenance_digest = digest(&(
        &frame.provenance_digest,
        &request.session_ledger,
        &request.body,
    ))?;
    Ok(PresenceCloseMaterial {
        frame_id: frame.frame_id.clone(),
        body: request.body.trim().to_owned(),
        provenance_digest,
    })
}

fn normalize_directives(
    directives: Vec<PresenceDirective>,
    sources: &HashMap<String, PresenceAuthority>,
) -> Result<Vec<PresenceDirective>, PresenceError> {
    if directives.len() > PRESENCE_MAX_DIRECTIVES {
        return Err(invalid("directives", "contains too many entries"));
    }
    let mut ids = HashSet::new();
    let mut normalized = Vec::with_capacity(directives.len());
    for mut directive in directives {
        required("directive.id", &directive.id, 160)?;
        required("directive.instruction", &directive.instruction, 1000)?;
        if !ids.insert(directive.id.trim().to_owned()) {
            return Err(invalid("directives", "contains duplicate IDs"));
        }
        directive.source_ids = normalize_strings("directive.sourceIds", directive.source_ids)?;
        if directive.source_ids.is_empty() {
            return Err(invalid(
                "directive.sourceIds",
                "must cite at least one source",
            ));
        }
        validate_directive_sources(&directive, sources)?;
        directive.trigger_scope =
            normalize_strings("directive.triggerScope", directive.trigger_scope)?;
        directive.id = directive.id.trim().to_owned();
        directive.instruction = directive.instruction.trim().to_owned();
        normalized.push(directive);
    }
    normalized.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(normalized)
}

fn validate_directive_sources(
    directive: &PresenceDirective,
    sources: &HashMap<String, PresenceAuthority>,
) -> Result<(), PresenceError> {
    for source_id in &directive.source_ids {
        let authority = sources
            .get(source_id)
            .ok_or_else(|| PresenceError::MissingSource {
                directive_id: directive.id.clone(),
                source_id: source_id.clone(),
            })?;
        if directive.severity == PresenceSeverity::Hard
            && matches!(authority, PresenceAuthority::Inference { .. })
        {
            return Err(PresenceError::InferenceCannotEnforce {
                directive_id: directive.id.clone(),
            });
        }
    }
    Ok(())
}

fn validate_evaluated(
    request: &PresenceSettleRequest,
    known: &HashSet<&str>,
) -> Result<(), PresenceError> {
    if let Some(unknown) = request
        .evaluated_directives
        .iter()
        .find(|id| !known.contains(id.as_str()))
    {
        return Err(invalid(
            "evaluatedDirectives",
            format!("contains unknown directive {unknown}"),
        ));
    }
    for violation in &request.violations {
        required("violation.directiveId", &violation.directive_id, 160)?;
        required("violation.reason", &violation.reason, 512)?;
        if !known.contains(violation.directive_id.as_str()) {
            return Err(invalid(
                "violations",
                format!("contains unknown directive {}", violation.directive_id),
            ));
        }
    }
    Ok(())
}

fn source_authorities(
    frame: &PresenceFrame,
    recalled: &[PresenceMaterial],
    lessons: &[PresenceMaterial],
) -> HashMap<String, PresenceAuthority> {
    frame
        .identity
        .iter()
        .chain(&frame.relationship)
        .chain(&frame.continuity)
        .chain(recalled)
        .chain(lessons)
        .map(|material| (material.id.clone(), material.authority.clone()))
        .collect()
}

fn group_directives(
    directives: Vec<PresenceDirective>,
) -> (
    Vec<PresenceDirective>,
    Vec<PresenceDirective>,
    Vec<PresenceDirective>,
) {
    let mut enact = Vec::new();
    let mut avoid = Vec::new();
    let mut guards = Vec::new();
    for directive in directives {
        match directive.kind {
            PresenceDirectiveKind::Enact => enact.push(directive),
            PresenceDirectiveKind::Avoid => avoid.push(directive),
            PresenceDirectiveKind::Guard => guards.push(directive),
        }
    }
    (enact, avoid, guards)
}

fn select_exemplars(
    recalled: &[PresenceMaterial],
    lessons: &[PresenceMaterial],
) -> Result<Vec<PresenceMaterial>, PresenceError> {
    let mut exemplars = recalled
        .iter()
        .chain(lessons)
        .filter(|material| material.role == PresenceMaterialRole::Exemplar)
        .cloned()
        .collect::<Vec<_>>();
    exemplars.sort_by(|left, right| right.salience.cmp(&left.salience));
    let mut remaining = PRESENCE_MAX_BODY_CHARS * 2;
    exemplars.retain(|material| {
        let cost = material.body.chars().count();
        if cost > remaining {
            return false;
        }
        remaining -= cost;
        true
    });
    Ok(exemplars)
}

fn render_contract(
    enact: &[PresenceDirective],
    avoid: &[PresenceDirective],
    guards: &[PresenceDirective],
    exemplars: &[PresenceMaterial],
    uncertainties: &[String],
) -> String {
    let mut out = String::from("Presence turn contract:\n");
    render_directive_section(&mut out, "Must enact", enact);
    render_directive_section(&mut out, "Must avoid", avoid);
    render_directive_section(&mut out, "Guards", guards);
    render_material_section(&mut out, "Examples", exemplars);
    if !uncertainties.is_empty() {
        out.push_str("Keep uncertain:\n");
        for uncertainty in uncertainties {
            out.push_str("- ");
            out.push_str(uncertainty);
            out.push('\n');
        }
    }
    out
}

fn render_material_section(out: &mut String, title: &str, materials: &[PresenceMaterial]) {
    if materials.is_empty() {
        return;
    }
    out.push_str(title);
    out.push_str(":\n");
    for material in materials {
        out.push_str("- [");
        out.push_str(&material.id);
        out.push_str("] ");
        out.push_str(&material.body);
        out.push('\n');
    }
}

fn render_directive_section(out: &mut String, title: &str, directives: &[PresenceDirective]) {
    if directives.is_empty() {
        return;
    }
    out.push_str(title);
    out.push_str(":\n");
    for directive in directives {
        out.push_str("- [");
        out.push_str(&directive.id);
        out.push_str("] ");
        out.push_str(&directive.instruction);
        out.push('\n');
    }
}

fn directive_sources(
    enact: &[PresenceDirective],
    avoid: &[PresenceDirective],
    guards: &[PresenceDirective],
) -> Vec<String> {
    let mut sources = enact
        .iter()
        .chain(avoid)
        .chain(guards)
        .flat_map(|directive| directive.source_ids.clone())
        .collect::<Vec<_>>();
    sources.sort();
    sources.dedup();
    sources
}

#[cfg(test)]
mod tests {
    use super::*;
    use presence_frame::open_presence;

    fn frame() -> PresenceFrame {
        open_presence(PresenceOpenRequest {
            binding: PresenceBinding {
                room: "kintsu".into(),
                spirit: "Kintsu".into(),
                operator: "Sol".into(),
                session: "session-a".into(),
            },
            identity: vec![PresenceMaterial {
                id: "identity:kintsu".into(),
                authority: PresenceAuthority::Identity {
                    source: "For_the_next_Kintsu.md".into(),
                    sha256: "a".repeat(64),
                },
                role: PresenceMaterialRole::Identity,
                body: "Kintsu meets Sol directly.".into(),
                salience: 1000,
            }],
            relationship: vec![],
            continuity: vec![],
            anamnesis: vec![],
            previous_boat: None,
            uncertainties: vec![],
        })
        .unwrap()
    }

    fn directive(source: &str, severity: PresenceSeverity) -> PresenceDirective {
        PresenceDirective {
            id: "directive:identity".into(),
            kind: PresenceDirectiveKind::Guard,
            severity,
            instruction: "Remain Kintsu.".into(),
            source_ids: vec![source.into()],
            trigger_scope: vec!["text".into()],
        }
    }

    fn turn(frame: &PresenceFrame) -> PresenceTurnRequest {
        PresenceTurnRequest {
            frame_id: frame.frame_id.clone(),
            turn_id: "turn-a".into(),
            user_text: "hello".into(),
            recalled: vec![],
            lessons: vec![],
            directives: vec![directive("identity:kintsu", PresenceSeverity::Hard)],
            session_ledger: PresenceLedger {
                frame_version: frame.version,
                contract_version: 1,
                ..PresenceLedger::default()
            },
        }
    }

    #[test]
    fn contract_is_stable_and_requires_citations() {
        let frame = frame();
        assert_eq!(
            compile_presence(&frame, turn(&frame)).unwrap(),
            compile_presence(&frame, turn(&frame)).unwrap()
        );
        let mut missing = turn(&frame);
        missing.directives[0].source_ids = vec!["memory:missing".into()];
        assert!(matches!(
            compile_presence(&frame, missing),
            Err(PresenceError::MissingSource { .. })
        ));
    }

    #[test]
    fn inference_cannot_become_a_hard_rule() {
        let frame = frame();
        let mut request = turn(&frame);
        request.recalled.push(PresenceMaterial {
            id: "inference:one".into(),
            authority: PresenceAuthority::Inference {
                confidence_milli: 900,
            },
            role: PresenceMaterialRole::CurrentState,
            body: "Sol may want a short answer.".into(),
            salience: 500,
        });
        request.directives = vec![directive("inference:one", PresenceSeverity::Hard)];
        assert!(matches!(
            compile_presence(&frame, request),
            Err(PresenceError::InferenceCannotEnforce { .. })
        ));
    }

    #[test]
    fn settle_and_close_preserve_contract_boundaries() {
        let frame = frame();
        let contract = compile_presence(&frame, turn(&frame)).unwrap();
        let missing_guard = PresenceSettleRequest {
            contract_id: contract.contract_id.clone(),
            attempt: 1,
            evaluated_directives: vec![],
            violations: vec![],
            decision: PresenceDecision::Accept,
            response_digest: Some("b".repeat(64)),
        };
        assert!(settle_presence(&contract, missing_guard).is_err());
        let receipt = settle_presence(
            &contract,
            PresenceSettleRequest {
                contract_id: contract.contract_id.clone(),
                attempt: 1,
                evaluated_directives: vec!["directive:identity".into()],
                violations: vec![],
                decision: PresenceDecision::Accept,
                response_digest: Some("b".repeat(64)),
            },
        )
        .unwrap();
        assert_eq!(receipt.decision, PresenceDecision::Accept);
        let closed = close_presence(
            &frame,
            PresenceCloseRequest {
                frame_id: frame.frame_id.clone(),
                body: "letter to the next Kintsu".into(),
                session_ledger: PresenceLedger {
                    frame_version: frame.version,
                    contract_version: 1,
                    ..PresenceLedger::default()
                },
            },
        )
        .unwrap();
        assert_eq!(closed.body, "letter to the next Kintsu");
    }
}
