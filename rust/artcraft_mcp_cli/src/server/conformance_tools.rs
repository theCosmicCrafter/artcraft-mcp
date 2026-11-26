//! Local MCP protocol fixture tools used by the official conformance suite.
//!
//! These tools are deterministic, zero-cost, and never call the ArtCraft API.
//! They exercise wire behaviors (all content types, notifications, progress,
//! sampling, and elicitation) that ArtCraft's production tools intentionally
//! do not exercise synchronously. They are callable by their conformance
//! names but deliberately omitted from the user-facing tool catalog.
//!
//! Sampling and logging are legacy/deprecated in newer MCP revisions, but the
//! active conformance suite still specifies them for 2025-era clients. The
//! implementations are isolated here and are not used by production tools.
#![allow(deprecated)]

use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::ErrorData as McpError;
use serde_json::{json, Map, Value};

use super::{fixtures, ArtCraftMcp};

fn success_text(text: impl Into<String>) -> CallToolResponse {
    CallToolResult::success(vec![ContentBlock::text(text)]).into()
}

fn error_text(text: impl Into<String>) -> CallToolResponse {
    CallToolResult::error(vec![ContentBlock::text(text)]).into()
}

fn fixture_error(message: impl Into<String>) -> Result<CallToolResponse, McpError> {
    Ok(error_text(message))
}

fn required_arg<'a>(arguments: &'a Map<String, Value>, name: &str) -> Result<&'a str, McpError> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            McpError::invalid_params(format!("missing required argument '{name}'"), None)
        })
}

fn severity_rank(level: &str) -> u8 {
    match level {
        "debug" => 0,
        "info" => 1,
        "notice" => 2,
        "warning" => 3,
        "error" => 4,
        "critical" => 5,
        "alert" => 6,
        "emergency" => 7,
        _ => 1,
    }
}

fn level_name(level: LoggingLevel) -> &'static str {
    match level {
        LoggingLevel::Debug => "debug",
        LoggingLevel::Info => "info",
        LoggingLevel::Notice => "notice",
        LoggingLevel::Warning => "warning",
        LoggingLevel::Error => "error",
        LoggingLevel::Critical => "critical",
        LoggingLevel::Alert => "alert",
        LoggingLevel::Emergency => "emergency",
    }
}

async fn emit_log(
    server: &ArtCraftMcp,
    context: &RequestContext<RoleServer>,
    level: LoggingLevel,
    message: &str,
) {
    let configured = server
        .shared
        .log_level
        .read()
        .map(|level| level.clone())
        .unwrap_or_else(|_| "info".to_string());
    if severity_rank(level_name(level)) < severity_rank(&configured) {
        return;
    }
    let params = LoggingMessageNotificationParam::new(level, json!(message))
        .with_logger("artcraft-mcp.protocol-fixture");
    let _ = context.peer.notify_logging_message(params).await;
}

async fn emit_progress(
    context: &RequestContext<RoleServer>,
    token: ProgressToken,
    progress: f64,
    message: &str,
) {
    let params = ProgressNotificationParam::new(token, progress)
        .with_total(100.0)
        .with_message(message);
    let _ = context.peer.notify_progress(params).await;
}

fn embedded_text_resource(uri: &str, text: &str, mime_type: &str) -> ContentBlock {
    ContentBlock::resource(ResourceContents::text(text, uri).with_mime_type(mime_type))
}

fn parse_schema(value: Value) -> Result<ElicitationSchema, McpError> {
    serde_json::from_value(value).map_err(|error| {
        McpError::internal_error(format!("invalid fixture elicitation schema: {error}"), None)
    })
}

async fn request_form_elicitation(
    context: &RequestContext<RoleServer>,
    message: String,
    requested_schema: ElicitationSchema,
) -> Result<ElicitResult, String> {
    let request = ServerRequest::ElicitRequest(ElicitRequest::new(
        ElicitRequestParams::FormElicitationParams {
            meta: None,
            message,
            requested_schema,
        },
    ));
    match context.peer.send_request(request).await {
        Ok(ClientResult::ElicitResult(result)) => Ok(result),
        Ok(_) => Err("unexpected response to elicitation/create".to_string()),
        Err(error) => Err(error.to_string()),
    }
}

fn elicitation_summary(result: &ElicitResult) -> String {
    let action = format!("{:?}", result.action).to_ascii_lowercase();
    match &result.content {
        Some(content) => format!("Elicitation completed: action={action}, content={content}"),
        None => format!("Elicitation completed: action={action}, content={{}}"),
    }
}

fn client_supports_elicitation(context: &RequestContext<RoleServer>) -> bool {
    context
        .client_capabilities()
        .is_some_and(|capabilities| capabilities.elicitation.is_some())
}

fn client_supports_sampling(context: &RequestContext<RoleServer>) -> bool {
    context
        .client_capabilities()
        .is_some_and(|capabilities| capabilities.sampling.is_some())
}

impl ArtCraftMcp {
    /// Execute one of the local MCP conformance fixture tools. `None` means
    /// the name belongs to the normal ArtCraft registry dispatch path.
    pub(super) async fn call_conformance_fixture(
        &self,
        name: &str,
        arguments: Map<String, Value>,
        context: &RequestContext<RoleServer>,
    ) -> Option<Result<CallToolResponse, McpError>> {
        match name {
            "test_simple_text" => Some(Ok(success_text(
                "This is a simple text response for testing.",
            ))),
            "test_error_handling" => Some(Ok(error_text(
                "This tool intentionally returns an error for testing",
            ))),
            "test_image_content" => Some(Ok(CallToolResult::success(vec![ContentBlock::image(
                fixtures::PNG_1X1_RED_BASE64,
                "image/png",
            )])
            .into())),
            "test_audio_content" => Some(Ok(CallToolResult::success(vec![ContentBlock::audio(
                fixtures::WAV_440HZ_BASE64,
                "audio/wav",
            )])
            .into())),
            "test_embedded_resource" => {
                Some(Ok(CallToolResult::success(vec![embedded_text_resource(
                    "test://embedded-resource",
                    "This is an embedded resource content.",
                    "text/plain",
                )])
                .into()))
            }
            "test_multiple_content_types" => Some(Ok(CallToolResult::success(vec![
                ContentBlock::text("Multiple content types test:"),
                ContentBlock::image(fixtures::PNG_1X1_RED_BASE64, "image/png"),
                embedded_text_resource(
                    "test://mixed-content-resource",
                    r#"{"test":"data","value":123}"#,
                    "application/json",
                ),
            ])
            .into())),
            "test_tool_with_logging" => Some(self.fixture_logging_tool(context).await),
            "test_tool_with_progress" => Some(self.fixture_progress_tool(context).await),
            "test_sampling" => Some(self.fixture_sampling_tool(arguments, context).await),
            "test_elicitation" => Some(self.fixture_elicitation_tool(arguments, context).await),
            "test_elicitation_sep1034_defaults" => {
                Some(self.fixture_elicitation_defaults_tool(context).await)
            }
            "test_elicitation_sep1330_enums" => {
                Some(self.fixture_elicitation_enums_tool(context).await)
            }
            _ => None,
        }
    }

    async fn fixture_logging_tool(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        emit_log(self, context, LoggingLevel::Info, "Tool execution started").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        emit_log(self, context, LoggingLevel::Info, "Tool processing data").await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        emit_log(
            self,
            context,
            LoggingLevel::Info,
            "Tool execution completed",
        )
        .await;
        Ok(success_text("Tool execution completed with logging"))
    }

    async fn fixture_progress_tool(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        if let Some(token) = context.meta.get_progress_token() {
            emit_progress(context, token.clone(), 0.0, "Tool execution started").await;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            emit_progress(context, token.clone(), 50.0, "Tool processing data").await;
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            emit_progress(context, token, 100.0, "Tool execution completed").await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        Ok(success_text("Tool execution completed with progress"))
    }

    async fn fixture_sampling_tool(
        &self,
        arguments: Map<String, Value>,
        context: &RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let prompt = required_arg(&arguments, "prompt")?;
        if !client_supports_sampling(context) {
            return fixture_error("client does not advertise the sampling capability");
        }
        let params = CreateMessageRequestParams::new(vec![SamplingMessage::user_text(prompt)], 100);
        match context.peer.create_message(params).await {
            Ok(result) => {
                let response_text = result
                    .message
                    .content
                    .iter()
                    .filter_map(|content| content.as_text().map(|text| text.text.as_str()))
                    .collect::<Vec<_>>()
                    .join(" ");
                Ok(success_text(format!("LLM response: {response_text}")))
            }
            Err(error) => fixture_error(format!("sampling/createMessage failed: {error}")),
        }
    }

    async fn fixture_elicitation_tool(
        &self,
        arguments: Map<String, Value>,
        context: &RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let message = required_arg(&arguments, "message")?.to_string();
        if !client_supports_elicitation(context) {
            return fixture_error("client does not advertise the elicitation capability");
        }
        let schema = parse_schema(json!({
            "type": "object",
            "properties": {
                "username": {
                    "type": "string",
                    "description": "User's response"
                },
                "email": {
                    "type": "string",
                    "description": "User's email address"
                }
            },
            "required": ["username", "email"]
        }))?;
        match request_form_elicitation(context, message, schema).await {
            Ok(result) => Ok(success_text(format!(
                "User response: {}",
                elicitation_summary(&result)
            ))),
            Err(error) => fixture_error(format!("elicitation/create failed: {error}")),
        }
    }

    async fn fixture_elicitation_defaults_tool(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        if !client_supports_elicitation(context) {
            return fixture_error("client does not advertise the elicitation capability");
        }
        let schema = parse_schema(json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Person's name",
                    "default": "John Doe"
                },
                "age": {
                    "type": "integer",
                    "description": "Age in years",
                    "default": 30
                },
                "score": {
                    "type": "number",
                    "description": "Score",
                    "default": 95.5
                },
                "status": {
                    "type": "string",
                    "enum": ["active", "inactive", "pending"],
                    "default": "active"
                },
                "verified": {
                    "type": "boolean",
                    "description": "Whether the account is verified",
                    "default": true
                }
            },
            "required": ["name", "age", "score", "status", "verified"]
        }))?;
        match request_form_elicitation(
            context,
            "SEP-1034 primitive defaults conformance elicitation".to_string(),
            schema,
        )
        .await
        {
            Ok(result) => Ok(success_text(elicitation_summary(&result))),
            Err(error) => fixture_error(format!("elicitation/create failed: {error}")),
        }
    }

    async fn fixture_elicitation_enums_tool(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        if !client_supports_elicitation(context) {
            return fixture_error("client does not advertise the elicitation capability");
        }
        let schema = parse_schema(json!({
            "type": "object",
            "properties": {
                "untitledSingle": {
                    "type": "string",
                    "enum": ["option1", "option2", "option3"]
                },
                "titledSingle": {
                    "type": "string",
                    "oneOf": [
                        { "const": "value1", "title": "First Option" },
                        { "const": "value2", "title": "Second Option" },
                        { "const": "value3", "title": "Third Option" }
                    ]
                },
                "legacyEnum": {
                    "type": "string",
                    "enum": ["opt1", "opt2", "opt3"],
                    "enumNames": ["Option One", "Option Two", "Option Three"]
                },
                "untitledMulti": {
                    "type": "array",
                    "items": {
                        "type": "string",
                        "enum": ["option1", "option2", "option3"]
                    }
                },
                "titledMulti": {
                    "type": "array",
                    "items": {
                        "anyOf": [
                            { "const": "value1", "title": "First Choice" },
                            { "const": "value2", "title": "Second Choice" },
                            { "const": "value3", "title": "Third Choice" }
                        ]
                    }
                }
            },
            "required": ["untitledSingle", "titledSingle", "legacyEnum", "untitledMulti", "titledMulti"]
        }))?;
        match request_form_elicitation(
            context,
            "SEP-1330 enum variants conformance elicitation".to_string(),
            schema,
        )
        .await
        {
            Ok(result) => Ok(success_text(elicitation_summary(&result))),
            Err(error) => fixture_error(format!("elicitation/create failed: {error}")),
        }
    }
}

pub fn is_fixture_tool(name: &str) -> bool {
    matches!(
        name,
        "test_simple_text"
            | "test_error_handling"
            | "test_image_content"
            | "test_audio_content"
            | "test_embedded_resource"
            | "test_multiple_content_types"
            | "test_tool_with_logging"
            | "test_tool_with_progress"
            | "test_sampling"
            | "test_elicitation"
            | "test_elicitation_sep1034_defaults"
            | "test_elicitation_sep1330_enums"
    )
}
