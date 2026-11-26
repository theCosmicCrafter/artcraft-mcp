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

use crate::client::ArtCraftClient;
use crate::registry::{self, CostTier, HOT_TOOLS, META_TOOLS};
use crate::types::ToolContent;
use crate::{Cli, ToolMode, auth, discovery, files_ext, telemetry, tools};

/// Static per-tier credit estimates used for the MRTR confirmation gate.
/// Real per-argument quotes come from artcraft_estimate_cost; the gate only
/// needs a conservative magnitude to decide whether to confirm.
fn tier_estimate(tier: CostTier) -> u64 {
    match tier {
        CostTier::Free => 0,
        CostTier::Low => 10,
        CostTier::Medium => 40,
        CostTier::High => 120,
    }
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
    pub startup_credentials: Option<
        artcraft_client::credentials::storyteller_credential_set::StorytellerCredentialSet,
    >,
    /// In-memory task manager backing io.modelcontextprotocol/tasks.
    /// Handles are unguessable UUIDs; durable cross-restart persistence is
    /// tracked in the SQLite task store (see tasks_store.rs).
    pub tasks: TaskManager,
    /// Spool for out-of-band file uploads (io.modelcontextprotocol/files).
    pub spool: files_ext::SharedSpool,
    /// Absolute loopback base URL of the ephemeral relay (stdio mode only).
    /// None in HTTP mode, where upload targets use relative paths.
    pub relay_base_url: std::sync::RwLock<Option<String>>,
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
        let mut capabilities = ServerCapabilities::builder().enable_tools().enable_resources().build();
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

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(crate::apps::list_resources())
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        crate::apps::read_resource(&request.uri)
            .ok_or_else(|| McpError::resource_not_found(format!("unknown resource {}", request.uri), None))
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
        let arguments = Value::Object(request.arguments.clone().unwrap_or_default());
        let client = self.client_for(&context);

        info!("Tool call: {}", name);

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

        // --- Files extension: inline data and out-of-band uploads ----------
        let client_files_caps = context
            .client_capabilities()
            .and_then(|c| c.extensions.as_ref().map(|e| e.get(files_ext::EXT_KEY).cloned()).flatten());
        let client_files_upload = client_files_caps.as_ref().is_some_and(|v| v.get("upload").is_some());
        let client_files_download = client_files_caps.as_ref().is_some_and(|v| v.get("download").is_some());

        match files_ext::detect_file_inputs(&arguments) {
            Ok(inputs) if !inputs.is_empty() => {
                // Inline base64 data: decode small files straight to disk.
                let mut all_inline = true;
                let mut rewritten = arguments.clone();
                for (key, meta) in &inputs {
                    if let Some(data) = &meta.data {
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
                            if let Some(saved) = map[&k].get("inline_saved").and_then(|v| v.as_str()) {
                                map.insert("file_path".to_string(), json!(saved));
                                map.remove(&k);
                            }
                        }
                    }
                    return Ok(tool_outcome(tools::execute_tool(&name, rewritten, &client).await));
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
                let quote = tier_estimate(m.cost);
                let needs_confirm = quote > self.shared.auto_approve_under
                    && quote >= self.shared.confirm_over_credits.min(u64::MAX);
                let threshold_hit = quote >= self.shared.confirm_over_credits
                    || self.shared.confirm_over_credits == 0;
                if (threshold_hit || quote > self.shared.auto_approve_under) && needs_confirm {
                    return self.issue_cost_confirmation(&name, arguments, quote);
                }
                if threshold_hit && quote > self.shared.auto_approve_under {
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
        quote: u64,
    ) -> Result<CallToolResponse, McpError> {
        let state = ConfirmState {
            kind: confirm_kind(),
            tool: name.to_string(),
            args: arguments,
            quote_credits: quote,
            idempotency_key: uuid::Uuid::new_v4().to_string(),
            expires_unix: chrono::Utc::now().timestamp() + 15 * 60,
        };
        let sealed = self
            .shared
            .state_codec
            .seal_json(&state)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let mut input_requests = InputRequests::new();
        input_requests.insert(
            "confirm".to_string(),
            InputRequest::Elicitation(ElicitRequest::new(
                ElicitRequestParams::FormElicitationParams {
                    meta: None,
                    message: format!(
                        "This generation is estimated at ~{} credits. Confirm to proceed?",
                        quote
                    ),
                    requested_schema: serde_json::from_value(json!({
                        "type": "object",
                        "properties": {
                            "confirm": {
                                "type": "boolean",
                                "title": "Confirm generation",
                                "description": format!("Spend ~{} credits on {}?", quote, name)
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
        let poll_args = json!({ "job_token": token });
        for _ in 0..240 {
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            match tools::execute_tool("artcraft_get_job_status", poll_args.clone(), client).await {
                Ok(status_contents) => {
                    let status_text: String =
                        status_contents.iter().map(|c| c.text.clone()).collect();
                    let lower = status_text.to_lowercase();
                    if lower.contains("complete") || lower.contains("succeeded") {
                        contents.extend(status_contents);
                        return Ok(contents);
                    }
                    if lower.contains("fail") || lower.contains("error") || lower.contains("cancel") {
                        contents.extend(status_contents);
                        return Ok(contents);
                    }
                }
                Err(_) => continue,
            }
        }
    }
    Ok(contents)
}

fn extract_job_token(text: &str) -> Option<String> {
    for line in text.lines() {
        let lower = line.to_lowercase();
        if lower.contains("job") && lower.contains("token") {
            if let Some(pos) = line.rfind(|c: char| c == ':' || c == '`') {
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

        Ok(tool_outcome(
            tools::execute_tool(&state.tool, args, client).await,
        ))
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
