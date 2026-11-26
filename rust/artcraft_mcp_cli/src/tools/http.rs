use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::Value;

use crate::client::ArtCraftClient;

fn http_client() -> reqwest::Client {
    reqwest::Client::new()
}

async fn json_response(response: reqwest::Response) -> Result<Value> {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        let truncated: String = body.chars().take(500).collect();
        return Err(anyhow!("HTTP error: {} {}", status, truncated.trim()));
    }
    if body.trim().is_empty() {
        return Ok(Value::Null);
    }
    Ok(serde_json::from_str(&body)?)
}

pub async fn raw_json_get(url: &str, client: &ArtCraftClient) -> Result<Value> {
    let request = http_client().get(url).header("Accept", "application/json");
    let response = client.apply_auth(request).send().await?;
    json_response(response).await
}

pub async fn raw_json_post(url: &str, body: Value, client: &ArtCraftClient) -> Result<Value> {
    let request = http_client()
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body);
    let response = client.apply_auth(request).send().await?;
    json_response(response).await
}

pub async fn raw_json_delete(url: &str, body: Value, client: &ArtCraftClient) -> Result<Value> {
    let request = http_client()
        .delete(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body);
    let response = client.apply_auth(request).send().await?;
    json_response(response).await
}

pub async fn raw_json_put(url: &str, body: Value, client: &ArtCraftClient) -> Result<Value> {
    let request = http_client()
        .put(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body);
    let response = client.apply_auth(request).send().await?;
    json_response(response).await
}

pub async fn raw_delete(url: &str, client: &ArtCraftClient) -> Result<Value> {
    let request = http_client()
        .delete(url)
        .header("Accept", "application/json");
    let response = client.apply_auth(request).send().await?;
    json_response(response).await
}

/// Typed JSON POST used for upstream `artcraft_api_defs` request types while
/// retaining control of the auth header (the upstream generated client helper
/// is still cookie-only).
pub async fn typed_json_post<Req: Serialize>(
    route_path: &str,
    body: Req,
    client: &ArtCraftClient,
) -> Result<Value> {
    let url = client.absolute_url(route_path);
    let request = http_client()
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body);
    let response = client.apply_auth(request).send().await?;
    json_response(response).await
}

pub async fn raw_multipart_post(
    url: &str,
    file_path: &str,
    text_fields: &[(&str, String)],
    client: &ArtCraftClient,
) -> Result<Value> {
    raw_multipart_post_named_file(url, file_path, "file", text_fields, client).await
}

pub async fn raw_multipart_post_named_file(
    url: &str,
    file_path: &str,
    file_field_name: &str,
    text_fields: &[(&str, String)],
    client: &ArtCraftClient,
) -> Result<Value> {
    let bytes = tokio::fs::read(file_path).await?;
    let filename = std::path::Path::new(file_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("upload.bin")
        .to_string();

    let mut form = reqwest::multipart::Form::new();
    for (name, value) in text_fields {
        form = form.text((*name).to_string(), value.clone());
    }
    form = form.part(
        file_field_name.to_string(),
        reqwest::multipart::Part::bytes(bytes).file_name(filename),
    );

    let request = http_client()
        .post(url)
        .header("Accept", "application/json")
        .multipart(form);
    let response = client.apply_auth(request).send().await?;
    json_response(response).await
}
