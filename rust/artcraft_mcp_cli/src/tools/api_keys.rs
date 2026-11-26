use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::client::ArtCraftClient;
use crate::tools::http::{raw_delete, raw_json_get, raw_json_post, raw_json_put};
use crate::types::{Tool, ToolContent};

const NOTE: &str = " Upstream API-key management routes currently require web-session cookie credentials; API-key/MCP bearer credentials are not accepted by these routes.";

fn schema(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required})
}

pub fn tools() -> Vec<Tool> {
    vec![
        Tool { name: "artcraft_create_api_key".into(), description: format!("Create an ArtCraft API key. The full secret is returned exactly once; store it immediately.{}", NOTE), input_schema: Some(schema(json!({"name":{"type":"string","maxLength":255},"description":{"type":"string","maxLength":512}}), &["name"])) },
        Tool { name: "artcraft_list_api_keys".into(), description: format!("List your API keys, including soft-deleted rows, newest first. Full secrets are never listed.{}", NOTE), input_schema: Some(schema(json!({"limit":{"type":"integer","minimum":1,"maximum":1000},"offset":{"type":"integer","minimum":0}}), &[])) },
        Tool { name: "artcraft_get_api_key".into(), description: format!("Get one API-key management record by its non-secret api_key_token.{}", NOTE), input_schema: Some(schema(json!({"api_key_token":{"type":"string"}}), &["api_key_token"])) },
        Tool { name: "artcraft_update_api_key".into(), description: format!("Update or clear an API key's description by its non-secret api_key_token.{}", NOTE), input_schema: Some(schema(json!({"api_key_token":{"type":"string"},"description":{"type":"string","maxLength":512}}), &["api_key_token"])) },
        Tool { name: "artcraft_delete_api_key".into(), description: format!("Soft-delete an API key by its non-secret api_key_token. Deleting an already-deleted key is treated as success upstream.{}", NOTE), input_schema: Some(schema(json!({"api_key_token":{"type":"string"}}), &["api_key_token"])) },
    ]
}

fn content(text: String) -> Vec<ToolContent> {
    vec![ToolContent {
        content_type: "text".into(),
        text,
    }]
}

fn required<'a>(arguments: &'a Value, key: &str) -> Result<&'a str> {
    arguments[key]
        .as_str()
        .ok_or_else(|| anyhow!("{} is required", key))
}

fn key_line(item: &Value) -> String {
    format!(
        "  - {} | {} | prefix:{} | created:{} | deleted:{}",
        item.get("token").and_then(|v| v.as_str()).unwrap_or("?"),
        item.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("(unnamed)"),
        item.get("truncated_api_key")
            .and_then(|v| v.as_str())
            .unwrap_or("?"),
        item.get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or("?"),
        item.get("maybe_deleted_at")
            .and_then(|v| v.as_str())
            .unwrap_or("live")
    )
}

pub async fn create_api_key(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let name = required(&arguments, "name")?.trim();
    if name.is_empty() {
        return Err(anyhow!("name must not be empty"));
    }
    if name.len() > 255 {
        return Err(anyhow!("name too long (max 255 chars)"));
    }
    let description = arguments["description"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if description.is_some_and(|s| s.len() > 512) {
        return Err(anyhow!("description too long (max 512 chars)"));
    }
    let response = raw_json_post(
        &client.absolute_url("/v1/api_keys/create"),
        json!({
            "name": name,
            "maybe_description": description,
        }),
        client,
    )
    .await?;
    let token = response
        .get("api_key_token")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let secret = response
        .get("api_key")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    Ok(content(format!(
        "API key created.\nManagement token: {}\nSecret API key (returned only once; store it now): {}\n\nUse it as ARTCRAFT_API_KEY or as Authorization: Bearer <key>.",
        token, secret
    )))
}

pub async fn list_api_keys(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let mut parts = Vec::new();
    if let Some(limit) = arguments["limit"].as_u64() {
        parts.push(format!("limit={}", limit));
    }
    if let Some(offset) = arguments["offset"].as_u64() {
        parts.push(format!("offset={}", offset));
    }
    let mut url = client.absolute_url("/v1/api_keys/list");
    if !parts.is_empty() {
        url.push('?');
        url.push_str(&parts.join("&"));
    }
    let response = raw_json_get(&url, client).await?;
    let rows = response
        .get("api_keys")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().map(key_line).collect::<Vec<_>>())
        .unwrap_or_default();
    Ok(content(if rows.is_empty() {
        "No API keys found.".into()
    } else {
        format!("Found {} API keys:\n{}", rows.len(), rows.join("\n"))
    }))
}

pub async fn get_api_key(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let token = required(&arguments, "api_key_token")?;
    let url = client.absolute_url(&format!("/v1/api_keys/{}", urlencoding::encode(token)));
    let response = raw_json_get(&url, client).await?;
    Ok(content(format!(
        "API key:\n{}",
        key_line(response.get("api_key").unwrap_or(&Value::Null))
    )))
}

pub async fn update_api_key(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let token = required(&arguments, "api_key_token")?;
    let description = arguments["description"].as_str();
    if description.is_some_and(|s| s.len() > 512) {
        return Err(anyhow!("description too long (max 512 chars)"));
    }
    let url = client.absolute_url(&format!("/v1/api_keys/{}", urlencoding::encode(token)));
    raw_json_put(&url, json!({"maybe_description": description}), client).await?;
    Ok(content(format!("API key {} description updated.", token)))
}

pub async fn delete_api_key(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let token = required(&arguments, "api_key_token")?;
    let url = client.absolute_url(&format!("/v1/api_keys/{}", urlencoding::encode(token)));
    raw_delete(&url, client).await?;
    Ok(content(format!("API key {} deleted.", token)))
}
