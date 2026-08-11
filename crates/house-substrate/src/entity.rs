use crate::config::{AppError, ROOM_KEY_RE};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::{collections::HashSet, sync::LazyLock};

static WORD_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\p{L}\p{N}]+").expect("Unicode word regex must compile"));

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EntityResolveParams {
    pub room: String,
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: u32,
}
fn default_limit() -> u32 {
    8
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityMatch {
    pub canonical_name: String,
    pub kind: String,
    pub matched_alias: String,
}
#[derive(Debug, Serialize)]
pub struct EntityResolveResult {
    pub ok: bool,
    pub matches: Vec<EntityMatch>,
}

#[derive(Debug)]
struct Entity {
    name: String,
    kind: String,
    aliases: Vec<String>,
}

fn normalize(value: &str) -> String {
    WORD_RE
        .find_iter(&value.replace('’', "'").to_lowercase())
        .map(|m| m.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn resolve_matches(query: &str, entities: &[Entity], limit: usize) -> Vec<EntityMatch> {
    let query = normalize(query);
    if query.is_empty() || limit == 0 {
        return vec![];
    }
    let haystack = format!(" {query} ");
    let mut candidates = Vec::new();
    for (entity_index, entity) in entities.iter().enumerate() {
        let canonical = normalize(&entity.name);
        if canonical.is_empty() {
            continue;
        }
        let mut labels = Vec::with_capacity(entity.aliases.len() + 1);
        labels.push(entity.name.as_str());
        labels.extend(entity.aliases.iter().map(String::as_str));
        let mut seen = HashSet::new();
        for (alias_index, label) in labels.into_iter().enumerate() {
            let alias = normalize(label);
            if alias.replace(' ', "").chars().count() < 3 || !seen.insert(alias.clone()) {
                continue;
            }
            let needle = format!(" {alias} ");
            let Some(position) = haystack.find(&needle) else {
                continue;
            };
            candidates.push((
                alias.split_whitespace().count(),
                position,
                entity_index * 1000 + alias_index,
                EntityMatch {
                    canonical_name: entity.name.clone(),
                    kind: entity.kind.clone(),
                    matched_alias: label.trim().to_string(),
                },
            ));
        }
    }
    candidates.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then(left.1.cmp(&right.1))
            .then(left.2.cmp(&right.2))
    });
    let mut seen = HashSet::new();
    let mut matches = Vec::new();
    for (_, _, _, candidate) in candidates {
        if seen.insert((normalize(&candidate.canonical_name), candidate.kind.clone())) {
            matches.push(candidate);
        }
        if matches.len() == limit {
            break;
        }
    }
    matches
}

pub async fn entity_resolve(
    pool: &PgPool,
    params: EntityResolveParams,
) -> Result<EntityResolveResult, AppError> {
    if !ROOM_KEY_RE.is_match(&params.room) {
        return Err(AppError::Invalid("room must be a lowercase slug".into()));
    }
    if params.limit > 32 {
        return Err(AppError::Invalid("limit must be from 0 through 32".into()));
    }
    if params.query.trim().is_empty() || params.limit == 0 {
        return Ok(EntityResolveResult {
            ok: true,
            matches: vec![],
        });
    }
    let rooms = if params.room == "house" {
        vec!["house".to_string()]
    } else {
        vec![params.room, "house".to_string()]
    };
    let rows = sqlx::query("SELECT name,kind,aliases FROM named_entities WHERE room=ANY($1) AND authority='active' ORDER BY name,id").bind(rooms).fetch_all(pool).await?;
    let entities = rows
        .iter()
        .map(|row| {
            Ok(Entity {
                name: row.try_get("name")?,
                kind: row.try_get("kind")?,
                aliases: row.try_get("aliases")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(EntityResolveResult {
        ok: true,
        matches: resolve_matches(&params.query, &entities, params.limit as usize),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entities() -> Vec<Entity> {
        vec![Entity {
            name: "North Star".into(),
            kind: "project".into(),
            aliases: vec!["the north-star".into(), "blue".into(), "blue team".into()],
        }]
    }
    #[test]
    fn normalization_is_boundary_safe_and_prefers_long_aliases() {
        assert_eq!(
            resolve_matches("Please check NORTH-STAR.", &entities(), 8)[0].canonical_name,
            "North Star"
        );
        assert_eq!(
            resolve_matches("blue team", &entities(), 8)[0].matched_alias,
            "blue team"
        );
        assert!(resolve_matches("blueness", &entities(), 8).is_empty());
    }
}
