use std::fmt;

use hearth::RoomKey;
use sha2::{Digest, Sha256};

use super::model::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PresenceError {
    Invalid {
        field: &'static str,
        reason: String,
    },
    ConflictingMaterial {
        material_id: String,
        field: &'static str,
    },
    MissingSource {
        directive_id: String,
        source_id: String,
    },
    InferenceCannotEnforce {
        directive_id: String,
    },
}

impl fmt::Display for PresenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid { field, reason } => write!(f, "invalid Presence {field}: {reason}"),
            Self::ConflictingMaterial { material_id, field } => write!(
                f,
                "Presence material {material_id} appears twice with conflicting {field}"
            ),
            Self::MissingSource {
                directive_id,
                source_id,
            } => {
                write!(
                    f,
                    "Presence directive {directive_id} cites missing source {source_id}"
                )
            }
            Self::InferenceCannotEnforce { directive_id } => write!(
                f,
                "Presence directive {directive_id} cannot enforce inference as a hard rule"
            ),
        }
    }
}

impl std::error::Error for PresenceError {}

// --- shared refusals -------------------------------------------------------
//
// Four sentences the whole domain refuses in. Every bound, every text field,
// every count, and every ceiling in this crate goes through one of them, so a
// refusal reads the same wherever a caller meets it and there is one place to
// change what it says.

pub(crate) fn required(
    field: &'static str,
    value: &str,
    max: usize,
) -> Result<(), PresenceError> {
    let length = value.trim().chars().count();
    if !(1..=max).contains(&length) {
        return Err(invalid(
            field,
            format!("must contain 1 to {max} characters"),
        ));
    }
    Ok(())
}

/// One list is too long.
///
/// enough: five separate `if len > MAX` sites said this, five chances for the
/// sentence to drift while every caller believed it was the same refusal.
pub(crate) fn bound_list(
    field: &'static str,
    len: usize,
    max: usize,
) -> Result<(), PresenceError> {
    if len > max {
        return Err(invalid(field, "contains too many entries"));
    }
    Ok(())
}

fn positive(field: &'static str, value: i64) -> Result<(), PresenceError> {
    if value <= 0 {
        return Err(invalid(field, "must be positive"));
    }
    Ok(())
}

fn at_most(field: &'static str, value: u16, max: u16) -> Result<(), PresenceError> {
    if value > max {
        return Err(invalid(field, format!("must be at most {max}")));
    }
    Ok(())
}

pub(crate) fn sha256(field: &'static str, value: &str) -> Result<(), PresenceError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(invalid(field, "must be a lowercase SHA-256 digest"));
    }
    Ok(())
}

pub(crate) fn invalid(field: &'static str, reason: impl ToString) -> PresenceError {
    PresenceError::Invalid {
        field,
        reason: reason.to_string(),
    }
}

// --- authority and material ------------------------------------------------

/// Check one authority against the facts it declares about itself.
///
/// enough: this was a seven-arm match with inner guards, a third copy of what
/// `priority` and `is_stable_identity` also knew. Order is positive, text,
/// digest, ceiling, which preserves each variant's original precedence: a
/// lesson reports its identifier before its version, an identity its source
/// before its digest.
fn validate_authority(authority: &PresenceAuthority) -> Result<(), PresenceError> {
    let facts = authority.facts();
    [
        facts.positive.map(|(field, value)| positive(field, value)),
        facts
            .text
            .map(|(field, value, max)| required(field, value, max)),
        facts.digest.map(|(field, value)| sha256(field, value)),
        facts
            .ceiling
            .map(|(field, value, max)| at_most(field, value, max)),
    ]
    .into_iter()
    .flatten()
    .try_fold((), |(), checked| checked)
}

type MaterialField = (&'static str, fn(&PresenceMaterial, &PresenceMaterial) -> bool);

/// The fields that make a material the record it is.
///
/// Two rows under one identifier may differ in salience, a retrieval hint, but
/// never in these. The table is what lets the refusal name the field.
const IDENTITY_BEARING_FIELDS: [MaterialField; 3] = [
    ("authority", |left, right| left.authority != right.authority),
    ("role", |left, right| left.role != right.role),
    ("body", |left, right| left.body != right.body),
];

fn conflicting_field(
    existing: &PresenceMaterial,
    candidate: &PresenceMaterial,
) -> Option<&'static str> {
    IDENTITY_BEARING_FIELDS
        .iter()
        .find(|(_, differs)| differs(existing, candidate))
        .map(|(field, _)| *field)
}

pub(crate) fn normalize_materials(
    materials: Vec<PresenceMaterial>,
    required_role: Option<PresenceMaterialRole>,
) -> Result<Vec<PresenceMaterial>, PresenceError> {
    let mut normalized = materials
        .into_iter()
        .map(|material| normalize_material(material, required_role))
        .collect::<Result<Vec<_>, _>>()?;
    finalize_materials(&mut normalized)?;
    Ok(normalized)
}

fn normalize_material(
    mut material: PresenceMaterial,
    required_role: Option<PresenceMaterialRole>,
) -> Result<PresenceMaterial, PresenceError> {
    required("material.id", &material.id, 160)?;
    required("material.body", &material.body, PRESENCE_MAX_BODY_CHARS)?;
    at_most("material.salience", material.salience, 1000)?;
    validate_authority(&material.authority)?;
    if let Some(role) = required_role
        && material.role != role
    {
        return Err(invalid(
            "material.role",
            format!("must be {}", role.as_str()),
        ));
    }
    material.id = material.id.trim().to_owned();
    material.body = material.body.trim().to_owned();
    Ok(material)
}

pub(crate) fn normalize_strings(
    field: &'static str,
    values: Vec<String>,
) -> Result<Vec<String>, PresenceError> {
    bound_list(field, values.len(), PRESENCE_MAX_LIST)?;
    let mut normalized = values
        .into_iter()
        .map(|value| {
            required(field, &value, 512)?;
            Ok(value.trim().to_owned())
        })
        .collect::<Result<Vec<String>, PresenceError>>()?;
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

pub(crate) fn finalize_materials(
    materials: &mut Vec<PresenceMaterial>,
) -> Result<(), PresenceError> {
    collapse_materials(materials)?;
    sort_materials(materials);
    Ok(())
}

/// Collapse repeats of one identifier, or refuse when the repeats disagree.
///
/// Repeats collapse only when they say the same thing. The louder salience
/// wins, so the result does not depend on the order a caller sent.
fn collapse_materials(materials: &mut Vec<PresenceMaterial>) -> Result<(), PresenceError> {
    let collapsed = std::mem::take(materials).into_iter().try_fold(
        Vec::new(),
        |mut kept: Vec<PresenceMaterial>, material| {
            match kept.iter_mut().find(|kept| kept.id == material.id) {
                Some(existing) => merge_or_refuse(existing, material)?,
                None => kept.push(material),
            }
            Ok(kept)
        },
    )?;
    *materials = collapsed;
    Ok(())
}

fn merge_or_refuse(
    existing: &mut PresenceMaterial,
    candidate: PresenceMaterial,
) -> Result<(), PresenceError> {
    if let Some(field) = conflicting_field(existing, &candidate) {
        return Err(PresenceError::ConflictingMaterial {
            material_id: candidate.id,
            field,
        });
    }
    existing.salience = existing.salience.max(candidate.salience);
    Ok(())
}

/// Refuse one identifier that names two different things across every group a
/// caller supplied at once.
///
/// enough: each group is normalized on its own, so a duplicate straddling
/// identity and continuity, or frame material and fresh recall, used to
/// survive into an authority map where the last writer won. Two records under
/// one identifier is a conflict to name, not a collision to order.
pub(crate) fn reject_conflicting_ids(
    groups: &[&[PresenceMaterial]],
) -> Result<(), PresenceError> {
    let mut combined = groups
        .iter()
        .flat_map(|group| group.iter().cloned())
        .collect::<Vec<_>>();
    collapse_materials(&mut combined)
}

pub(crate) fn bound_material_count(
    groups: &[&[PresenceMaterial]],
) -> Result<(), PresenceError> {
    let total = groups.iter().map(|group| group.len()).sum::<usize>();
    bound_list("materials", total, PRESENCE_MAX_MATERIALS)
}

fn sort_materials(materials: &mut [PresenceMaterial]) {
    materials.sort_by(|left, right| {
        left.authority
            .priority()
            .cmp(&right.authority.priority())
            .then_with(|| right.salience.cmp(&left.salience))
            .then_with(|| left.id.cmp(&right.id))
    });
}

pub(crate) fn select_materials(
    materials: Vec<PresenceMaterial>,
    remaining: &mut usize,
    preserve_stable: bool,
) -> Result<Vec<PresenceMaterial>, PresenceError> {
    let mut selected = Vec::new();
    for material in materials {
        let cost = material.body.chars().count();
        if cost <= *remaining {
            *remaining -= cost;
            selected.push(material);
        } else if preserve_stable && material.authority.is_stable_identity() {
            return Err(invalid(
                "identity",
                "stable identity exceeds the packet budget",
            ));
        }
    }
    Ok(selected)
}

// --- binding, frame, and ledger -------------------------------------------

pub(crate) fn validate_binding(binding: &PresenceBinding) -> Result<(), PresenceError> {
    RoomKey::new(binding.room.clone()).map_err(|error| invalid("binding.room", error))?;
    required("binding.spirit", &binding.spirit, 80)?;
    required("binding.operator", &binding.operator, 80)?;
    required("binding.session", &binding.session, 256)
}

/// The request must address the frame that is actually live.
///
/// enough: compile and close each carried their own copy of this pair. A
/// caller that reconnected against a newer frame must be refused by name at
/// both doors, in the same words.
pub(crate) fn require_active_frame(
    frame: &PresenceFrame,
    frame_id: &str,
    frame_version: u32,
) -> Result<(), PresenceError> {
    if frame_id != frame.frame_id {
        return Err(invalid("frameId", "does not name the active frame"));
    }
    if frame_version != frame.version {
        return Err(invalid("frameVersion", "does not match the active frame"));
    }
    Ok(())
}

/// Check the ledger the Host is about to inject.
///
/// The Host authors this, but a pure function still refuses what it cannot
/// stand behind: an unbounded repair list is a Host bug and should surface
/// here rather than in a sealed boat.
pub(crate) fn validate_ledger(ledger: &PresenceLedger) -> Result<(), PresenceError> {
    [
        ("ledger.recentRegisters", &ledger.recent_registers),
        ("ledger.formsOfAddress", &ledger.forms_of_address),
        ("ledger.repairRuleIds", &ledger.repair_rule_ids),
        ("ledger.unresolvedThreads", &ledger.unresolved_threads),
    ]
    .into_iter()
    .try_for_each(|(field, values)| normalize_strings(field, values.clone()).map(drop))?;
    bound_list(
        "ledger.repairRuleIds",
        ledger.repair_rule_ids.len(),
        PRESENCE_MAX_REPAIR_RULES,
    )?;
    bound_list(
        "ledger.relationshipClaims",
        ledger.relationship_claims.len(),
        PRESENCE_MAX_LEDGER_CLAIMS,
    )?;
    normalize_materials(
        ledger.relationship_claims.clone(),
        Some(PresenceMaterialRole::Relationship),
    )
    .map(drop)
}

// --- rendering -------------------------------------------------------------

/// Render one titled list of `- line` rows, or nothing when the list is empty.
///
/// enough: frame material, contract material, contract directives, frame
/// uncertainty, and contract uncertainty were five copies of this loop across
/// two files.
pub(crate) fn render_lines<'a>(
    out: &mut String,
    title: &str,
    lines: impl IntoIterator<Item = &'a str>,
) {
    let body = lines
        .into_iter()
        .map(|line| format!("- {line}\n"))
        .collect::<String>();
    if body.is_empty() {
        return;
    }
    out.push_str(title);
    out.push_str(":\n");
    out.push_str(&body);
}

/// Render a titled list of `- [id] text` rows: a cited line is a labelled line.
pub(crate) fn render_section<'a>(
    out: &mut String,
    title: &str,
    rows: impl IntoIterator<Item = (&'a str, &'a str)>,
) {
    let labelled = rows
        .into_iter()
        .map(|(id, text)| format!("[{id}] {text}"))
        .collect::<Vec<_>>();
    render_lines(out, title, labelled.iter().map(String::as_str));
}

/// Every material as a citable row, for the renderers.
pub(crate) fn material_rows(
    materials: &[PresenceMaterial],
) -> impl Iterator<Item = (&str, &str)> {
    materials
        .iter()
        .map(|material| (material.id.as_str(), material.body.as_str()))
}

/// Every directive as a citable row, for the renderers.
pub(crate) fn directive_rows(
    directives: &[PresenceDirective],
) -> impl Iterator<Item = (&str, &str)> {
    directives
        .iter()
        .map(|directive| (directive.id.as_str(), directive.instruction.as_str()))
}

pub(crate) fn digest(value: &impl serde::Serialize) -> Result<String, PresenceError> {
    let bytes = serde_json::to_vec(value).map_err(|error| invalid("digest", error))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
