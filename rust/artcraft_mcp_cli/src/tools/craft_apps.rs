use anyhow::Result;
use serde_json::{json, Value};

use crate::client::ArtCraftClient;
use crate::tools::http::raw_json_get;
use crate::types::{Tool, ToolContent};

pub fn tools() -> Vec<Tool> {
    vec![Tool {
        name: "artcraft_get_craft_apps_release_info".into(),
        description: "Get the latest Craft app release information cached by the public ArtCraft backend: app versions, release assets, repository/app renames, and launcher notices. Public and free.".into(),
        input_schema: Some(json!({"type":"object","properties":{}})),
    }]
}

pub async fn get_craft_apps_release_info(
    _arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let response =
        raw_json_get(&client.absolute_url("/v1/craft_apps/release_info"), client).await?;
    let empty = Vec::new();
    let apps = response
        .pointer("/release_info/apps")
        .and_then(|v| v.as_object());
    let mut lines = Vec::new();
    if let Some(apps) = apps {
        for (key, app) in apps {
            let name = app.get("name").and_then(|v| v.as_str()).unwrap_or(key);
            let version = app
                .pointer("/latest_release/version")
                .and_then(|v| v.as_str())
                .unwrap_or("no release fetched");
            let tag = app
                .pointer("/latest_release/tag")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let published = app
                .pointer("/latest_release/published_at")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown date");
            let assets = app
                .pointer("/latest_release/assets")
                .and_then(|v| v.as_array())
                .unwrap_or(&empty)
                .len();
            lines.push(format!(
                "  - {} ({}): {} {} — published {}, {} assets",
                name, key, version, tag, published, assets
            ));
        }
    }
    lines.sort();
    let fetched = response
        .pointer("/release_info/last_fetched")
        .and_then(|v| v.as_str())
        .unwrap_or("not fully fetched yet");
    let text = if lines.is_empty() {
        format!(
            "No Craft app release entries are currently available. Cache last fully fetched: {}",
            fetched
        )
    } else {
        format!(
            "Craft app release info (cache last fully fetched: {}):\n{}",
            fetched,
            lines.join("\n")
        )
    };
    Ok(vec![ToolContent {
        content_type: "text".into(),
        text,
    }])
}
