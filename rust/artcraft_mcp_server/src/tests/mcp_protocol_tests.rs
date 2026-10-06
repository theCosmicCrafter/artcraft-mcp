use serde_json::{json, Value};

use crate::mcp_protocol::{
  CallToolParams, CallToolResult, JsonRpcRequest, JsonRpcResponse, ListToolsResult, McpTool,
  ServerInfo,
};

#[test]
fn jsonrpc_request_round_trips() {
  let raw = json!({
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/list",
    "params": null
  });

  let req: JsonRpcRequest = serde_json::from_value(raw).unwrap();
  assert_eq!(req.jsonrpc, "2.0");
  assert_eq!(req.id, Some(Value::Number(1.into())));
  assert_eq!(req.method, "tools/list");
}

#[test]
fn jsonrpc_response_serializes_success() {
  let response = JsonRpcResponse::success(Value::Number(7.into()), Value::String("hello".into()));
  let value = serde_json::to_value(response).unwrap();

  assert_eq!(value["jsonrpc"], "2.0");
  assert_eq!(value["id"], 7);
  assert_eq!(value["result"], "hello");
  assert!(value.get("error").is_none());
}

#[test]
fn jsonrpc_response_serializes_error() {
  let response = JsonRpcResponse::error(Value::Null, -32603, "Internal error");
  let value = serde_json::to_value(response).unwrap();

  assert_eq!(value["jsonrpc"], "2.0");
  assert_eq!(value["id"], Value::Null);
  assert_eq!(value["error"]["code"], -32603);
  assert_eq!(value["error"]["message"], "Internal error");
}

#[test]
fn list_tools_result_contains_expected_fields() {
  let tools = vec![McpTool {
    name: "get_credits".to_string(),
    description: "Get credits".to_string(),
    input_schema: json!({"type": "object"}),
  }];

  let result = ListToolsResult { tools };
  let value = serde_json::to_value(result).unwrap();

  assert!(value["tools"].is_array());
  assert_eq!(value["tools"][0]["name"], "get_credits");
  assert_eq!(value["tools"][0]["description"], "Get credits");
}

#[test]
fn call_tool_params_parses_arguments() {
  let raw = json!({
    "name": "generate_image",
    "arguments": {
      "prompt": "a corgi"
    }
  });

  let params: CallToolParams = serde_json::from_value(raw).unwrap();
  assert_eq!(params.name, "generate_image");
  let args = params.arguments.as_ref().unwrap();
  assert_eq!(args["prompt"], "a corgi");
}

#[test]
fn mcp_tool_result_serializes_content() {
  use crate::mcp_protocol::McpContent;

  let result = CallToolResult {
    content: vec![McpContent::Text { text: "hello".to_string() }],
    is_error: Some(false),
  };

  let value = serde_json::to_value(result).unwrap();
  assert_eq!(value["content"][0]["text"], "hello");
  assert_eq!(value["isError"], false);
}

#[test]
fn server_info_uses_expected_name_and_version() {
  let info = ServerInfo {
    name: "artcraft-mcp-server".to_string(),
    version: "0.1.0".to_string(),
  };
  let value = serde_json::to_value(info).unwrap();
  assert_eq!(value["name"], "artcraft-mcp-server");
  assert_eq!(value["version"], "0.1.0");
}
