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
    assert_eq!(count, 99, "legacy clients must see all 99 currently supported tools");
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
fn search_tools_finds_tts_catalog() {
    let meta = modern_meta();
    let call = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
        "name": "artcraft_search_tools",
        "arguments": {"query": "voice clone"},
        "_meta": meta
    }});
    let input = format!("{}\n", serde_json::to_string(&call).unwrap());
    let out = run_stdio(&input);
    assert!(out.contains("artcraft_tts_search_models"), "search should find TTS tools: {}", &out[..out.len().min(500)]);
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

/// MRTR cost gate: a billable generation from an elicitation-capable modern
/// client returns input_required with a sealed requestState; declining the
/// elicitation completes with a cancellation and zero credits spent.
#[test]
fn mrtr_cost_confirmation_decline_path() {
    let meta = modern_meta();
    let call = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
        "name": "artcraft_generate_video",
        "arguments": {"prompt": "a test frog", "model": "seedance_2_fast"},
        "_meta": meta
    }});

    // Probe round 1 alone first to obtain the requestState.
    let probe = serde_json::json!({"jsonrpc":"2.0","id":9,"method":"ping","params":{}});
    let input = format!("{}
{}
", serde_json::to_string(&call).unwrap(), serde_json::to_string(&probe).unwrap());
    let out = run_stdio(&input);
    let msg = out
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .find(|m| m.get("id") == Some(&serde_json::json!(1)))
        .expect("first-round response");
    let result = &msg["result"];
    assert_eq!(
        result["resultType"], serde_json::json!("input_required"),
        "billable generation must gate on confirmation: {}",
        &out[..out.len().min(400)]
    );
    let state = result["requestState"].as_str().expect("sealed requestState").to_string();
    assert!(!state.is_empty());

    // Round 2 against the same server instance: decline the confirmation.
    // (requestState is sealed with a per-process key, as rmcp documents, so the
    // retry must reach the same server instance — exactly what real clients do.)
    let round1 = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
        "name": "artcraft_generate_video",
        "arguments": {"prompt": "a test frog", "model": "seedance_2_fast"},
        "_meta": meta
    }});
    let (r1, r2) = {
        // The sealed state differs per process; obtain it from a live same-process round.
        use std::io::{BufRead, BufReader, Write};
        let mut child = Command::new(binary())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn server");
        let mut stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut reader = BufReader::new(stdout);
        stdin.write_all(format!("{}
", serde_json::to_string(&round1).unwrap()).as_bytes()).unwrap();
        stdin.flush().unwrap();
        let mut line1 = String::new();
        reader.read_line(&mut line1).unwrap();
        let r1: serde_json::Value = serde_json::from_str(&line1).unwrap();
        let live_state = r1["result"]["requestState"].as_str().unwrap().to_string();
        let retry = serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name": "artcraft_generate_video",
            "arguments": {"prompt": "a test frog", "model": "seedance_2_fast"},
            "requestState": live_state,
            "inputResponses": {"confirm": {"action": "decline"}},
            "_meta": meta
        }});
        stdin.write_all(format!("{}
", serde_json::to_string(&retry).unwrap()).as_bytes()).unwrap();
        stdin.flush().unwrap();
        let mut line2 = String::new();
        reader.read_line(&mut line2).unwrap();
        drop(stdin);
        let _ = child.kill();
        let _ = child.wait();
        (r1, serde_json::from_str::<serde_json::Value>(&line2).unwrap())
    };
    assert_eq!(r1["result"]["resultType"], serde_json::json!("input_required"));
    let text = r2.to_string();
    assert!(
        text.contains("cancelled") || text.contains("no credits spent"),
        "declined confirmation must cancel cleanly: {}",
        &text[..text.len().min(500)]
    );
}

/// MRTR state integrity: a tampered requestState is rejected, never executed.
#[test]
fn mrtr_tampered_state_rejected() {
    let meta = modern_meta();
    let retry = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{
        "name": "artcraft_generate_video",
        "arguments": {"prompt": "x", "model": "seedance_2_fast"},
        "requestState": "tampered-not-a-real-state",
        "inputResponses": {"confirm": {"action": "accept", "content": {"confirm": true}}},
        "_meta": meta
    }});
    let input = format!("{}
", serde_json::to_string(&retry).unwrap());
    let out = run_stdio(&input);
    assert!(
        out.contains("tampered or unknown request state") || out.contains("-32602"),
        "tampered state must be rejected: {}",
        &out[..out.len().min(400)]
    );
}

fn response_by_id(out: &str, id: u64) -> serde_json::Value {
    out.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|message| message.get("id") == Some(&serde_json::json!(id)))
        .unwrap_or_else(|| panic!("missing response id {id}; output: {out}"))
}

/// The 2025-era protocol surface used by the official conformance suite:
/// ArtCraft prompt templates, deterministic resources/templates, and completion.
#[test]
fn prompts_resources_and_completion_surface() {
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"prompts/list","params":{}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":3,"method":"prompts/get","params":{"name":"test_prompt_with_arguments","arguments":{"arg1":"alpha","arg2":"beta"}}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":4,"method":"resources/read","params":{"uri":"test://static-text"}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":5,"method":"resources/read","params":{"uri":"test://template/123/data"}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":6,"method":"resources/read","params":{"uri":"test://static-binary"}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":7,"method":"completion/complete","params":{"ref":{"type":"ref/prompt","name":"test_prompt_with_arguments"},"argument":{"name":"arg1","value":"test"}}}"#,
        "\n"
    );
    let out = run_stdio(input);

    let prompts = response_by_id(&out, 2);
    let prompt_names: Vec<&str> = prompts["result"]["prompts"]
        .as_array()
        .expect("prompts array")
        .iter()
        .filter_map(|prompt| prompt["name"].as_str())
        .collect();
    assert!(prompt_names.contains(&"artcraft_image_generation"));
    assert!(prompt_names.contains(&"artcraft_costed_generation_plan"));
    assert!(
        !prompt_names.contains(&"test_simple_prompt"),
        "zero-cost protocol fixtures are callable but not part of the public creative catalog"
    );

    let prompt = response_by_id(&out, 3);
    assert!(prompt["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap()
        .contains("arg1='alpha', arg2='beta'"));

    let text_resource = response_by_id(&out, 4);
    assert_eq!(
        text_resource["result"]["contents"][0]["text"],
        serde_json::json!("This is the content of the static text resource.")
    );
    let template_resource = response_by_id(&out, 5);
    assert_eq!(template_resource["result"]["contents"][0]["mimeType"], serde_json::json!("application/json"));
    assert!(template_resource["result"]["contents"][0]["text"]
        .as_str()
        .unwrap()
        .contains("\"id\":\"123\""));
    let binary_resource = response_by_id(&out, 6);
    assert_eq!(binary_resource["result"]["contents"][0]["mimeType"], serde_json::json!("image/png"));
    assert!(binary_resource["result"]["contents"][0]["blob"].as_str().unwrap().len() > 40);

    let completion = response_by_id(&out, 7);
    let values = completion["result"]["completion"]["values"].as_array().expect("completion values");
    assert!(values.iter().any(|value| value.as_str() == Some("testValue1")));
}

/// Logging level state, filtered logging notifications, progress tokens, and
/// rich zero-cost fixture content are independent of the ArtCraft API.
#[test]
fn protocol_fixture_tools_emit_notifications_and_progress() {
    // Keep stdin open and drive the calls sequentially. Closing stdin
    // immediately is a shutdown signal for rmcp and can cancel slower in-flight
    // notification-bearing requests before their terminal response is written.
    use std::io::{BufRead, BufReader, Write};
    let mut child = Command::new(binary())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn server");
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let mut reader = BufReader::new(stdout);
    let mut lines = Vec::new();

    fn write_request(stdin: &mut impl Write, request: &str) {
        stdin.write_all(request.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }
    fn read_until_id(
        reader: &mut impl BufRead,
        lines: &mut Vec<String>,
        id: u64,
    ) {
        loop {
            let mut line = String::new();
            assert!(reader.read_line(&mut line).unwrap() > 0, "server closed stdout before id {id}");
            let done = serde_json::from_str::<serde_json::Value>(&line)
                .ok()
                .and_then(|message| message.get("id").and_then(|id_value| id_value.as_u64()))
                == Some(id);
            lines.push(line);
            if done {
                return;
            }
        }
    }

    write_request(&mut stdin, r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#);
    write_request(&mut stdin, r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#);
    read_until_id(&mut reader, &mut lines, 1);
    write_request(&mut stdin, r#"{"jsonrpc":"2.0","id":2,"method":"logging/setLevel","params":{"level":"debug"}}"#);
    read_until_id(&mut reader, &mut lines, 2);
    write_request(&mut stdin, r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"test_tool_with_logging","arguments":{}}}"#);
    read_until_id(&mut reader, &mut lines, 3);
    write_request(&mut stdin, r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"test_tool_with_progress","arguments":{},"_meta":{"progressToken":"smoke-progress"}}}"#);
    read_until_id(&mut reader, &mut lines, 4);
    write_request(&mut stdin, r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"test_image_content","arguments":{}}}"#);
    read_until_id(&mut reader, &mut lines, 5);
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();

    let out = lines.join("");
    assert!(response_by_id(&out, 2).get("result").is_some(), "logging/setLevel must succeed: {out}");
    assert!(response_by_id(&out, 3).get("result").is_some());
    assert!(response_by_id(&out, 4).get("result").is_some());

    let messages: Vec<serde_json::Value> = out
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .collect();
    let log_notifications = messages
        .iter()
        .filter(|message| message["method"].as_str() == Some("notifications/message"))
        .count();
    assert!(log_notifications >= 3, "expected at least 3 logging notifications: {out}");
    let progress_values: Vec<f64> = messages
        .iter()
        .filter(|message| message["method"].as_str() == Some("notifications/progress"))
        .filter_map(|message| message["params"]["progress"].as_f64())
        .collect();
    for expected in [0.0, 50.0, 100.0] {
        assert!(progress_values.contains(&expected), "missing progress {expected}: {out}");
    }
    let image = response_by_id(&out, 5);
    assert_eq!(image["result"]["content"][0]["type"], serde_json::json!("image"));
    assert_eq!(image["result"]["content"][0]["mimeType"], serde_json::json!("image/png"));
}
