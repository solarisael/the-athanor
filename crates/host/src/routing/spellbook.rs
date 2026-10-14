use super::worker_lane;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Familiar {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub lane: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub omp_agent: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model_role: String,
    pub description: String,
    #[serde(default)]
    pub temperament: Option<String>,
    #[serde(default)]
    pub appearance: Option<String>,
}

/// The effective spellbook: the House page with the room page laid over it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Spellbook {
    pub version: u8,
    pub collective: String,
    pub collective_aliases: Vec<String>,
    pub spellbook_aliases: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub guide: String,
    pub familiars: Vec<Familiar>,
}

/// One spellbook file as written. A key the page omits inherits the House
/// value; a key it names replaces that value whole, an empty value included.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SpellbookPage {
    #[serde(default, deserialize_with = "present")]
    pub version: Option<u8>,
    #[serde(default, deserialize_with = "present")]
    pub collective: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub collective_aliases: Option<Vec<String>>,
    #[serde(default, deserialize_with = "present")]
    pub spellbook_aliases: Option<Vec<String>>,
    #[serde(default, deserialize_with = "present")]
    pub guide: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub familiars: Option<Vec<Familiar>>,
}

// Missing keys inherit; an explicit null must not silently erase an override.
fn present<'de, T: Deserialize<'de>, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

impl SpellbookPage {
    /// Familiars are keyed by id: a room entry replaces the House entry with
    /// the same id whole, and an explicit empty list clears the chapter.
    pub fn over(self, house: SpellbookPage) -> SpellbookPage {
        let familiars = match (self.familiars, house.familiars) {
            (Some(room), Some(house)) if !room.is_empty() => Some(overlay_familiars(house, room)),
            (room, house) => room.or(house),
        };
        SpellbookPage {
            version: self.version.or(house.version),
            collective: self.collective.or(house.collective),
            collective_aliases: self.collective_aliases.or(house.collective_aliases),
            spellbook_aliases: self.spellbook_aliases.or(house.spellbook_aliases),
            guide: self.guide.or(house.guide),
            familiars,
        }
    }
}

fn overlay_familiars(mut house: Vec<Familiar>, room: Vec<Familiar>) -> Vec<Familiar> {
    for familiar in room {
        match house
            .iter_mut()
            .find(|inherited| inherited.id.trim() == familiar.id.trim())
        {
            Some(inherited) => *inherited = familiar,
            None => house.push(familiar),
        }
    }
    house
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamiliarStatus {
    pub ok: bool,
    pub errors: Vec<String>,
    pub spellbook: Option<Spellbook>,
}

fn unique_nonempty(values: &mut Vec<String>) {
    let mut seen = HashMap::<String, ()>::new();
    values.retain(|value| {
        let value = value.trim();
        !value.is_empty() && seen.insert(value.to_owned(), ()).is_none()
    });
}

fn spellbook_errors(spellbook: &mut Spellbook) -> Vec<String> {
    let mut errors = Vec::new();
    if spellbook.version != 1 {
        errors.push("Familiar spellbook version must be 1.".into());
    }
    spellbook.collective = spellbook.collective.trim().into();
    if spellbook.collective.is_empty() && !spellbook.familiars.is_empty() {
        errors.push("Familiar spellbook collective is required when it binds familiars.".into());
    }
    spellbook.guide = spellbook.guide.trim().into();
    unique_nonempty(&mut spellbook.collective_aliases);
    unique_nonempty(&mut spellbook.spellbook_aliases);
    errors
}

fn normalize_familiar(familiar: &mut Familiar) {
    familiar.id = familiar.id.trim().into();
    familiar.name = familiar.name.trim().into();
    familiar.lane = familiar.lane.trim().into();
    familiar.omp_agent = familiar.omp_agent.trim().into();
    familiar.model_role = familiar.model_role.trim().into();
    familiar.description = familiar.description.trim().into();
    unique_nonempty(&mut familiar.aliases);
}

fn familiar_identity_errors(index: usize, familiar: &Familiar) -> Vec<String> {
    let mut errors = Vec::new();
    if familiar.id.is_empty() {
        errors.push(format!("Familiar at index {index} requires an id."));
    } else if !valid_familiar_id(&familiar.id) {
        errors.push(format!(
            "Familiar id '{}' must use lowercase kebab-case.",
            familiar.id
        ));
    }
    if familiar.name.is_empty() {
        errors.push(format!(
            "Familiar '{}' requires a name.",
            if familiar.id.is_empty() {
                index.to_string()
            } else {
                familiar.id.clone()
            }
        ));
    }
    errors
}

fn familiar_lane_errors(familiar: &Familiar) -> Vec<String> {
    let mut errors = Vec::new();
    if worker_lane(&familiar.lane).is_none() {
        errors.push(format!(
            "Familiar '{}' uses unknown worker lane '{}'.",
            familiar.id,
            if familiar.lane.is_empty() {
                "<empty>"
            } else {
                &familiar.lane
            }
        ));
    }
    errors
}

fn familiar_route_errors(familiar: &Familiar) -> Vec<String> {
    let mut errors = Vec::new();
    if familiar.omp_agent.is_empty() != familiar.model_role.is_empty() {
        errors.push(format!(
            "Familiar '{}' must provide ompAgent and modelRole together.",
            familiar.id
        ));
    }
    if !familiar.omp_agent.is_empty() && !valid_familiar_id(&familiar.omp_agent) {
        errors.push(format!(
            "Familiar '{}' OMP agent '{}' must use lowercase kebab-case.",
            familiar.id, familiar.omp_agent
        ));
    }
    if !familiar.model_role.is_empty() && !valid_model_role(&familiar.model_role) {
        errors.push(format!(
            "Familiar '{}' model role '{}' must use @lowercase_role syntax.",
            familiar.id, familiar.model_role
        ));
    }
    errors
}

fn familiar_description_errors(familiar: &Familiar) -> Vec<String> {
    if familiar.description.is_empty() {
        vec![format!(
            "Familiar '{}' requires a description.",
            familiar.id
        )]
    } else {
        Vec::new()
    }
}

fn familiar_lookup_errors(
    familiar: &Familiar,
    owners: &mut HashMap<String, String>,
) -> Vec<String> {
    let mut errors = Vec::new();
    for key in std::iter::once(&familiar.id)
        .chain(std::iter::once(&familiar.name))
        .chain(familiar.aliases.iter())
    {
        let key = key.to_lowercase();
        if key.is_empty() {
            continue;
        }
        if let Some(owner) = owners.insert(key.clone(), familiar.id.clone()) {
            if owner != familiar.id {
                errors.push(format!(
                    "Familiar lookup key '{key}' is already owned by '{owner}'."
                ));
            }
        }
    }
    errors
}

pub fn validate_spellbook(page: SpellbookPage) -> FamiliarStatus {
    let mut spellbook = Spellbook {
        version: page.version.unwrap_or_default(),
        collective: page.collective.unwrap_or_default(),
        collective_aliases: page.collective_aliases.unwrap_or_default(),
        spellbook_aliases: page.spellbook_aliases.unwrap_or_default(),
        guide: page.guide.unwrap_or_default(),
        familiars: page.familiars.unwrap_or_default(),
    };
    let mut errors = spellbook_errors(&mut spellbook);
    let mut owners = HashMap::<String, String>::new();
    for (index, familiar) in spellbook.familiars.iter_mut().enumerate() {
        normalize_familiar(familiar);
        errors.extend(familiar_identity_errors(index, familiar));
        errors.extend(familiar_lane_errors(familiar));
        errors.extend(familiar_route_errors(familiar));
        errors.extend(familiar_description_errors(familiar));
        errors.extend(familiar_lookup_errors(familiar, &mut owners));
    }
    FamiliarStatus {
        ok: errors.is_empty(),
        spellbook: errors.is_empty().then_some(spellbook),
        errors,
    }
}

fn valid_familiar_id(value: &str) -> bool {
    value.chars().enumerate().all(|(index, character)| {
        if index == 0 {
            character.is_ascii_lowercase()
        } else {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        }
    })
}

fn valid_model_role(value: &str) -> bool {
    let Some(role) = value.strip_prefix('@') else {
        return false;
    };
    role.chars().enumerate().all(|(index, character)| {
        if index == 0 {
            character.is_ascii_lowercase()
        } else {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
                || character == '_'
        }
    })
}
