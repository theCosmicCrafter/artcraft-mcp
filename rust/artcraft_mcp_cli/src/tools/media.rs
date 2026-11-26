use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use uuid::Uuid;

use crate::client::ArtCraftClient;
use crate::tools::http::{raw_json_delete, raw_json_get, raw_json_post, raw_multipart_post};
use crate::types::{Tool, ToolContent};

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "artcraft_upload_image".to_string(),
            description: "Upload an image for use as reference or keyframe in generation.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "Local file path to image" }
                },
                "required": ["file_path"]
            })),
        },
        Tool {
            name: "artcraft_upload_video".to_string(),
            description: "Upload a video for use as reference in generation.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string" }
                },
                "required": ["file_path"]
            })),
        },
        Tool {
            name: "artcraft_upload_audio".to_string(),
            description: "Upload an audio file.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "file_path": { "type": "string" }
                },
                "required": ["file_path"]
            })),
        },
        Tool {
            name: "artcraft_get_media_file".to_string(),
            description: "Get details about a media file by token.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "media_token": { "type": "string" }
                },
                "required": ["media_token"]
            })),
        },
        Tool {
            name: "artcraft_download_media_file".to_string(),
            description: "Download a generated media file (image, video, audio, 3D mesh, or splat) directly to local disk.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "media_token": {
                        "type": "string",
                        "description": "Media file token to download"
                    },
                    "download_dir": {
                        "type": "string",
                        "description": "Optional local directory path to save the file into. Defaults to ~/Artcraft/downloads or user downloads folder."
                    },
                    "filename": {
                        "type": "string",
                        "description": "Optional custom filename with extension (e.g., 'render.png')"
                    }
                },
                "required": ["media_token"]
            })),
        },
        Tool {
            name: "artcraft_list_media_files".to_string(),
            description: "List all session media files with pagination.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "cursor": { "type": "string" },
                    "limit": { "type": "integer" }
                }
            })),
        },
        Tool {
            name: "artcraft_search_media".to_string(),
            description: "Search session or featured media.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            })),
        },
        Tool {
            name: "artcraft_delete_media_file".to_string(),
            description: "Delete a media file by token.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "media_token": { "type": "string" }
                },
                "required": ["media_token"]
            })),
        },
        Tool {
            name: "artcraft_rename_media_file".to_string(),
            description: "Rename a media file.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "media_token": { "type": "string" },
                    "title": { "type": "string" }
                },
                "required": ["media_token", "title"]
            })),
        },
        Tool {
            name: "artcraft_set_media_visibility".to_string(),
            description: "Set media visibility to public or private.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "media_token": { "type": "string" },
                    "is_public": { "type": "boolean" }
                },
                "required": ["media_token", "is_public"]
            })),
        },
    ]
}

async fn upload_media_file(
    arguments: Value,
    client: &ArtCraftClient,
    route_path: &str,
    noun: &str,
) -> Result<Vec<ToolContent>> {
    let file_path = arguments["file_path"]
        .as_str()
        .ok_or_else(|| anyhow!("file_path is required"))?;

    let mut fields = vec![("uuid_idempotency_token", Uuid::new_v4().to_string())];
    if let Some(title) = arguments["title"].as_str() {
        fields.push(("maybe_title", title.to_string()));
    }
    if let Some(visibility) = arguments["visibility"].as_str() {
        fields.push(("maybe_visibility", visibility.to_string()));
    }

    let url = client.absolute_url(route_path);
    let response = raw_multipart_post(&url, file_path, &fields, client).await?;
    let media_token = response
        .get("media_file_token")
        .and_then(|v| v.as_str())
        .unwrap_or("?");

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!(
            "{} uploaded successfully.\nMedia token: {}",
            noun, media_token
        ),
    }])
}

pub async fn upload_image(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    upload_media_file(arguments, client, "/v1/media_files/upload/image", "Image").await
}

pub async fn upload_video(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    upload_media_file(
        arguments,
        client,
        "/v1/media_files/upload/new_video",
        "Video",
    )
    .await
}

pub async fn upload_audio(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    upload_media_file(arguments, client, "/v1/media_files/upload/audio", "Audio").await
}

pub async fn get_media_file(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let token_str = arguments["media_token"]
        .as_str()
        .ok_or_else(|| anyhow!("media_token is required"))?;

    let url = client.absolute_url(&format!("/v1/media_files/file/{}", token_str));
    let response = raw_json_get(&url, client).await?;

    let info = response
        .get("media_file")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let text = format!(
        "Media file details:\n  Token: {}\n  Title: {}\n  Type: {}\n  Class: {}\n  Created: {}\n  CDN URL: {}",
        info.get("token").and_then(|v| v.as_str()).unwrap_or(token_str),
        info.get("maybe_title").and_then(|v| v.as_str()).unwrap_or("(untitled)"),
        info.get("media_type").and_then(|v| v.as_str()).unwrap_or("?"),
        info.get("media_class").and_then(|v| v.as_str()).unwrap_or("?"),
        info.get("created_at").and_then(|v| v.as_str()).unwrap_or("?"),
        info.pointer("/media_links/cdn_url").and_then(|v| v.as_str()).unwrap_or("(none)")
    );

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text,
    }])
}

pub async fn list_media_files(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    // `/v1/media_files/list` is feature-flag gated upstream (ExploreMedia) and
    // can reject otherwise valid sessions. The supported replacement is the
    // per-user v2 listing; resolve the current username and include uploads.
    let session_url = client.absolute_url("/v1/session");
    let session = raw_json_get(&session_url, client).await?;
    let username = session
        .pointer("/user/core_info/username")
        .or_else(|| session.pointer("/user/username"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("not logged in; cannot list user media files"))?;

    let page_size = arguments["limit"].as_u64().unwrap_or(25).clamp(1, 100);
    let page_index = arguments["cursor"]
        .as_str()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let url = client.absolute_url(&format!(
        "/v1/media_files/list_v2/user/{}?include_user_uploads=true&page_size={}&page_index={}",
        urlencoding::encode(username),
        page_size,
        page_index
    ));
    let response = raw_json_get(&url, client).await?;

    let results = response
        .get("results")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|item| {
                    let token = item.get("token").and_then(|v| v.as_str()).unwrap_or("?");
                    let title = item
                        .get("maybe_title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("(untitled)");
                    let class = item
                        .get("media_class")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    let visibility = item
                        .get("creator_set_visibility")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    format!("  - {} | {} | {} | {}", token, title, class, visibility)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut text = if results.is_empty() {
        "No media files found.".to_string()
    } else {
        format!(
            "Found {} media files:\n{}",
            results.len(),
            results.join("\n")
        )
    };
    if response
        .pointer("/pagination/has_more")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        text.push_str(&format!("\nNext cursor: {}", page_index + 1));
    }

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text,
    }])
}

pub async fn search_media(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let query = arguments["query"]
        .as_str()
        .ok_or_else(|| anyhow!("query is required"))?;

    let api_hostname = client.api_host.to_api_hostname_and_scheme();
    let url = format!(
        "{}/v1/media_files/search_session?search_term={}",
        api_hostname,
        urlencoding::encode(query)
    );
    let response = raw_json_get(&url, client).await?;

    let results = response
        .get("results")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|item| {
                    let token = item.get("token").and_then(|v| v.as_str()).unwrap_or("?");
                    let title = item
                        .get("maybe_title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("(untitled)");
                    let class = item
                        .get("media_class")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    format!("  - {} | {} | {}", token, title, class)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let text = if results.is_empty() {
        "No media files found matching your search.".to_string()
    } else {
        format!(
            "Found {} results for '{}':\n{}",
            results.len(),
            query,
            results.join("\n")
        )
    };

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text,
    }])
}

pub async fn rename_media_file(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let token_str = arguments["media_token"]
        .as_str()
        .ok_or_else(|| anyhow!("media_token is required"))?;
    let title = arguments["title"]
        .as_str()
        .ok_or_else(|| anyhow!("title is required"))?;

    let api_hostname = client.api_host.to_api_hostname_and_scheme();
    let url = format!("{}/v1/media_files/rename/{}", api_hostname, token_str);
    let body = json!({ "title": title });
    raw_json_post(&url, body, client).await?;

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!("Media file {} renamed to '{}'.", token_str, title),
    }])
}

pub async fn set_media_visibility(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let token_str = arguments["media_token"]
        .as_str()
        .ok_or_else(|| anyhow!("media_token is required"))?;
    let is_public = arguments["is_public"]
        .as_bool()
        .ok_or_else(|| anyhow!("is_public boolean is required"))?;

    let visibility = if is_public { "public" } else { "private" };
    let api_hostname = client.api_host.to_api_hostname_and_scheme();
    let url = format!("{}/v1/media_files/visibility/{}", api_hostname, token_str);
    let body = json!({ "visibility": visibility });
    raw_json_post(&url, body, client).await?;

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!("Media file {} visibility set to {}.", token_str, visibility),
    }])
}

pub async fn delete_media_file(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let token_str = arguments["media_token"]
        .as_str()
        .ok_or_else(|| anyhow!("media_token is required"))?;

    let url = client.absolute_url(&format!("/v1/media_files/file/{}", token_str));
    raw_json_delete(&url, json!({ "set_delete": true, "as_mod": false }), client).await?;

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!("Media file {} deleted successfully.", token_str),
    }])
}

pub async fn download_media_file(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let token_str = arguments["media_token"]
        .as_str()
        .ok_or_else(|| anyhow!("media_token is required"))?;
    let url = client.absolute_url(&format!("/v1/media_files/file/{}", token_str));
    let response = raw_json_get(&url, client).await?;
    let info = response
        .get("media_file")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let cdn_url_string = info
        .pointer("/media_links/cdn_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("media file has no CDN URL"))?
        .to_string();
    let cdn_url = reqwest::Url::parse(&cdn_url_string)?;

    let download_dir = if let Some(dir) = arguments["download_dir"].as_str() {
        std::path::PathBuf::from(dir)
    } else {
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        let artcraft_downloads = home.join("Artcraft").join("downloads");
        if artcraft_downloads.exists() {
            artcraft_downloads
        } else {
            dirs::download_dir().unwrap_or(home)
        }
    };

    std::fs::create_dir_all(&download_dir)?;

    let extension = {
        let path = cdn_url.path();
        if let Some(ext) = std::path::Path::new(path)
            .extension()
            .and_then(|s| s.to_str())
        {
            ext.to_string()
        } else {
            "bin".to_string()
        }
    };

    let filename = if let Some(name) = arguments["filename"].as_str() {
        name.to_string()
    } else {
        let base_name = info
            .get("maybe_title")
            .and_then(|v| v.as_str())
            .unwrap_or(token_str);
        let safe_name: String = base_name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        format!("{}.{}", safe_name, extension)
    };

    let target_file_path = download_dir.join(&filename);

    let resp = reqwest::get(cdn_url.as_str()).await?;
    let bytes = resp.bytes().await?;
    std::fs::write(&target_file_path, &bytes)?;

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!(
            "Media file downloaded successfully.\nToken: {}\nSaved to: {}\nSize: {} bytes ({:.2} MB)\nCDN URL: {}",
            token_str,
            target_file_path.display(),
            bytes.len(),
            bytes.len() as f64 / 1_048_576.0,
            cdn_url.as_str()
        ),
    }])
}
