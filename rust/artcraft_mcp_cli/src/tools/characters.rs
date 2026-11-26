use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::client::ArtCraftClient;
use crate::tools::http::{raw_json_delete, raw_json_get, raw_json_post};
use crate::types::{Tool, ToolContent};

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "artcraft_create_character".to_string(),
            description: "Create a character from a reference image for consistent identity. Returns an inference job token; poll it and then list characters when complete.".to_string(),
            input_schema: Some(json!({
                "type":"object",
                "properties":{
                    "name":{"type":"string"},
                    "image_media_token":{"type":"string"},
                    "description":{"type":"string"},
                    "model":{"type":"string","enum":["seedance_2p0"],"description":"Currently only seedance_2p0 is supported upstream"}
                },
                "required":["name","image_media_token"]
            })),
        },
        Tool {
            name: "artcraft_list_characters".to_string(),
            description: "List user characters for the current session.".to_string(),
            input_schema: Some(json!({"type":"object","properties":{"cursor":{"type":"integer"}}})),
        },
        Tool {
            name: "artcraft_get_character".to_string(),
            description: "Get a character by token.".to_string(),
            input_schema: Some(json!({"type":"object","properties":{"character_token":{"type":"string"}},"required":["character_token"]})),
        },
        Tool {
            name: "artcraft_delete_character".to_string(),
            description: "Delete a character.".to_string(),
            input_schema: Some(json!({"type":"object","properties":{"character_token":{"type":"string"}},"required":["character_token"]})),
        },
    ]
}

pub async fn create_character(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let name = arguments["name"]
        .as_str()
        .ok_or_else(|| anyhow!("name is required"))?;
    let image_token = arguments["image_media_token"]
        .as_str()
        .ok_or_else(|| anyhow!("image_media_token is required"))?;
    let model = arguments["model"].as_str().unwrap_or("seedance_2p0");
    if model != "seedance_2p0" {
        return Err(anyhow!("only seedance_2p0 is currently supported upstream"));
    }

    let url = client.absolute_url("/v1/character/create");
    let mut body = json!({
        "uuid_idempotency_token": Uuid::new_v4().to_string(),
        "model": model,
        "image_media_token": image_token,
        "character_name": name,
    });
    if let Some(description) = arguments["description"].as_str() {
        body["character_description"] = json!(description);
    }
    let response = raw_json_post(&url, body, client).await?;
    let job_token = response
        .get("inference_job_token")
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!(
            "Character creation queued successfully.\nJob token: {}\n\nPoll artcraft_get_job_status, then use artcraft_list_characters after completion to obtain the character token.",
            job_token
        ),
    }])
}

pub async fn list_characters(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let mut url = client.absolute_url("/v1/characters/session");
    if let Some(cursor) = arguments["cursor"].as_u64() {
        url.push_str(&format!("?cursor={}", cursor));
    }
    let response = raw_json_get(&url, client).await?;

    let results = response
        .get("characters")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|item| {
                    let token = item.get("token").and_then(|v| v.as_str()).unwrap_or("?");
                    let name = item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("(unnamed)");
                    format!("  - {} | {}", token, name)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut text = if results.is_empty() {
        "No characters found.".to_string()
    } else {
        format!(
            "Found {} characters:\n{}",
            results.len(),
            results.join("\n")
        )
    };
    if let Some(next) = response.get("next_cursor").and_then(|v| v.as_u64()) {
        text.push_str(&format!("\nNext cursor: {}", next));
    }

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text,
    }])
}

pub async fn get_character(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let token_str = arguments["character_token"]
        .as_str()
        .ok_or_else(|| anyhow!("character_token is required"))?;

    let url = client.absolute_url(&format!("/v1/character/{}", token_str));
    let response = raw_json_get(&url, client).await?;
    let character = response
        .get("character")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let name = character
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let token = character
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap_or(token_str);
    let description = character
        .get("maybe_description")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!(
            "Character:\n  Token: {}\n  Name: {}\n  Description: {}",
            token, name, description
        ),
    }])
}

pub async fn delete_character(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let token_str = arguments["character_token"]
        .as_str()
        .ok_or_else(|| anyhow!("character_token is required"))?;

    let url = client.absolute_url(&format!("/v1/character/{}", token_str));
    raw_json_delete(&url, json!({}), client).await?;

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!("Character {} deleted successfully.", token_str),
    }])
}
