use crate::error::DomainError;
use crate::room::RoomKey;
use serde::{Deserialize, Serialize};

const MAX_RECALL_TOP_K: u32 = 1_000;

/// Most records a manual projection hands back whole. This is the same seat
/// count the Host viewport keeps for one presentation; the cap trims by count
/// and says so in `warnings`, it never clips a selected record's body.
pub const MANUAL_RECORD_CAP: usize = 5;

/// How a recall's selected records reach the caller.
///
/// `Auto` is the passive working set a turn receives without asking: bounded
/// excerpts under the context budget. `Manual` is an operator-visible tool
/// read: every selected record carries its complete authoritative database
/// body, bounded only by `MANUAL_RECORD_CAP`. The projection is named at the
/// request boundary; nothing infers it from query syntax.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecallProjection {
    #[default]
    Auto,
    Manual,
}

impl RecallProjection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
        }
    }

    pub const fn is_auto(&self) -> bool {
        matches!(self, Self::Auto)
    }

    pub const fn is_manual(&self) -> bool {
        matches!(self, Self::Manual)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecallRequest {
    room: RoomKey,
    query: String,
    semantic_top_k: u32,
    semantic_min_similarity: f64,
    content_top_k: u32,
    content_min_similarity: f64,
    temporal_decay: bool,
    projection: RecallProjection,
}

impl RecallRequest {
    pub fn new(
        room: RoomKey,
        query: String,
        semantic_top_k: u32,
        semantic_min_similarity: f64,
        content_top_k: u32,
        content_min_similarity: f64,
    ) -> Result<Self, DomainError> {
        if query.trim().is_empty() {
            return Err(DomainError::EmptyQuery);
        }
        for (field, value) in [
            ("semantic_top_k", semantic_top_k),
            ("content_top_k", content_top_k),
        ] {
            if value == 0 || value > MAX_RECALL_TOP_K {
                return Err(DomainError::InvalidTopK {
                    field: field.into(),
                    value,
                });
            }
        }
        for (field, value) in [
            ("semantic_min_similarity", semantic_min_similarity),
            ("content_min_similarity", content_min_similarity),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(DomainError::InvalidThreshold {
                    field: field.into(),
                    value,
                });
            }
        }
        Ok(Self {
            room,
            query,
            semantic_top_k,
            semantic_min_similarity,
            content_top_k,
            content_min_similarity,
            temporal_decay: false,
            projection: RecallProjection::Auto,
        })
    }

    pub fn with_temporal_decay(mut self, temporal_decay: bool) -> Self {
        self.temporal_decay = temporal_decay;
        self
    }

    pub fn with_projection(mut self, projection: RecallProjection) -> Self {
        self.projection = projection;
        self
    }

    pub fn room(&self) -> &RoomKey {
        &self.room
    }
    pub fn query(&self) -> &str {
        &self.query
    }
    pub const fn semantic_top_k(&self) -> u32 {
        self.semantic_top_k
    }
    pub const fn semantic_min_similarity(&self) -> f64 {
        self.semantic_min_similarity
    }
    pub const fn content_top_k(&self) -> u32 {
        self.content_top_k
    }
    pub const fn content_min_similarity(&self) -> f64 {
        self.content_min_similarity
    }
    pub const fn temporal_decay(&self) -> bool {
        self.temporal_decay
    }
    pub const fn projection(&self) -> RecallProjection {
        self.projection
    }
}

#[cfg(test)]
mod tests {
    use super::{MANUAL_RECORD_CAP, RecallProjection, RecallRequest};
    use crate::room::RoomKey;

    #[test]
    fn projection_defaults_to_auto_and_is_named_explicitly() {
        let request =
            RecallRequest::new(RoomKey::new("lab").unwrap(), "alpha".into(), 8, 0.4, 8, 0.3)
                .unwrap();
        assert_eq!(request.projection(), RecallProjection::Auto);
        assert!(request.projection().is_auto());
        let manual = request.with_projection(RecallProjection::Manual);
        assert!(manual.projection().is_manual());
        assert_eq!(manual.projection().as_str(), "manual");
        assert_eq!(
            serde_json::to_string(&RecallProjection::Manual).unwrap(),
            r#""manual""#
        );
        assert_eq!(
            serde_json::from_str::<RecallProjection>(r#""auto""#).unwrap(),
            RecallProjection::Auto
        );
        assert!(MANUAL_RECORD_CAP >= 1);
    }
}
