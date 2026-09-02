use std::collections::{HashMap, HashSet};

use super::model::*;
use super::support::{
    PresenceError, bound_list, digest, directive_rows, invalid, material_rows, normalize_materials,
    normalize_strings, reject_conflicting_ids, render_lines, render_section, require_active_frame,
    required, sha256, validate_ledger,
};

/// Compile one turn's contract against the frame and the Host's ledger.
///
/// `ledger` arrives from the Host, never from the caller. The request carries
/// only the frame version it believes it is talking to, so a stale caller is
/// refused by name instead of authoring the session's own memory of itself.
pub fn compile_presence(
    frame: &PresenceFrame,
    ledger: &PresenceLedger,
    mut request: PresenceTurnRequest,
) -> Result<PresenceContract, PresenceError> {
    required("frameId", &request.frame_id, 160)?;
    required("turnId", &request.turn_id, 160)?;
    required("userText", &request.user_text, 8192)?;
    validate_ledger(ledger)?;
    require_active_frame(frame, &request.frame_id, request.frame_version)?;

    request.recalled = normalize_materials(request.recalled, None)?;
    request.lessons = normalize_materials(request.lessons, Some(PresenceMaterialRole::Rule))?;
    reject_conflicting_ids(&[
        &frame.identity,
        &frame.relationship,
        &frame.continuity,
        &request.recalled,
        &request.lessons,
        &ledger.relationship_claims,
    ])?;
    let sources = source_authorities(frame, &request.recalled, &request.lessons, ledger)?;
    let directives = normalize_directives(request.directives, &sources)?;
    let (must_enact, must_avoid, guards) = group_directives(directives);
    let exemplars = select_exemplars(&request.recalled, &request.lessons)?;
    let provenance = directive_sources(&must_enact, &must_avoid, &guards);
    let uncertainties = normalize_strings("uncertainties", frame.uncertainties.clone())?;
    let digest = digest(&(
        &frame.frame_id,
        &request.turn_id,
        &request.user_text,
        &ledger.contract_version,
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
        contract_version: ledger.contract_version,
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
    if !(1..=PRESENCE_MAX_ATTEMPTS).contains(&request.attempt) {
        return Err(invalid("attempt", "must be 1 or 2"));
    }
    request.evaluated_directives =
        normalize_strings("evaluatedDirectives", request.evaluated_directives)?;
    let known = hard_and_soft_directives(contract)
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

/// Every directive the contract issued, in one sequence.
///
/// Guards were the only group swept, which let a hard `must_enact` or
/// `must_avoid` rule pass unevaluated and still be called Accept.
fn hard_and_soft_directives(
    contract: &PresenceContract,
) -> impl Iterator<Item = &PresenceDirective> {
    contract
        .must_enact
        .iter()
        .chain(&contract.must_avoid)
        .chain(&contract.guards)
}

type AcceptRequirement = (
    &'static str,
    &'static str,
    fn(&PresenceSettleRequest) -> bool,
);

/// What Accept means, stated once.
///
/// Each row is a way an Accept is false on its face, before any directive is
/// considered.
const ACCEPT_REQUIREMENTS: [AcceptRequirement; 2] = [
    ("decision", "accept cannot carry violations", |request| {
        !request.violations.is_empty()
    }),
    (
        "responseDigest",
        "accept requires the emitted response digest",
        |request| request.response_digest.is_none(),
    ),
];

fn validate_acceptance(
    contract: &PresenceContract,
    request: &PresenceSettleRequest,
) -> Result<(), PresenceError> {
    if request.decision != PresenceDecision::Accept {
        return Ok(());
    }
    if let Some((field, reason, _)) = ACCEPT_REQUIREMENTS
        .iter()
        .find(|(_, _, unmet)| unmet(request))
    {
        return Err(invalid(field, *reason));
    }
    let evaluated = request
        .evaluated_directives
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    if let Some(missing) = hard_and_soft_directives(contract).find(|directive| {
        directive.severity == PresenceSeverity::Hard && !evaluated.contains(directive.id.as_str())
    }) {
        return Err(invalid(
            "evaluatedDirectives",
            format!(
                "hard {} directive {} was not evaluated",
                missing.kind.as_str(),
                missing.id
            ),
        ));
    }
    Ok(())
}

/// Seal the close material against the Host's own ledger.
pub fn close_presence(
    frame: &PresenceFrame,
    ledger: &PresenceLedger,
    request: PresenceCloseRequest,
) -> Result<PresenceCloseMaterial, PresenceError> {
    require_active_frame(frame, &request.frame_id, request.frame_version)?;
    required("body", &request.body, PRESENCE_MAX_CLOSE_BODY_BYTES)?;
    validate_ledger(ledger)?;
    let provenance_digest = digest(&(&frame.provenance_digest, ledger, &request.body))?;
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
    bound_list("directives", directives.len(), PRESENCE_MAX_DIRECTIVES)?;
    let mut ids = HashSet::new();
    let mut normalized = directives
        .into_iter()
        .map(|directive| {
            let named = directive.id.trim().to_owned();
            if !ids.insert(named) {
                return Err(invalid("directives", "contains duplicate IDs"));
            }
            normalize_directive(directive, sources)
        })
        .collect::<Result<Vec<_>, _>>()?;
    normalized.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(normalized)
}

fn normalize_directive(
    mut directive: PresenceDirective,
    sources: &HashMap<String, PresenceAuthority>,
) -> Result<PresenceDirective, PresenceError> {
    required("directive.id", &directive.id, 160)?;
    required("directive.instruction", &directive.instruction, 1000)?;
    directive.source_ids = normalize_strings("directive.sourceIds", directive.source_ids)?;
    if directive.source_ids.is_empty() {
        return Err(invalid(
            "directive.sourceIds",
            "must cite at least one source",
        ));
    }
    validate_directive_sources(&directive, sources)?;
    directive.trigger_scope = normalize_strings("directive.triggerScope", directive.trigger_scope)?;
    directive.id = directive.id.trim().to_owned();
    directive.instruction = directive.instruction.trim().to_owned();
    Ok(directive)
}

fn validate_directive_sources(
    directive: &PresenceDirective,
    sources: &HashMap<String, PresenceAuthority>,
) -> Result<(), PresenceError> {
    directive.source_ids.iter().try_for_each(|source_id| {
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
        Ok(())
    })
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
    request.violations.iter().try_for_each(|violation| {
        required("violation.directiveId", &violation.directive_id, 160)?;
        required("violation.reason", &violation.reason, 512)?;
        if !known.contains(violation.directive_id.as_str()) {
            return Err(invalid(
                "violations",
                format!("contains unknown directive {}", violation.directive_id),
            ));
        }
        Ok(())
    })
}

/// Which authority stands behind each citable source identifier.
///
/// enough: this used to `collect()` into a map, so when two records shared an
/// identifier the last one silently became the authority every hard directive
/// was checked against — an inference could inherit canon's standing by being
/// listed second. Refusing when an insert displaces a different authority
/// makes that impossible whatever order the caller sent.
fn source_authorities(
    frame: &PresenceFrame,
    recalled: &[PresenceMaterial],
    lessons: &[PresenceMaterial],
    ledger: &PresenceLedger,
) -> Result<HashMap<String, PresenceAuthority>, PresenceError> {
    frame
        .identity
        .iter()
        .chain(&frame.relationship)
        .chain(&frame.continuity)
        .chain(recalled)
        .chain(lessons)
        .chain(&ledger.relationship_claims)
        .try_fold(HashMap::new(), |mut sources, material| {
            let displaced = sources.insert(material.id.clone(), material.authority.clone());
            match displaced {
                Some(previous) if previous != material.authority => {
                    Err(PresenceError::ConflictingMaterial {
                        material_id: material.id.clone(),
                        field: "authority",
                    })
                }
                _ => Ok(sources),
            }
        })
}

/// Split directives into the three groups a contract issues.
///
/// One filter per kind rather than a loop with a three-arm match.
fn group_directives(
    directives: Vec<PresenceDirective>,
) -> (
    Vec<PresenceDirective>,
    Vec<PresenceDirective>,
    Vec<PresenceDirective>,
) {
    let of_kind = |kind: PresenceDirectiveKind| {
        directives
            .iter()
            .filter(|directive| directive.kind == kind)
            .cloned()
            .collect::<Vec<_>>()
    };
    (
        of_kind(PresenceDirectiveKind::Enact),
        of_kind(PresenceDirectiveKind::Avoid),
        of_kind(PresenceDirectiveKind::Guard),
    )
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
    render_section(&mut out, "Must enact", directive_rows(enact));
    render_section(&mut out, "Must avoid", directive_rows(avoid));
    render_section(&mut out, "Guards", directive_rows(guards));
    render_section(&mut out, "Examples", material_rows(exemplars));
    render_lines(
        &mut out,
        "Keep uncertain",
        uncertainties.iter().map(String::as_str),
    );
    out
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
    use super::super::open_presence;
    use super::*;

    fn authentication() -> PresenceAuthentication {
        PresenceAuthentication {
            binding: PresenceBinding {
                room: "kintsu".into(),
                spirit: "Kintsu".into(),
                operator: "Sol".into(),
                session: "session-a".into(),
            },
            capabilities: vec![PresenceCapability::RoomState],
        }
    }

    fn frame() -> PresenceFrame {
        let authentication = authentication();
        let request = PresenceOpenRequest {
            binding: authentication.binding.clone(),
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
        };
        open_presence(authentication, request).unwrap()
    }

    fn ledger(frame: &PresenceFrame) -> PresenceLedger {
        PresenceLedger {
            frame_version: frame.version,
            contract_version: 1,
            ..PresenceLedger::default()
        }
    }

    fn directive(
        source: &str,
        kind: PresenceDirectiveKind,
        severity: PresenceSeverity,
    ) -> PresenceDirective {
        PresenceDirective {
            id: format!("directive:{}", kind.as_str()),
            kind,
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
            directives: vec![directive(
                "identity:kintsu",
                PresenceDirectiveKind::Guard,
                PresenceSeverity::Hard,
            )],
            frame_version: frame.version,
        }
    }

    #[test]
    fn contract_is_stable_and_requires_citations() {
        let frame = frame();
        assert_eq!(
            compile_presence(&frame, &ledger(&frame), turn(&frame)).unwrap(),
            compile_presence(&frame, &ledger(&frame), turn(&frame)).unwrap()
        );
        let mut missing = turn(&frame);
        missing.directives[0].source_ids = vec!["memory:missing".into()];
        assert!(matches!(
            compile_presence(&frame, &ledger(&frame), missing),
            Err(PresenceError::MissingSource { .. })
        ));
    }

    #[test]
    fn a_stale_frame_version_refuses_by_name() {
        let frame = frame();
        let mut stale = turn(&frame);
        stale.frame_version = frame.version + 1;
        assert_eq!(
            compile_presence(&frame, &ledger(&frame), stale),
            Err(invalid("frameVersion", "does not match the active frame"))
        );
    }

    #[test]
    fn the_contract_carries_the_host_ledger_contract_version() {
        let frame = frame();
        let mut ledger = ledger(&frame);
        ledger.contract_version = 4;
        let contract = compile_presence(&frame, &ledger, turn(&frame)).unwrap();
        assert_eq!(contract.contract_version, 4);
    }

    #[test]
    fn a_new_contract_version_yields_a_new_contract_identity() {
        let frame = frame();
        let first = compile_presence(&frame, &ledger(&frame), turn(&frame)).unwrap();
        let mut later = ledger(&frame);
        later.contract_version = 2;
        let second = compile_presence(&frame, &later, turn(&frame)).unwrap();
        assert_ne!(first.contract_id, second.contract_id);
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
        request.directives = vec![directive(
            "inference:one",
            PresenceDirectiveKind::Guard,
            PresenceSeverity::Hard,
        )];
        assert!(matches!(
            compile_presence(&frame, &ledger(&frame), request),
            Err(PresenceError::InferenceCannotEnforce { .. })
        ));
    }

    #[test]
    fn one_source_id_carrying_two_authorities_refuses() {
        let frame = frame();
        let mut request = turn(&frame);
        request.recalled.push(PresenceMaterial {
            id: "identity:kintsu".into(),
            authority: PresenceAuthority::Inference {
                confidence_milli: 900,
            },
            role: PresenceMaterialRole::Identity,
            body: "Kintsu meets Sol directly.".into(),
            salience: 1000,
        });
        assert_eq!(
            compile_presence(&frame, &ledger(&frame), request),
            Err(PresenceError::ConflictingMaterial {
                material_id: "identity:kintsu".into(),
                field: "authority",
            })
        );
    }

    #[test]
    fn accept_must_evaluate_every_hard_directive_in_all_three_groups() {
        let frame = frame();
        let mut request = turn(&frame);
        request.directives = vec![
            directive(
                "identity:kintsu",
                PresenceDirectiveKind::Enact,
                PresenceSeverity::Hard,
            ),
            directive(
                "identity:kintsu",
                PresenceDirectiveKind::Avoid,
                PresenceSeverity::Hard,
            ),
            directive(
                "identity:kintsu",
                PresenceDirectiveKind::Guard,
                PresenceSeverity::Hard,
            ),
        ];
        let contract = compile_presence(&frame, &ledger(&frame), request).unwrap();
        assert_eq!(contract.must_enact.len(), 1);
        assert_eq!(contract.must_avoid.len(), 1);
        assert_eq!(contract.guards.len(), 1);

        for named in ["directive:enact", "directive:avoid", "directive:guard"] {
            let partial = ["directive:enact", "directive:avoid", "directive:guard"]
                .into_iter()
                .filter(|id| *id != named)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let error = settle_presence(
                &contract,
                PresenceSettleRequest {
                    contract_id: contract.contract_id.clone(),
                    attempt: 1,
                    evaluated_directives: partial,
                    violations: vec![],
                    decision: PresenceDecision::Accept,
                    response_digest: Some("b".repeat(64)),
                },
            )
            .expect_err("an unevaluated hard directive cannot be accepted");
            assert!(
                error.to_string().contains(named),
                "refusal must name {named}, said: {error}"
            );
        }

        let receipt = settle_presence(
            &contract,
            PresenceSettleRequest {
                contract_id: contract.contract_id.clone(),
                attempt: 1,
                evaluated_directives: vec![
                    "directive:enact".into(),
                    "directive:avoid".into(),
                    "directive:guard".into(),
                ],
                violations: vec![],
                decision: PresenceDecision::Accept,
                response_digest: Some("b".repeat(64)),
            },
        )
        .unwrap();
        assert_eq!(receipt.decision, PresenceDecision::Accept);
    }

    #[test]
    fn a_hard_violation_cannot_be_accepted() {
        let frame = frame();
        let contract = compile_presence(&frame, &ledger(&frame), turn(&frame)).unwrap();
        assert_eq!(
            settle_presence(
                &contract,
                PresenceSettleRequest {
                    contract_id: contract.contract_id.clone(),
                    attempt: 1,
                    evaluated_directives: vec!["directive:guard".into()],
                    violations: vec![PresenceViolation {
                        directive_id: "directive:guard".into(),
                        reason: "the response was empty".into(),
                    }],
                    decision: PresenceDecision::Accept,
                    response_digest: Some("b".repeat(64)),
                },
            ),
            Err(invalid("decision", "accept cannot carry violations"))
        );
    }

    #[test]
    fn a_hard_violation_settles_as_a_refusal() {
        let frame = frame();
        let contract = compile_presence(&frame, &ledger(&frame), turn(&frame)).unwrap();
        let receipt = settle_presence(
            &contract,
            PresenceSettleRequest {
                contract_id: contract.contract_id.clone(),
                attempt: 1,
                evaluated_directives: vec!["directive:guard".into()],
                violations: vec![PresenceViolation {
                    directive_id: "directive:guard".into(),
                    reason: "the response was empty".into(),
                }],
                decision: PresenceDecision::Refuse,
                response_digest: None,
            },
        )
        .unwrap();
        assert_eq!(receipt.decision, PresenceDecision::Refuse);
        assert_eq!(receipt.violations.len(), 1);
    }

    #[test]
    fn close_seals_the_body_against_the_host_ledger() {
        let frame = frame();
        let mut sealed = ledger(&frame);
        sealed.repair_rule_ids = vec!["directive:guard".into()];
        let closed = close_presence(
            &frame,
            &sealed,
            PresenceCloseRequest {
                frame_id: frame.frame_id.clone(),
                body: "letter to the next Kintsu".into(),
                frame_version: frame.version,
            },
        )
        .unwrap();
        assert_eq!(closed.body, "letter to the next Kintsu");

        let plain = close_presence(
            &frame,
            &ledger(&frame),
            PresenceCloseRequest {
                frame_id: frame.frame_id.clone(),
                body: "letter to the next Kintsu".into(),
                frame_version: frame.version,
            },
        )
        .unwrap();
        assert_ne!(closed.provenance_digest, plain.provenance_digest);
    }

    #[test]
    fn an_over_full_repair_list_refuses_before_it_seals() {
        let frame = frame();
        let mut swollen = ledger(&frame);
        swollen.repair_rule_ids = (0..=PRESENCE_MAX_REPAIR_RULES)
            .map(|index| format!("rule:{index}"))
            .collect();
        assert_eq!(
            close_presence(
                &frame,
                &swollen,
                PresenceCloseRequest {
                    frame_id: frame.frame_id.clone(),
                    body: "letter".into(),
                    frame_version: frame.version,
                },
            ),
            Err(invalid("ledger.repairRuleIds", "contains too many entries"))
        );
    }
}
