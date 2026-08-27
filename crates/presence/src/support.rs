use std::collections::HashSet;
use std::fmt;

use hearth::RoomKey;
use sha2::{Digest, Sha256};

use crate::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PresenceError {
    Invalid {
        field: &'static str,
        reason: String,
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

#[doc(hidden)]
pub fn validate_binding(binding: &PresenceBinding) -> Result<(), PresenceError> {
    RoomKey::new(binding.room.clone()).map_err(|error| invalid("binding.room", error))?;
    required("binding.spirit", &binding.spirit, 80)?;
    required("binding.operator", &binding.operator, 80)?;
    required("binding.session", &binding.session, 256)
}

#[doc(hidden)]
pub fn normalize_materials(
    materials: Vec<PresenceMaterial>,
    required_role: Option<PresenceMaterialRole>,
) -> Result<Vec<PresenceMaterial>, PresenceError> {
    let mut normalized = Vec::with_capacity(materials.len());
    for mut material in materials {
        required("material.id", &material.id, 160)?;
        required("material.body", &material.body, PRESENCE_MAX_BODY_CHARS)?;
        if material.salience > 1000 {
            return Err(invalid("material.salience", "must be at most 1000"));
        }
        validate_authority(&material.authority)?;
        if let Some(role) = required_role
            && material.role != role
        {
            return Err(invalid(
                "material.role",
                format!("must be {}", role_name(role)),
            ));
        }
        material.id = material.id.trim().to_owned();
        material.body = material.body.trim().to_owned();
        normalized.push(material);
    }
    dedupe_materials(&mut normalized);
    sort_materials(&mut normalized);
    Ok(normalized)
}

#[doc(hidden)]
pub fn normalize_strings(
    field: &'static str,
    values: Vec<String>,
) -> Result<Vec<String>, PresenceError> {
    if values.len() > PRESENCE_MAX_LIST {
        return Err(invalid(field, "contains too many entries"));
    }
    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        required(field, &value, 512)?;
        normalized.push(value.trim().to_owned());
    }
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

#[doc(hidden)]
pub fn validate_ledger(ledger: &PresenceLedger) -> Result<(), PresenceError> {
    for (field, values) in [
        ("ledger.recentRegisters", &ledger.recent_registers),
        ("ledger.formsOfAddress", &ledger.forms_of_address),
        ("ledger.repairRuleIds", &ledger.repair_rule_ids),
        ("ledger.unresolvedThreads", &ledger.unresolved_threads),
    ] {
        normalize_strings(field, values.clone())?;
    }
    normalize_materials(
        ledger.relationship_claims.clone(),
        Some(PresenceMaterialRole::Relationship),
    )?;
    Ok(())
}

#[doc(hidden)]
pub fn select_materials(
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

#[doc(hidden)]
pub fn bound_material_count(groups: &[&[PresenceMaterial]]) -> Result<(), PresenceError> {
    if groups.iter().map(|group| group.len()).sum::<usize>() > PRESENCE_MAX_MATERIALS {
        return Err(invalid("materials", "contains too many entries"));
    }
    Ok(())
}

#[doc(hidden)]
pub fn required(field: &'static str, value: &str, max: usize) -> Result<(), PresenceError> {
    let length = value.trim().chars().count();
    if length == 0 || length > max {
        return Err(invalid(
            field,
            format!("must contain 1 to {max} characters"),
        ));
    }
    Ok(())
}

#[doc(hidden)]
pub fn sha256(field: &'static str, value: &str) -> Result<(), PresenceError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(invalid(field, "must be a lowercase SHA-256 digest"));
    }
    Ok(())
}

#[doc(hidden)]
pub fn digest(value: &impl serde::Serialize) -> Result<String, PresenceError> {
    let bytes = serde_json::to_vec(value).map_err(|error| invalid("digest", error))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[doc(hidden)]
pub fn invalid(field: &'static str, reason: impl ToString) -> PresenceError {
    PresenceError::Invalid {
        field,
        reason: reason.to_string(),
    }
}

#[doc(hidden)]
pub fn finalize_materials(materials: &mut Vec<PresenceMaterial>) {
    dedupe_materials(materials);
    sort_materials(materials);
}

fn validate_authority(authority: &PresenceAuthority) -> Result<(), PresenceError> {
    match authority {
        PresenceAuthority::Canon { entity_id } => required("authority.entityId", entity_id, 160),
        PresenceAuthority::Identity {
            source,
            sha256: hash,
        } => {
            required("authority.source", source, 512)?;
            sha256("authority.sha256", hash)
        }
        PresenceAuthority::Memory { memory_id } | PresenceAuthority::PaperBoat { memory_id }
            if *memory_id <= 0 =>
        {
            Err(invalid("authority.memoryId", "must be positive"))
        }
        PresenceAuthority::Lesson { lesson_id, version } => {
            if *lesson_id <= 0 {
                return Err(invalid("authority.lessonId", "must be positive"));
            }
            required("authority.version", version, 80)
        }
        PresenceAuthority::Anamnesis { source } => required("authority.source", source, 512),
        PresenceAuthority::Inference { confidence_milli } if *confidence_milli > 1000 => {
            Err(invalid("authority.confidenceMilli", "must be at most 1000"))
        }
        _ => Ok(()),
    }
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

fn dedupe_materials(materials: &mut Vec<PresenceMaterial>) {
    let mut seen = HashSet::new();
    materials.retain(|material| seen.insert(material.id.clone()));
}

fn role_name(role: PresenceMaterialRole) -> &'static str {
    match role {
        PresenceMaterialRole::Identity => "identity",
        PresenceMaterialRole::Relationship => "relationship",
        PresenceMaterialRole::Counsel => "counsel",
        PresenceMaterialRole::Continuity => "continuity",
        PresenceMaterialRole::Rule => "rule",
        PresenceMaterialRole::Exemplar => "exemplar",
        PresenceMaterialRole::CurrentState => "current_state",
    }
}
