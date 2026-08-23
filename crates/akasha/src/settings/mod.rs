use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::{PgPool, Row, types::Json};
use std::io;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct RoomSettings {
    pub remember_section_split_chars: usize,
    pub remember_chunk_chars: usize,
    pub remember_chunk_overlap_chars: usize,
    pub recall_temporal_half_life_days: f64,
    pub recall_temporal_durability_curve_power: i32,
    pub recall_semantic_similarity_weight: f64,
    pub recall_semantic_rank_weight: f64,
    pub recall_content_similarity_weight: f64,
    pub recall_content_rank_weight: f64,
    pub recall_semantic_lexical_score_weight: f64,
    pub recall_semantic_lexical_rank_weight: f64,
    pub recall_thread_base_weight: f64,
    pub recall_thread_rank_weight: f64,
    pub cluster_stale_chunk_count: i64,
    pub cluster_stale_fraction: f64,
    pub cluster_stale_days: i64,
    pub insula_retention_days: i16,
    pub backup_keep_count: usize,
    pub house_language: String,
    pub house_tz: String,
}

impl Default for RoomSettings {
    fn default() -> Self {
        Self {
            remember_section_split_chars: 4_000,
            remember_chunk_chars: 2_200,
            remember_chunk_overlap_chars: 200,
            recall_temporal_half_life_days: 7.0,
            recall_temporal_durability_curve_power: 2,
            recall_semantic_similarity_weight: 0.6,
            recall_semantic_rank_weight: 0.4,
            recall_content_similarity_weight: 0.6,
            recall_content_rank_weight: 0.4,
            recall_semantic_lexical_score_weight: 0.15,
            recall_semantic_lexical_rank_weight: 0.05,
            recall_thread_base_weight: 0.35,
            recall_thread_rank_weight: 0.55,
            cluster_stale_chunk_count: 250,
            cluster_stale_fraction: 0.05,
            cluster_stale_days: 7,
            insula_retention_days: 14,
            backup_keep_count: 3,
            house_language: "portuguese".into(),
            house_tz: "America/Sao_Paulo".into(),
        }
    }
}

impl RoomSettings {
    /// One visible database door: callers load once, then pass this value through.
    pub async fn load(pool: &PgPool, room: &str) -> Result<Self, sqlx::Error> {
        let rows =
            sqlx::query("SELECT key,value FROM room_settings WHERE room_key=$1 ORDER BY key")
                .bind(room)
                .fetch_all(pool)
                .await?;
        Self::from_rows(rows.into_iter().map(|row| {
            Ok((
                row.try_get::<String, _>("key")?,
                row.try_get::<Json<Value>, _>("value")?.0,
            ))
        }))
    }

    fn from_rows<I>(rows: I) -> Result<Self, sqlx::Error>
    where
        I: IntoIterator<Item = Result<(String, Value), sqlx::Error>>,
    {
        let mut settings = Self::default();
        for row in rows {
            let (key, value) = row?;
            match key.as_str() {
                "remember_section_split_chars" => {
                    settings.remember_section_split_chars = decode(value)?
                }
                "remember_chunk_chars" => settings.remember_chunk_chars = decode(value)?,
                "remember_chunk_overlap_chars" => {
                    settings.remember_chunk_overlap_chars = decode(value)?
                }
                "recall_temporal_half_life_days" => {
                    settings.recall_temporal_half_life_days = decode(value)?
                }
                "recall_temporal_durability_curve_power" => {
                    settings.recall_temporal_durability_curve_power = decode(value)?
                }
                "recall_semantic_similarity_weight" => {
                    settings.recall_semantic_similarity_weight = decode(value)?
                }
                "recall_semantic_rank_weight" => {
                    settings.recall_semantic_rank_weight = decode(value)?
                }
                "recall_content_similarity_weight" => {
                    settings.recall_content_similarity_weight = decode(value)?
                }
                "recall_content_rank_weight" => {
                    settings.recall_content_rank_weight = decode(value)?
                }
                "recall_semantic_lexical_score_weight" => {
                    settings.recall_semantic_lexical_score_weight = decode(value)?
                }
                "recall_semantic_lexical_rank_weight" => {
                    settings.recall_semantic_lexical_rank_weight = decode(value)?
                }
                "recall_thread_base_weight" => settings.recall_thread_base_weight = decode(value)?,
                "recall_thread_rank_weight" => settings.recall_thread_rank_weight = decode(value)?,
                "cluster_stale_chunk_count" => settings.cluster_stale_chunk_count = decode(value)?,
                "cluster_stale_fraction" => settings.cluster_stale_fraction = decode(value)?,
                "cluster_stale_days" => settings.cluster_stale_days = decode(value)?,
                "insula_retention_days" => settings.insula_retention_days = decode(value)?,
                "backup_keep_count" => settings.backup_keep_count = decode(value)?,
                "house_language" => settings.house_language = decode(value)?,
                "house_tz" => settings.house_tz = decode(value)?,
                _ => return Err(decode_error(format!("unknown room setting key {key}"))),
            }
        }
        settings.validate()?;
        Ok(settings)
    }

    fn validate(&self) -> Result<(), sqlx::Error> {
        if self.remember_chunk_chars == 0
            || self.remember_section_split_chars < self.remember_chunk_chars
            || self.remember_chunk_overlap_chars >= self.remember_chunk_chars
        {
            return Err(decode_error("invalid remember chunk settings"));
        }
        if !self.recall_temporal_half_life_days.is_finite()
            || self.recall_temporal_half_life_days <= 0.0
            || self.recall_temporal_durability_curve_power <= 0
        {
            return Err(decode_error("invalid recall temporal settings"));
        }
        for value in [
            self.recall_semantic_similarity_weight,
            self.recall_semantic_rank_weight,
            self.recall_content_similarity_weight,
            self.recall_content_rank_weight,
            self.recall_semantic_lexical_score_weight,
            self.recall_semantic_lexical_rank_weight,
            self.recall_thread_base_weight,
            self.recall_thread_rank_weight,
            self.cluster_stale_fraction,
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(decode_error(
                    "room setting weights must be finite and nonnegative",
                ));
            }
        }
        if self.cluster_stale_chunk_count <= 0
            || self.cluster_stale_days <= 0
            || self.cluster_stale_fraction > 1.0
            || self.insula_retention_days <= 0
            || self.backup_keep_count == 0
        {
            return Err(decode_error(
                "room setting limit must be positive and in range",
            ));
        }
        if self.house_language.is_empty()
            || !self
                .house_language
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
            || self.house_tz.trim().is_empty()
        {
            return Err(decode_error("invalid house locale setting"));
        }
        Ok(())
    }
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, sqlx::Error> {
    serde_json::from_value(value).map_err(|error| sqlx::Error::Decode(Box::new(error)))
}

fn decode_error(message: impl Into<String>) -> sqlx::Error {
    sqlx::Error::Decode(Box::new(io::Error::new(
        io::ErrorKind::InvalidData,
        message.into(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn settings_load_uses_complete_current_defaults() {
        let loaded = RoomSettings::from_rows(std::iter::empty()).unwrap();
        assert_eq!(
            serde_json::to_value(loaded).unwrap(),
            json!({
                "remember_section_split_chars": 4_000,
                "remember_chunk_chars": 2_200,
                "remember_chunk_overlap_chars": 200,
                "recall_temporal_half_life_days": 7.0,
                "recall_temporal_durability_curve_power": 2,
                "recall_semantic_similarity_weight": 0.6,
                "recall_semantic_rank_weight": 0.4,
                "recall_content_similarity_weight": 0.6,
                "recall_content_rank_weight": 0.4,
                "recall_semantic_lexical_score_weight": 0.15,
                "recall_semantic_lexical_rank_weight": 0.05,
                "recall_thread_base_weight": 0.35,
                "recall_thread_rank_weight": 0.55,
                "cluster_stale_chunk_count": 250,
                "cluster_stale_fraction": 0.05,
                "cluster_stale_days": 7,
                "insula_retention_days": 14,
                "backup_keep_count": 3,
                "house_language": "portuguese",
                "house_tz": "America/Sao_Paulo",
            })
        );
    }

    #[test]
    fn settings_load_rejects_unknown_or_invalid_rows() {
        assert!(
            RoomSettings::from_rows([Ok(("missing_default".into(), Value::Bool(true)))]).is_err()
        );
        assert!(
            RoomSettings::from_rows([Ok((
                "remember_chunk_overlap_chars".into(),
                Value::from(2_200),
            ))])
            .is_err()
        );
    }
}
