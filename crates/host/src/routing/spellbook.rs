use super::worker_lane;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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
    /// The scope whose page defines this familiar. The Host sets it after
    /// parsing; a page that names `origin` is refused as an unknown field.
    #[serde(default, skip_deserializing, skip_serializing_if = "String::is_empty")]
    pub origin: String,
}

/// The effective spellbook: the House page with the room page laid over it,
/// plus the familiars other rooms define and neither of those two does.
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

/// One stored spellbook page. A key the page omits inherits the House
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
    /// Foreign pages and familiars that were left out. They never refuse the book.
    pub warnings: Vec<String>,
    /// Lookup keys two or more foreign familiars claim. Dispatch by them is refused.
    pub contested: Vec<ContestedKey>,
    pub spellbook: Option<Spellbook>,
}

/// A lookup key that more than one foreign familiar claims and neither this
/// room nor the House owns.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContestedKey {
    pub key: String,
    pub claims: Vec<FamiliarClaim>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamiliarClaim {
    pub familiar: String,
    pub scope: String,
}

impl ContestedKey {
    pub fn refusal(&self) -> String {
        let claims = self
            .claims
            .iter()
            .map(|claim| format!("{} ({})", claim.familiar, claim.scope))
            .collect::<Vec<_>>();
        format!(
            "Familiar key '{}' is contested: {}. Define it in this room or in the house to choose.",
            self.key,
            claims.join(", ")
        )
    }
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

/// Normalize one familiar and judge it alone: identity, lane, route, description.
fn familiar_errors(index: usize, familiar: &mut Familiar) -> Vec<String> {
    normalize_familiar(familiar);
    let mut errors = familiar_identity_errors(index, familiar);
    errors.extend(familiar_lane_errors(familiar));
    errors.extend(familiar_route_errors(familiar));
    errors.extend(familiar_description_errors(familiar));
    errors
}

/// The case-insensitive keys dispatch accepts for one familiar.
pub(super) fn lookup_keys(familiar: &Familiar) -> impl Iterator<Item = String> + '_ {
    std::iter::once(&familiar.id)
        .chain(std::iter::once(&familiar.name))
        .chain(familiar.aliases.iter())
        .map(|key| key.to_lowercase())
        .filter(|key| !key.is_empty())
}

fn familiar_lookup_errors(
    familiar: &Familiar,
    owners: &mut HashMap<String, String>,
) -> Vec<String> {
    let mut errors = Vec::new();
    for key in lookup_keys(familiar) {
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
        errors.extend(familiar_errors(index, familiar));
        errors.extend(familiar_lookup_errors(familiar, &mut owners));
    }
    FamiliarStatus {
        ok: errors.is_empty(),
        spellbook: errors.is_empty().then_some(spellbook),
        errors,
        warnings: Vec::new(),
        contested: Vec::new(),
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

/// The shared scope. Every other scope is a room key.
pub const HOUSE_SCOPE: &str = "house";

/// One `spellbook_pages` row: its scope and its stored page document.
pub type StoredPage = (String, Value);

/// One scope that contributed to the effective spellbook, in precedence order.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpellbookSource {
    /// `room` for the requester's room, `house`, or `other` for another room.
    pub layer: &'static str,
    pub scope: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpellbookIdentity {
    pub collective: String,
    pub collective_aliases: Vec<String>,
    pub spellbook_aliases: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSpellbook {
    pub sources: Vec<SpellbookSource>,
    pub status: FamiliarStatus,
    /// Per origin scope: that scope's page laid over the House page.
    pub identities: BTreeMap<String, SpellbookIdentity>,
}

pub fn unloaded(sources: Vec<SpellbookSource>, errors: Vec<String>) -> LoadedSpellbook {
    LoadedSpellbook {
        sources,
        status: FamiliarStatus {
            ok: false,
            errors,
            warnings: Vec::new(),
            contested: Vec::new(),
            spellbook: None,
        },
        identities: BTreeMap::new(),
    }
}

/// Parse one stored page and stamp its familiars with their origin scope.
fn parse_page(scope: &str, page: &Value) -> Result<SpellbookPage, String> {
    let mut page = serde_json::from_value::<SpellbookPage>(page.clone())
        .map_err(|error| format!("Spellbook page '{scope}' is not a valid page: {error}"))?;
    let mut ids = HashSet::new();
    if let Some(familiar) = page
        .familiars
        .as_deref()
        .unwrap_or_default()
        .iter()
        .find(|familiar| !ids.insert(familiar.id.trim().to_owned()))
    {
        return Err(format!(
            "Spellbook page '{scope}' repeats id '{}'.",
            familiar.id.trim()
        ));
    }
    for familiar in page.familiars.iter_mut().flatten() {
        familiar.origin = scope.into();
    }
    Ok(page)
}

fn identity(page: &SpellbookPage) -> SpellbookIdentity {
    let mut identity = SpellbookIdentity {
        collective: page.collective.as_deref().unwrap_or_default().trim().into(),
        collective_aliases: page.collective_aliases.clone().unwrap_or_default(),
        spellbook_aliases: page.spellbook_aliases.clone().unwrap_or_default(),
    };
    unique_nonempty(&mut identity.collective_aliases);
    unique_nonempty(&mut identity.spellbook_aliases);
    identity
}

fn stored<'a>(pages: &'a [StoredPage], scope: &str) -> Option<&'a Value> {
    pages
        .iter()
        .find(|(stored, _)| stored == scope)
        .map(|(_, page)| page)
}

/// The effective spellbook for `room`. Precedence, highest first: the room,
/// the House, then every other room. The room and the House refuse the book
/// when either is invalid; another room's page or familiar can only be left
/// out with a warning, and a foreign id or key two rooms share is contested.
pub fn effective_spellbook(room: &str, pages: &[StoredPage]) -> LoadedSpellbook {
    let mut layers = Vec::new();
    for (layer, scope) in [("house", HOUSE_SCOPE), ("room", room)] {
        let Some(page) = stored(pages, scope) else {
            continue;
        };
        layers.push((layer, scope, parse_page(scope, page)));
    }
    let mut sources = layers
        .iter()
        .rev()
        .map(|(layer, scope, _)| SpellbookSource {
            layer,
            scope: (*scope).into(),
        })
        .collect::<Vec<_>>();
    if layers.is_empty() {
        return unloaded(
            sources,
            vec![format!(
                "No spellbook page for this room ('{room}') or the house."
            )],
        );
    }

    let mut house = SpellbookPage::default();
    let mut base = SpellbookPage::default();
    for (layer, _, page) in layers {
        match page {
            Ok(page) => {
                if layer == "house" {
                    house = page.clone();
                }
                base = page.over(base);
            }
            Err(refusal) => return unloaded(sources, vec![refusal]),
        }
    }
    let mut identities = BTreeMap::from([
        (HOUSE_SCOPE.to_owned(), identity(&house)),
        (room.to_owned(), identity(&base)),
    ]);
    let mut status = validate_spellbook(base);
    let Some(mut book) = status.spellbook.take() else {
        return LoadedSpellbook {
            sources,
            status,
            identities,
        };
    };

    let owned_ids = book
        .familiars
        .iter()
        .map(|familiar| familiar.id.clone())
        .collect::<HashSet<_>>();
    let owned_keys = book
        .familiars
        .iter()
        .flat_map(lookup_keys)
        .collect::<HashSet<_>>();

    // Every foreign definition of an id neither the room nor the House owns,
    // invalid ones included: an invalid twin still makes the id ambiguous.
    let mut definitions = BTreeMap::<String, Vec<(String, Option<Familiar>)>>::new();
    let mut others = pages
        .iter()
        .filter(|(scope, _)| scope != HOUSE_SCOPE && scope != room)
        .collect::<Vec<_>>();
    others.sort_by(|left, right| left.0.cmp(&right.0));
    for (scope, page) in others {
        let page = match parse_page(scope, page) {
            Ok(page) => page,
            Err(refusal) => {
                status
                    .warnings
                    .push(format!("{refusal} Its familiars are excluded."));
                continue;
            }
        };
        identities.insert(scope.clone(), identity(&page.clone().over(house.clone())));
        for (index, mut familiar) in page.familiars.unwrap_or_default().into_iter().enumerate() {
            let errors = familiar_errors(index, &mut familiar);
            if owned_ids.contains(&familiar.id) {
                continue;
            }
            if !errors.is_empty() {
                status.warnings.push(format!(
                    "Spellbook page '{scope}' familiar excluded: {}",
                    errors.join(" ")
                ));
            }
            if familiar.id.is_empty() {
                continue;
            }
            definitions
                .entry(familiar.id.clone())
                .or_default()
                .push((scope.clone(), errors.is_empty().then_some(familiar)));
        }
    }

    let mut claims = BTreeMap::<String, Vec<FamiliarClaim>>::new();
    let mut accepted = Vec::new();
    for (id, defined) in definitions {
        if defined.len() > 1 {
            claims.insert(
                id.to_lowercase(),
                defined
                    .iter()
                    .map(|(scope, _)| FamiliarClaim {
                        familiar: id.clone(),
                        scope: scope.clone(),
                    })
                    .collect(),
            );
            continue;
        }
        accepted.extend(defined.into_iter().filter_map(|(_, familiar)| familiar));
    }
    accepted.sort_by(|left, right| left.origin.cmp(&right.origin));
    for familiar in &accepted {
        for key in lookup_keys(familiar) {
            if owned_keys.contains(&key) {
                continue;
            }
            let claimants = claims.entry(key).or_default();
            if !claimants.iter().any(|claim| claim.familiar == familiar.id) {
                claimants.push(FamiliarClaim {
                    familiar: familiar.id.clone(),
                    scope: familiar.origin.clone(),
                });
            }
        }
    }
    for (key, claims) in claims {
        if claims.len() > 1 {
            let contest = ContestedKey { key, claims };
            status.warnings.push(contest.refusal());
            status.contested.push(contest);
        }
    }

    let contributors = accepted
        .iter()
        .map(|familiar| familiar.origin.clone())
        .collect::<BTreeSet<_>>();
    sources.extend(contributors.into_iter().map(|scope| SpellbookSource {
        layer: "other",
        scope,
    }));
    book.familiars.extend(accepted);
    status.spellbook = Some(book);
    LoadedSpellbook {
        sources,
        status,
        identities,
    }
}

/// Refusals for replacing `scope`'s page with `page`. A room may write its own
/// page or the House page. A room page must validate over the House page; a
/// House page must validate alone and must not break a room page that
/// validates over the current House page.
pub fn spellbook_write_errors(
    room: &str,
    scope: &str,
    page: &Value,
    pages: &[StoredPage],
) -> Vec<String> {
    if scope != room && scope != HOUSE_SCOPE {
        return vec![format!(
            "Spellbook write refused: scope '{scope}' is neither this room ('{room}') nor '{HOUSE_SCOPE}'."
        )];
    }
    let page = match parse_page(scope, page) {
        Ok(page) => page,
        Err(refusal) => return vec![refusal],
    };
    let current_house = match stored(pages, HOUSE_SCOPE).map(|page| parse_page(HOUSE_SCOPE, page)) {
        None => SpellbookPage::default(),
        Some(Ok(house)) => house,
        Some(Err(refusal)) if scope == room => return vec![refusal],
        Some(Err(_)) => SpellbookPage::default(),
    };
    if scope == room {
        return validate_spellbook(page.over(current_house)).errors;
    }

    let mut errors = validate_spellbook(page.clone()).errors;
    for (other, stored) in pages.iter().filter(|(other, _)| other != HOUSE_SCOPE) {
        let Ok(room_page) = parse_page(other, stored) else {
            continue;
        };
        let before = validate_spellbook(room_page.clone().over(current_house.clone())).ok;
        let after = validate_spellbook(room_page.over(page.clone())).errors;
        if before && !after.is_empty() {
            errors.push(format!("Room '{other}' would break: {}", after.join(" ")));
        }
    }
    errors
}
