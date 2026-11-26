use anyhow::Result;
use serde_json::{json, Value};

use crate::client::ArtCraftClient;
use crate::tools::http::raw_json_get;
use crate::types::{Tool, ToolContent};

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "artcraft_tts_generate".to_string(),
            description: "Deprecated: upstream legacy TTS inference is shut down. This tool does not enqueue work.".to_string(),
            input_schema: Some(json!({"type":"object","properties":{"text":{"type":"string"},"voice_model":{"type":"string"}},"required":["text","voice_model"]})),
        },
        Tool {
            name: "artcraft_tts_search_models".to_string(),
            description: "List legacy TTS voice/model metadata using the upstream /tts/list compatibility endpoint.".to_string(),
            input_schema: Some(json!({"type":"object","properties":{}})),
        },
    ]
}

pub async fn tts_generate(_arguments: Value, _client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: "Legacy TTS inference is shut down upstream. The old /v1/tts/inference compatibility route only returns a synthetic job token and cannot produce audio, so this MCP no longer enqueues TTS jobs.".to_string(),
    }])
}

pub async fn tts_search_models(
    _arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let url = client.absolute_url("/tts/list");
    let response = raw_json_get(&url, client).await?;

    let models = response
        .get("models")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|item| {
                    let token = item
                        .get("model_token")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    let title = item
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("(untitled)");
                    format!("  - {} | {}", token, title)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let text = if models.is_empty() {
        "No TTS models found.".to_string()
    } else {
        format!("Found {} TTS models:\n{}", models.len(), models.join("\n"))
    };

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text,
    }])
}
