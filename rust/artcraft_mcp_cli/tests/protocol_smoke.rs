//! Integration smoke tests for the v2 protocol surface. These exercise the
//! binary end-to-end over stdio: legacy handshake regression, progressive
//! discovery for modern clients, meta-tool behavior, and coercion.

use std::io::Write;
use std::process::{Command, Stdio};

fn binary() -> String {
    // cargo sets CARGO_BIN_EXE_<name> for integration tests.
    env!("CARGO_BIN_EXE_artcraft-mcp").to_string()
}

fn run_stdio(input: &str) -> String {
    let mut child = Command::new(binary())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn server");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(input.as_bytes())
        .expect("write stdin");
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("wait");
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn modern_meta() -> serde_json::Value {
    serde_json::json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "1"},
        "io.modelcontextprotocol/clientCapabilities": {"elicitation": {}, "extensions": {}}
    })
}

#[test]
fn legacy_client_gets_full_catalog() {
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#,
        "\n"
    );
    let out = run_stdio(input);
    let tools_line = out
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|m| m.get("id") == Some(&serde_json::json!(2)))
        .expect("tools/list response");
    let count = tools_line["result"]["tools"].as_array().unwrap().len();
    assert_eq!(count, 61, "legacy clients must see all 61 tools");
}

#[test]
fn modern_client_gets_progressive_catalog() {
    let meta = modern_meta();
    let list = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta": meta}});
    let input = format!("{}\n", serde_json::to_string(&list).unwrap());
    let out = run_stdio(&input);
    let msg = out
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|m| m.get("id") == Some(&serde_json::json!(1)))
        .expect("tools/list response");
    let tools = msg["result"]["tools"].as_array().unwrap();
    assert!(tools.len() <= 13, "progressive catalog should be small, got {}", tools.len());
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    for required in ["artcraft_search_tools", "artcraft_describe_tool", "artcraft_call_deferred_tool", "artcraft_generate_image", "artcraft_get_credits"] {
        assert!(names.contains(&required), "missing hot/meta tool {}", required);
    }
    assert_eq!(msg["result"]["_meta"]["ttlMs"], serde_json::json!(300000));
}

#[test]
fn search_tools_finds_voice_catalog() {
    let meta = modern_meta();
    let call = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
        "name": "artcraft_search_tools",
        "arguments": {"query": "voice clone"},
        "_meta": meta
    }});
    let input = format!("{}\n", serde_json::to_string(&call).unwrap());
    let out = run_stdio(&input);
    assert!(out.contains("artcraft_create_voice"), "search should find voice tools: {}", &out[..out.len().min(500)]);
}

#[test]
fn coercion_meta_tool_rejects_unknown_tool() {
    let meta = modern_meta();
    let call = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
        "name": "artcraft_call_deferred_tool",
        "arguments": {"name": "artcraft_nonexistent", "arguments": {}},
        "_meta": meta
    }});
    let input = format!("{}\n", serde_json::to_string(&call).unwrap());
    let out = run_stdio(&input);
    assert!(out.contains("unknown tool") || out.contains("isError") || out.contains("is_error"), "expected error: {}", &out[..out.len().min(400)]);
}
