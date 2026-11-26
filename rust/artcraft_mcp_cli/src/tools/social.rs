use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::client::ArtCraftClient;
use crate::tools::http::{raw_json_get, raw_json_post};
use crate::types::{Tool, ToolContent};

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "artcraft_create_bookmark".to_string(),
            description: "Bookmark a media file.".to_string(),
            input_schema: Some(json!({"type":"object","properties":{"media_token":{"type":"string"}},"required":["media_token"]})),
        },
        Tool {
            name: "artcraft_list_bookmarks".to_string(),
            description: "List your bookmarks.".to_string(),
            input_schema: Some(json!({"type":"object","properties":{}})),
        },
        Tool {
            name: "artcraft_delete_bookmark".to_string(),
            description: "Delete a bookmark by its bookmark token.".to_string(),
            input_schema: Some(json!({"type":"object","properties":{"bookmark_token":{"type":"string"}},"required":["bookmark_token"]})),
        },
        Tool {
            name: "artcraft_rate_media".to_string(),
            description: "Rate a media file (thumbs up/down).".to_string(),
            input_schema: Some(json!({"type":"object","properties":{"media_token":{"type":"string"},"rating":{"type":"string","enum":["up","down","neutral"]}},"required":["media_token","rating"]})),
        },
        Tool {
            name: "artcraft_create_comment".to_string(),
            description: "Comment on a media file.".to_string(),
            input_schema: Some(json!({"type":"object","properties":{"media_token":{"type":"string"},"text":{"type":"string"}},"required":["media_token","text"]})),
        },
        Tool {
            name: "artcraft_list_comments".to_string(),
            description: "List comments on a media file.".to_string(),
            input_schema: Some(json!({"type":"object","properties":{"media_token":{"type":"string"}},"required":["media_token"]})),
        },
        Tool {
            name: "artcraft_delete_comment".to_string(),
            description: "Delete a comment by its comment token.".to_string(),
            input_schema: Some(json!({"type":"object","properties":{"comment_token":{"type":"string"},"as_mod":{"type":"boolean"}},"required":["comment_token"]})),
        },
        Tool {
            name: "artcraft_list_tags".to_string(),
            description: "List tags on a media file.".to_string(),
            input_schema: Some(json!({"type":"object","properties":{"media_token":{"type":"string"}},"required":["media_token"]})),
        },
        Tool {
            name: "artcraft_set_tags".to_string(),
            description: "Replace the complete tag set on a media file. Pass an empty tags array to clear tags.".to_string(),
            input_schema: Some(json!({"type":"object","properties":{"media_token":{"type":"string"},"tags":{"type":"array","items":{"type":"string"}},"add":{"type":"boolean","description":"Deprecated compatibility input; ignored. The upstream endpoint always replaces the full set."}},"required":["media_token","tags"]})),
        },
    ]
}

pub async fn create_bookmark(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let media_token = arguments["media_token"]
        .as_str()
        .ok_or_else(|| anyhow!("media_token is required"))?;

    let url = client.absolute_url("/v1/user_bookmarks/create");
    let body = json!({
        "entity_token": media_token,
        "entity_type": "media_file",
    });
    let response = raw_json_post(&url, body, client).await?;
    let bookmark_token = response
        .get("user_bookmark_token")
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!(
            "Bookmarked media {}.\nBookmark token: {}",
            media_token, bookmark_token
        ),
    }])
}

pub async fn list_bookmarks(
    _arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    // Upstream removed `/v1/user_bookmarks/list/session`; the live replacement
    // is keyed by username. Resolve the current session first, then query the
    // live list route.
    let session_url = client.absolute_url("/v1/session");
    let session = raw_json_get(&session_url, client).await?;
    let username = session
        .pointer("/user/core_info/username")
        .or_else(|| session.pointer("/user/username"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("not logged in; cannot list user bookmarks"))?;

    let url = client.absolute_url(&format!(
        "/v1/user_bookmarks/list/user/{}",
        urlencoding::encode(username)
    ));
    let response = raw_json_get(&url, client).await?;

    let bookmarks = response
        .get("results")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|item| {
                    let token = item.get("token").and_then(|v| v.as_str()).unwrap_or("?");
                    let entity = item
                        .get("details")
                        .and_then(|d| d.get("entity_token"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    format!("  - {} -> {}", token, entity)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let text = if bookmarks.is_empty() {
        "No bookmarks found.".to_string()
    } else {
        format!(
            "Found {} bookmarks:\n{}",
            bookmarks.len(),
            bookmarks.join("\n")
        )
    };

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text,
    }])
}

pub async fn delete_bookmark(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let bookmark_token = arguments["bookmark_token"]
        .as_str()
        .ok_or_else(|| anyhow!("bookmark_token is required"))?;
    let url = client.absolute_url(&format!("/v1/user_bookmarks/delete/{}", bookmark_token));
    raw_json_post(&url, json!({ "as_mod": Value::Null }), client).await?;
    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!("Bookmark {} deleted successfully.", bookmark_token),
    }])
}

pub async fn rate_media(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let media_token = arguments["media_token"]
        .as_str()
        .ok_or_else(|| anyhow!("media_token is required"))?;
    let rating = arguments["rating"]
        .as_str()
        .ok_or_else(|| anyhow!("rating is required"))?;
    let rating_value = match rating {
        "up" => "positive",
        "down" => "negative",
        "neutral" => "neutral",
        _ => return Err(anyhow!("rating must be 'up', 'down', or 'neutral'")),
    };

    let url = client.absolute_url("/v1/user_rating/rate");
    let body = json!({
        "entity_type": "media_file",
        "entity_token": media_token,
        "rating_value": rating_value,
    });
    raw_json_post(&url, body, client).await?;

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!("Rated media {} {}.", media_token, rating),
    }])
}

pub async fn create_comment(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let media_token = arguments["media_token"]
        .as_str()
        .ok_or_else(|| anyhow!("media_token is required"))?;
    let text = arguments["text"]
        .as_str()
        .ok_or_else(|| anyhow!("text is required"))?;

    let url = client.absolute_url("/v1/comments/new");
    let body = json!({
        "uuid_idempotency_token": Uuid::new_v4().to_string(),
        "entity_type": "media_file",
        "entity_token": media_token,
        "comment_markdown": text,
    });
    let response = raw_json_post(&url, body, client).await?;
    let comment_token = response
        .get("comment_token")
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!(
            "Comment posted on media {}.\nComment token: {}",
            media_token, comment_token
        ),
    }])
}

pub async fn list_comments(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let media_token = arguments["media_token"]
        .as_str()
        .ok_or_else(|| anyhow!("media_token is required"))?;

    let url = client.absolute_url(&format!("/v1/comments/list/media_file/{}", media_token));
    let response = raw_json_get(&url, client).await?;

    let comments = response
        .get("comments")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|item| {
                    let text = item
                        .get("comment_markdown")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    let author = item
                        .get("username")
                        .and_then(|v| v.as_str())
                        .unwrap_or("anonymous");
                    let token = item.get("token").and_then(|v| v.as_str()).unwrap_or("?");
                    format!("  - {} | {}: {}", token, author, text)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let text = if comments.is_empty() {
        "No comments found.".to_string()
    } else {
        format!(
            "Found {} comments:\n{}",
            comments.len(),
            comments.join("\n")
        )
    };

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text,
    }])
}

pub async fn delete_comment(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let comment_token = arguments["comment_token"]
        .as_str()
        .ok_or_else(|| anyhow!("comment_token is required"))?;
    let as_mod = arguments["as_mod"].as_bool().unwrap_or(false);
    let url = client.absolute_url(&format!("/v1/comments/delete/{}", comment_token));
    raw_json_post(&url, json!({ "as_mod": as_mod }), client).await?;
    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!("Comment {} deleted successfully.", comment_token),
    }])
}

pub async fn list_tags(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let media_token = arguments["media_token"]
        .as_str()
        .ok_or_else(|| anyhow!("media_token is required"))?;

    let url = client.absolute_url(&format!("/v1/tags/media_file/list/{}", media_token));
    let response = raw_json_get(&url, client).await?;

    let tags = response
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|item| {
                    item.get("tag_value")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?")
                        .to_string()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let text = if tags.is_empty() {
        "No tags found.".to_string()
    } else {
        format!("Tags: {}", tags.join(", "))
    };

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text,
    }])
}

pub async fn set_tags(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let media_token = arguments["media_token"]
        .as_str()
        .ok_or_else(|| anyhow!("media_token is required"))?;
    let tags = arguments["tags"]
        .as_array()
        .ok_or_else(|| anyhow!("tags array is required"))?
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect::<Vec<_>>();

    let url = client.absolute_url(&format!("/v1/tags/media_file/set/{}", media_token));
    let body = json!({
        "maybe_tags": Value::Null,
        "maybe_tags_list": tags,
    });
    raw_json_post(&url, body, client).await?;

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!("Tag set replaced on media {}.", media_token),
    }])
}
