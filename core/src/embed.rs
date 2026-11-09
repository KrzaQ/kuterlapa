//! The embedding provider: turns text into vectors for search by meaning
//! (search.rs). Default: kuterlapa-embed, with a local model or any
//! OpenAI-compatible server (see embed/ and docs/protocol.md).

use crate::{paths, provider, socket};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::time::Duration;

pub fn supervise(command: Vec<String>) {
    provider::supervise("embed", command);
}

/// Vectors for `texts`; `query` marks a search query, which some models
/// embed with an instruction in front.
pub fn vectors(texts: &[String], query: bool) -> Result<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let request = json!({ "kind": if query { "query" } else { "document" }, "texts": texts });
    let answer = socket::request_waiting(&paths::embed_socket(), &format!("embed {request}"), Duration::from_secs(120))
        .context("the embedding provider (kuterlapa-embed) is not running")?;
    if let Some(e) = answer.strip_prefix("error: ") {
        bail!("embedding provider: {e}");
    }
    let value: Value = serde_json::from_str(&answer).context("embedding provider answer")?;
    let vectors = value["vectors"]
        .as_array()
        .context("no vectors")?
        .iter()
        .map(|v| v.as_array().map(|a| a.iter().filter_map(Value::as_f64).map(|x| x as f32).collect()).unwrap_or_default())
        .collect::<Vec<Vec<f32>>>();
    if vectors.len() != texts.len() {
        bail!("embedding provider gave {} vectors for {} texts", vectors.len(), texts.len());
    }
    Ok(vectors)
}
