use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::client::ArtCraftClient;
use crate::tools::http::{raw_delete, raw_json_get, raw_json_post, raw_json_put};
use crate::types::{Tool, ToolContent};

const WEB_SESSION_NOTE: &str = " Upstream folder endpoints currently require web-session cookie credentials; API-key/MCP bearer credentials are not accepted by these routes.";

fn pagination_schema() -> Value {
    json!({
        "cursor": {"type":"string", "description":"Opaque cursor returned by a previous page"},
        "limit": {"type":"integer", "minimum":1, "maximum":1000, "description":"Page size (upstream default 100, maximum 1000)"}
    })
}

fn obj(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required})
}

pub fn tools() -> Vec<Tool> {
    vec![
        Tool { name: "artcraft_list_folders".into(), description: format!("List all live folders you own, newest first.{}", WEB_SESSION_NOTE), input_schema: Some(obj(pagination_schema(), &[])) },
        Tool { name: "artcraft_create_folder".into(), description: format!("Create a folder, optionally under a parent folder.{}", WEB_SESSION_NOTE), input_schema: Some(obj(json!({
            "name":{"type":"string","maxLength":255},
            "parent_folder_token":{"type":"string"},
            "color_code":{"type":"string","maxLength":16}
        }), &["name"])) },
        Tool { name: "artcraft_get_folder".into(), description: format!("Get one owned folder by token.{}", WEB_SESSION_NOTE), input_schema: Some(obj(json!({"folder_token":{"type":"string"}}), &["folder_token"])) },
        Tool { name: "artcraft_delete_folder".into(), description: format!("Soft-delete an owned folder.{}", WEB_SESSION_NOTE), input_schema: Some(obj(json!({"folder_token":{"type":"string"}}), &["folder_token"])) },
        Tool { name: "artcraft_rename_folder".into(), description: format!("Rename an owned folder.{}", WEB_SESSION_NOTE), input_schema: Some(obj(json!({"folder_token":{"type":"string"},"new_name":{"type":"string","maxLength":255}}), &["folder_token","new_name"])) },
        Tool { name: "artcraft_set_folder_star".into(), description: format!("Set or clear the star flag on an owned folder.{}", WEB_SESSION_NOTE), input_schema: Some(obj(json!({"folder_token":{"type":"string"},"has_star":{"type":"boolean"}}), &["folder_token","has_star"])) },
        Tool { name: "artcraft_set_folder_color".into(), description: format!("Set or clear an owned folder's color code. Pass no color_code (or an empty value) to clear it.{}", WEB_SESSION_NOTE), input_schema: Some(obj(json!({"folder_token":{"type":"string"},"color_code":{"type":"string","maxLength":16}}), &["folder_token"])) },
        Tool { name: "artcraft_set_folder_cover_image".into(), description: format!("Set or clear an owned folder's custom cover image. The media file must be owned by you; image/video files are used directly and other media may resolve through their own cover image.{}", WEB_SESSION_NOTE), input_schema: Some(obj(json!({"folder_token":{"type":"string"},"media_file_token":{"type":"string"}}), &["folder_token"])) },
        Tool { name: "artcraft_list_subfolders".into(), description: format!("List subfolders of an owned parent folder.{}", WEB_SESSION_NOTE), input_schema: Some(obj(json!({"folder_token":{"type":"string"},"cursor":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":1000}}), &["folder_token"])) },
        Tool { name: "artcraft_add_subfolders".into(), description: format!("Bulk-reparent up to 500 folders under a parent folder. Invalid, unowned, self-referential, and cycle-forming tokens are silently skipped upstream.{}", WEB_SESSION_NOTE), input_schema: Some(obj(json!({"folder_token":{"type":"string"},"subfolder_tokens":{"type":"array","items":{"type":"string"},"maxItems":500}}), &["folder_token","subfolder_tokens"])) },
        Tool { name: "artcraft_remove_subfolders".into(), description: format!("Bulk-unparent up to 500 subfolders from a parent folder. Rows not currently parented there are silently skipped upstream.{}", WEB_SESSION_NOTE), input_schema: Some(obj(json!({"folder_token":{"type":"string"},"subfolder_tokens":{"type":"array","items":{"type":"string"},"maxItems":500}}), &["folder_token","subfolder_tokens"])) },
        Tool { name: "artcraft_list_folder_media_files".into(), description: format!("List media files in an owned folder, with visibility/cover filtering applied upstream.{}", WEB_SESSION_NOTE), input_schema: Some(obj(json!({"folder_token":{"type":"string"},"cursor":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":1000}}), &["folder_token"])) },
        Tool { name: "artcraft_list_media_files_without_folder".into(), description: format!("List your media files that are not in any live folder.{}", WEB_SESSION_NOTE), input_schema: Some(obj(json!({"cursor":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":1000},"filter_media_class":{"type":"string","enum":["image","video","mesh","splat"]}}), &[])) },
        Tool { name: "artcraft_add_media_to_folder".into(), description: format!("Bulk-add up to 500 media files to a folder. Missing/deleted tokens are silently skipped upstream; existing memberships are idempotent.{}", WEB_SESSION_NOTE), input_schema: Some(obj(json!({"folder_token":{"type":"string"},"media_file_tokens":{"type":"array","items":{"type":"string"},"maxItems":500}}), &["folder_token","media_file_tokens"])) },
        Tool { name: "artcraft_remove_media_from_folder".into(), description: format!("Bulk-remove up to 500 media files from a folder.{}", WEB_SESSION_NOTE), input_schema: Some(obj(json!({"folder_token":{"type":"string"},"media_file_tokens":{"type":"array","items":{"type":"string"},"maxItems":500}}), &["folder_token","media_file_tokens"])) },
        Tool { name: "artcraft_move_media_to_folder".into(), description: format!("Atomically move up to 500 media files from a source folder into the destination folder token.{}", WEB_SESSION_NOTE), input_schema: Some(obj(json!({"folder_token":{"type":"string","description":"Destination folder token"},"source_folder_token":{"type":"string"},"media_file_tokens":{"type":"array","items":{"type":"string"},"maxItems":500}}), &["folder_token","source_folder_token","media_file_tokens"])) },
    ]
}

fn text_content(text: String) -> Vec<ToolContent> {
    vec![ToolContent {
        content_type: "text".to_string(),
        text,
    }]
}

fn required_str<'a>(arguments: &'a Value, key: &str) -> Result<&'a str> {
    arguments[key]
        .as_str()
        .ok_or_else(|| anyhow!("{} is required", key))
}

fn required_strings(arguments: &Value, key: &str) -> Result<Vec<String>> {
    arguments[key]
        .as_array()
        .ok_or_else(|| anyhow!("{} array is required", key))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .ok_or_else(|| anyhow!("{} entries must be strings", key))
        })
        .collect()
}

fn append_pagination(url: &mut String, arguments: &Value) {
    let mut parts = Vec::new();
    if let Some(cursor) = arguments["cursor"].as_str().filter(|s| !s.is_empty()) {
        parts.push(format!("cursor={}", urlencoding::encode(cursor)));
    }
    if let Some(limit) = arguments["limit"].as_u64() {
        parts.push(format!("limit={}", limit));
    }
    if !parts.is_empty() {
        url.push('?');
        url.push_str(&parts.join("&"));
    }
}

fn folder_line(item: &Value) -> String {
    let token = item.get("token").and_then(|v| v.as_str()).unwrap_or("?");
    let name = item
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("(unnamed)");
    let star = if item
        .get("has_star")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        " starred"
    } else {
        ""
    };
    let color = item
        .get("maybe_color_code")
        .and_then(|v| v.as_str())
        .map(|c| format!(" color:{}", c))
        .unwrap_or_default();
    let parent = item
        .get("maybe_parent_folder_token")
        .and_then(|v| v.as_str())
        .map(|p| format!(" parent:{}", p))
        .unwrap_or_default();
    format!("  - {} | {}{}{}{}", token, name, star, color, parent)
}

fn media_line(item: &Value) -> String {
    let token = item.get("token").and_then(|v| v.as_str()).unwrap_or("?");
    let title = item
        .get("maybe_title")
        .and_then(|v| v.as_str())
        .unwrap_or("(untitled)");
    let class = item
        .get("media_class")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let media_type = item
        .get("media_type")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    format!("  - {} | {} | {} / {}", token, title, class, media_type)
}

fn list_response_text(
    response: &Value,
    array_key: &str,
    noun: &str,
    line_fn: fn(&Value) -> String,
) -> String {
    let rows = response
        .get(array_key)
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().map(line_fn).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut text = if rows.is_empty() {
        format!("No {} found.", noun)
    } else {
        format!("Found {} {}:\n{}", rows.len(), noun, rows.join("\n"))
    };
    if let Some(cursor) = response.get("maybe_cursor").and_then(|v| v.as_str()) {
        text.push_str(&format!("\nNext cursor: {}", cursor));
    }
    text
}

pub async fn list_folders(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let mut url = client.absolute_url("/v1/folders/list_all");
    append_pagination(&mut url, &arguments);
    let response = raw_json_get(&url, client).await?;
    Ok(text_content(list_response_text(
        &response,
        "folders",
        "folders",
        folder_line,
    )))
}

pub async fn create_folder(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let name = required_str(&arguments, "name")?.trim();
    if name.is_empty() {
        return Err(anyhow!("name must not be empty"));
    }
    if name.len() > 255 {
        return Err(anyhow!("name too long (max 255 chars)"));
    }
    let body = json!({
        "name": name,
        "maybe_parent_folder_token": arguments["parent_folder_token"].as_str(),
        "maybe_color_code": arguments["color_code"].as_str(),
    });
    let response = raw_json_post(&client.absolute_url("/v1/folders/create"), body, client).await?;
    let folder = response.get("folder").cloned().unwrap_or_else(|| json!({}));
    Ok(text_content(format!(
        "Folder created successfully.\n{}",
        folder_line(&folder)
    )))
}

pub async fn get_folder(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let token = required_str(&arguments, "folder_token")?;
    let url = client.absolute_url(&format!(
        "/v1/folders/folder/{}",
        urlencoding::encode(token)
    ));
    let response = raw_json_get(&url, client).await?;
    let folder = response.get("folder").cloned().unwrap_or_else(|| json!({}));
    Ok(text_content(format!("Folder:\n{}", folder_line(&folder))))
}

pub async fn delete_folder(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let token = required_str(&arguments, "folder_token")?;
    let url = client.absolute_url(&format!(
        "/v1/folders/folder/{}",
        urlencoding::encode(token)
    ));
    raw_delete(&url, client).await?;
    Ok(text_content(format!(
        "Folder {} deleted successfully.",
        token
    )))
}

pub async fn rename_folder(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let token = required_str(&arguments, "folder_token")?;
    let new_name = required_str(&arguments, "new_name")?.trim();
    if new_name.is_empty() {
        return Err(anyhow!("new_name must not be empty"));
    }
    if new_name.len() > 255 {
        return Err(anyhow!("new_name too long (max 255 chars)"));
    }
    let url = client.absolute_url(&format!(
        "/v1/folders/folder/{}/rename",
        urlencoding::encode(token)
    ));
    raw_json_put(&url, json!({"new_name": new_name}), client).await?;
    Ok(text_content(format!(
        "Folder {} renamed to '{}'.",
        token, new_name
    )))
}

pub async fn set_folder_star(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let token = required_str(&arguments, "folder_token")?;
    let has_star = arguments["has_star"]
        .as_bool()
        .ok_or_else(|| anyhow!("has_star boolean is required"))?;
    let url = client.absolute_url(&format!(
        "/v1/folders/folder/{}/star",
        urlencoding::encode(token)
    ));
    raw_json_put(&url, json!({"has_star": has_star}), client).await?;
    Ok(text_content(format!(
        "Folder {} star set to {}.",
        token, has_star
    )))
}

pub async fn set_folder_color(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let token = required_str(&arguments, "folder_token")?;
    let color = arguments["color_code"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(code) = color {
        if code.len() > 16 {
            return Err(anyhow!("color_code too long (max 16 chars)"));
        }
    }
    let url = client.absolute_url(&format!(
        "/v1/folders/folder/{}/color_code",
        urlencoding::encode(token)
    ));
    raw_json_put(&url, json!({"maybe_color_code": color}), client).await?;
    Ok(text_content(match color {
        Some(code) => format!("Folder {} color set to {}.", token, code),
        None => format!("Folder {} color cleared.", token),
    }))
}

pub async fn set_folder_cover_image(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let token = required_str(&arguments, "folder_token")?;
    let media_token = arguments["media_file_token"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let url = client.absolute_url(&format!(
        "/v1/folders/folder/{}/cover_image",
        urlencoding::encode(token)
    ));
    let response =
        raw_json_put(&url, json!({"maybe_media_file_token": media_token}), client).await?;
    let resolved = response
        .get("maybe_resolved_cover_media_file_token")
        .and_then(|v| v.as_str());
    Ok(text_content(match resolved {
        Some(resolved) => format!(
            "Folder {} cover image set; resolved cover media token: {}",
            token, resolved
        ),
        None => format!("Folder {} cover image cleared.", token),
    }))
}

pub async fn list_subfolders(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let token = required_str(&arguments, "folder_token")?;
    let mut url = client.absolute_url(&format!(
        "/v1/folders/subfolders/{}",
        urlencoding::encode(token)
    ));
    append_pagination(&mut url, &arguments);
    let response = raw_json_get(&url, client).await?;
    Ok(text_content(list_response_text(
        &response,
        "subfolders",
        "subfolders",
        folder_line,
    )))
}

pub async fn add_subfolders(arguments: Value, client: &ArtCraftClient) -> Result<Vec<ToolContent>> {
    let token = required_str(&arguments, "folder_token")?;
    let subfolders = required_strings(&arguments, "subfolder_tokens")?;
    if subfolders.len() > 500 {
        return Err(anyhow!("too many subfolders in one request (max 500)"));
    }
    let url = client.absolute_url(&format!(
        "/v1/folders/subfolders/{}/bulk_add",
        urlencoding::encode(token)
    ));
    let response = raw_json_post(&url, json!({"subfolder_tokens": subfolders}), client).await?;
    let accepted = response
        .get("accepted_subfolder_tokens")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    Ok(text_content(format!(
        "Subfolder bulk-add complete. Accepted {} folder tokens.",
        accepted
    )))
}

pub async fn remove_subfolders(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let token = required_str(&arguments, "folder_token")?;
    let subfolders = required_strings(&arguments, "subfolder_tokens")?;
    if subfolders.len() > 500 {
        return Err(anyhow!("too many subfolders in one request (max 500)"));
    }
    let url = client.absolute_url(&format!(
        "/v1/folders/subfolders/{}/bulk_remove",
        urlencoding::encode(token)
    ));
    let response = raw_json_post(&url, json!({"subfolder_tokens": subfolders}), client).await?;
    let removed = response
        .get("removed_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    Ok(text_content(format!(
        "Subfolder bulk-remove complete. Unparented {} rows.",
        removed
    )))
}

pub async fn list_folder_media_files(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let token = required_str(&arguments, "folder_token")?;
    let mut url = client.absolute_url(&format!(
        "/v1/folders/media_files/{}",
        urlencoding::encode(token)
    ));
    append_pagination(&mut url, &arguments);
    let response = raw_json_get(&url, client).await?;
    Ok(text_content(list_response_text(
        &response,
        "media_files",
        "media files",
        media_line,
    )))
}

pub async fn list_media_files_without_folder(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let mut url = client.absolute_url("/v1/folders/media_files_without_folder");
    let mut parts = Vec::new();
    if let Some(cursor) = arguments["cursor"].as_str().filter(|s| !s.is_empty()) {
        parts.push(format!("cursor={}", urlencoding::encode(cursor)));
    }
    if let Some(limit) = arguments["limit"].as_u64() {
        parts.push(format!("limit={}", limit));
    }
    if let Some(class) = arguments["filter_media_class"]
        .as_str()
        .filter(|s| !s.is_empty())
    {
        parts.push(format!("filter_media_class={}", urlencoding::encode(class)));
    }
    if !parts.is_empty() {
        url.push('?');
        url.push_str(&parts.join("&"));
    }
    let response = raw_json_get(&url, client).await?;
    Ok(text_content(list_response_text(
        &response,
        "media_files",
        "unfoldered media files",
        media_line,
    )))
}

pub async fn add_media_to_folder(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let token = required_str(&arguments, "folder_token")?;
    let media_tokens = required_strings(&arguments, "media_file_tokens")?;
    if media_tokens.len() > 500 {
        return Err(anyhow!("too many media files in one request (max 500)"));
    }
    let url = client.absolute_url(&format!(
        "/v1/folders/media_files/{}/bulk_add",
        urlencoding::encode(token)
    ));
    let response = raw_json_post(&url, json!({"media_file_tokens": media_tokens}), client).await?;
    let accepted = response
        .get("accepted_media_file_tokens")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    Ok(text_content(format!(
        "Folder media bulk-add complete. Accepted {} media file tokens.",
        accepted
    )))
}

pub async fn remove_media_from_folder(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let token = required_str(&arguments, "folder_token")?;
    let media_tokens = required_strings(&arguments, "media_file_tokens")?;
    if media_tokens.len() > 500 {
        return Err(anyhow!("too many media files in one request (max 500)"));
    }
    let url = client.absolute_url(&format!(
        "/v1/folders/media_files/{}/bulk_remove",
        urlencoding::encode(token)
    ));
    let response = raw_json_post(&url, json!({"media_file_tokens": media_tokens}), client).await?;
    let removed = response
        .get("removed_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    Ok(text_content(format!(
        "Folder media bulk-remove complete. Removed {} membership rows.",
        removed
    )))
}

pub async fn move_media_to_folder(
    arguments: Value,
    client: &ArtCraftClient,
) -> Result<Vec<ToolContent>> {
    let destination = required_str(&arguments, "folder_token")?;
    let source = required_str(&arguments, "source_folder_token")?;
    let media_tokens = required_strings(&arguments, "media_file_tokens")?;
    if media_tokens.len() > 500 {
        return Err(anyhow!("too many media files in one request (max 500)"));
    }
    let url = client.absolute_url(&format!(
        "/v1/folders/media_files/{}/bulk_move",
        urlencoding::encode(destination)
    ));
    let response = raw_json_post(
        &url,
        json!({"source_folder": source, "media_file_tokens": media_tokens}),
        client,
    )
    .await?;
    let removed = response
        .get("removed_from_source_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let added = response
        .get("added_to_destination_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    Ok(text_content(format!("Folder media move complete. Removed {} rows from source; inserted {} rows into destination.", removed, added)))
}
