use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::client::ArtCraftClient;
use crate::tools::http::{raw_json_get, raw_json_post};
use crate::types::{Tool, ToolContent};

fn schema(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required})
}

pub fn tools() -> Vec<Tool> {
    vec![
        Tool { name: "artcraft_list_featured_media".into(), description: "List public featured media files. Supports upstream pagination and class/type/engine/product filters.".into(), input_schema: Some(schema(json!({
            "sort_ascending":{"type":"boolean"},"page_size":{"type":"integer","minimum":1},"cursor":{"type":"string"},"cursor_is_reversed":{"type":"boolean"},
            "filter_media_classes":{"type":"string","description":"Comma-separated media classes, e.g. image,video"},
            "filter_media_type":{"type":"string","description":"Comma-separated media types, e.g. jpg,png,mp4"},
            "filter_engine_categories":{"type":"string","description":"Comma-separated engine categories"},
            "filter_products":{"type":"string","description":"Comma-separated product categories"}
        }), &[])) },
        Tool { name: "artcraft_search_featured_media".into(), description: "Search public featured media files by text.".into(), input_schema: Some(schema(json!({
            "query":{"type":"string"},
            "filter_media_classes":{"type":"string"},
            "filter_media_type":{"type":"string"},
            "filter_engine_categories":{"type":"string"}
        }), &["query"])) },
        Tool { name: "artcraft_is_featured_item".into(), description: "Check whether a user, media file, or model weight is featured.".into(), input_schema: Some(schema(json!({
            "entity_type":{"type":"string","enum":["user","media_file","model_weight"]},
            "entity_token":{"type":"string"}
        }), &["entity_type","entity_token"])) },
        Tool { name: "artcraft_list_beta_keys".into(), description: "List beta keys visible to your account. Non-moderators are automatically scoped to keys they refer. Requires web-session cookie auth.".into(), input_schema: Some(schema(json!({
            "sort_ascending":{"type":"boolean"},"page_size":{"type":"integer","minimum":1},"page_index":{"type":"integer","minimum":0},
            "referrer_username":{"type":"string","description":"Moderator-only username scope"},
            "filter":{"type":"string","enum":["all","redeemed","unredeemed"]}
        }), &[])) },
        Tool { name: "artcraft_redeem_beta_key".into(), description: "Redeem a beta key on the current account. Requires web-session cookie auth; redemption is a one-way account mutation.".into(), input_schema: Some(schema(json!({"beta_key":{"type":"string"}}), &["beta_key"])) },
    ]
}

fn content(text: String) -> Vec<ToolContent> {
    vec![ToolContent {
        content_type: "text".into(),
        text,
    }]
}

fn media_line(item: &Value) -> String {
    format!(
        "  - {} | {} | {} / {}",
        item.get("token").and_then(|v| v.as_str()).unwrap_or("?"),
        item.get("maybe_title")
            .and_then(|v| v.as_str())
            .unwrap_or("(untitled)"),
        item.get("media_class")
            .and_then(|v| v.as_str())
            .unwrap_or("?"),
        item.get("media_type")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
    )
}

fn append_query(url: &mut String, parts: Vec<(&str, String)>) {
    let parts: Vec<String> = parts
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{}={}", k, urlencoding::encode(&v)))
        .collect();
    if !parts.is_empty() {
        url.push('?');
        url.push_str(&parts.join("&"));
    }
}

fn opt_bool(arguments: &Value, key: &str) -> String {
    arguments[key]
        .as_bool()
        .map(|v| v.to_string())
        .unwrap_or_default()
}

fn opt_num(arguments: &Value, key: &str) -> String {
    arguments[key]
        .as_u64()
        .map(|v| v.to_string())
        .unwrap_or_default()
}

fn opt_str(arguments: &Value, key: &str) -> String {
    arguments[key].as_str().unwrap_or("").to_string()
}

pub async fn list_featured_media(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let mut url = client.absolute_url("/v1/media_files/list_featured");
    append_query(
        &mut url,
        vec![
            ("sort_ascending", opt_bool(&arguments, "sort_ascending")),
            ("page_size", opt_num(&arguments, "page_size")),
            ("cursor", opt_str(&arguments, "cursor")),
            (
                "cursor_is_reversed",
                opt_bool(&arguments, "cursor_is_reversed"),
            ),
            (
                "filter_media_classes",
                opt_str(&arguments, "filter_media_classes"),
            ),
            (
                "filter_media_type",
                opt_str(&arguments, "filter_media_type"),
            ),
            (
                "filter_engine_categories",
                opt_str(&arguments, "filter_engine_categories"),
            ),
            ("filter_products", opt_str(&arguments, "filter_products")),
        ],
    );
    let response = raw_json_get(&url, client).await?;
    let rows = response
        .get("results")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().map(media_line).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut text = if rows.is_empty() {
        "No featured media found.".into()
    } else {
        format!(
            "Found {} featured media files:\n{}",
            rows.len(),
            rows.join("\n")
        )
    };
    if let Some(next) = response
        .pointer("/pagination/maybe_next")
        .and_then(|v| v.as_str())
    {
        text.push_str(&format!("\nNext cursor: {}", next));
    }
    Ok(content(text))
}

pub async fn search_featured_media(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let query = arguments["query"]
        .as_str()
        .ok_or_else(|| anyhow!("query is required"))?;
    let mut url = client.absolute_url("/v1/media_files/search_featured");
    append_query(
        &mut url,
        vec![
            ("search_term", query.to_string()),
            (
                "filter_media_classes",
                opt_str(&arguments, "filter_media_classes"),
            ),
            (
                "filter_media_type",
                opt_str(&arguments, "filter_media_type"),
            ),
            (
                "filter_engine_categories",
                opt_str(&arguments, "filter_engine_categories"),
            ),
        ],
    );
    let response = raw_json_get(&url, client).await?;
    let rows = response
        .get("results")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().map(media_line).collect::<Vec<_>>())
        .unwrap_or_default();
    Ok(content(if rows.is_empty() {
        "No featured media matched.".into()
    } else {
        format!(
            "Found {} featured media files:\n{}",
            rows.len(),
            rows.join("\n")
        )
    }))
}

pub async fn is_featured_item(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let entity_type = arguments["entity_type"]
        .as_str()
        .ok_or_else(|| anyhow!("entity_type is required"))?;
    let entity_token = arguments["entity_token"]
        .as_str()
        .ok_or_else(|| anyhow!("entity_token is required"))?;
    let url = client.absolute_url(&format!(
        "/v1/featured_item/is_featured/{}/{}",
        urlencoding::encode(entity_type),
        urlencoding::encode(entity_token)
    ));
    let response = raw_json_get(&url, client).await?;
    let featured = response
        .get("is_featured")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    Ok(content(format!(
        "{} {} featured: {}",
        entity_type, entity_token, featured
    )))
}

fn beta_line(item: &Value) -> String {
    format!(
        "  - {} | {} | key:{} | distributed:{} | redeemed:{}",
        item.get("token").and_then(|v| v.as_str()).unwrap_or("?"),
        item.get("product").and_then(|v| v.as_str()).unwrap_or("?"),
        item.get("key_value")
            .and_then(|v| v.as_str())
            .unwrap_or("?"),
        item.get("is_distributed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        item.get("maybe_redeemed_at")
            .and_then(|v| v.as_str())
            .unwrap_or("no")
    )
}

pub async fn list_beta_keys(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let mut url = client.absolute_url("/v1/beta_keys/list");
    append_query(
        &mut url,
        vec![
            ("sort_ascending", opt_bool(&arguments, "sort_ascending")),
            ("page_size", opt_num(&arguments, "page_size")),
            ("page_index", opt_num(&arguments, "page_index")),
            (
                "maybe_referrer_username",
                opt_str(&arguments, "referrer_username"),
            ),
            ("filter", opt_str(&arguments, "filter")),
        ],
    );
    let response = raw_json_get(&url, client).await?;
    let rows = response
        .get("beta_keys")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().map(beta_line).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut text = if rows.is_empty() {
        "No beta keys found.".into()
    } else {
        format!("Found {} beta keys:\n{}", rows.len(), rows.join("\n"))
    };
    if let Some(total) = response
        .pointer("/pagination/total_page_count")
        .and_then(|v| v.as_u64())
    {
        text.push_str(&format!("\nTotal pages: {}", total));
    }
    Ok(content(text))
}

pub async fn redeem_beta_key(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let key = arguments["beta_key"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("beta_key is required"))?;
    raw_json_post(
        &client.absolute_url("/v1/beta_keys/redeem"),
        json!({"beta_key": key}),
        client,
    )
    .await?;
    Ok(content("Beta key redeemed successfully.".into()))
}
