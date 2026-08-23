use crate::config::AppError;
use reqwest::Client;
use serde::Serialize;
use std::time::Duration;

pub(crate) async fn embed(
    client: &Client,
    url: &str,
    model: &str,
    chunks: &[(String, usize, usize, Option<String>)],
    dim: usize,
) -> Result<Vec<Vec<f32>>, AppError> {
    #[derive(Serialize)]
    struct Input<'a> {
        model: &'a str,
        input: Vec<String>,
    }
    let input = chunks.iter().map(|x| format!("passage: {}", x.0)).collect();
    let value: serde_json::Value = client
        .post(url)
        .timeout(Duration::from_secs(20))
        .json(&Input { model, input })
        .send()
        .await
        .map_err(|e| AppError::Embedding(e.to_string()))?
        .error_for_status()
        .map_err(|e| AppError::Embedding(e.to_string()))?
        .json()
        .await
        .map_err(|e| AppError::Embedding(e.to_string()))?;
    let arr = value
        .get("embeddings")
        .or_else(|| value.get("data"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| AppError::Embedding("response lacks embeddings".into()))?;
    let mut out = Vec::new();
    for item in arr {
        let v = item
            .as_array()
            .or_else(|| item.get("embedding").and_then(|x| x.as_array()))
            .ok_or_else(|| AppError::Embedding("invalid embedding vector".into()))?;
        let row: Vec<f32> = v
            .iter()
            .map(|x| {
                x.as_f64()
                    .map(|n| n as f32)
                    .ok_or_else(|| AppError::Embedding("non-numeric embedding".into()))
            })
            .collect::<Result<_, _>>()?;
        if row.len() != dim {
            return Err(AppError::Embedding(format!(
                "embedding dimension {} != {}",
                row.len(),
                dim
            )));
        }
        out.push(row);
    }
    Ok(out)
}
