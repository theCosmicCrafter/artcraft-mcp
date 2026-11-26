//! MCP server core: an rmcp `ServerHandler` over the 61-tool registry, with
//! dual transport entry points (stdio and stateless Streamable HTTP).

use std::borrow::Cow;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::http::{HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use rmcp::{ErrorData as McpError, ServerHandler, ServiceExt};
use serde_json::{Map, Value};
use tracing::{info, warn};

use crate::client::ArtCraftClient;
use crate::types::ToolContent;
use crate::{Cli, ToolMode, auth, telemetry, tools};

/// Shared, cheaply cloneable server state. The HTTP service factory builds a
/// fresh handler per request, so anything persistent lives behind this Arc.
#[derive(Clone)]
pub struct ArtCraftMcp {
    pub shared: Arc<SharedState>,
}

pub struct SharedState {
    pub tool_mode: ToolMode,
    pub confirm_over_credits: u64,
    pub auto_approve_under: u64,
    pub no_confirm: bool,
    /// HMAC codec sealing MRTR/upload `requestState` continuations.
    pub state_codec: RequestStateCodec,
    /// Server-level credentials resolved once at startup (env/files/cookies).
    pub startup_credentials: Option<
        artcraft_client::credentials::storyteller_credential_set::StorytellerCredentialSet,
    >,
}

impl ArtCraftMcp {
    pub fn new(cli: &Cli) -> Self {
        let key = state_signing_key();
        let state_codec = RequestStateCodec::try_new(key)
            .expect("request-state codec key must be valid HMAC key material");
        Self {
            shared: Arc::new(SharedState {
                tool_mode: cli.tool_mode,
                confirm_over_credits: cli.confirm_over_credits,
                auto_approve_under: cli.auto_approve_under,
                no_confirm: cli.no_confirm,
                state_codec,
                startup_credentials: auth::load_credentials(),
            }),
        }
    }

    /// Build the API client for one request. Bearer-carried credential override
    /// arrives in a later phase; today every request uses the startup chain.
    fn client_for(&self, _context: &RequestContext<RoleServer>) -> ArtCraftClient {
        ArtCraftClient::new(self.shared.startup_credentials.clone())
    }
}

/// Load the request-state signing key: `ARTCRAFT_MCP_STATE_SECRET` when set
/// (required for multi-instance HTTP deployments), else an ephemeral random
/// key for this single-process run.
fn state_signing_key() -> Vec<u8> {
    if let Ok(secret) = std::env::var("ARTCRAFT_MCP_STATE_SECRET") {
        let trimmed = secret.trim();
        if !trimmed.is_empty() {
            return trimmed.as_bytes().to_vec();
        }
    }
    use rand::RngCore;
    let mut key = vec![0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);
    key
}

/// Extension capabilities this server advertises in server/discover.
/// Populated as each extension phase lands (tasks in Phase 3, files in
/// Phase 4, apps in Phase 5); advertising an unimplemented extension would
/// violate the extensions lifecycle.
pub fn extension_capabilities() -> ExtensionCapabilities {
    ExtensionCapabilities::new()
}

/// Read a W3C trace context out of request `_meta` into the current span.
fn trace_meta(meta: Option<&MetaObject>) {
    if let Some(meta) = meta {
        let value = serde_json::to_value(&meta.0).unwrap_or(Value::Null);
        telemetry::extract_trace_context(Some(&value));
    }
}

impl ServerHandler for ArtCraftMcp {
    fn get_info(&self) -> ServerConfig {
        let mut capabilities = ServerCapabilities::builder().enable_tools().build();
        let extensions = extension_capabilities();
        if !extensions.is_empty() {
            capabilities.extensions = Some(extensions);
        }
        let mut info = ServerConfig::new(capabilities);
        info.protocol_version = ProtocolVersion::V_2026_07_28;
        info.server_info = Implementation::new("artcraft-mcp", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "ArtCraft generative media: image, video, audio, 3D and splat generation. \
             Use artcraft_estimate_cost before expensive generations."
                .to_string(),
        );
        info
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        trace_meta(request.as_ref().and_then(|r| r.meta.as_ref().map(|m| &m.0)));
        let defs = tools::get_all_tools();
        let mut converted: Vec<Tool> = defs.into_iter().map(convert_tool).collect();
        // Canonical deterministic ordering keeps client KV caches stable.
        converted.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(ListToolsResult {
            tools: converted,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        trace_meta(request.meta.as_ref().map(|m| &m.0));

        let name = request.name.to_string();
        let arguments = Value::Object(request.arguments.clone().unwrap_or_default());
        let client = self.client_for(&context);

        info!("Tool call: {}", name);
        match tools::execute_tool(&name, arguments, &client).await {
            Ok(contents) => Ok(CallToolResult::success(
                contents.into_iter().map(content_block).collect(),
            )
            .into()),
            Err(e) => {
                warn!("Tool {} failed: {}", name, e);
                Ok(CallToolResult::error(vec![rmcp::model::ContentBlock::text(format!(
                    "Error: {}",
                    e
                ))])
                .into())
            }
        }
    }
}

fn convert_tool(def: crate::types::Tool) -> Tool {
    let schema_value = def.input_schema.unwrap_or_else(|| {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    });
    let schema_object: Map<String, Value> = match schema_value {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    Tool::new(
        Cow::<'static, str>::Owned(def.name),
        Cow::<'static, str>::Owned(def.description),
        std::sync::Arc::new(schema_object),
    )
}

fn content_block(content: ToolContent) -> rmcp::model::ContentBlock {
    rmcp::model::ContentBlock::text(content.text)
}

// ---------------------------------------------------------------------------
// Transports
// ---------------------------------------------------------------------------

pub async fn serve_stdio(cli: Cli) -> anyhow::Result<()> {
    let service = ArtCraftMcp::new(&cli);
    let server = service.serve(rmcp::transport::stdio()).await?;
    server.waiting().await?;
    Ok(())
}

pub async fn serve_http(cli: Cli) -> anyhow::Result<()> {
    let addr: SocketAddr = cli
        .bind
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid --bind address '{}': {}", cli.bind, e))?;

    if !addr.ip().is_loopback() && !cli.allow_remote {
        anyhow::bail!(
            "refusing to bind non-loopback address {} without --allow-remote",
            addr
        );
    }
    if !addr.ip().is_loopback() && std::env::var("ARTCRAFT_MCP_BEARER_TOKEN").is_err() {
        anyhow::bail!("remote binds require bearer auth: set ARTCRAFT_MCP_BEARER_TOKEN");
    }

    let handler = ArtCraftMcp::new(&cli);
    let ct = tokio_util::sync::CancellationToken::new();

    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(true)
        .with_json_response(true)
        .with_cancellation_token(ct.child_token());

    let factory_handler = handler.clone();
    let service = StreamableHttpService::new(
        move || Ok(factory_handler.clone()),
        LocalSessionManager::default().into(),
        config,
    );

    let router = axum::Router::new()
        .nest_service("/mcp", service)
        .route("/health", axum::routing::get(|| async { "OK" }))
        .layer(axum::middleware::from_fn(origin_and_auth_middleware));

    info!("artcraft-mcp listening on http://{}/mcp", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            ct.cancel();
        })
        .await?;
    Ok(())
}

/// Security middleware for the HTTP transport:
/// - Validate `Origin` (DNS-rebinding protection): absent (non-browser
///   clients), loopback hosts, or localhost are accepted; anything else is 403.
/// - When ARTCRAFT_MCP_BEARER_TOKEN is configured, require a matching bearer.
async fn origin_and_auth_middleware(
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let headers: &HeaderMap = request.headers();

    if let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok()) {
        let allowed = origin.is_empty()
            || origin.starts_with("http://localhost")
            || origin.starts_with("http://127.0.0.1")
            || origin.starts_with("https://localhost")
            || origin.starts_with("http://[::1]")
            || origin == "null";
        if !allowed {
            warn!(origin, "rejecting disallowed Origin");
            return Err(StatusCode::FORBIDDEN);
        }
    }

    if let Ok(expected) = std::env::var("ARTCRAFT_MCP_BEARER_TOKEN") {
        let presented = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        if !auth::bearer_access_allowed(Some(expected.trim()), presented) {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    Ok(next.run(request).await)
}
