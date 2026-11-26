use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::client::ArtCraftClient;
use crate::tools::http::raw_json_post;
use crate::types::{Tool, ToolContent};

fn schema(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required})
}

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "artcraft_convert_fbx_to_gltf".into(),
            description: "Queue a legacy Studio FBX-to-glTF conversion job for an existing FBX media file. High-compute upstream job; access is feature-flagged and cookie/web-session auth is the reliable mode.".into(),
            input_schema: Some(schema(json!({"media_file_token":{"type":"string","description":"Existing FBX media file token"}}), &["media_file_token"])),
        },
        Tool {
            name: "artcraft_convert_render_engine_scene".into(),
            description: "Queue a legacy render-engine scene-to-video conversion job for an existing engine scene/media file (scene.ron, BVH, GLB, etc.). High-compute upstream job; access is feature-flagged and cookie/web-session auth is the reliable mode.".into(),
            input_schema: Some(schema(json!({
                "media_file_token":{"type":"string"},
                "camera":{"type":"string","enum":["static","orbit","pan","zoom"]},
                "camera_speed":{"type":"number"},
                "skybox":{"type":"string"}
            }), &["media_file_token"])),
        },
        Tool {
            name: "artcraft_convert_bvh_to_workflow".into(),
            description: "Legacy alias of the render-engine scene conversion route, exposed upstream as /v1/conversion/enqueue_bvh_to_workflow. High-compute upstream job; access is feature-flagged.".into(),
            input_schema: Some(schema(json!({
                "media_file_token":{"type":"string"},
                "camera":{"type":"string","enum":["static","orbit","pan","zoom"]},
                "camera_speed":{"type":"number"},
                "skybox":{"type":"string"}
            }), &["media_file_token"])),
        },
    ]
}

fn content(text: String) -> Vec<ToolContent> {
    vec![ToolContent {
        content_type: "text".into(),
        text,
    }]
}

fn media_token(arguments: &Value) -> Result<&str> {
    arguments["media_file_token"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("media_file_token is required"))
}

async fn enqueue(
    arguments: Value,
    client: &ArtCraftClient,
    route: &str,
    noun: &str,
) -> Result<Vec<ToolContent>> {
    let token = media_token(&arguments)?;
    let mut body = json!({
        "uuid_idempotency_token": Uuid::new_v4().to_string(),
        "media_file_token": token,
    });
    if route != "/v1/conversion/enqueue_fbx_to_gltf" {
        if let Some(camera) = arguments["camera"].as_str() {
            body["camera"] = json!(camera);
        }
        if let Some(speed) = arguments["camera_speed"].as_f64() {
            body["camera_speed"] = json!(speed);
        }
        if let Some(skybox) = arguments["skybox"].as_str() {
            body["skybox"] = json!(skybox);
        }
    }
    let response = raw_json_post(&client.absolute_url(route), body, client).await?;
    let job = response
        .get("inference_job_token")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    Ok(content(format!(
        "{} job queued.\nJob token: {}\n\nPoll artcraft_get_job_status for completion.",
        noun, job
    )))
}

pub async fn convert_fbx_to_gltf(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    enqueue(
        arguments,
        client,
        "/v1/conversion/enqueue_fbx_to_gltf",
        "FBX-to-glTF conversion",
    )
    .await
}

pub async fn convert_render_engine_scene(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    enqueue(
        arguments,
        client,
        "/v1/conversion/enqueue_render_engine_scene",
        "Render-engine scene conversion",
    )
    .await
}

pub async fn convert_bvh_to_workflow(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    enqueue(
        arguments,
        client,
        "/v1/conversion/enqueue_bvh_to_workflow",
        "BVH-to-workflow conversion",
    )
    .await
}
