use crate::settings::RoomSettings;

use chrono::{DateTime, Utc};

pub(super) fn memory_temporal_factor(
    meta: &serde_json::Value,
    created_at: DateTime<Utc>,
    now: DateTime<Utc>,
    settings: &RoomSettings,
) -> (Option<f64>, f64) {
    let giga = &meta["giga"];
    let durability = giga["durability"]
        .as_f64()
        .filter(|value| value.is_finite() && (0.0..=1.0).contains(value));
    // Promotion must not reset the age of evidence that already has a reviewed timestamp.
    let anchor = giga["decay_anchor_at"]
        .as_str()
        .filter(|_| giga["decay_anchor"] == "candidate_created_at")
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map_or(created_at, |value| value.with_timezone(&Utc));
    let age_days = (now - anchor).num_seconds().max(0) as f64 / 86_400.0;
    let factor = (-std::f64::consts::LN_2
        * age_days
        * (1.0 - durability.unwrap_or(0.0)).powi(settings.recall_temporal_durability_curve_power)
        / settings.recall_temporal_half_life_days)
        .exp();

    (durability, factor)
}

pub(super) fn weighted_lane_score(chunk: &serde_json::Value, score_field: &str) -> f64 {
    chunk[score_field].as_f64().unwrap_or(0.0) * chunk["temporal_weight"].as_f64().unwrap_or(1.0)
}

pub(super) fn compare_weighted_lane(
    left: &serde_json::Value,
    right: &serde_json::Value,
    score_field: &str,
) -> std::cmp::Ordering {
    weighted_lane_score(right, score_field)
        .partial_cmp(&weighted_lane_score(left, score_field))
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| {
            left["source_path"]
                .as_str()
                .cmp(&right["source_path"].as_str())
        })
        .then_with(|| {
            left["chunk_index"]
                .as_i64()
                .cmp(&right["chunk_index"].as_i64())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use serde_json::json;

    #[test]
    fn unrated_memories_decay_with_age_regardless_of_origin() {
        let now = DateTime::parse_from_rfc3339("2026-10-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let settings = RoomSettings::default();

        for meta in [
            json!({}),
            json!({"origin": "direct-write"}),
            json!({"origin": "giga-promotion"}),
            json!(null),
        ] {
            for (days, expected) in [(0, 1.0), (60, 0.5), (120, 0.25), (-60, 1.0)] {
                let (durability, weight) =
                    memory_temporal_factor(&meta, now - Duration::days(days), now, &settings);
                assert_eq!(durability, None);
                assert!((weight - expected).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn reviewed_durability_and_anchor_survive_without_an_origin_gate() {
        let now = DateTime::parse_from_rfc3339("2026-10-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let anchor = now - Duration::days(240);
        let settings = RoomSettings::default();

        for origin in ["giga-promotion", "direct-write"] {
            for (durability, expected) in [(0.0, 0.0625), (0.5, 0.5), (1.0, 1.0)] {
                let meta = json!({
                    "origin": origin,
                    "giga": {
                        "durability": durability,
                        "decay_anchor": "candidate_created_at",
                        "decay_anchor_at": anchor.to_rfc3339(),
                    },
                });
                let (actual, weight) = memory_temporal_factor(&meta, now, now, &settings);
                assert_eq!(actual, Some(durability));
                assert!((weight - expected).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn invalid_metadata_uses_creation_time_and_configured_half_life() {
        let now = DateTime::parse_from_rfc3339("2026-10-14T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let settings = RoomSettings {
            recall_temporal_half_life_days: 30.0,
            ..RoomSettings::default()
        };

        for durability in [json!(-0.1), json!(1.1), json!("permanent"), json!(null)] {
            let meta = json!({"giga": {
                "durability": durability,
                "decay_anchor": "candidate_created_at",
                "decay_anchor_at": "invalid",
            }});
            let (actual, weight) =
                memory_temporal_factor(&meta, now - Duration::days(30), now, &settings);
            assert_eq!(actual, None);
            assert!((weight - 0.5).abs() < 1e-12);
        }
    }
}
