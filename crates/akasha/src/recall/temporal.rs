use chrono::{DateTime, Utc};

pub(super) fn giga_temporal_factor(
    meta: &serde_json::Value,
    now: DateTime<Utc>,
) -> (Option<f64>, f64) {
    let Some(object) = meta.as_object() else {
        return (None, 1.0);
    };
    if object.get("origin").and_then(|value| value.as_str()) != Some("giga-promotion") {
        return (None, 1.0);
    }
    let Some(giga) = object.get("giga").and_then(|value| value.as_object()) else {
        return (None, 1.0);
    };
    let Some(durability) = giga.get("durability").and_then(|value| value.as_f64()) else {
        return (None, 1.0);
    };
    if !durability.is_finite()
        || !(0.0..=1.0).contains(&durability)
        || giga.get("decay_anchor").and_then(|value| value.as_str()) != Some("candidate_created_at")
    {
        return (None, 1.0);
    }
    let Some(anchor) = giga
        .get("decay_anchor_at")
        .and_then(|value| value.as_str())
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
    else {
        return (None, 1.0);
    };
    let age_days = (now - anchor).num_seconds() as f64 / 86_400.0;
    if age_days <= 0.0 || durability >= 1.0 {
        return (Some(durability), 1.0);
    }
    let factor = (-std::f64::consts::LN_2 * age_days * (1.0 - durability).powi(2) / 7.0).exp();
    (
        Some(durability),
        if factor.is_finite() { factor } else { 1.0 },
    )
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
