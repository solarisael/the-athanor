use presence::internal::{
    bound_material_count, digest, finalize_materials, invalid, normalize_materials,
    normalize_strings, select_materials, validate_binding,
};
use presence::*;

pub fn open_presence(request: PresenceOpenRequest) -> Result<PresenceFrame, PresenceError> {
    let (binding, mut identity, mut relationship, mut continuity, uncertainties) =
        prepare_open_materials(request)?;
    let mut remaining = PRESENCE_MAX_PACKET_CHARS;
    identity = select_materials(identity, &mut remaining, true)?;
    relationship = select_materials(relationship, &mut remaining, false)?;
    continuity = select_materials(continuity, &mut remaining, false)?;

    let provenance_digest = digest(&(&binding, &identity, &relationship, &continuity))?;
    let frame_id = format!(
        "presence-frame-{}",
        digest(&(&provenance_digest, &uncertainties))?
    );
    let rendered = render_frame(
        &binding,
        &identity,
        &relationship,
        &continuity,
        &uncertainties,
    );

    Ok(PresenceFrame {
        frame_id,
        version: PRESENCE_VERSION,
        binding,
        identity,
        relationship,
        continuity,
        uncertainties,
        provenance_digest,
        rendered,
    })
}

type OpenMaterials = (
    PresenceBinding,
    Vec<PresenceMaterial>,
    Vec<PresenceMaterial>,
    Vec<PresenceMaterial>,
    Vec<String>,
);

fn prepare_open_materials(
    mut request: PresenceOpenRequest,
) -> Result<OpenMaterials, PresenceError> {
    validate_binding(&request.binding)?;
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
    finalize_materials(&mut continuity);
    bound_material_count(&[&identity, &relationship, &continuity])?;
    let uncertainties = normalize_strings("uncertainties", request.uncertainties)?;
    Ok((
        request.binding,
        identity,
        relationship,
        continuity,
        uncertainties,
    ))
}

fn render_frame(
    binding: &PresenceBinding,
    identity: &[PresenceMaterial],
    relationship: &[PresenceMaterial],
    continuity: &[PresenceMaterial],
    uncertainties: &[String],
) -> String {
    let mut out = format!(
        "Presence v{PRESENCE_VERSION}: {} with {} in room {}.\n",
        binding.spirit, binding.operator, binding.room
    );
    render_material_section(&mut out, "Identity", identity);
    render_material_section(&mut out, "Relationship", relationship);
    render_material_section(&mut out, "Continuity", continuity);
    if !uncertainties.is_empty() {
        out.push_str("Uncertainty:\n");
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

#[cfg(test)]
mod tests {
    use super::*;

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
            binding: PresenceBinding {
                room: "kintsu".into(),
                spirit: "Kintsu".into(),
                operator: "Sol".into(),
                session: "session-a".into(),
            },
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
            open_presence(request()).unwrap(),
            open_presence(request()).unwrap()
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
        let frame = open_presence(request).unwrap();
        assert_eq!(frame.identity[0].id, "identity:kintsu");
        assert!(frame.continuity.len() < 20);
    }
}
