//! Experimental io.modelcontextprotocol/files extension (Phase 4).
//!
//! Uploads: ArtCraft's backend has no presigned upload URLs (uploads are
//! cookie-authenticated multipart POSTs), so out-of-band bytes arrive through
//! a loopback relay the server hosts itself: the client PUTs to
//! `/v1/mcp-uploads/<token>` (path target in HTTP mode, absolute loopback URL
//! in stdio mode), the relay spools bytes to a temp file, and the retried
//! tool call re-verifies size/type before running the multipart upload.
//!
//! Downloads: ArtCraft media URLs are plain anonymous GETs, so download
//! targets point directly at them and are carried in result `_meta` instead
//! of model-visible text.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::put;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::RwLock;

pub const EXT_KEY: &str = "io.modelcontextprotocol/files";
pub const MAX_INLINE_BYTES: u64 = 524_288;
/// 512 MiB hard ceiling for spooled relay uploads.
pub const MAX_UPLOAD_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub struct FileInputMeta {
    pub id: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub size: u64,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    #[allow(dead_code)] // carried for spec completeness; verified post-upload only when present
    pub digest: Option<String>,
    /// Inline base64 data; `null` is explicitly invalid per the draft.
    #[serde(default)]
    pub data: Option<String>,
}

/// Scan tool arguments for FileInput-shaped objects (id + mimeType + size).
/// Returns (argument key, parsed metadata, has_inline_data).
pub fn detect_file_inputs(args: &Value) -> Result<Vec<(String, FileInputMeta)>> {
    let mut found = Vec::new();
    let Some(map) = args.as_object() else { return Ok(found) };
    for (key, value) in map {
        for candidate in std::iter::once(value).chain(value.as_array().into_iter().flatten()) {
            let Some(obj) = candidate.as_object() else { continue };
            if !(obj.contains_key("id") && obj.contains_key("mimeType") && obj.contains_key("size")) {
                continue;
            }
            // `data: null` is explicitly invalid.
            if obj.get("data").is_some_and(|d| d.is_null()) {
                return Err(anyhow!("file input '{}': data:null is invalid", key));
            }
            let meta: FileInputMeta = serde_json::from_value(candidate.clone())
                .map_err(|e| anyhow!("file input '{}': malformed metadata: {}", key, e))?;
            found.push((key.clone(), meta));
        }
    }
    Ok(found)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingUpload {
    pub file_id: String,
    pub arg_key: String,
    pub token: String,
    pub mime_type: String,
    pub declared_size: u64,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadState {
    pub tool: String,
    pub args: Value,
    pub uploads: Vec<PendingUpload>,
    pub expires_unix: i64,
}

/// In-memory index of spooled upload files. Bytes live in the OS temp dir;
/// entries are one-shot (removed when the retried call consumes them).
#[derive(Default)]
pub struct UploadSpool {
    inner: RwLock<BTreeMap<String, PathBuf>>,
}

impl UploadSpool {
    pub async fn register(&self, token: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("artcraft-mcp-upload-{}", token));
        self.inner.write().await.insert(token.to_string(), path.clone());
        path
    }

    pub async fn lookup(&self, token: &str) -> Option<PathBuf> {
        self.inner.read().await.get(token).cloned()
    }

    pub async fn take(&self, token: &str) -> Option<PathBuf> {
        self.inner.write().await.remove(token)
    }
}

pub type SharedSpool = Arc<UploadSpool>;

/// Axum route mounted at /v1/mcp-uploads/{token} in HTTP mode and on the
/// ephemeral stdio relay listener.
pub fn relay_router(spool: SharedSpool) -> axum::Router {
    axum::Router::new()
        .route("/v1/mcp-uploads/{token}", put(relay_put).post(relay_put))
        .with_state(spool)
}

async fn relay_put(
    State(spool): State<SharedSpool>,
    Path(token): Path<String>,
    body: Bytes,
) -> impl IntoResponse {
    // Unguessable tokens only; hex/uuid shape check keeps path traversal out.
    if token.len() < 16
        || !token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    }
    if body.len() as u64 > MAX_UPLOAD_BYTES {
        return axum::http::StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    let Some(path) = spool.lookup(&token).await else {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    };
    match tokio::fs::write(&path, &body).await {
        Ok(()) => axum::http::StatusCode::OK.into_response(),
        Err(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}


/// Compose one upload target for a pending upload.
/// `path_mode` uses a relative path under the MCP address (Streamable HTTP);
/// otherwise an absolute loopback URL for the ephemeral stdio relay.
pub fn upload_target(upload: &PendingUpload, base_url: Option<&str>) -> Value {
    let mut target = json!({
        "fileId": upload.file_id,
        "method": "PUT",
        "credentials": "omit",
        "expiresAt": chrono::Utc::now()
            .checked_add_signed(chrono::Duration::minutes(15))
            .map(|t| t.to_rfc3339())
            .unwrap_or_default(),
        "headers": { "Content-Type": upload.mime_type },
    });
    match base_url {
        Some(base) => {
            target["url"] = json!(format!("{}/v1/mcp-uploads/{}", base, upload.token));
        }
        None => {
            target["path"] = json!(format!("/v1/mcp-uploads/{}", upload.token));
        }
    }
    target
}

/// Build the `_meta["io.modelcontextprotocol/files"]` download attachment for
/// a tool result. Keys are stable per target so clients can correlate.
pub fn download_meta(targets: Vec<(String, String, Option<String>)>) -> Value {
    let entries: Vec<Value> = targets
        .into_iter()
        .map(|(id, url, mime)| {
            let mut t = json!({ "fileId": id, "url": url });
            if let Some(m) = mime {
                t["headers"] = json!({ "Accept": m });
            }
            t
        })
        .collect();
    json!({ EXT_KEY: { "downloads": entries } })
}

/// Extract the first http(s) URL from text content — generation results and
/// media downloads embed the ArtCraft media URL in their text.
pub fn extract_first_url(text: &str) -> Option<String> {
    for word in text.split(|c: char| c.is_whitespace() || c == '`' || c == '"') {
        let w = word.trim_matches(|c| c == '.' || c == ',' || c == ')' || c == '(');
        if w.starts_with("http://") || w.starts_with("https://") {
            return Some(w.to_string());
        }
    }
    None
}
