//! Progressive discovery meta-tools (Phase 2): artcraft_search_tools,
//! artcraft_describe_tool, artcraft_call_deferred_tool.
//!
//! Token-weighted search over names/categories/tags/descriptions, detail
//! levels per Anthropic's progressive-disclosure research, and dispatch into
//! the deferred tool handlers with schema-guided argument coercion.

use anyhow::{Result, anyhow};
use serde_json::Value;

use crate::client::ArtCraftClient;
use crate::registry::{REGISTRY, ToolMeta, meta_for};
use crate::types::{Tool, ToolContent};

pub fn meta_tool_defs() -> Vec<Tool> {
    vec![
        Tool {
            name: "artcraft_search_tools".to_string(),
            description: "Search the full ArtCraft tool catalog by keyword, category, or capability. Returns matching tool names with summaries; use artcraft_describe_tool for full schemas before calling artcraft_call_deferred_tool.".to_string(),
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search terms (e.g. 'lora weights', 'upload', 'video')"
                    },
                    "category": {
                        "type": "string",
                        "enum": ["image", "video", "3d", "splat", "audio", "character", "lora", "jobs", "account", "community", "files"],
                        "description": "Optional category filter"
                    },
                    "detail_level": {
                        "type": "string",
                        "enum": ["names", "summary", "full"],
                        "description": "Amount of detail per match (default: summary)"
                    }
                },
                "required": ["query"]
            })),
        },
        Tool {
            name: "artcraft_describe_tool".to_string(),
            description: "Get the full input schema, output description, and usage examples for one or more deferred tools before invoking them through artcraft_call_deferred_tool.".to_string(),
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "names": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Tool names to describe (1-5)"
                    }
                },
                "required": ["names"]
            })),
        },
        Tool {
            name: "artcraft_call_deferred_tool".to_string(),
            description: "Execute any ArtCraft tool that is not in the hot list. Arguments are validated and coerced against the target tool's schema, so stringified numbers/booleans ('5', 'true') are accepted.".to_string(),
            input_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "The exact tool name (from artcraft_search_tools)"
                    },
                    "arguments": {
                        "type": "object",
                        "description": "Arguments matching the target tool's input schema"
                    }
                },
                "required": ["name"]
            })),
        },
    ]
}

fn tool_def_by_name(name: &str) -> Option<Tool> {
    crate::tools::get_all_tools()
        .into_iter()
        .find(|t| t.name == name)
}

/// Tokenize into lowercase alphanumeric terms.
fn terms(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect()
}

fn score(meta: &ToolMeta, description: &str, query_terms: &[String]) -> u32 {
    let mut score = 0u32;
    let name = meta.name.to_lowercase();
    let tags: Vec<String> = meta.tags.iter().map(|t| t.to_lowercase()).collect();
    let desc = description.to_lowercase();
    let cat = meta.category.as_str().to_lowercase();
    for term in query_terms {
        if name.contains(term.as_str()) {
            score += 8;
        }
        if tags.iter().any(|t| t.contains(term.as_str())) {
            score += 5;
        }
        if cat.contains(term.as_str()) {
            score += 4;
        }
        if desc.contains(term.as_str()) {
            score += 2;
        }
    }
    score
}

pub async fn search_tools(arguments: Value) -> Result<Vec<ToolContent>> {
    let query = arguments["query"].as_str().unwrap_or("");
    let category_filter = arguments["category"].as_str();
    let detail = arguments["detail_level"].as_str().unwrap_or("summary");

    if query.trim().is_empty() && category_filter.is_none() {
        return Err(anyhow!("query is required (or pass a category to browse)"));
    }

    let query_terms = terms(query);
    let all_defs = crate::tools::get_all_tools();

    let mut scored: Vec<(u32, &ToolMeta, Option<&Tool>)> = REGISTRY
        .iter()
        .filter(|m| {
            category_filter
                .map(|c| c.eq_ignore_ascii_case(m.category.as_str()))
                .unwrap_or(true)
        })
        .map(|m| {
            let def = all_defs.iter().find(|t| t.name == m.name);
            let desc = def.map(|t| t.description.as_str()).unwrap_or("");
            let s = if query_terms.is_empty() {
                1
            } else {
                score(m, desc, &query_terms)
            };
            (s, m, def)
        })
        .filter(|(s, _, _)| *s > 0)
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.name.cmp(b.1.name)));
    scored.truncate(15);

    let mut out = String::new();
    if scored.is_empty() {
        out.push_str("No tools matched. Try broader terms or browse with a category filter.\n");
    }
    for (s, m, def) in &scored {
        match detail {
            "names" => {
                out.push_str(m.name);
                out.push('\n');
            }
            "full" => {
                out.push_str(&format!(
                    "## {} (category: {}, cost: {}, async: {}, score: {})\n",
                    m.name,
                    m.category.as_str(),
                    m.cost.as_str(),
                    m.async_job,
                    s
                ));
                if let Some(d) = def {
                    out.push_str(&d.description);
                    out.push('\n');
                    if let Some(schema) = &d.input_schema {
                        out.push_str("```json\n");
                        out.push_str(&serde_json::to_string_pretty(schema)?);
                        out.push_str("\n```\n");
                    }
                }
            }
            _ => {
                let short = def
                    .map(|d| {
                        d.description
                            .split('.')
                            .next()
                            .unwrap_or("")
                            .to_string()
                    })
                    .unwrap_or_default();
                out.push_str(&format!(
                    "- {} [{}] — {}\n",
                    m.name,
                    m.category.as_str(),
                    short
                ));
            }
        }
    }

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: out,
    }])
}

pub async fn describe_tool(arguments: Value) -> Result<Vec<ToolContent>> {
    let names = arguments["names"]
        .as_array()
        .ok_or_else(|| anyhow!("names must be an array of tool names"))?;
    if names.is_empty() || names.len() > 5 {
        return Err(anyhow!("pass between 1 and 5 tool names"));
    }

    let mut out = String::new();
    for name_val in names {
        let name = name_val.as_str().unwrap_or("");
        match tool_def_by_name(name) {
            Some(def) => {
                let meta = meta_for(name);
                out.push_str(&format!("## {}\n", def.name));
                out.push_str(&def.description);
                out.push('\n');
                if let Some(m) = meta {
                    out.push_str(&format!(
                        "category: {}, cost tier: {}, async job: {}\n",
                        m.category.as_str(),
                        m.cost.as_str(),
                        m.async_job
                    ));
                }
                if let Some(schema) = &def.input_schema {
                    out.push_str("inputSchema:\n```json\n");
                    out.push_str(&serde_json::to_string_pretty(schema)?);
                    out.push_str("\n```\n");
                }
                if let Some(example) = example_for(name) {
                    out.push_str("example:\n```json\n");
                    out.push_str(example);
                    out.push_str("\n```\n");
                }
                out.push('\n');
            }
            None => {
                out.push_str(&format!("## {}\nNo such tool. Use artcraft_search_tools to find valid names.\n\n", name));
            }
        }
    }

    Ok(vec![ToolContent {
        content_type: "text".to_string(),
        text: out,
    }])
}

/// A handful of realistic usage examples for the most common deferred tools.
fn example_for(name: &str) -> Option<&'static str> {
    Some(match name {
        "artcraft_inpaint_image" => {
            r#"{"image_media_token": "mf_abc123", "mask_media_token": "mf_def456", "prompt": "replace the sky with a sunset"}"#
        }
        "artcraft_edit_video" => {
            r#"{"model": "switch_x", "video_media_token": "mf_abc123", "prompt": "make it look like nighttime"}"#
        }
        "artcraft_edit_image" => {
            r#"{"model": "gemini_25_flash", "image_media_token": "mf_abc123", "prompt": "make the cube blue"}"#
        }
        "artcraft_upload_image" => r#"{"file_path": "C:/art/character.png"}"#,
        "artcraft_generate_splat" => {
            r#"{"model": "marble_1p1_plus", "image_media_token": "mf_abc123"}"#
        }
        _ => return None,
    })
}

/// Dispatch through the deferred-tool path with schema-guided coercion.
pub async fn call_deferred_tool(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let name = arguments["name"]
        .as_str()
        .ok_or_else(|| anyhow!("name is required"))?;

    if crate::registry::META_TOOLS.contains(&name) {
        return Err(anyhow!("{} is a meta-tool; call it directly", name));
    }
    let def = tool_def_by_name(name)
        .ok_or_else(|| anyhow!("unknown tool '{}' — use artcraft_search_tools", name))?;

    let raw_args = arguments.get("arguments").cloned().unwrap_or(Value::Null);
    let coerced = match (&def.input_schema, raw_args) {
        (Some(schema), Value::Object(map)) => {
            Value::Object(crate::coercion::coerce_object(map, schema))
        }
        (_, other) => other,
    };

    crate::tools::execute_tool(name, coerced, client).await
}

