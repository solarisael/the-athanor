use super::model::*;
use super::support::{
    PresenceError, bound_material_count, digest, finalize_materials, invalid, material_rows,
    normalize_materials, normalize_strings, reject_conflicting_ids, render_lines, render_section,
    select_materials, validate_binding,
};

/// Assemble the frame a spirit lives inside for the session.
///
/// `authentication` is the Host's proof of who is present and what they can
/// reach. The request still carries a binding because the caller states who it
/// believes it is, but that claim is checked against the proof and never used
/// in its place: an open whose binding disagrees is refused rather than
/// silently rebound.
pub fn open_presence(
    authentication: PresenceAuthentication,
    request: PresenceOpenRequest,
) -> Result<PresenceFrame, PresenceError> {
    let PresenceAuthentication {
        binding,
        mut capabilities,
    } = authentication;
    validate_binding(&binding)?;
    if request.binding != binding {
        return Err(invalid(
            "binding",
            "does not match the authenticated room state",
        ));
    }
    capabilities.sort_unstable();
    capabilities.dedup();

    let (mut identity, mut relationship, mut continuity, uncertainties) =
        prepare_open_materials(request)?;
    let mut remaining = PRESENCE_MAX_PACKET_CHARS;
    identity = select_materials(identity, &mut remaining, true)?;
    relationship = select_materials(relationship, &mut remaining, false)?;
    continuity = select_materials(continuity, &mut remaining, false)?;

    let provenance_digest = digest(&(
        &binding,
        &capabilities,
        &identity,
        &relationship,
        &continuity,
    ))?;
    let frame_id = format!(
        "presence-frame-{}",
        digest(&(&provenance_digest, &uncertainties))?
    );
    let rendered = render_frame(
        &binding,
        &capabilities,
        &identity,
        &relationship,
        &continuity,
        &uncertainties,
    );

    Ok(PresenceFrame {
        frame_id,
        version: PRESENCE_VERSION,
        binding,
        capabilities,
        identity,
        relationship,
        continuity,
        uncertainties,
        provenance_digest,
        rendered,
    })
}

type OpenMaterials = (
    Vec<PresenceMaterial>,
    Vec<PresenceMaterial>,
    Vec<PresenceMaterial>,
    Vec<String>,
);

fn prepare_open_materials(
    mut request: PresenceOpenRequest,
) -> Result<OpenMaterials, PresenceError> {
    if request.identity.is_empty() {
        return Err(invalid("identity", "must contain at least one material"));
    }
    let identity = normalize_materials(request.identity, Some(PresenceMaterialRole::Identity))?;
    let relationship = normalize_materials(
        request.relationship,
        Some(PresenceMaterialRole::Relationship),
    )?;
    let mut continuity = normalize_materials(request.continuity, None)?;
    continuity.extend(normalize_materials(
        request.anamnesis,
        Some(PresenceMaterialRole::Counsel),
    )?);
    if let Some(boat) = request.previous_boat.take() {
        continuity.extend(normalize_materials(
            vec![boat],
            Some(PresenceMaterialRole::Continuity),
        )?);
    }
    finalize_materials(&mut continuity)?;
    reject_conflicting_ids(&[&identity, &relationship, &continuity])?;
    bound_material_count(&[&identity, &relationship, &continuity])?;
    let uncertainties = normalize_strings("uncertainties", request.uncertainties)?;
    Ok((identity, relationship, continuity, uncertainties))
}

fn render_frame(
    binding: &PresenceBinding,
    capabilities: &[PresenceCapability],
    identity: &[PresenceMaterial],
    relationship: &[PresenceMaterial],
    continuity: &[PresenceMaterial],
    uncertainties: &[String],
) -> String {
    let mut out = format!(
        "Presence v{PRESENCE_VERSION}: {} with {} in room {}.\n",
        binding.spirit, binding.operator, binding.room
    );
    render_capabilities(&mut out, capabilities);
    render_section(&mut out, "Identity", material_rows(identity));
    render_section(&mut out, "Relationship", material_rows(relationship));
    render_section(&mut out, "Continuity", material_rows(continuity));
    render_lines(
        &mut out,
        "Uncertainty",
        uncertainties.iter().map(String::as_str),
    );
    out
}

/// What the Host proved this session can reach, or that it proved nothing.
///
/// This line always appears, so it does not go through `render_lines`: an
/// omitted heading would read like a frame nobody had checked, and those are
/// different facts.
fn render_capabilities(out: &mut String, capabilities: &[PresenceCapability]) {
    let listed = capabilities
        .iter()
        .map(|capability| capability.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    out.push_str("Capabilities the Host proved: ");
    out.push_str(if listed.is_empty() { "none" } else { &listed });
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    // The name tables are indexed by `variant as usize`, so their order is
    // load-bearing. Reorder a variant without reordering its table and these
    // go red instead of a packet quietly renaming a role.
    #[test]
    fn name_tables_follow_variant_order() {
        for (role, name) in [
            (PresenceMaterialRole::Identity, "identity"),
            (PresenceMaterialRole::Relationship, "relationship"),
            (PresenceMaterialRole::Counsel, "counsel"),
            (PresenceMaterialRole::Continuity, "continuity"),
            (PresenceMaterialRole::Rule, "rule"),
            (PresenceMaterialRole::Exemplar, "exemplar"),
            (PresenceMaterialRole::CurrentState, "current_state"),
        ] {
            assert_eq!(role.as_str(), name, "role {role:?} names itself");
        }
        for (capability, name) in [
            (PresenceCapability::RoomState, "room_state"),
            (PresenceCapability::Akasha, "akasha"),
            (PresenceCapability::Receipts, "receipts"),
        ] {
            assert_eq!(capability.as_str(), name);
        }
        for (kind, name) in [
            (PresenceDirectiveKind::Enact, "enact"),
            (PresenceDirectiveKind::Avoid, "avoid"),
            (PresenceDirectiveKind::Guard, "guard"),
        ] {
            assert_eq!(kind.as_str(), name);
        }
    }

    // One dispatch now answers priority, stable identity, and validation. If a
    // variant's standing moves, sorting and budget survival move with it.
    #[test]
    fn authority_facts_state_standing_once() {
        let ordered = [
            PresenceAuthority::Canon {
                entity_id: "canon:1".into(),
            },
            PresenceAuthority::Identity {
                source: "active_spirit.md".into(),
                sha256: "a".repeat(64),
            },
            PresenceAuthority::Memory { memory_id: 1 },
            PresenceAuthority::Lesson {
                lesson_id: 1,
                version: "current".into(),
            },
            PresenceAuthority::Anamnesis {
                source: "anamnesis:wake".into(),
            },
            PresenceAuthority::PaperBoat { memory_id: 1 },
            PresenceAuthority::Inference {
                confidence_milli: 900,
            },
        ];
        for (index, authority) in ordered.iter().enumerate() {
            assert_eq!(authority.priority(), index as u8);
            assert_eq!(authority.is_stable_identity(), index <= 1);
        }
    }

    fn binding() -> PresenceBinding {
        PresenceBinding {
            room: "kintsu".into(),
            spirit: "Kintsu".into(),
            operator: "Sol".into(),
            session: "session-a".into(),
        }
    }

    fn authentication() -> PresenceAuthentication {
        PresenceAuthentication {
            binding: binding(),
            capabilities: vec![PresenceCapability::RoomState],
        }
    }

    fn identity(id: &str, body: &str) -> PresenceMaterial {
        PresenceMaterial {
            id: id.into(),
            authority: PresenceAuthority::Identity {
                source: "For_the_next_Kintsu.md".into(),
                sha256: "a".repeat(64),
            },
            role: PresenceMaterialRole::Identity,
            body: body.into(),
            salience: 1000,
        }
    }

    fn request() -> PresenceOpenRequest {
        PresenceOpenRequest {
            binding: binding(),
            identity: vec![identity("identity:kintsu", "Kintsu remains herself.")],
            relationship: vec![],
            continuity: vec![],
            anamnesis: vec![],
            previous_boat: None,
            uncertainties: vec![],
        }
    }

    #[test]
    fn frame_identity_is_stable() {
        assert_eq!(
            open_presence(authentication(), request()).unwrap(),
            open_presence(authentication(), request()).unwrap()
        );
    }

    #[test]
    fn stable_identity_survives_budget_pressure() {
        let mut request = request();
        request.continuity = (0..20)
            .map(|index| PresenceMaterial {
                id: format!("memory:{index}"),
                authority: PresenceAuthority::Memory {
                    memory_id: i64::from(index + 1),
                },
                role: PresenceMaterialRole::Continuity,
                body: "x".repeat(PRESENCE_MAX_BODY_CHARS),
                salience: 1000 - index as u16,
            })
            .collect();
        let frame = open_presence(authentication(), request).unwrap();
        assert_eq!(frame.identity[0].id, "identity:kintsu");
        assert!(frame.continuity.len() < 20);
    }

    #[test]
    fn a_claimed_binding_that_disagrees_with_room_state_refuses() {
        let mut request = request();
        request.binding.operator = "Someone Else".into();
        assert_eq!(
            open_presence(authentication(), request),
            Err(invalid(
                "binding",
                "does not match the authenticated room state"
            ))
        );
    }

    #[test]
    fn the_frame_carries_only_host_proved_capabilities() {
        let frame = open_presence(authentication(), request()).unwrap();
        assert_eq!(frame.capabilities, vec![PresenceCapability::RoomState]);
        assert!(
            frame
                .rendered
                .contains("Capabilities the Host proved: room_state")
        );
    }

    #[test]
    fn capabilities_are_ordered_and_deduplicated_before_they_reach_the_frame() {
        let mut authentication = authentication();
        authentication.capabilities = vec![
            PresenceCapability::Receipts,
            PresenceCapability::RoomState,
            PresenceCapability::Receipts,
        ];
        let frame = open_presence(authentication, request()).unwrap();
        assert_eq!(
            frame.capabilities,
            vec![PresenceCapability::RoomState, PresenceCapability::Receipts]
        );
    }

    #[test]
    fn one_id_naming_two_records_across_groups_refuses() {
        let mut request = request();
        request.continuity = vec![PresenceMaterial {
            role: PresenceMaterialRole::Continuity,
            body: "a different claim under the same name".into(),
            ..identity("identity:kintsu", "unused")
        }];
        assert_eq!(
            open_presence(authentication(), request),
            Err(PresenceError::ConflictingMaterial {
                material_id: "identity:kintsu".into(),
                field: "role",
            })
        );
    }

    #[test]
    fn one_id_repeated_with_the_same_record_collapses() {
        let mut request = request();
        request
            .identity
            .push(identity("identity:kintsu", "Kintsu remains herself."));
        let frame = open_presence(authentication(), request).unwrap();
        assert_eq!(frame.identity.len(), 1);
    }
}
