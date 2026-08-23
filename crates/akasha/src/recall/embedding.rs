use crate::config::{AppError, embedding_model_timeout, giga_keep_alive};
use reqwest::Client;
use serde::Serialize;
use std::time::Duration;

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
) -> Result<Vec<Vec<f32>>, AppError> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let keep_alive = giga_keep_alive()?;
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
    let value: serde_json::Value = client
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
        .map_err(|e| AppError::Embedding(e.to_string()))?
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
    if items.len() != texts.len() {
        return Err(AppError::Embedding(format!(
            "embedding count {} != {}",
            items.len(),
            texts.len()
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

async fn embed_text(
    client: &Client,
    url: &str,
    model: &str,
    prefix: &str,
    text: &str,
    dim: usize,
) -> Result<Vec<f32>, AppError> {
    embed_texts(
        client,
        url,
        model,
        prefix,
        &[text.to_owned()],
        dim,
        embedding_model_timeout()?,
    )
    .await?
    .into_iter()
    .next()
    .ok_or_else(|| AppError::Embedding("response lacks embedding".into()))
}

pub(super) async fn embed_query(
    client: &Client,
    url: &str,
    model: &str,
    query: &str,
    dim: usize,
) -> Result<Vec<f32>, AppError> {
    embed_text(client, url, model, "query", query, dim).await
}
