use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::client::ArtCraftClient;
use crate::tools::http::{raw_json_delete, raw_json_get};
use crate::types::{Tool, ToolContent};

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "artcraft_list_weights".to_string(),
            description: "List available model weights (LoRAs).".to_string(),
            input_schema: Some(json!({"type":"object","properties":{"cursor":{"type":"string"},"limit":{"type":"integer"}}})),
        },
        Tool {
            name: "artcraft_search_weights".to_string(),
            description: "Search model weights by keyword. Note: upstream search is retired and currently returns an empty successful result set.".to_string(),
            input_schema: Some(json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"]})),
        },
        Tool {
            name: "artcraft_get_weight".to_string(),
            description: "Get details of a specific weight.".to_string(),
            input_schema: Some(json!({"type":"object","properties":{"weight_token":{"type":"string"}},"required":["weight_token"]})),
        },
        Tool {
            name: "artcraft_delete_weight".to_string(),
            description: "Delete a model weight.".to_string(),
            input_schema: Some(json!({"type":"object","properties":{"weight_token":{"type":"string"}},"required":["weight_token"]})),
        },
    ]
}

fn weight_lines(values: &[Value]) -> Vec<String> {
    values
        .iter()
        .map(|item| {
            let token = item
                .get("weight_token")
                .or_else(|| item.get("token"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("(untitled)");
            format!("  - {} | {}", token, title)
        })
        .collect()
}

pub async fn list_weights(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let limit = arguments["limit"].as_u64().unwrap_or(25);
    let mut url = client.absolute_url(&format!("/v1/weights/list?page_size={}", limit));
    if let Some(cursor) = arguments["cursor"].as_str() {
        url.push_str(&format!("&cursor={}", urlencoding::encode(cursor)));
    }
    let response = raw_json_get(&url, client).await?;

    let weights = response
        .get("results")
        .and_then(|v| v.as_array())
        .map(|arr| weight_lines(arr))
        .unwrap_or_default();

    let text = if weights.is_empty() {
        "No weights found.".to_string()
    } else {
        format!("Found {} weights:\n{}", weights.len(), weights.join("\n"))
    };

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text,
    }])
}

pub async fn search_weights(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let query = arguments["query"]
        .as_str()
        .ok_or_else(|| anyhow!("query is required"))?;

    let url = client.absolute_url(&format!(
        "/v1/weights/search?search_term={}",
        urlencoding::encode(query)
    ));
    let response = raw_json_get(&url, client).await?;

    let weights = response
        .get("weights")
        .and_then(|v| v.as_array())
        .map(|arr| weight_lines(arr))
        .unwrap_or_default();

    let text = if weights.is_empty() {
        format!("No weights found matching '{}' (upstream weight search is retired and may always return empty).", query)
    } else {
        format!(
            "Found {} results for '{}':\n{}",
            weights.len(),
            query,
            weights.join("\n")
        )
    };

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text,
    }])
}

pub async fn get_weight(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let token_str = arguments["weight_token"]
        .as_str()
        .ok_or_else(|| anyhow!("weight_token is required"))?;

    let url = client.absolute_url(&format!("/v1/weights/weight/{}", token_str));
    let response = raw_json_get(&url, client).await?;

    let title = response
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let token = response
        .get("weight_token")
        .and_then(|v| v.as_str())
        .unwrap_or(token_str);

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!("Weight:\n  Token: {}\n  Title: {}", token, title),
    }])
}

pub async fn delete_weight(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let token_str = arguments["weight_token"]
        .as_str()
        .ok_or_else(|| anyhow!("weight_token is required"))?;

    let url = client.absolute_url(&format!("/v1/weights/weight/{}", token_str));
    raw_json_delete(&url, json!({ "set_delete": true, "as_mod": false }), client).await?;

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!("Weight {} deleted successfully.", token_str),
    }])
}
