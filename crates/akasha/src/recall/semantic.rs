use super::Phase;
use super::embedding::embed_query;
use super::evidence::{bounded_excerpt, candidate_terms, subject_retry_text};
use super::memory_reference::MemoryReferences;
use super::scope::ScopeFilter;
use super::temporal::memory_temporal_factor;
use crate::config::{AppError, Config, EMBED_DIMENSION, HTTP_CLIENT};
use crate::insula_writer::EmitterSpan;
use crate::settings::RoomSettings;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};

pub(super) const SEMANTIC_ORIGINAL_EVIDENCE: &str = "semantic cosine similarity";
const SEMANTIC_RETRY_EVIDENCE: &str = "semantic cosine similarity (subject retry)";
pub(super) fn vector_literal(vector: &[f32]) -> String {
    format!(
        "[{}]",
        vector
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}
/// The semantic lane's fixed context: scope, floor and limits never change
/// between the original pass and the subject retry, only the vector does.
pub(super) struct SemanticLane<'a> {
    pub(super) pool: &'a PgPool,
    pub(super) rooms: &'a [String],
    pub(super) scope: &'a ScopeFilter,
    pub(super) min_similarity: f64,
    pub(super) fetch_limit: Option<i64>,
    pub(super) temporal_decay: bool,
    pub(super) decay_now: DateTime<Utc>,
    pub(super) settings: &'a RoomSettings,
    pub(super) query_terms: &'a [String],
}

impl SemanticLane<'_> {
    pub(super) async fn search(
        &self,
        cfg: &Config,
        query: &str,
        references: &MemoryReferences,
        vector_text: &str,
        allow_retry: bool,
        warnings: &mut Vec<String>,
        span: Option<&EmitterSpan>,
    ) -> Result<Vec<serde_json::Value>, AppError> {
        let phase = Phase::start(span, "recall.semantic");
        let chunks = self.chunks(vector_text, SEMANTIC_ORIGINAL_EVIDENCE).await?;
        let top_similarity = if chunks.is_empty() {
            let top = self.top_similarity(vector_text).await?;
            warnings.push(self.floor_miss_warning(top));
            top
        } else {
            None
        };
        phase.ok();

        // A changed query can help a below-floor result, never an empty corpus.
        let subject = (allow_retry && top_similarity.is_some())
            .then(|| subject_retry_text(query, references))
            .flatten();
        let (Some(subject), Some(url)) = (subject, cfg.embed_url.as_deref()) else {
            return Ok(chunks);
        };

        let phase = Phase::start(span, "recall.semantic_retry");
        match self.subject_retry(cfg, url, &subject).await {
            Ok(retried) => {
                phase.ok();
                warnings.push(if retried.is_empty() {
                    format!(
                        "semantic subject retry `{subject}` empty at floor {:.2}",
                        self.min_similarity
                    )
                } else {
                    format!(
                        "semantic hits from subject retry `{subject}`: {} at floor {:.2}",
                        retried.len(),
                        self.min_similarity
                    )
                });
                Ok(retried)
            }
            Err(error) => {
                phase.degraded("recall.semantic_retry_absent");
                warnings.push(format!(
                    "semantic subject retry `{subject}` absent: {error}"
                ));
                Ok(chunks)
            }
        }
    }

    /// Chunks at or above the floor; `evidence` names the pass that found them.
    async fn chunks(
        &self,
        vector_text: &str,
        evidence: &'static str,
    ) -> Result<Vec<serde_json::Value>, AppError> {
        let semantic_sql = format!(
            r#"SELECT memory_id,source_path,title,heading_path,body,char_start,char_end,chunk_index,meta,created_at,sim
               FROM (
                 SELECT m.source_path,
                        m.id AS memory_id,
                        coalesce(m.title,'') AS title,
                        coalesce(c.heading_path,'') AS heading_path,
                        c.body,c.char_start,c.char_end,c.chunk_index,m.meta AS meta,m.created_at,
                        (1-(c.body_embedding <=> $1::vector))::double precision AS sim
                 FROM memory_chunks c
                 JOIN memories m ON m.id=c.memory_id
                 WHERE m.room = ANY($2::text[])
                   AND m.archived_at IS NULL
                   AND m.superseded_by IS NULL
                   AND COALESCE(m.type,'') <> $5
                   AND c.body_embedding IS NOT NULL{}
               ) ranked
               WHERE sim >= $3
               ORDER BY sim DESC,source_path,chunk_index
               LIMIT $4"#,
            self.scope.predicate("m", 6)
        );
        let semantic_rows = self
            .scope
            .bind(
                sqlx::query(&semantic_sql)
                    .bind(vector_text)
                    .bind(self.rooms)
                    .bind(self.min_similarity)
                    .bind(self.fetch_limit)
                    .bind(origami::boats::MEMORY_KIND),
            )
            .fetch_all(self.pool)
            .await?;
        let mut semantic_chunks = Vec::new();
        for row in semantic_rows {
            let sim: f64 = row.try_get("sim")?;
            if sim < self.min_similarity {
                continue;
            }
            let source_path: String = row.try_get("source_path")?;
            let title: Option<String> = row.try_get("title")?;
            let heading_path: Option<String> = row.try_get("heading_path")?;
            let body: String = row.try_get("body")?;
            let meta: serde_json::Value = row.try_get("meta")?;
            let (durability, temporal_weight) = if self.temporal_decay {
                memory_temporal_factor(
                    &meta,
                    row.try_get("created_at")?,
                    self.decay_now,
                    self.settings,
                )
            } else {
                (None, 1.0)
            };
            let (matched_terms, missing_terms, coverage) = candidate_terms(
                self.query_terms,
                &[
                    &source_path,
                    title.as_deref().unwrap_or(""),
                    heading_path.as_deref().unwrap_or(""),
                    &body,
                ],
            );
            semantic_chunks.push(serde_json::json!({"memory_id":row.try_get::<i64,_>("memory_id")?,"source_path":source_path,"title":title,"heading_path":heading_path,"body":bounded_excerpt(&body),"char_start":row.try_get::<i32,_>("char_start")?,"char_end":row.try_get::<i32,_>("char_end")?,"chunk_index":row.try_get::<i32,_>("chunk_index")?,"sim":sim,"durability":durability,"temporal_weight":temporal_weight,"matched_terms":matched_terms,"missing_terms":missing_terms,"term_coverage":coverage,"evidence":evidence}));
        }
        Ok(semantic_chunks)
    }

    /// The best similarity under the floor. A second cheap roundtrip is
    /// acceptable only when the floor pass came back empty, so silence is
    /// impossible. `None` means no embedded chunk exists in scope.
    async fn top_similarity(&self, vector_text: &str) -> Result<Option<f64>, AppError> {
        let probe_sql = format!(
            r#"SELECT MAX((1-(c.body_embedding <=> $1::vector))::double precision)
               FROM memory_chunks c
               JOIN memories m ON m.id=c.memory_id
               WHERE m.room = ANY($2::text[])
                 AND m.archived_at IS NULL
                 AND m.superseded_by IS NULL
                 AND COALESCE(m.type,'') <> $3
                 AND c.body_embedding IS NOT NULL{}"#,
            self.scope.predicate("m", 4)
        );
        let top_similarity: Option<f64> = self
            .scope
            .bind_scalar(
                sqlx::query_scalar(&probe_sql)
                    .bind(vector_text)
                    .bind(self.rooms)
                    .bind(origami::boats::MEMORY_KIND),
            )
            .fetch_one(self.pool)
            .await?;
        Ok(top_similarity)
    }

    fn floor_miss_warning(&self, top_similarity: Option<f64>) -> String {
        match top_similarity {
            Some(top) => format!(
                "semantic lane empty: top sim {top:.2} < floor {:.2} (rooms {})",
                self.min_similarity,
                self.rooms.join(",")
            ),
            None => format!(
                "semantic lane empty: no embedded chunks (rooms {})",
                self.rooms.join(",")
            ),
        }
    }

    async fn subject_retry(
        &self,
        cfg: &Config,
        url: &str,
        subject: &str,
    ) -> Result<Vec<serde_json::Value>, AppError> {
        let vector = embed_query(
            &HTTP_CLIENT,
            url,
            &cfg.embed_model,
            subject,
            EMBED_DIMENSION,
        )
        .await?;
        self.chunks(&vector_literal(&vector), SEMANTIC_RETRY_EVIDENCE)
            .await
    }
}
