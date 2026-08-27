use crate::error::DomainError;
use crate::room::RoomKey;

const MAX_RECALL_TOP_K: u32 = 1_000;

#[derive(Clone, Debug, PartialEq)]
pub struct RecallRequest {
    room: RoomKey,
    query: String,
    semantic_top_k: u32,
    semantic_min_similarity: f64,
    content_top_k: u32,
    content_min_similarity: f64,
    temporal_decay: bool,
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
        })
    }

    pub fn with_temporal_decay(mut self, temporal_decay: bool) -> Self {
        self.temporal_decay = temporal_decay;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recall_constructor_compatibility_defaults_decay_off_and_builder_opts_in() {
        let request =
            RecallRequest::new(RoomKey::new("lab").unwrap(), "alpha".into(), 8, 0.5, 8, 0.3)
                .unwrap();
        assert!(!request.temporal_decay());

        let decayed = request.with_temporal_decay(true);
        assert!(decayed.temporal_decay());
    }
}
