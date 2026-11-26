use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use uuid::Uuid;

use artcraft_api_defs::generate::image::angle::flux_2_lora_edit_image_angle::{
    Flux2LoraEditImageAngleImageSize, Flux2LoraEditImageAngleNumImages,
    Flux2LoraEditImageAngleRequest,
};
use artcraft_api_defs::generate::image::bg_removal::remove_image_background::RemoveImageBackgroundRequest;
use artcraft_api_defs::generate::image::edit::flux_pro_kontext_max_edit_image::{
    FluxProKontextMaxEditImageNumImages, FluxProKontextMaxEditImageRequest,
};
use artcraft_api_defs::generate::image::edit::gemini_25_flash_edit_image::{
    Gemini25FlashEditImageImageQuality, Gemini25FlashEditImageNumImages,
    Gemini25FlashEditImageRequest,
};
use artcraft_api_defs::generate::image::edit::gpt_image_1_edit_image::{
    GptImage1EditImageImageQuality, GptImage1EditImageImageSize, GptImage1EditImageNumImages,
    GptImage1EditImageRequest,
};
use artcraft_api_defs::generate::image::inpaint::flux_dev_juggernaut_inpaint_image::{
    FluxDevJuggernautInpaintImageNumImages, FluxDevJuggernautInpaintImageRequest,
};
use artcraft_api_defs::generate::video::edit::beeble_switchx_edit_video::BeebleSwitchXEditVideoRequest;
use tokens::tokens::media_files::MediaFileToken;

use crate::client::ArtCraftClient;
use crate::tools::http::typed_json_post;
use crate::types::{Tool, ToolContent};

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "artcraft_edit_image".to_string(),
            description: "Re-edit an image using Flux Pro Kontext Max, Gemini 2.5 Flash, GPT-Image 1, Qwen, or SeedEdit 3.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "model": { "type": "string" },
                    "prompt": { "type": "string" },
                    "image_media_token": { "type": "string" }
                },
                "required": ["model", "prompt", "image_media_token"]
            })),
        },
        Tool {
            name: "artcraft_remove_background".to_string(),
            description: "Remove the background from an image.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "image_media_token": { "type": "string" }
                },
                "required": ["image_media_token"]
            })),
        },
        Tool {
            name: "artcraft_inpaint_image".to_string(),
            description: "Inpaint an image using a mask and prompt.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "model": { "type": "string" },
                    "prompt": { "type": "string" },
                    "image_media_token": { "type": "string" },
                    "mask_media_token": { "type": "string" }
                },
                "required": ["prompt", "image_media_token"]
            })),
        },
        Tool {
            name: "artcraft_edit_image_angle".to_string(),
            description: "Manipulate camera angle of an image using Flux 2 LoRA or Qwen.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "image_media_token": { "type": "string" },
                    "horizontal_angle": { "type": "number" },
                    "vertical_angle": { "type": "number" },
                    "zoom": { "type": "number" }
                },
                "required": ["image_media_token"]
            })),
        },
        Tool {
            name: "artcraft_edit_video".to_string(),
            description: "Edit a video using Beeble SwitchX.".to_string(),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "video_media_token": { "type": "string" },
                    "prompt": { "type": "string" }
                },
                "required": ["video_media_token", "prompt"]
            })),
        },
    ]
}

pub async fn edit_image(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let model_str = arguments["model"]
        .as_str()
        .ok_or_else(|| anyhow!("model is required"))?;
    let prompt = arguments["prompt"]
        .as_str()
        .ok_or_else(|| anyhow!("prompt is required"))?;
    let image_token = arguments["image_media_token"]
        .as_str()
        .ok_or_else(|| anyhow!("image_media_token is required"))?;

    let image_media_token = MediaFileToken::new_from_str(image_token);
    let idempotency_token = Uuid::new_v4().to_string();

    let (job_token, success) = match model_str.to_lowercase().as_str() {
        "flux_pro_kontext_max" | "flux_pro_kontext" | "kontext" | "flux" => {
            let request = FluxProKontextMaxEditImageRequest {
                uuid_idempotency_token: idempotency_token,
                prompt: Some(prompt.to_string()),
                image_media_token,
                num_images: Some(FluxProKontextMaxEditImageNumImages::One),
            };
            let response = typed_json_post(artcraft_api_defs::generate::image::edit::flux_pro_kontext_max_edit_image::FLUX_PRO_KONTEXT_MAX_EDIT_IMAGE_PATH, request, client).await?;
            (
                response
                    .get("inference_job_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string(),
                response
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            )
        }
        "gpt_image_1" | "gpt_image" | "gpt" | "openai" => {
            let request = GptImage1EditImageRequest {
                uuid_idempotency_token: idempotency_token,
                prompt: Some(prompt.to_string()),
                image_media_tokens: Some(vec![image_media_token]),
                image_size: Some(GptImage1EditImageImageSize::Square),
                num_images: Some(GptImage1EditImageNumImages::One),
                image_quality: Some(GptImage1EditImageImageQuality::High),
            };
            let response = typed_json_post(artcraft_api_defs::generate::image::edit::gpt_image_1_edit_image::GPT_IMAGE_1_EDIT_IMAGE_PATH, request, client).await?;
            (
                response
                    .get("inference_job_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string(),
                response
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            )
        }
        "gemini_25_flash" | "gemini" | "google" => {
            let request = Gemini25FlashEditImageRequest {
                uuid_idempotency_token: idempotency_token,
                prompt: Some(prompt.to_string()),
                image_media_tokens: Some(vec![image_media_token]),
                num_images: Some(Gemini25FlashEditImageNumImages::One),
                image_quality: Some(Gemini25FlashEditImageImageQuality::High),
            };
            let response = typed_json_post(artcraft_api_defs::generate::image::edit::gemini_25_flash_edit_image::GEMINI_25_FLASH_EDIT_IMAGE_PATH, request, client).await?;
            (
                response
                    .get("inference_job_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string(),
                response
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            )
        }
        _ => {
            return Err(anyhow!(
                "Unknown edit model: {}. Use flux_pro_kontext_max, gpt_image_1, or gemini_25_flash",
                model_str
            ))
        }
    };

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!(
            "Image edit queued successfully.\nJob token: {}\nStatus: {}\n\nPoll for results using artcraft_get_job_status with job_token '{}'",
            job_token,
            if success { "success" } else { "failed" },
            job_token
        ),
    }])
}

pub async fn remove_background(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let image_token = arguments["image_media_token"]
        .as_str()
        .ok_or_else(|| anyhow!("image_media_token is required"))?;

    let media_file_token = Some(MediaFileToken::new_from_str(image_token));
    let idempotency_token = Uuid::new_v4().to_string();

    let request = RemoveImageBackgroundRequest {
        uuid_idempotency_token: idempotency_token,
        media_file_token,
    };

    let response = typed_json_post(artcraft_api_defs::generate::image::bg_removal::remove_image_background::REMOVE_IMAGE_BACKGROUND_PATH, request, client).await?;

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!(
            "Background removal queued successfully.\nJob token: {}\nStatus: {}\n\nPoll for results using artcraft_get_job_status with job_token '{}'",
            response.get("inference_job_token").and_then(|v| v.as_str()).unwrap_or("?"),
            if response.get("success").and_then(|v| v.as_bool()).unwrap_or(false) { "success" } else { "failed" },
            response.get("inference_job_token").and_then(|v| v.as_str()).unwrap_or("?")
        ),
    }])
}

pub async fn inpaint_image(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let prompt = arguments["prompt"]
        .as_str()
        .ok_or_else(|| anyhow!("prompt is required"))?;
    let image_token = arguments["image_media_token"]
        .as_str()
        .ok_or_else(|| anyhow!("image_media_token is required"))?;
    let mask_token = arguments["mask_media_token"]
        .as_str()
        .ok_or_else(|| anyhow!("mask_media_token is required"))?;

    let image_media_token = MediaFileToken::new_from_str(image_token);
    let mask_media_token = MediaFileToken::new_from_str(mask_token);
    let idempotency_token = Uuid::new_v4().to_string();

    let request = FluxDevJuggernautInpaintImageRequest {
        uuid_idempotency_token: idempotency_token,
        prompt: Some(prompt.to_string()),
        image_media_token,
        mask_media_token,
        num_images: Some(FluxDevJuggernautInpaintImageNumImages::One),
    };

    let response = typed_json_post(artcraft_api_defs::generate::image::inpaint::flux_dev_juggernaut_inpaint_image::FLUX_DEV_JUGGERNAUT_INPAINT_PATH, request, client).await?;

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!(
            "Inpainting queued successfully.\nJob token: {}\nStatus: {}\n\nPoll for results using artcraft_get_job_status with job_token '{}'",
            response.get("inference_job_token").and_then(|v| v.as_str()).unwrap_or("?"),
            if response.get("success").and_then(|v| v.as_bool()).unwrap_or(false) { "success" } else { "failed" },
            response.get("inference_job_token").and_then(|v| v.as_str()).unwrap_or("?")
        ),
    }])
}

pub async fn edit_image_angle(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let image_token = arguments["image_media_token"]
        .as_str()
        .ok_or_else(|| anyhow!("image_media_token is required"))?;

    let image_media_token = MediaFileToken::new_from_str(image_token);
    let idempotency_token = Uuid::new_v4().to_string();

    let request = Flux2LoraEditImageAngleRequest {
        uuid_idempotency_token: idempotency_token,
        image_media_token,
        horizontal_angle: arguments["horizontal_angle"].as_f64(),
        vertical_angle: arguments["vertical_angle"].as_f64(),
        zoom: arguments["zoom"].as_f64(),
        num_images: Some(Flux2LoraEditImageAngleNumImages::One),
        image_size: Some(Flux2LoraEditImageAngleImageSize::Square),
    };

    let response = typed_json_post(artcraft_api_defs::generate::image::angle::flux_2_lora_edit_image_angle::FLUX_2_LORA_EDIT_IMAGE_ANGLE_PATH, request, client).await?;

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!(
            "Angle edit queued successfully.\nJob token: {}\nStatus: {}\n\nPoll for results using artcraft_get_job_status with job_token '{}'",
            response.get("inference_job_token").and_then(|v| v.as_str()).unwrap_or("?"),
            if response.get("success").and_then(|v| v.as_bool()).unwrap_or(false) { "success" } else { "failed" },
            response.get("inference_job_token").and_then(|v| v.as_str()).unwrap_or("?")
        ),
    }])
}

pub async fn edit_video(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let video_token = arguments["video_media_token"]
        .as_str()
        .ok_or_else(|| anyhow!("video_media_token is required"))?;
    let prompt = arguments["prompt"]
        .as_str()
        .ok_or_else(|| anyhow!("prompt is required"))?;

    let source_video_media_token = Some(MediaFileToken::new_from_str(video_token));
    let idempotency_token = Uuid::new_v4().to_string();

    let request = BeebleSwitchXEditVideoRequest {
        uuid_idempotency_token: idempotency_token,
        source_video_media_token,
        reference_image_media_token: None,
        prompt: Some(prompt.to_string()),
    };

    let response = typed_json_post(artcraft_api_defs::generate::video::edit::beeble_switchx_edit_video::BEEBLE_SWITCHX_EDIT_VIDEO_PATH, request, client).await?;

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: format!(
            "Video edit queued successfully.\nJob token: {}\nStatus: {}\n\nPoll for results using artcraft_get_job_status with job_token '{}'",
            response.get("inference_job_token").and_then(|v| v.as_str()).unwrap_or("?"),
            if response.get("success").and_then(|v| v.as_bool()).unwrap_or(false) { "success" } else { "failed" },
            response.get("inference_job_token").and_then(|v| v.as_str()).unwrap_or("?")
        ),
    }])
}
