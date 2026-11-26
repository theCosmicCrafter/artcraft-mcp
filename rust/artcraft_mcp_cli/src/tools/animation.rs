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
            name: "artcraft_create_face_animation".into(),
            description: "Queue a legacy SadTalker face-animation/lipsync job from one audio source and one image source. This is a paid/high-compute legacy upstream job; poll the returned inference job token. Cookie/web-session auth is the reliable auth mode.".into(),
            input_schema: Some(schema(json!({
                "audio_media_file_token":{"type":"string","description":"Audio source as a media file token (choose exactly one audio source)"},
                "audio_media_upload_token":{"type":"string","description":"Audio source as a media upload token"},
                "tts_result_token":{"type":"string","description":"Audio source as a legacy TTS result token"},
                "voice_conversion_result_token":{"type":"string","description":"Audio source as a legacy voice-conversion result token"},
                "image_media_file_token":{"type":"string","description":"Image source as a media file token (choose exactly one image source)"},
                "image_media_upload_token":{"type":"string","description":"Image source as a media upload token"},
                "visibility":{"type":"string","enum":["public","private"]},
                "dimensions":{"type":"string","enum":["twitter_landscape","twitter_portrait","twitter_square"],"description":"Output dimensions preset; upstream default twitter_landscape"},
                "make_still":{"type":"boolean"},
                "disable_face_enhancement":{"type":"boolean"},
                "remove_watermark":{"type":"boolean","description":"Only honored when the upstream plan permits watermark removal"}
            }), &[])),
        },
        Tool {
            name: "artcraft_create_rerender_animation".into(),
            description: "Queue a legacy video rerender job. Upstream marks this product as never launched, so worker availability may be limited; treat this as a high-cost experimental legacy capability. Cookie/web-session auth is the reliable auth mode.".into(),
            input_schema: Some(schema(json!({
                "video_media_file_token":{"type":"string"},
                "sd_model_token":{"type":"string","description":"Stable-diffusion model weight token"},
                "lora_model_token":{"type":"string","description":"Optional LoRA model weight token"},
                "prompt":{"type":"string"},
                "a_prompt":{"type":"string","description":"Prompt describing the original video"},
                "n_prompt":{"type":"string","description":"Negative prompt"},
                "seed":{"type":"integer"},
                "visibility":{"type":"string","enum":["public","private"]}
            }), &["video_media_file_token","sd_model_token"])),
        },
    ]
}

fn content(text: String) -> Vec<ToolContent> {
    vec![ToolContent {
        content_type: "text".into(),
        text,
    }]
}

fn count_present(arguments: &Value, keys: &[&str]) -> usize {
    keys.iter()
        .filter(|key| arguments[*key].as_str().is_some_and(|s| !s.is_empty()))
        .count()
}

pub async fn create_face_animation(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let audio_keys = [
        "audio_media_file_token",
        "audio_media_upload_token",
        "tts_result_token",
        "voice_conversion_result_token",
    ];
    let image_keys = ["image_media_file_token", "image_media_upload_token"];
    if count_present(&arguments, &audio_keys) != 1 {
        return Err(anyhow!("choose exactly one audio source token"));
    }
    if count_present(&arguments, &image_keys) != 1 {
        return Err(anyhow!("choose exactly one image source token"));
    }

    let body = json!({
        "uuid_idempotency_token": Uuid::new_v4().to_string(),
        "audio_source": {
            "maybe_media_file_token": arguments["audio_media_file_token"].as_str(),
            "maybe_media_upload_token": arguments["audio_media_upload_token"].as_str(),
            "maybe_tts_result_token": arguments["tts_result_token"].as_str(),
            "maybe_voice_conversion_result_token": arguments["voice_conversion_result_token"].as_str(),
        },
        "image_source": {
            "maybe_media_file_token": arguments["image_media_file_token"].as_str(),
            "maybe_media_upload_token": arguments["image_media_upload_token"].as_str(),
        },
        "creator_set_visibility": arguments["visibility"].as_str(),
        "dimensions": arguments["dimensions"].as_str(),
        "make_still": arguments["make_still"].as_bool(),
        "disable_face_enhancement": arguments["disable_face_enhancement"].as_bool(),
        "remove_watermark": arguments["remove_watermark"].as_bool(),
    });
    let response = raw_json_post(
        &client.absolute_url("/v1/animation/face_animation/create"),
        body,
        client,
    )
    .await?;
    let job = response
        .get("inference_job_token")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    Ok(content(format!(
        "Face animation job queued.\nJob token: {}\n\nPoll artcraft_get_job_status for completion.",
        job
    )))
}

pub async fn create_rerender_animation(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let video = arguments["video_media_file_token"]
        .as_str()
        .ok_or_else(|| anyhow!("video_media_file_token is required"))?;
    let sd_model = arguments["sd_model_token"]
        .as_str()
        .ok_or_else(|| anyhow!("sd_model_token is required"))?;
    let body = json!({
        "uuid_idempotency_token": Uuid::new_v4().to_string(),
        "prompt": arguments["prompt"].as_str(),
        "a_prompt": arguments["a_prompt"].as_str(),
        "n_prompt": arguments["n_prompt"].as_str(),
        "video_source": video,
        "sd_model_token": sd_model,
        "lora_model_token": arguments["lora_model_token"].as_str(),
        "seed": arguments["seed"].as_i64().map(|v| v as i32),
        "creator_set_visibility": arguments["visibility"].as_str(),
    });
    let response = raw_json_post(
        &client.absolute_url("/v1/animation/rerender/create"),
        body,
        client,
    )
    .await?;
    let job = response
        .get("inference_job_token")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    Ok(content(format!("Rerender animation job queued.\nJob token: {}\n\nPoll artcraft_get_job_status for completion. Upstream marks this legacy product as never launched, so worker availability may be limited.", job)))
}
