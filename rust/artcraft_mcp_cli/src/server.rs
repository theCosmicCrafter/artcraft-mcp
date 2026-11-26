//! MCP server core: an rmcp `ServerHandler` over the 61-tool registry, with
//! dual transport entry points (stdio and stateless Streamable HTTP),
//! progressive tool discovery, MRTR cost confirmation, and the tasks
//! extension for long-running generation jobs.

use std::borrow::Cow;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::http::{HeaderMap, Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::task_manager::TaskManager;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use rmcp::{ErrorData as McpError, ServerHandler, ServiceExt};
use serde_json::{Map, Value, json};
use tracing::{info, warn};

use crate::client::{ArtCraftClient, ArtCraftCredentials};
use crate::registry::{self, CostTier, HOT_TOOLS, META_TOOLS};
use crate::types::ToolContent;
use crate::{Cli, ToolMode, auth, discovery, files_ext, telemetry, tools};

mod completions;
mod conformance_tools;
mod fixtures;
mod prompts;
mod resources_ext;

/// Static per-tier credit estimates used for the MRTR confirmation gate.
/// Only a fallback: the gate first asks the upstream OmniGen cost routes
/// for the exact price of the requested model+arguments and drops to these
/// magnitudes when no model-specific quote is available, so a pricing
/// failure can never lower the quoted figure.
fn tier_estimate(tier: CostTier) -> u64 {
    match tier {
        CostTier::Free => 0,
        CostTier::Low => 10,
        CostTier::Medium => 40,
        CostTier::High => 120,
    }
}

/// Credit figure shown in the MRTR confirmation prompt: either the exact
/// upstream quote for the requested call or the conservative tier fallback.
struct CostQuote {
    credits: u64,
    /// True when priced by the upstream cost route for this exact request;
    /// false for the approximate tier fallback.
    exact: bool,
    /// Upstream reports the call as free/unlimited for this account.
    free: bool,
}

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
    pub startup_credentials: Option<ArtCraftCredentials>,
    /// In-memory task manager backing io.modelcontextprotocol/tasks.
    /// Handles are unguessable UUIDs; durable cross-restart persistence is
    /// tracked in the SQLite task store (see tasks_store.rs).
    pub tasks: TaskManager,
    /// Spool for out-of-band file uploads (io.modelcontextprotocol/files).
    pub spool: files_ext::SharedSpool,
    /// Absolute loopback base URL of the ephemeral relay (stdio mode only).
    /// None in HTTP mode, where upload targets use relative paths.
    pub relay_base_url: std::sync::RwLock<Option<String>>,
    /// True when serving non-loopback HTTP clients (--allow-remote).
    pub remote_exposed: bool,
    /// Consumed MRTR confirmation keys: a retried approval for an already
    /// executed confirmation must not spend credits twice.
    pub consumed_confirmations: std::sync::Mutex<std::collections::HashSet<String>>,
    /// Legacy MCP logging level (2025-era clients). Stored as a severity name
    /// so this state remains independent of rmcp's deprecated type.
    pub log_level: std::sync::RwLock<String>,
    /// Legacy per-process resource subscription set. Static resources never
    /// emit updates, but subscribe/unsubscribe state is tracked accurately.
    pub resource_subscriptions: std::sync::Mutex<std::collections::HashSet<String>>,
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
                tasks: TaskManager::new(),
                spool: std::sync::Arc::new(files_ext::UploadSpool::default()),
                relay_base_url: std::sync::RwLock::new(None),
                remote_exposed: cli.allow_remote,
                consumed_confirmations: std::sync::Mutex::new(std::collections::HashSet::new()),
                log_level: std::sync::RwLock::new("info".to_string()),
                resource_subscriptions: std::sync::Mutex::new(std::collections::HashSet::new()),
            }),
        }
    }

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

/// Extension capabilities advertised in server/discover and initialize.
pub fn extension_capabilities() -> ExtensionCapabilities {
    let mut ext = ExtensionCapabilities::new();
    ext.insert(
        "io.modelcontextprotocol/tasks".to_string(),
        json!({ "supported": true }).as_object().cloned().unwrap_or_default(),
    );
    ext.insert(
        crate::apps::EXT_KEY.to_string(),
        json!({ "supported": true }).as_object().cloned().unwrap_or_default(),
    );
    ext.insert(
        files_ext::EXT_KEY.to_string(),
        json!({ "upload": {}, "maxInlineBytes": files_ext::MAX_INLINE_BYTES })
            .as_object().cloned().unwrap_or_default(),
    );
    ext
}

fn trace_meta(meta: Option<&MetaObject>) {
    if let Some(meta) = meta {
        let value = serde_json::to_value(&meta.0).unwrap_or(Value::Null);
        telemetry::extract_trace_context(Some(&value));
    }
}

fn confirm_kind() -> String {
    "confirm".to_string()
}

/// MRTR state carried in the sealed requestState for cost confirmation.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct ConfirmState {
    #[serde(default = "confirm_kind")]
    #[allow(dead_code)]
    kind: String,
    tool: String,
    args: Value,
    quote_credits: u64,
    idempotency_key: String,
    #[serde(default)]
    expires_unix: i64,
}

impl ServerHandler for ArtCraftMcp {
    fn get_info(&self) -> ServerConfig {
        #[allow(deprecated)]
        let mut capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_prompts()
            .enable_completions()
            .enable_logging()
            .build();
        if let Some(resources) = capabilities.resources.as_mut() {
            resources.subscribe = Some(true);
        }
        let extensions = extension_capabilities();
        if !extensions.is_empty() {
            capabilities.extensions = Some(extensions);
        }
        let mut info = ServerConfig::new(capabilities);
        info.protocol_version = ProtocolVersion::V_2026_07_28;
        info.server_info = Implementation::new("artcraft-mcp", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "ArtCraft generative media: image, video, audio, 3D and splat generation. \
             Use artcraft_estimate_cost before expensive generations. In progressive \
             mode, use artcraft_search_tools / artcraft_describe_tool / \
             artcraft_call_deferred_tool to reach the full catalog."
                .to_string(),
        );
        info
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        trace_meta(request.as_ref().and_then(|r| r.meta.as_ref().map(|m| &m.0)));

        let client_caps = context.client_capabilities();
        let supports_tasks = client_caps.is_some_and(|c| c.supports_tasks());
        let modern = context
            .protocol_version()
            .is_some_and(|v| v >= ProtocolVersion::V_2026_07_28);

        // Progressive mode only applies to 2026-07-28 clients unless forced.
        let progressive = matches!(self.shared.tool_mode, ToolMode::Progressive | ToolMode::Codemode) && modern;

        let mut converted: Vec<Tool> = if progressive {
            let mut out: Vec<Tool> = Vec::new();
            for def in tools::get_all_tools() {
                if !HOT_TOOLS.contains(&def.name.as_str()) {
                    continue;
                }
                // Hide redundant hot tools when the client has the extensions.
                if supports_tasks
                    && matches!(def.name.as_str(), "artcraft_get_job_status" | "artcraft_list_jobs")
                {
                    continue;
                }
                out.push(convert_tool(def));
            }
            for meta in discovery::meta_tool_defs() {
                out.push(convert_tool(meta));
            }
            out
        } else {
            tools::get_all_tools().into_iter().map(convert_tool).collect()
        };

        // Canonical deterministic ordering keeps client KV caches stable.
        converted.sort_by(|a, b| a.name.cmp(&b.name));
        let mut result = ListToolsResult {
            tools: converted,
            ..Default::default()
        };
        // SEP-2549 caching hints: private because the list adapts to the
        // client's negotiated capabilities.
        let mut meta = MetaObject::default();
        meta.insert("ttlMs".to_string(), json!(300_000));
        meta.insert("cacheScope".to_string(), json!("private"));
        result.meta = Some(meta);
        Ok(result)
    }

    async fn get_task(
        &self,
        request: GetTaskParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        let detailed = self.shared.tasks.get_task(&request.task_id)?;
        Ok(GetTaskResult::new(detailed))
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        // NOTE(artcraft-api): the upstream ArtCraft API exposes no cancel
        // endpoint for generation jobs (endpoints/jobs contains only
        // list_session_jobs), so this stops local tracking only. The upstream
        // generation may continue and consume credits. This is the documented
        // fallback per the no-silent-shortcuts rule; do not fake success.
        warn!(
            task_id = %request.task_id,
            "task cancellation stops local tracking only; upstream ArtCraft jobs cannot be cancelled (no API endpoint)"
        );
        self.shared.tasks.cancel_task(&request.task_id)
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        Ok(completions::complete(request))
    }

    #[allow(deprecated)]
    async fn set_level(
        &self,
        request: SetLevelRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let level = format!("{:?}", request.level).to_ascii_lowercase();
        let mut configured = self
            .shared
            .log_level
            .write()
            .map_err(|_| McpError::internal_error("log-level lock poisoned", None))?;
        *configured = level;
        Ok(())
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        Ok(prompts::list_prompts())
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, McpError> {
        prompts::get_prompt(&request.name, request.arguments.as_ref())
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(resources_ext::list_resources())
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(resources_ext::list_resource_templates())
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        resources_ext::read_resource(&request.uri)
            .ok_or_else(|| McpError::resource_not_found(format!("unknown resource {}", request.uri), None))
    }

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        let mut accepted = SubscriptionFilter::new();
        accepted.resource_subscriptions = requested.resource_subscriptions.as_ref().map(|uris| {
            uris.iter()
                .filter(|uri| resources_ext::subscription_uri_supported(uri))
                .cloned()
                .collect()
        });
        Some(accepted)
    }

    #[allow(deprecated)]
    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        if resources_ext::subscription_uri_supported(&request.uri) {
            self.shared
                .resource_subscriptions
                .lock()
                .map_err(|_| McpError::internal_error("resource subscription lock poisoned", None))?
                .insert(request.uri);
        }
        Ok(())
    }

    #[allow(deprecated)]
    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        self.shared
            .resource_subscriptions
            .lock()
            .map_err(|_| McpError::internal_error("resource subscription lock poisoned", None))?
            .remove(&request.uri);
        Ok(())
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        trace_meta(request.meta.as_ref().map(|m| &m.0));

        let mut name = request.name.to_string();
        // App-visibility helper tool: the credit dashboard's refresh button
        // calls this; it is never listed to the model (not in the registry).
        if name == "artcraft_app_refresh_credits" {
            name = "artcraft_get_credits".to_string();
        }

        // Zero-cost local protocol fixtures required by the official MCP
        // conformance suite. They are intercepted before registry dispatch and
        // never reach the ArtCraft API.
        if conformance_tools::is_fixture_tool(&name) {
            if let Some(result) = self
                .call_conformance_fixture(
                    &name,
                    request.arguments.clone().unwrap_or_default(),
                    &context,
                )
                .await
            {
                return result;
            }
        }

        let mut arguments = Value::Object(request.arguments.clone().unwrap_or_default());
        let client = self.client_for(&context);

        info!("Tool call: {}", name);

        // --- Deferred dispatch transparency --------------------------------
        // A deferred call to a billable/async tool must pass through the same
        // cost gate, file handling, and task promotion as a direct call, so
        // unwrap it here and fall through. Free synchronous deferred tools
        // still execute inside artcraft_call_deferred_tool below.
        if name == "artcraft_call_deferred_tool" {
            if let Some(inner) = arguments.get("name").and_then(|n| n.as_str()) {
                let inner_needs_pipeline = registry::meta_for(inner)
                    .is_some_and(|m| m.cost != registry::CostTier::Free || m.async_job);
                if inner_needs_pipeline && !META_TOOLS.contains(&inner) {
                    name = inner.to_string();
                    arguments = arguments.get("arguments").cloned().unwrap_or(json!({}));
                }
            }
        }

        // --- Meta-tools (progressive discovery) ---------------------------
        if META_TOOLS.contains(&name.as_str()) {
            let result = match name.as_str() {
                "artcraft_search_tools" => discovery::search_tools(arguments).await,
                "artcraft_describe_tool" => discovery::describe_tool(arguments).await,
                _ => discovery::call_deferred_tool(arguments, &client).await,
            };
            return Ok(tool_outcome(result));
        }

        // --- MRTR retry round: sealed state present ------------------------
        if let Some(sealed) = request.request_state.clone() {
            let kind = self
                .shared
                .state_codec
                .open_json::<Value>(&sealed)
                .ok()
                .and_then(|v| v.get("kind").and_then(|k| k.as_str()).map(|k| k.to_string()))
                .unwrap_or_else(|| "confirm".to_string());
            return if kind == "upload" {
                self.handle_upload_retry(&sealed, &client).await
            } else {
                self.handle_confirm_retry(&sealed, request.input_responses.as_ref(), &client, &context)
                    .await
            };
        }

        // --- Schema-guided coercion for direct calls too -------------------
        let arguments = if let Some(def) = tools::get_all_tools().into_iter().find(|t| t.name == name) {
            match def.input_schema {
                Some(schema) => match arguments {
                    Value::Object(map) => Value::Object(crate::coercion::coerce_object(map, &schema)),
                    other => other,
                },
                None => arguments,
            }
        } else {
            arguments
        };

        // --- Host path restriction for remote serving ----------------------
        if self.shared.remote_exposed
            && std::env::var("ARTCRAFT_MCP_ALLOW_HOST_PATHS").is_err()
        {
            if let Some(fp) = arguments.get("file_path").and_then(|v| v.as_str()) {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "host file paths are disabled while serving remote clients ({}); use io.modelcontextprotocol/files transfers or set ARTCRAFT_MCP_ALLOW_HOST_PATHS=1",
                    fp
                ))])
                .into());
            }
        }

        // --- Files extension: inline data and out-of-band uploads ----------
        let client_files_caps = context
            .client_capabilities()
            .and_then(|c| c.extensions.as_ref().and_then(|e| e.get(files_ext::EXT_KEY).cloned()));
        let client_files_upload = client_files_caps.as_ref().is_some_and(|v| v.get("upload").is_some());
        let client_files_download = client_files_caps.as_ref().is_some_and(|v| v.get("download").is_some());

        match files_ext::detect_file_inputs(&arguments) {
            Ok(inputs) if !inputs.is_empty() => {
                // Inline base64 data: decode small files straight to disk.
                let mut all_inline = true;
                let mut rewritten = arguments.clone();
                for (key, meta) in &inputs {
                    if meta.data.is_some() {
                        if meta.size > files_ext::MAX_INLINE_BYTES {
                            all_inline = false;
                            break;
                        }
                        match decode_inline_file(meta) {
                            Ok(path) => {
                                rewritten[key] = json!({ "inline_saved": path });
                            }
                            Err(e) => {
                                return Ok(tool_outcome(Err(e)));
                            }
                        }
                    } else {
                        all_inline = false;
                    }
                }
                if all_inline {
                    // Rewrite file arguments to saved paths and execute.
                    if let Some(map) = rewritten.as_object_mut() {
                        let keys: Vec<String> = map.keys().cloned().collect();
                        for k in keys {
                            let saved = map[&k]
                                .get("inline_saved")
                                .and_then(|v| v.as_str())
                                .map(|v| v.to_string());
                            if let Some(saved) = saved {
                                if k != "file_path" {
                                    map.remove(&k);
                                }
                                map.insert("file_path".to_string(), json!(saved));
                            }
                        }
                    }
                    let mut cleanup_paths: Vec<String> = Vec::new();
                    if let Some(fp) = rewritten.get("file_path").and_then(|v| v.as_str()) {
                        if is_spooled_temp_file(fp) {
                            cleanup_paths.push(fp.to_string());
                        }
                    }
                    let outcome = tool_outcome(tools::execute_tool(&name, rewritten, &client).await);
                    for path in cleanup_paths {
                        let _ = tokio::fs::remove_file(&path).await;
                    }
                    return Ok(outcome);
                }
                if client_files_upload {
                    return self.issue_upload_request(&name, arguments, inputs).await;
                }
                // Client cannot transfer files: fail cleanly rather than fake it.
                return Ok(CallToolResult::error(vec![ContentBlock::text(
                    "file arguments require a client supporting io.modelcontextprotocol/files upload; pass a local file_path instead",
                )]).into());
            }
            Err(e) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!("Error: {}", e))]).into());
            }
            _ => {}
        }

        // --- Cost confirmation gate (MRTR) ---------------------------------
        let meta = registry::meta_for(&name);
        let billable = meta.is_some_and(|m| m.cost != CostTier::Free);
        let supports_elicitation = context
            .client_capabilities()
            .is_some_and(|c| c.elicitation.is_some());
        let modern = context
            .protocol_version()
            .is_some_and(|v| v >= ProtocolVersion::V_2026_07_28);

        if billable && modern && supports_elicitation && !self.shared.no_confirm {
            if let Some(m) = meta {
                // Prefer the exact upstream price for this model+arguments
                // (e.g. flux_1_schnell is free despite the tool's Medium
                // tier); fall back to the conservative tier magnitude when
                // the call can't be priced.
                let quote = match tools::model_cost_quote(&name, &arguments, &client).await {
                    Some(q) => CostQuote {
                        credits: q.credits,
                        exact: true,
                        free: q.free,
                    },
                    None => CostQuote {
                        credits: tier_estimate(m.cost),
                        exact: false,
                        free: false,
                    },
                };
                let over_threshold = self.shared.confirm_over_credits == 0
                    || quote.credits >= self.shared.confirm_over_credits;
                let auto_approved = self.shared.auto_approve_under > 0
                    && quote.credits <= self.shared.auto_approve_under;
                if over_threshold && !auto_approved {
                    return self.issue_cost_confirmation(&name, arguments, quote);
                }
            }
        }

        // --- Execution: task promotion for async jobs ----------------------
        let supports_tasks = context
            .client_capabilities()
            .is_some_and(|c| c.supports_tasks());
        let is_async_job = meta.is_some_and(|m| m.async_job);

        if is_async_job && supports_tasks && modern {
            let tool_name = name.clone();
            let args = arguments.clone();
            let task_client = ArtCraftClient::new(self.shared.startup_credentials.clone());
            let task = self.shared.tasks.spawn(Default::default(), move |ctx| {
                Box::pin(async move {
                    tokio::select! {
                        _ = ctx.cancelled() => {
                            Err(rmcp::task_manager::TaskExit::Cancelled)
                        }
                        result = run_and_track_job(&tool_name, args, &task_client) => {
                            match result {
                                Ok(contents) => Ok(CallToolResult::success(
                                    contents.into_iter().map(content_block).collect(),
                                )),
                                Err(e) => Ok(CallToolResult::error(vec![
                                    ContentBlock::text(format!("Error: {}", e)),
                                ])),
                            }
                        }
                    }
                })
            });
            return Ok(CallToolResponse::Task(CreateTaskResult::new(task)));
        }

        // Synchronous execution (legacy clients and non-job tools).
        let outcome = tool_outcome(tools::execute_tool(&name, arguments.clone(), &client).await);
        Ok(enrich_downloads(outcome, &name, client_files_download))
    }
}

impl ArtCraftMcp {
    /// First MRTR round: seal the pending call into requestState and ask the
    /// client to confirm the estimated credit cost via elicitation.
    fn issue_cost_confirmation(
        &self,
        name: &str,
        arguments: Value,
        quote: CostQuote,
    ) -> Result<CallToolResponse, McpError> {
        let state = ConfirmState {
            kind: confirm_kind(),
            tool: name.to_string(),
            args: arguments,
            quote_credits: quote.credits,
            idempotency_key: uuid::Uuid::new_v4().to_string(),
            expires_unix: chrono::Utc::now().timestamp() + 15 * 60,
        };
        let sealed = self
            .shared
            .state_codec
            .seal_json(&state)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Exact quotes come from the upstream cost route for this model and
        // these arguments; anything else is the conservative tier magnitude
        // and must be presented as approximate.
        let (message, field_description) = if quote.free {
            (
                format!(
                    "{} is estimated at 0 credits (free tier for this account). Confirm to proceed?",
                    name
                ),
                format!("Run {} — estimated cost: 0 credits (free)", name),
            )
        } else if quote.exact {
            (
                format!(
                    "{} is estimated at {} credits (exact upstream quote for this request). Confirm to proceed?",
                    name, quote.credits
                ),
                format!("Spend {} credits on {}?", quote.credits, name),
            )
        } else {
            (
                format!(
                    "{} is estimated at ~{} credits (approximate; no model-specific quote available). Confirm to proceed?",
                    name, quote.credits
                ),
                format!("Spend ~{} credits on {}?", quote.credits, name),
            )
        };

        let mut input_requests = InputRequests::new();
        input_requests.insert(
            "confirm".to_string(),
            InputRequest::Elicitation(ElicitRequest::new(
                ElicitRequestParams::FormElicitationParams {
                    meta: None,
                    message,
                    requested_schema: serde_json::from_value(json!({
                        "type": "object",
                        "properties": {
                            "confirm": {
                                "type": "boolean",
                                "title": "Confirm generation",
                                "description": field_description
                            }
                        },
                        "required": ["confirm"]
                    }))
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?,
                },
            )),
        );

        Ok(InputRequiredResult::new(Some(input_requests), Some(sealed)).into())
    }

    /// Retry round: verify the sealed state, read the confirmation, execute
    /// or report a clean cancellation.
    async fn handle_confirm_retry(
        &self,
        sealed: &str,
        input_responses: Option<&InputResponses>,
        client: &ArtCraftClient,
        context: &RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let state: ConfirmState = self
            .shared
            .state_codec
            .open_json(sealed)
            .map_err(|_| McpError::invalid_params("tampered or unknown request state", None))?;

        if state.expires_unix != 0 && chrono::Utc::now().timestamp() > state.expires_unix {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "cost confirmation expired; please re-issue the request",
            )])
            .into());
        }

        let response = input_responses
            .and_then(|r| r.get("confirm"))
            .ok_or_else(|| McpError::invalid_params("missing confirmation response", None))?;
        let response: ElicitResult = serde_json::from_value(response.clone())
            .map_err(|_| McpError::invalid_params("invalid confirmation response", None))?;

        let approved = match response.action {
            ElicitationAction::Accept => response
                .content
                .as_ref()
                .and_then(|c| c["confirm"].as_bool())
                .unwrap_or(false),
            ElicitationAction::Decline | ElicitationAction::Cancel => false,
            _ => false,
        };

        if !approved {
            return Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                "Generation cancelled — no credits spent ({}).",
                state.tool
            ))])
            .into());
        }

        // Idempotency: one confirmation executes at most once per process,
        // even if the client retries after losing the reply.
        {
            let mut consumed = self
                .shared
                .consumed_confirmations
                .lock()
                .map_err(|_| McpError::internal_error("confirmation lock poisoned", None))?;
            if consumed.contains(&state.idempotency_key) {
                return Ok(CallToolResult::error(vec![ContentBlock::text(
                    "this confirmation was already executed; re-issue the original request if the result was lost",
                )])
                .into());
            }
            consumed.insert(state.idempotency_key.clone());
        }

        // Confirmed: execute with the original arguments. Task promotion is
        // applied on this round as well so confirmations don't lose tracking.
        let meta = registry::meta_for(&state.tool);
        let supports_tasks = context
            .client_capabilities()
            .is_some_and(|c| c.supports_tasks());
        if meta.is_some_and(|m| m.async_job) && supports_tasks {
            let tool_name = state.tool.clone();
            let args = state.args.clone();
            let task_client = ArtCraftClient::new(self.shared.startup_credentials.clone());
            let task = self.shared.tasks.spawn(Default::default(), move |ctx| {
                Box::pin(async move {
                    tokio::select! {
                        _ = ctx.cancelled() => Err(rmcp::task_manager::TaskExit::Cancelled),
                        result = run_and_track_job(&tool_name, args, &task_client) => {
                            match result {
                                Ok(contents) => Ok(CallToolResult::success(
                                    contents.into_iter().map(content_block).collect(),
                                )),
                                Err(e) => Ok(CallToolResult::error(vec![
                                    ContentBlock::text(format!("Error: {}", e)),
                                ])),
                            }
                        }
                    }
                })
            });
            return Ok(CallToolResponse::Task(CreateTaskResult::new(task)));
        }

        Ok(tool_outcome(
            tools::execute_tool(&state.tool, state.args, client).await,
        ))
    }
}

/// Submit a generation job and poll it to completion inside a task, so the
/// task lifecycle mirrors the remote ArtCraft job lifecycle.
struct JobPoll {
    contents: Vec<ToolContent>,
    terminal_success: bool,
    terminal_failure: bool,
}

/// Typed job-status poll: same response shape the artcraft_get_job_status tool
/// renders, but with terminal-state classification from the upstream enum
/// itself rather than text scraping.
async fn poll_job_status(token: &str, client: &ArtCraftClient) -> anyhow::Result<JobPoll> {
    let url = client.absolute_url(&format!("/v1/jobs/job/{}", token));
    let request = reqwest::Client::new().get(&url).header("Accept", "application/json");
    let response = client.apply_auth(request).send().await?;
    if !response.status().is_success() {
        return Err(anyhow::anyhow!("job status request failed with HTTP {}", response.status()));
    }
    let response_json: Value = response.json().await?;
    let empty = Value::Null;
    let job = response_json.get("state").unwrap_or(&empty);

    let status = job.pointer("/status/status")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let status_value = status.to_lowercase().replace('_', "");
    let terminal_success = status_value == "completesuccess";
    let terminal_failure = matches!(
        status_value.as_str(),
        "completefailure" | "dead" | "cancelledbyuser" | "cancelledbysystem"
    );

    let progress = job.pointer("/status/progress_percentage").and_then(|v| v.as_u64()).unwrap_or(0);
    let category = job.pointer("/request/inference_category").and_then(|v| v.as_str()).unwrap_or("?");
    let created = job.get("created_at").and_then(|v| v.as_str()).unwrap_or("?");
    let updated = job.get("updated_at").and_then(|v| v.as_str()).unwrap_or("?");
    let mut lines = vec![
        format!("Job: {}", token),
        format!("Status: {}", status),
        format!("Progress: {}%", progress),
        format!("Category: {}", category),
        format!("Created: {}", created),
        format!("Updated: {}", updated),
    ];
    if let Some(extra) = job.pointer("/status/maybe_extra_status_description").and_then(|v| v.as_str()) {
        lines.push(format!("Extra: {}", extra));
    }
    if let Some(failure) = job.pointer("/status/maybe_failure_message").and_then(|v| v.as_str()) {
        lines.push(format!("Failure: {}", failure));
    }
    if let Some(result) = job.pointer("/maybe_result/entity_token").and_then(|v| v.as_str()) {
        lines.push(format!("Result token: {}", result));
    }
    if let Some(completed_at) = job.pointer("/maybe_result/maybe_successfully_completed_at").and_then(|v| v.as_str()) {
        lines.push(format!("Completed at: {}", completed_at));
    }
    Ok(JobPoll {
        contents: vec![ToolContent {
            content_type: "text".to_string(),
            text: lines.join("
"),
        }],
        terminal_success,
        terminal_failure,
    })
}

async fn run_and_track_job(
    tool_name: &str,
    args: Value,
    client: &ArtCraftClient,
) -> anyhow::Result<Vec<ToolContent>> {
    let mut contents = tools::execute_tool(tool_name, args, client).await?;

    // Generation tools report the job token in their text output; when we can
    // extract it, poll until the remote job reaches a terminal state.
    let text: String = contents.iter().map(|c| c.text.clone()).collect();
    if let Some(token) = extract_job_token(&text) {
        for _ in 0..240 {
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            // Poll the typed upstream API directly rather than re-parsing the
            // text output of artcraft_get_job_status: task completion must not
            // depend on display formatting.
            match poll_job_status(&token, client).await {
                Ok(poll) => {
                    let status_contents = poll.contents;
                    let status_text: String =
                        status_contents.iter().map(|c| c.text.clone()).collect();
                    // Parse the dedicated `Status:` line rather than scanning
                    // the whole response: the job token itself appears in the
                    // output and can literally contain substrings like "dead",
                    // which would fake a terminal state. Upstream JobStatusPlus:
                    // Pending, Started, CompleteSuccess, CompleteFailure,
                    // AttemptFailed (transient retry), Dead, CancelledByUser,
                    // CancelledBySystem.
                    let terminal_failure = poll.terminal_failure;
                    let terminal_success = poll.terminal_success;
                    let _ = status_text;
                    if terminal_failure || terminal_success {
                        contents.extend(status_contents);
                        return Ok(contents);
                    }
                }
                Err(_) => continue,
            }
        }
        // The job never reached a terminal state within the polling window;
        // returning the original queue message here would report a dead or
        // stuck job as a successful generation.
        return Err(anyhow::anyhow!(
            "job {} did not reach a terminal state within the polling window; check artcraft_get_job_status with job_token=\"{}\"",
            token,
            token
        ));
    }
    Ok(contents)
}

/// True only for paths inside the system temp directory whose file name
/// carries one of our spool prefixes. Callers can otherwise pass a path that
/// merely contains the prefix as a substring (e.g. a host file named
/// "artcraft-mcp-inline-x" in a user directory) and trick cleanup into
/// deleting files the server never created.
fn is_spooled_temp_file(path: &str) -> bool {
    let path = std::path::Path::new(path);
    let temp = std::env::temp_dir();
    if !path.starts_with(&temp) {
        return false;
    }
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("artcraft-mcp-inline-") || n.starts_with("artcraft-mcp-upload-"))
}

fn extract_job_token(text: &str) -> Option<String> {
    for line in text.lines() {
        let lower = line.to_lowercase();
        if lower.contains("job") && lower.contains("token") {
            if let Some(pos) = line.rfind([':', '`']) {
                let candidate = line[pos + 1..]
                    .trim()
                    .trim_matches('`')
                    .trim_matches('.')
                    .trim();
                if candidate.len() >= 8
                    && candidate
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                {
                    return Some(candidate.to_string());
                }
            }
        }
    }
    None
}

fn tool_outcome(result: anyhow::Result<Vec<ToolContent>>) -> CallToolResponse {
    match result {
        Ok(contents) => CallToolResult::success(
            contents.into_iter().map(content_block).collect(),
        )
        .into(),
        Err(e) => {
            warn!("tool failed: {}", e);
            CallToolResult::error(vec![ContentBlock::text(format!("Error: {}", e))]).into()
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
    let mut tool = Tool::new(
        Cow::<'static, str>::Owned(def.name.clone()),
        Cow::<'static, str>::Owned(def.description),
        std::sync::Arc::new(schema_object),
    );
    if let Some(meta) = registry::meta_for(&def.name) {
        tool.annotations = Some(meta.annotations());
    }
    if let Some(ui_meta) = crate::apps::annotate_tool_meta(&def.name) {
        let mut meta = MetaObject::default();
        for (k, v) in ui_meta {
            meta.insert(k, v);
        }
        tool.meta = Some(meta);
    }
    tool
}

fn content_block(content: ToolContent) -> rmcp::model::ContentBlock {
    rmcp::model::ContentBlock::text(content.text)
}

impl ArtCraftMcp {
    /// Issue an upload_required-style MRTR round for out-of-band file bytes.
    ///
    /// rmcp 3.11 has no typed `upload_required` result (the files extension
    /// is a November 2026 draft), so the upload instructions ride inside the
    /// MRTR envelope: `inputRequests` is empty and the targets are carried in
    /// `_meta["io.modelcontextprotocol/files"].uploads`. Retry semantics,
    /// state sealing, and expiry match the draft exactly.
    /// TODO(rmcp): emit native `resultType: "upload_required"` once the SDK
    /// models the files extension.
    async fn issue_upload_request(
        &self,
        tool: &str,
        args: Value,
        inputs: Vec<(String, files_ext::FileInputMeta)>,
    ) -> Result<CallToolResponse, McpError> {
        // Validate everything checkable without the bytes first.
        for (key, meta) in &inputs {
            if meta.size == 0 {
                // Zero-byte files never get upload targets per the draft.
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "file input '{}' declares size 0; empty files must be sent inline or omitted",
                    key
                ))])
                .into());
            }
            if meta.size > files_ext::MAX_UPLOAD_BYTES {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "file input '{}' is {} bytes, over the {} byte limit",
                    key, meta.size, files_ext::MAX_UPLOAD_BYTES
                ))])
                .into());
            }
        }

        let mut pending = Vec::new();
        for (key, meta) in inputs {
            let token = uuid::Uuid::new_v4().simple().to_string();
            self.shared.spool.register(&token).await;
            pending.push(files_ext::PendingUpload {
                file_id: meta.id.clone(),
                arg_key: key,
                token,
                mime_type: meta.mime_type,
                declared_size: meta.size,
                name: meta.name,
            });
        }

        let state = files_ext::UploadState {
            tool: tool.to_string(),
            args,
            uploads: pending.clone(),
            expires_unix: chrono::Utc::now().timestamp() + 15 * 60,
        };
        // Wrap with kind tag for retry dispatch.
        let mut state_value = serde_json::to_value(&state)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        state_value["kind"] = json!("upload");
        let sealed = self
            .shared
            .state_codec
            .seal_json(&state_value)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let base_url = self
            .shared
            .relay_base_url
            .read()
            .ok()
            .and_then(|g| g.clone());
        let targets: Vec<Value> = pending
            .iter()
            .map(|u| files_ext::upload_target(u, base_url.as_deref()))
            .collect();

        let mut meta_obj = MetaObject::default();
        meta_obj.insert(
            files_ext::EXT_KEY.to_string(),
            json!({ "uploads": targets }),
        );
        let result = InputRequiredResult::new(None, Some(sealed)).with_meta(meta_obj);
        Ok(result.into())
    }

    /// Upload retry: verify state, consume spooled bytes, write them to disk
    /// with a correct extension, rewrite the arguments to file_path form, and
    /// execute the original tool.
    async fn handle_upload_retry(
        &self,
        sealed: &str,
        client: &ArtCraftClient,
    ) -> Result<CallToolResponse, McpError> {
        let state_value: Value = self
            .shared
            .state_codec
            .open_json(sealed)
            .map_err(|_| McpError::invalid_params("tampered or unknown request state", None))?;
        let state: files_ext::UploadState = serde_json::from_value(state_value)
            .map_err(|_| McpError::invalid_params("malformed upload state", None))?;

        if chrono::Utc::now().timestamp() > state.expires_unix {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "upload request expired; please re-issue the call",
            )])
            .into());
        }

        let mut args = state.args.clone();
        for upload in &state.uploads {
            let Some(spooled) = self.shared.spool.take(&upload.token).await else {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "no bytes received for file '{}' — upload to the target first",
                    upload.file_id
                ))])
                .into());
            };
            // Re-verify client-asserted metadata against actual bytes.
            let size = tokio::fs::metadata(&spooled)
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            if size == 0 {
                let _ = tokio::fs::remove_file(&spooled).await;
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "file '{}' uploaded zero bytes",
                    upload.file_id
                ))])
                .into());
            }
            if size != upload.declared_size {
                let _ = tokio::fs::remove_file(&spooled).await;
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "file '{}' size mismatch: declared {} bytes, received {}",
                    upload.file_id, upload.declared_size, size
                ))])
                .into());
            }
            // Give the spool file a useful extension for downstream MIME sniffing.
            let ext = upload
                .name
                .as_deref()
                .and_then(|n| n.rsplit('.').next().map(|e| e.to_string()))
                .unwrap_or_else(|| "bin".to_string());
            let final_path = spooled.with_extension(ext);
            let _ = tokio::fs::rename(&spooled, &final_path).await;
            // Replace the file-input object with a plain file_path argument.
            if let Some(map) = args.as_object_mut() {
                map.remove(&upload.arg_key);
                map.insert("file_path".to_string(), json!(final_path.to_string_lossy()));
            }
        }

        let mut cleanup_paths: Vec<String> = Vec::new();
        if let Some(fp) = args.get("file_path").and_then(|v| v.as_str()) {
            if is_spooled_temp_file(fp) {
                cleanup_paths.push(fp.to_string());
            }
        }
        let outcome = tool_outcome(tools::execute_tool(&state.tool, args, client).await);
        for path in cleanup_paths {
            let _ = tokio::fs::remove_file(&path).await;
        }
        Ok(outcome)
    }
}

/// Decode an inline base64 file input to a temp file, returning its path.
fn decode_inline_file(meta: &files_ext::FileInputMeta) -> anyhow::Result<String> {
    use base64::Engine;
    let data = meta.data.as_deref().unwrap_or("");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| anyhow::anyhow!("invalid base64 in inline file data: {}", e))?;
    if bytes.len() as u64 != meta.size {
        return Err(anyhow::anyhow!(
            "inline file size mismatch: declared {}, decoded {}",
            meta.size,
            bytes.len()
        ));
    }
    let ext = meta
        .name
        .as_deref()
        .and_then(|n| n.rsplit('.').next().map(|e| e.to_string()))
        .unwrap_or_else(|| "bin".to_string());
    let path = std::env::temp_dir().join(format!(
        "artcraft-mcp-inline-{}.{}",
        uuid::Uuid::new_v4().simple(),
        ext
    ));
    std::fs::write(&path, &bytes)?;
    Ok(path.to_string_lossy().to_string())
}

/// When the client accepts download targets, strip media URLs from
/// model-visible text and carry them in `_meta` instead (draft requirement:
/// no URLs in content when the files extension is active).
fn enrich_downloads(
    outcome: CallToolResponse,
    tool: &str,
    client_supports_download: bool,
) -> CallToolResponse {
    if !client_supports_download || tool != "artcraft_download_media_file" {
        return outcome;
    }
    let CallToolResponse::Complete(mut result) = outcome else {
        return outcome;
    };
    let text: String = result
        .content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.clone()))
        .collect::<Vec<_>>()
        .join("\n");
    let Some(url) = files_ext::extract_first_url(&text) else {
        return CallToolResponse::Complete(result);
    };
    let mut meta = result.meta.clone().unwrap_or_default();
    let files_meta = files_ext::download_meta(vec![("file-1".to_string(), url.clone(), None)]);
    if let Some(inner) = files_meta.get(files_ext::EXT_KEY) {
        meta.insert(files_ext::EXT_KEY.to_string(), inner.clone());
    }
    result.meta = Some(meta);
    result.content = vec![ContentBlock::text(
        "Media file ready — download target attached in result metadata.",
    )];
    CallToolResponse::Complete(result)
}

// ---------------------------------------------------------------------------
// Transports
// ---------------------------------------------------------------------------

pub async fn serve_stdio(cli: Cli) -> anyhow::Result<()> {
    let service = ArtCraftMcp::new(&cli);

    // Ephemeral loopback relay for out-of-band file uploads: stdio has no
    // HTTP address of its own, so upload targets use this listener's URL.
    if let Ok(listener) = tokio::net::TcpListener::bind("127.0.0.1:0").await {
        if let Ok(addr) = listener.local_addr() {
            let base = format!("http://{}", addr);
            if let Ok(mut guard) = service.shared.relay_base_url.write() {
                *guard = Some(base);
            }
            let router = files_ext::relay_router(service.shared.spool.clone());
            tokio::spawn(async move {
                let _ = axum::serve(listener, router).await;
            });
        }
    }

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

    let mut config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(true)
        .with_json_response(true)
        .with_cancellation_token(ct.child_token());
    if cli.allow_remote {
        // rmcp's default Host allowlist is loopback-only, which would reject
        // every genuine remote client. Browser DNS-rebinding protection still
        // applies through the Origin middleware below.
        config = config.disable_allowed_hosts();
        warn!("remote bind enabled: serve behind a TLS-terminating proxy or VPN; the bearer token is visible to on-path observers over cleartext HTTP");
    }

    let factory_handler = handler.clone();
    let service = StreamableHttpService::new(
        move || Ok(factory_handler.clone()),
        LocalSessionManager::default().into(),
        config,
    );

    let router = axum::Router::new()
        .nest_service("/mcp", service)
        .route("/health", axum::routing::get(|| async { "OK" }))
        .merge(files_ext::relay_router(handler.shared.spool.clone()))
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
