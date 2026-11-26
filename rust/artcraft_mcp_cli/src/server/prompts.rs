//! Prompt-template library for ArtCraft generation workflows.
//!
//! The `artcraft_*` prompts are the user-facing library returned by
//! `prompts/list`. The `test_*` prompts are small protocol fixtures required
//! by the MCP conformance suite; they are intentionally callable but not
//! advertised as part of ArtCraft's creative prompt catalog.

use rmcp::model::*;
use rmcp::ErrorData as McpError;
use serde_json::{json, Map, Value};

use super::fixtures;

fn argument(name: &str, description: &str, required: bool) -> PromptArgument {
    PromptArgument::new(name)
        .with_description(description)
        .with_required(required)
}

fn artcraft_prompt_definitions() -> Vec<Prompt> {
    vec![
        Prompt::new(
            "artcraft_image_generation",
            Some("Compose a cost-aware ArtCraft image generation request"),
            Some(vec![
                argument("subject", "Subject or scene to depict", true),
                argument("style", "Visual style or medium", false),
                argument(
                    "aspect_ratio",
                    "Desired aspect ratio such as 1:1, 16:9, or 9:16",
                    false,
                ),
                argument("negative_prompt", "Elements to avoid", false),
            ]),
        )
        .with_title("ArtCraft Image Generation"),
        Prompt::new(
            "artcraft_video_generation",
            Some("Compose a cost-aware ArtCraft video generation request"),
            Some(vec![
                argument("subject", "Subject, scene, or action to depict", true),
                argument("motion", "Camera movement or motion direction", false),
                argument("duration", "Target duration such as 2s, 4s, or 8s", false),
                argument(
                    "aspect_ratio",
                    "Desired aspect ratio such as 16:9 or 9:16",
                    false,
                ),
            ]),
        )
        .with_title("ArtCraft Video Generation"),
        Prompt::new(
            "artcraft_audio_generation",
            Some("Compose a cost-aware ArtCraft music or audio generation request"),
            Some(vec![
                argument(
                    "subject",
                    "Theme, lyrics seed, or soundscape to create",
                    true,
                ),
                argument("genre", "Musical genre or sound-design style", false),
                argument("duration", "Target duration", false),
                argument(
                    "vocals",
                    "Vocal treatment such as instrumental, choir, or spoken word",
                    false,
                ),
            ]),
        )
        .with_title("ArtCraft Audio Generation"),
        Prompt::new(
            "artcraft_3d_asset",
            Some("Compose a cost-aware ArtCraft 3D object or Gaussian splat generation request"),
            Some(vec![
                argument(
                    "object",
                    "Object, creature, environment, or scene to model",
                    true,
                ),
                argument("style", "Art direction and level of realism", false),
                argument(
                    "topology",
                    "Topology target such as game-ready-low-poly or high-detail-sculpt",
                    false,
                ),
                argument("output_type", "Desired output: mesh or splat", false),
            ]),
        )
        .with_title("ArtCraft 3D Asset"),
        Prompt::new(
            "artcraft_costed_generation_plan",
            Some("Plan a generation across image, video, audio, 3D, or splat with cost preflight"),
            Some(vec![
                argument(
                    "modality",
                    "Generation modality: image, video, audio, 3d, or splat",
                    true,
                ),
                argument(
                    "creative_brief",
                    "Creative goal and important constraints",
                    true,
                ),
                argument(
                    "budget_credits",
                    "Maximum credits the user is willing to spend",
                    false,
                ),
            ]),
        )
        .with_title("Costed Generation Plan"),
    ]
}

pub fn list_prompts() -> ListPromptsResult {
    ListPromptsResult::with_all_items(artcraft_prompt_definitions())
}

fn required_string<'a>(arguments: &'a Map<String, Value>, name: &str) -> Result<&'a str, McpError> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            McpError::invalid_params(format!("missing required prompt argument '{name}'"), None)
        })
}

fn optional_string<'a>(arguments: &'a Map<String, Value>, name: &str) -> Option<&'a str> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

fn user_text(text: impl Into<String>) -> PromptMessage {
    PromptMessage::new_text(Role::User, text)
}

fn complete(messages: Vec<PromptMessage>, description: &str) -> GetPromptResponse {
    GetPromptResult::new(messages)
        .with_description(description)
        .into()
}

fn get_artcraft_prompt(
    name: &str,
    arguments: &Map<String, Value>,
) -> Result<GetPromptResponse, McpError> {
    match name {
        "artcraft_image_generation" => {
            let subject = required_string(arguments, "subject")?;
            let mut text = format!(
                "Create an ArtCraft image generation request for this subject: {subject}.\n\
                 Use artcraft_estimate_cost before any billable generation, then use artcraft_generate_image only after the cost is acceptable.\n\
                 Return a concise final prompt plus recommended model/settings."
            );
            if let Some(style) = optional_string(arguments, "style") {
                text.push_str(&format!("\nStyle direction: {style}."));
            }
            if let Some(aspect_ratio) = optional_string(arguments, "aspect_ratio") {
                text.push_str(&format!("\nAspect ratio: {aspect_ratio}."));
            }
            if let Some(negative) = optional_string(arguments, "negative_prompt") {
                text.push_str(&format!("\nAvoid: {negative}."));
            }
            Ok(complete(
                vec![user_text(text)],
                "Cost-aware ArtCraft image generation prompt",
            ))
        }
        "artcraft_video_generation" => {
            let subject = required_string(arguments, "subject")?;
            let mut text = format!(
                "Create an ArtCraft video generation request for: {subject}.\n\
                 Preflight with artcraft_estimate_cost before generation because video is a high-cost modality.\n\
                 Produce a shot-level prompt with subject, action, lighting, camera movement, and style."
            );
            if let Some(motion) = optional_string(arguments, "motion") {
                text.push_str(&format!("\nMotion/camera direction: {motion}."));
            }
            if let Some(duration) = optional_string(arguments, "duration") {
                text.push_str(&format!("\nTarget duration: {duration}."));
            }
            if let Some(aspect_ratio) = optional_string(arguments, "aspect_ratio") {
                text.push_str(&format!("\nAspect ratio: {aspect_ratio}."));
            }
            Ok(complete(
                vec![user_text(text)],
                "Cost-aware ArtCraft video generation prompt",
            ))
        }
        "artcraft_audio_generation" => {
            let subject = required_string(arguments, "subject")?;
            let mut text = format!(
                "Create an ArtCraft audio generation request for: {subject}.\n\
                 Estimate cost first, then generate audio or speech with the least expensive model that fits the brief.\n\
                 Include arrangement, instrumentation, tempo, mood, and production notes."
            );
            if let Some(genre) = optional_string(arguments, "genre") {
                text.push_str(&format!("\nGenre/style: {genre}."));
            }
            if let Some(duration) = optional_string(arguments, "duration") {
                text.push_str(&format!("\nTarget duration: {duration}."));
            }
            if let Some(vocals) = optional_string(arguments, "vocals") {
                text.push_str(&format!("\nVocal treatment: {vocals}."));
            }
            Ok(complete(
                vec![user_text(text)],
                "Cost-aware ArtCraft audio generation prompt",
            ))
        }
        "artcraft_3d_asset" => {
            let object = required_string(arguments, "object")?;
            let output_type = optional_string(arguments, "output_type").unwrap_or("mesh");
            let mut text = format!(
                "Create an ArtCraft {output_type} generation request for: {object}.\n\
                 Use artcraft_estimate_cost or artcraft_estimate_splat_cost before generation.\n\
                 Specify silhouette, materials, scale, intended use, and quality constraints."
            );
            if let Some(style) = optional_string(arguments, "style") {
                text.push_str(&format!("\nArt direction: {style}."));
            }
            if let Some(topology) = optional_string(arguments, "topology") {
                text.push_str(&format!("\nTopology/quality target: {topology}."));
            }
            Ok(complete(
                vec![user_text(text)],
                "Cost-aware ArtCraft 3D generation prompt",
            ))
        }
        "artcraft_costed_generation_plan" => {
            let modality = required_string(arguments, "modality")?;
            let brief = required_string(arguments, "creative_brief")?;
            let mut text = format!(
                "Plan an ArtCraft {modality} generation for this creative brief:\n{brief}\n\n\
                 Steps:\n\
                 1. Rewrite the brief as a high-quality generation prompt.\n\
                 2. Choose the cheapest suitable model and settings.\n\
                 3. Call artcraft_estimate_cost before any billable call.\n\
                 4. State the estimated credits and ask for confirmation when required.\n\
                 5. Track the resulting job with artcraft_get_job_status or the tasks extension."
            );
            if let Some(budget) = optional_string(arguments, "budget_credits") {
                text.push_str(&format!("\nBudget ceiling: {budget} credits. Do not exceed it without explicit approval."));
            }
            Ok(complete(
                vec![user_text(text)],
                "ArtCraft generation plan with cost preflight",
            ))
        }
        _ => Err(McpError::invalid_params(
            format!("unknown prompt '{name}'"),
            None,
        )),
    }
}

/// Canonical MCP protocol fixture prompts. These mirror the conformance
/// suite's documented fixtures exactly enough to exercise argument
/// substitution, embedded resources, and image content.
fn get_protocol_fixture_prompt(
    name: &str,
    arguments: &Map<String, Value>,
) -> Result<Option<GetPromptResponse>, McpError> {
    let response = match name {
        "test_simple_prompt" => complete(
            vec![user_text("This is a simple prompt for testing.")],
            "Simple MCP prompt fixture",
        ),
        "test_prompt_with_arguments" => {
            let arg1 = required_string(arguments, "arg1")?;
            let arg2 = required_string(arguments, "arg2")?;
            complete(
                vec![user_text(format!(
                    "Prompt with arguments: arg1='{arg1}', arg2='{arg2}'"
                ))],
                "Parameterized MCP prompt fixture",
            )
        }
        "test_prompt_with_embedded_resource" => {
            let uri = required_string(arguments, "resourceUri")?;
            complete(
                vec![
                    PromptMessage::new(
                        Role::User,
                        ContentBlock::resource(
                            ResourceContents::text("Embedded resource content for testing.", uri)
                                .with_mime_type("text/plain"),
                        ),
                    ),
                    user_text("Please process the embedded resource above."),
                ],
                "MCP embedded-resource prompt fixture",
            )
        }
        "test_prompt_with_image" => complete(
            vec![
                PromptMessage::new(
                    Role::User,
                    ContentBlock::image(fixtures::PNG_1X1_RED_BASE64, "image/png"),
                ),
                user_text("Please analyze the image above."),
            ],
            "MCP image prompt fixture",
        ),
        _ => return Ok(None),
    };
    Ok(Some(response))
}

pub fn get_prompt(
    name: &str,
    arguments: Option<&Map<String, Value>>,
) -> Result<GetPromptResponse, McpError> {
    let empty = Map::new();
    let arguments = arguments.unwrap_or(&empty);
    if let Some(fixture) = get_protocol_fixture_prompt(name, arguments)? {
        return Ok(fixture);
    }
    get_artcraft_prompt(name, arguments)
}

/// Completion candidates for user-facing prompt arguments. Free-text fields
/// intentionally return an empty list; completion must not invent required
/// creative content. Enum-like fields return useful static vocabularies.
pub fn prompt_completion_candidates(prompt_name: &str, argument_name: &str) -> Vec<&'static str> {
    match (prompt_name, argument_name) {
        ("artcraft_image_generation", "style") => vec![
            "photorealistic",
            "cinematic",
            "anime",
            "watercolor",
            "concept art",
            "isometric",
            "oil painting",
        ],
        ("artcraft_image_generation", "aspect_ratio")
        | ("artcraft_video_generation", "aspect_ratio") => {
            vec!["1:1", "16:9", "9:16", "4:3", "3:4", "21:9"]
        }
        ("artcraft_video_generation", "motion") => vec![
            "static camera",
            "slow push-in",
            "orbit",
            "tracking shot",
            "crane shot",
            "handheld",
        ],
        ("artcraft_video_generation", "duration") => vec!["2s", "4s", "6s", "8s", "10s"],
        ("artcraft_audio_generation", "genre") => vec![
            "ambient",
            "cinematic orchestral",
            "electronic",
            "lo-fi",
            "rock",
            "sound design",
        ],
        ("artcraft_audio_generation", "duration") => vec!["10s", "30s", "60s", "2min", "3min"],
        ("artcraft_audio_generation", "vocals") => vec![
            "instrumental",
            "female vocals",
            "male vocals",
            "choir",
            "spoken word",
        ],
        ("artcraft_3d_asset", "topology") => vec![
            "game-ready-low-poly",
            "high-detail-sculpt",
            "printable-manifold",
            "stylized-mid-poly",
        ],
        ("artcraft_3d_asset", "output_type") => vec!["mesh", "splat"],
        ("artcraft_costed_generation_plan", "modality") => {
            vec!["image", "video", "audio", "3d", "splat"]
        }
        // Conformance fixture arguments are free-form; these examples merely
        // demonstrate ranked prefix filtering without pretending to know the
        // user's creative intent.
        ("test_prompt_with_arguments", "arg1") => vec!["testValue1", "testAlpha", "example"],
        ("test_prompt_with_arguments", "arg2") => vec!["testValue2", "testBeta", "example"],
        _ => Vec::new(),
    }
}

/// Machine-readable representation used by catalog resources.
pub fn prompt_catalog_json() -> Value {
    let prompts = artcraft_prompt_definitions()
        .into_iter()
        .map(|prompt| {
            json!({
                "name": prompt.name,
                "title": prompt.title,
                "description": prompt.description,
                "arguments": prompt.arguments.unwrap_or_default().into_iter().map(|arg| json!({
                    "name": arg.name,
                    "description": arg.description,
                    "required": arg.required.unwrap_or(false),
                })).collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    json!({ "prompts": prompts })
}
