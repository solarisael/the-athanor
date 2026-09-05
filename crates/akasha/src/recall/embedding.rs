use crate::config::{AppError, RECALL_EMBED_TIMEOUT, giga_keep_alive};
use reqwest::Client;
use serde::Serialize;
use std::time::Duration;

/// Why an embedding did not arrive. A timeout is the embedder being slow,
/// not wrong: recall degrades to its lexical lanes and names the class
/// `embed_timeout` so the tail is countable apart from real failures.
#[derive(Debug)]
pub(super) enum EmbedError {
    Timeout(Duration),
    Failed(AppError),
}

impl std::fmt::Display for EmbedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout(limit) => {
                write!(f, "embedding timed out after {} ms", limit.as_millis())
            }
            Self::Failed(error) => write!(f, "{error}"),
        }
    }
}

impl From<EmbedError> for AppError {
    fn from(error: EmbedError) -> Self {
        match error {
            EmbedError::Timeout(_) => AppError::Embedding(error.to_string()),
            EmbedError::Failed(error) => error,
        }
    }
}

// The `query: ` / `passage: ` prefix pair is load-bearing. Do not remove it.
// Without prefixes this model floors unrelated text near 0.50: absolute values
// look higher while separating nothing, and the true/noise gap goes negative.
// The indexer's `passage: ` and this `query: ` are one decision — project
// lesson 126 (the-athanor) carries the measured comparison.
pub(super) async fn embed_texts(
    client: &Client,
    url: &str,
    model: &str,
    prefix: &str,
    texts: &[String],
    dim: usize,
    timeout: Duration,
) -> Result<Vec<Vec<f32>>, EmbedError> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let keep_alive = giga_keep_alive().map_err(EmbedError::Failed)?;
    // The embedder is on the hot path — every user prompt embeds a query. Ollama's
    // default 5 minute keep_alive evicted it during idle gaps and charged a cold
    // reload on the next turn, which is felt directly as latency. Residency costs
    // 2.1 GB of VRAM; measured 2026-07-25.
    #[derive(Serialize)]
    struct Input<'a> {
        model: &'a str,
        input: Vec<String>,
        keep_alive: &'a str,
    }
    let response = client
        .post(url)
        .timeout(timeout)
        .json(&Input {
            model,
            input: texts
                .iter()
                .map(|text| format!("{prefix}: {text}"))
                .collect(),
            keep_alive: &keep_alive,
        })
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                EmbedError::Timeout(timeout)
            } else {
                EmbedError::Failed(AppError::Embedding(e.to_string()))
            }
        })?;
    parse_embeddings(response, texts.len(), dim)
        .await
        .map_err(EmbedError::Failed)
}

async fn parse_embeddings(
    response: reqwest::Response,
    expected: usize,
    dim: usize,
) -> Result<Vec<Vec<f32>>, AppError> {
    let value: serde_json::Value = response
        .error_for_status()
        .map_err(|e| AppError::Embedding(e.to_string()))?
        .json()
        .await
        .map_err(|e| AppError::Embedding(e.to_string()))?;
    let items = value
        .get("embeddings")
        .or_else(|| value.get("data"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| AppError::Embedding("response lacks embeddings".into()))?;
    if items.len() != expected {
        return Err(AppError::Embedding(format!(
            "embedding count {} != {}",
            items.len(),
            expected
        )));
    }
    items
        .iter()
        .map(|item| {
            let values = item
                .as_array()
                .or_else(|| item.get("embedding").and_then(serde_json::Value::as_array))
                .ok_or_else(|| AppError::Embedding("invalid embedding".into()))?;
            let row = values
                .iter()
                .map(|value| {
                    value
                        .as_f64()
                        .map(|number| number as f32)
                        .ok_or_else(|| AppError::Embedding("non-numeric embedding".into()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            if row.len() != dim {
                return Err(AppError::Embedding(format!(
                    "embedding dimension {} != {}",
                    row.len(),
                    dim
                )));
            }
            Ok(row)
        })
        .collect()
}

/// The recall-path embedding: one query, bounded by `RECALL_EMBED_TIMEOUT`.
pub(super) async fn embed_query(
    client: &Client,
    url: &str,
    model: &str,
    query: &str,
    dim: usize,
) -> Result<Vec<f32>, EmbedError> {
    embed_texts(
        client,
        url,
        model,
        "query",
        &[query.to_owned()],
        dim,
        RECALL_EMBED_TIMEOUT,
    )
    .await?
    .into_iter()
    .next()
    .ok_or_else(|| EmbedError::Failed(AppError::Embedding("response lacks embedding".into())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EMBED_DIMENSION;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// An embedder that accepts the connection and reads the request. With
    /// `answer` it replies once and closes; without, it never answers: the
    /// shape of a stuck Ollama.
    async fn embedder(answer: Option<&'static str>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("addr").port();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut sink = [0u8; 4096];
                    let Ok(read) = socket.read(&mut sink).await else {
                        return;
                    };
                    let Some(answer) = answer else {
                        while socket.read(&mut sink).await.is_ok_and(|n| n > 0) {}
                        return;
                    };
                    if read > 0 {
                        let _ = socket.write_all(answer.as_bytes()).await;
                    }
                });
            }
        });
        format!("http://127.0.0.1:{port}/api/embed")
    }

    /// The client timeout bounds a stuck embedder and is reported as the
    /// timeout class, not a generic failure — recall degrades by that name.
    #[tokio::test]
    async fn stuck_embedder_times_out_as_embed_timeout() {
        let url = embedder(None).await;
        let limit = Duration::from_millis(120);
        let started = std::time::Instant::now();
        let result = embed_texts(
            &Client::new(),
            &url,
            "model",
            "query",
            &["what is the flush ceiling".to_owned()],
            EMBED_DIMENSION,
            limit,
        )
        .await;
        let elapsed = started.elapsed();
        assert!(
            matches!(result, Err(EmbedError::Timeout(seen)) if seen == limit),
            "expected timeout, got {result:?}"
        );
        assert!(elapsed >= limit, "returned before the limit: {elapsed:?}");
        assert!(
            elapsed < Duration::from_secs(3),
            "recall would have waited on the old shared timeout: {elapsed:?}"
        );
        assert_eq!(
            AppError::from(EmbedError::Timeout(limit)).to_string(),
            "embedding error: embedding timed out after 120 ms"
        );
    }

    /// A prompt server error is a failure, not a timeout: the two classes
    /// must not blur, or the tail count lies.
    #[tokio::test]
    async fn failing_embedder_is_not_the_timeout_class() {
        let url = embedder(Some(
            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        ))
        .await;
        let result = embed_texts(
            &Client::new(),
            &url,
            "model",
            "query",
            &["anything".to_owned()],
            EMBED_DIMENSION,
            Duration::from_secs(5),
        )
        .await;
        assert!(
            matches!(result, Err(EmbedError::Failed(AppError::Embedding(_)))),
            "expected failure, got {result:?}"
        );
    }
}
