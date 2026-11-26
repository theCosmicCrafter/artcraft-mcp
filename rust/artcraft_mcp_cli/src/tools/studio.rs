use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::client::ArtCraftClient;
use crate::tools::http::raw_multipart_post_named_file;
use crate::types::{Tool, ToolContent};

fn schema(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required})
}

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "artcraft_upload_studio_scene_snapshot".into(),
            description: "Upload an image snapshot of a 3D Image Studio scene. Accepts JPEG, PNG, GIF, or WebP bytes; upstream stores it as an intermediate system file. Cookie/web-session auth is the reliable auth mode for this legacy route.".into(),
            input_schema: Some(schema(json!({
                "file_path":{"type":"string","description":"Local image file to upload as the scene snapshot"},
                "scene_media_token":{"type":"string","description":"Optional engine scene media token to link"},
                "visibility":{"type":"string","enum":["public","private"],"description":"Optional visibility"}
            }), &["file_path"])),
        },
        Tool {
            name: "artcraft_upload_studio_shot".into(),
            description: "Upload a Studio render as a zip of image frames. Upstream converts the frames to MP4 at the requested frame rate (default 12 fps). Requires web-session cookie auth.".into(),
            input_schema: Some(schema(json!({
                "file_path":{"type":"string","description":"Local .zip file containing image frames"},
                "frame_rate_fps":{"type":"integer","minimum":1,"maximum":255,"description":"Output video frame rate; upstream default 12"},
                "scene_source_media_file_token":{"type":"string","description":"Optional engine scene media token to link"},
                "title":{"type":"string"},
                "visibility":{"type":"string","enum":["public","private"],"description":"Optional visibility"}
            }), &["file_path"])),
        },
    ]
}

fn content(text: String) -> Vec<ToolContent> {
    vec![ToolContent {
        content_type: "text".into(),
        text,
    }]
}

fn required_path(arguments: &Value) -> Result<&str> {
    arguments["file_path"]
        .as_str()
        .ok_or_else(|| anyhow!("file_path is required"))
}

pub async fn upload_studio_scene_snapshot(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let file_path = required_path(&arguments)?;
    let mut fields = vec![("uuid_idempotency_token", Uuid::new_v4().to_string())];
    if let Some(token) = arguments["scene_media_token"]
        .as_str()
        .filter(|s| !s.is_empty())
    {
        fields.push(("scene_media_token", token.to_string()));
    }
    if let Some(visibility) = arguments["visibility"].as_str().filter(|s| !s.is_empty()) {
        fields.push(("maybe_visibility", visibility.to_string()));
    }
    let url = client.absolute_url("/v1/image_studio/scene_snapshot");
    let response =
        raw_multipart_post_named_file(&url, file_path, "snapshot", &fields, client).await?;
    let token = response
        .get("snapshot_media_token")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    Ok(content(format!(
        "Studio scene snapshot uploaded successfully.\nSnapshot media token: {}",
        token
    )))
}

pub async fn upload_studio_shot(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let file_path = required_path(&arguments)?;
    if !file_path.to_ascii_lowercase().ends_with(".zip") {
        return Err(anyhow!(
            "studio shot file_path must be a .zip archive of image frames"
        ));
    }
    let mut fields = vec![("uuid_idempotency_token", Uuid::new_v4().to_string())];
    if let Some(fps) = arguments["frame_rate_fps"].as_u64() {
        if fps == 0 || fps > 255 {
            return Err(anyhow!("frame_rate_fps must be between 1 and 255"));
        }
        fields.push(("maybe_frame_rate_fps", fps.to_string()));
    }
    if let Some(token) = arguments["scene_source_media_file_token"]
        .as_str()
        .filter(|s| !s.is_empty())
    {
        fields.push(("maybe_scene_source_media_file_token", token.to_string()));
    }
    if let Some(title) = arguments["title"].as_str().filter(|s| !s.is_empty()) {
        fields.push(("maybe_title", title.to_string()));
    }
    if let Some(visibility) = arguments["visibility"].as_str().filter(|s| !s.is_empty()) {
        fields.push(("maybe_visibility", visibility.to_string()));
    }
    let url = client.absolute_url("/v1/media_files/upload/studio_shot");
    let response = raw_multipart_post_named_file(&url, file_path, "file", &fields, client).await?;
    let token = response
        .get("media_file_token")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    Ok(content(format!(
        "Studio shot uploaded and converted successfully.\nMedia token: {}",
        token
    )))
}
