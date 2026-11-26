use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::client::ArtCraftClient;
use crate::tools::http::{raw_json_get, raw_json_post};
use crate::types::{Tool, ToolContent};

fn schema(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required})
}

pub fn tools() -> Vec<Tool> {
    vec![
        Tool { name: "artcraft_create_mcp_session".into(), description: "Create an MCP private session. The private session token and refresh token are returned exactly once; store them immediately. Upstream accepts an API key or web session for this route, not an existing MCP session.".into(), input_schema: Some(schema(json!({
            "client_name":{"type":"string","maxLength":255},
            "client_version":{"type":"string","maxLength":255},
            "client_vendor":{"type":"string","maxLength":255}
        }), &[])) },
        Tool { name: "artcraft_list_mcp_sessions".into(), description: "List your MCP sessions, including terminated sessions, newest first. Secret private session tokens are never listed. Upstream accepts an API key or web session for this route.".into(), input_schema: Some(schema(json!({"limit":{"type":"integer","minimum":1,"maximum":1000},"offset":{"type":"integer","minimum":0}}), &[])) },
        Tool { name: "artcraft_refresh_mcp_session".into(), description: "Refresh the calling MCP private session and rotate its refresh token. This route must be called while the server is authenticated with the MCP private session token; it is deliberately non-idempotent.".into(), input_schema: Some(schema(json!({"private_refresh_token":{"type":"string"}}), &["private_refresh_token"])) },
        Tool { name: "artcraft_revoke_mcp_session".into(), description: "Revoke the calling MCP private session. This route must be called while the server is authenticated with that MCP private session token. Revoked sessions are unrecoverable.".into(), input_schema: Some(schema(json!({}), &[])) },
        Tool { name: "artcraft_delete_mcp_session".into(), description: "Terminate an MCP session by its non-secret management token. Upstream accepts an API key or web session for this dashboard-management route.".into(), input_schema: Some(schema(json!({"session_token":{"type":"string"}}), &["session_token"])) },
    ]
}

fn content(text: String) -> Vec<ToolContent> {
    vec![ToolContent {
        content_type: "text".into(),
        text,
    }]
}

fn session_line(item: &Value) -> String {
    let client = item
        .get("maybe_mcp_client_name")
        .and_then(|v| v.as_str())
        .unwrap_or("(unknown client)");
    format!(
        "  - {} | {} | created:{} | expires:{} | terminated:{}",
        item.get("token").and_then(|v| v.as_str()).unwrap_or("?"),
        client,
        item.get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or("?"),
        item.get("expires_at")
            .and_then(|v| v.as_str())
            .unwrap_or("?"),
        item.get("maybe_deleted_at")
            .and_then(|v| v.as_str())
            .unwrap_or("live")
    )
}

fn client_field(arguments: &Value, key: &str) -> Result<Option<String>> {
    let value = arguments[key]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if value.is_some_and(|s| s.len() > 255) {
        return Err(anyhow!("{} too long (max 255 chars)", key));
    }
    Ok(value.map(str::to_string))
}

pub async fn create_mcp_session(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let response = raw_json_post(
        &client.absolute_url("/v1/mcp/session/create"),
        json!({
            "maybe_mcp_client_name": client_field(&arguments, "client_name")?,
            "maybe_mcp_client_version": client_field(&arguments, "client_version")?,
            "maybe_mcp_client_vendor": client_field(&arguments, "client_vendor")?,
        }),
        client,
    )
    .await?;
    let session = response
        .get("private_session_token")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let refresh = response
        .get("private_refresh_token")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    Ok(content(format!(
        "MCP session created.\nPrivate session token (returned only once; store it now): {}\nPrivate refresh token (rotates on every refresh): {}\n\nUse the session token as ARTCRAFT_MCP_PRIVATE_SESSION_TOKEN or Authorization: Bearer <token>. Sessions expire after 14 days unless refreshed.",
        session, refresh
    )))
}

pub async fn list_mcp_sessions(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let mut parts = Vec::new();
    if let Some(limit) = arguments["limit"].as_u64() {
        parts.push(format!("limit={}", limit));
    }
    if let Some(offset) = arguments["offset"].as_u64() {
        parts.push(format!("offset={}", offset));
    }
    let mut url = client.absolute_url("/v1/mcp/session/list");
    if !parts.is_empty() {
        url.push('?');
        url.push_str(&parts.join("&"));
    }
    let response = raw_json_get(&url, client).await?;
    let rows = response
        .get("mcp_sessions")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().map(session_line).collect::<Vec<_>>())
        .unwrap_or_default();
    Ok(content(if rows.is_empty() {
        "No MCP sessions found.".into()
    } else {
        format!("Found {} MCP sessions:\n{}", rows.len(), rows.join("\n"))
    }))
}

pub async fn refresh_mcp_session(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let refresh = arguments["private_refresh_token"]
        .as_str()
        .ok_or_else(|| anyhow!("private_refresh_token is required"))?;
    let response = raw_json_post(
        &client.absolute_url("/v1/mcp/session/refresh"),
        json!({
            "private_refresh_token": refresh,
        }),
        client,
    )
    .await?;
    let new_refresh = response
        .get("private_refresh_token")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    Ok(content(format!(
        "MCP session refreshed. Store the replacement private refresh token now; the prior refresh token is invalid.\nNew private refresh token: {}",
        new_refresh
    )))
}

pub async fn revoke_mcp_session(
    _arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    raw_json_post(
        &client.absolute_url("/v1/mcp/session/revoke"),
        json!({}),
        client,
    )
    .await?;
    Ok(content("Calling MCP session revoked successfully.".into()))
}

pub async fn delete_mcp_session(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let token = arguments["session_token"]
        .as_str()
        .ok_or_else(|| anyhow!("session_token is required"))?;
    let url = client.absolute_url(&format!(
        "/v1/mcp/session/{}/delete",
        urlencoding::encode(token)
    ));
    raw_json_post(&url, json!({}), client).await?;
    Ok(content(format!("MCP session {} terminated.", token)))
}
