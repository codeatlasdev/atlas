//! The MCP wire protocol over stdio: versions, ids, errors, batching, schemas, EOF.

use super::*;

#[test]
fn initialize_negotiates_every_supported_version() {
    let d = tempfile::TempDir::new().unwrap();
    for v in ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"] {
        let mut c = McpClient::spawn(d.path());
        let r = c.request("initialize", json!({"protocolVersion": v}));
        assert_eq!(r["result"]["protocolVersion"], v);
        assert!(r["result"]["capabilities"]["tools"].is_object());
        assert_eq!(r["result"]["serverInfo"]["name"], "atlas");
        assert!(
            r["result"]["instructions"]
                .as_str()
                .unwrap()
                .contains("atlas_status")
        );
    }
    let mut c = McpClient::spawn(d.path());
    let r = c.request("initialize", json!({"protocolVersion": "2099-01-01"}));
    assert_eq!(r["result"]["protocolVersion"], "2025-11-25");
    let r = c.request("initialize", json!({}));
    assert_eq!(r["result"]["protocolVersion"], "2025-11-25");
}

#[test]
fn ids_are_echoed_exactly_and_notifications_are_silent() {
    let d = tempfile::TempDir::new().unwrap();
    let mut c = McpClient::spawn(d.path());
    for id in [json!("abc"), json!(42), json!(0), json!(-7), json!("")] {
        c.send(&json!({"jsonrpc":"2.0","id":id,"method":"ping"}));
        let r = c.recv_id(&id);
        assert_eq!(r["id"], id);
        assert_eq!(r["result"], json!({}));
    }
    // Numeric-looking string stays a string.
    c.send(&json!({"jsonrpc":"2.0","id":"7","method":"ping"}));
    let r = c.recv_id(&json!("7"));
    assert!(r["id"].is_string());

    // Notifications (known, unknown, with params): never answered.
    c.notify("notifications/initialized");
    c.notify("notifications/cancelled");
    c.notify("totally/unknown");
    c.send(&json!({"jsonrpc":"2.0","method":"ping"})); // ping without id = notification
    c.assert_silent(Duration::from_millis(400));
    // And the server is still alive.
    assert_eq!(c.request("ping", json!({}))["result"], json!({}));
}

#[test]
fn json_rpc_errors_use_correct_codes() {
    let d = tempfile::TempDir::new().unwrap();
    let mut c = McpClient::spawn(d.path());

    c.send_raw("{not json");
    let r = c.recv_line(RPC_TIMEOUT).unwrap();
    assert_eq!(r["error"]["code"], -32700);
    assert_eq!(r["id"], Value::Null);

    // Valid JSON but not a request object.
    c.send_raw("42");
    let r = c.recv_line(RPC_TIMEOUT).unwrap();
    assert_eq!(r["error"]["code"], -32600);
    assert_eq!(r["id"], Value::Null);

    c.send_raw(r#"{"jsonrpc":"2.0","id":1}"#);
    let r = c.recv_line(RPC_TIMEOUT).unwrap();
    assert_eq!(r["error"]["code"], -32600);
    assert_eq!(r["id"], 1);

    // Bad id type.
    c.send_raw(r#"{"jsonrpc":"2.0","id":{"a":1},"method":"ping"}"#);
    let r = c.recv_line(RPC_TIMEOUT).unwrap();
    assert_eq!(r["error"]["code"], -32600);
    assert_eq!(r["id"], Value::Null);

    // Wrong jsonrpc version.
    c.send_raw(r#"{"jsonrpc":"1.0","id":2,"method":"ping"}"#);
    let r = c.recv_id(&json!(2));
    assert_eq!(r["error"]["code"], -32600);

    // Unknown method.
    let r = c.request("sampling/createMessage", json!({}));
    assert_eq!(r["error"]["code"], -32601);

    // tools/call: missing name, bad arguments type, unknown tool → -32602.
    for (id, params) in [
        (10, json!({})),
        (11, json!({"name": 5})),
        (12, json!({"name":"atlas_logs","arguments":[1]})),
        (13, json!({"name":"does_not_exist"})),
    ] {
        c.send(&json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":params}));
        let r = c.recv_id(&json!(id));
        assert_eq!(r["error"]["code"], -32602, "{r}");
    }

    // Bad tool *arguments* are tool errors (isError), not protocol errors.
    let (err, text) = c.call("atlas_logs", json!({"lines": "many"}));
    assert!(err, "{text}");
    let (err, text) = c.call("atlas_logs", json!({"level": "loud"}));
    assert!(err, "{text}");
    let (err, text) = c.call("atlas_logs_search", json!({}));
    assert!(err && text.contains("query"), "{text}");
    let (err, text) = c.call("atlas_logs_search", json!({"query": "/(/"}));
    assert!(err && text.contains("regex"), "{text}");
    let (err, _) = c.call("atlas_logs", json!({"service": "../etc/passwd"}));
    assert!(err);

    // Invalid UTF-8 on the wire is a parse error, not a crash.
    c.stdin.as_mut().unwrap().write_all(b"\xff\xfe\n").unwrap();
    let r = c.recv_line(RPC_TIMEOUT).unwrap();
    assert_eq!(r["error"]["code"], -32700);
    assert_eq!(c.request("ping", json!({}))["result"], json!({}));
}

#[test]
fn batching_only_with_2025_03_26() {
    let d = tempfile::TempDir::new().unwrap();
    let batch = json!([
        {"jsonrpc":"2.0","id":"a","method":"ping"},
        {"jsonrpc":"2.0","method":"notifications/initialized"},
        {"jsonrpc":"2.0","id":2,"method":"tools/list"}
    ]);

    // Before initialize and under newer/older versions: rejected.
    for v in [
        None,
        Some("2025-11-25"),
        Some("2025-06-18"),
        Some("2024-11-05"),
    ] {
        let mut c = McpClient::spawn(d.path());
        if let Some(v) = v {
            c.request("initialize", json!({"protocolVersion": v}));
        }
        c.send(&batch);
        let r = c.recv_line(RPC_TIMEOUT).unwrap();
        assert_eq!(r["error"]["code"], -32600, "version {v:?}: {r}");
        assert_eq!(r["id"], Value::Null);
        assert_eq!(c.request("ping", json!({}))["result"], json!({}));
    }

    // 2025-03-26: batch answered per message, notifications dropped.
    let mut c = McpClient::spawn(d.path());
    c.request("initialize", json!({"protocolVersion": "2025-03-26"}));
    c.send(&batch);
    let r = c.recv_line(RPC_TIMEOUT).unwrap();
    let arr = r.as_array().expect("batch response is an array");
    assert_eq!(arr.len(), 2);
    assert!(
        arr.iter()
            .any(|m| m["id"] == "a" && m["result"] == json!({}))
    );
    assert!(
        arr.iter()
            .any(|m| m["id"] == 2 && m["result"]["tools"].is_array())
    );
    // All-notification batch → no response at all; empty batch → invalid request.
    c.send(&json!([{"jsonrpc":"2.0","method":"notifications/initialized"}]));
    c.assert_silent(Duration::from_millis(300));
    c.send(&json!([]));
    let r = c.recv_line(RPC_TIMEOUT).unwrap();
    assert_eq!(r["error"]["code"], -32600);
}

#[test]
fn ping_and_tools_work_before_initialize() {
    let d = tempfile::TempDir::new().unwrap();
    let mut c = McpClient::spawn(d.path());
    assert_eq!(c.request("ping", json!({}))["result"], json!({}));
    let r = c.request("tools/list", json!({}));
    // The catalogue's exact list is pinned in atlas-tui (`mcp::tools` tests); here: the tools an
    // agent starts with are listed before `initialize`.
    let names: Vec<&str> = r["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    for want in ["atlas_status", "atlas_logs", "atlas_wait_ready"] {
        assert!(names.contains(&want), "{want} missing from {names:?}");
    }
    let (err, text) = c.call("atlas_status", json!({}));
    assert!(!err, "{text}");
    // Still handshakes fine afterwards, and ping works after too.
    c.handshake();
    assert_eq!(c.request("ping", json!({}))["result"], json!({}));
}

#[test]
fn resources_and_prompts_over_stdio() {
    let d = tempfile::TempDir::new().unwrap();
    let mut c = McpClient::spawn(d.path());
    c.handshake();
    let r = c.request("resources/list", json!({}));
    assert_eq!(r["result"]["resources"].as_array().unwrap().len(), 2);
    let r = c.request(
        "prompts/get",
        json!({"name": "diagnose", "arguments": {"service": "api"}}),
    );
    let text = r["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap();
    assert!(text.contains("\"api\""), "{text}");
}

#[test]
fn eof_exits_zero_promptly_and_stdout_is_json_only() {
    let d = tempfile::TempDir::new().unwrap();
    let mut c = McpClient::spawn(d.path());
    c.handshake();
    c.request("tools/list", json!({}));
    assert_eq!(c.close_and_wait_exit(), 0);
    // No stray non-JSON output was produced (recv_line would have panicked); drain.
    while let Ok(l) = c.rx.try_recv() {
        serde_json::from_str::<Value>(&l).expect("only JSON on stdout");
    }

    // EOF with a request still in flight: the answer is delivered before exit.
    let mut c = McpClient::spawn(d.path());
    c.send(&json!({"jsonrpc":"2.0","id":9,"method":"ping"}));
    assert_eq!(c.close_and_wait_exit(), 0);
    let line = c.rx.recv_timeout(HANG).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["id"], 9);
}

#[test]
fn tools_list_schemas_are_valid_and_described() {
    let d = tempfile::TempDir::new().unwrap();
    let mut c = McpClient::spawn(d.path());
    c.handshake();
    let r = c.request("tools/list", json!({}));
    let tools = r["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for want in [
        "atlas_status",
        "atlas_metrics",
        "atlas_logs",
        "atlas_logs_search",
        "atlas_config",
        "atlas_restart_service",
        "atlas_stop_service",
        "atlas_start_service",
        "atlas_run_action",
        "atlas_wait_ready",
        "atlas_heavy_status",
        "atlas_last_result",
        "atlas_affected",
        "atlas_estimate",
    ] {
        assert!(names.contains(&want), "missing tool {want}");
    }
    let name_ok = |n: &str| {
        (1..=128).contains(&n.len())
            && n.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    };
    let required: &[(&str, &[&str])] = &[
        ("atlas_logs_search", &["query"]),
        ("atlas_restart_service", &["service"]),
        ("atlas_stop_service", &["service"]),
        ("atlas_start_service", &["service"]),
        ("atlas_metrics", &["service"]),
        ("atlas_run_action", &["action"]),
        ("atlas_last_result", &["name"]),
    ];
    // atlas_affected and atlas_estimate have no required key (files/base default to git; action
    // or command is checked by the tool, which names the missing one).
    for t in tools {
        let n = t["name"].as_str().unwrap();
        assert!(name_ok(n), "bad tool name {n}");
        assert!(
            t["description"].as_str().unwrap().len() > 40,
            "{n}: description too thin"
        );
        let s = &t["inputSchema"];
        assert_eq!(s["type"], "object", "{n}");
        let props = s["properties"].as_object().expect("properties object");
        for (k, p) in props {
            assert!(p["type"].is_string(), "{n}.{k} needs a type");
            assert!(
                p["description"].as_str().is_some_and(|d| !d.is_empty()) || p["enum"].is_array(),
                "{n}.{k} needs a description"
            );
        }
        if let Some(req) = s.get("required") {
            for r in req.as_array().unwrap() {
                assert!(
                    props.contains_key(r.as_str().unwrap()),
                    "{n}: required {r} undeclared"
                );
            }
        }
        if let Some((_, want)) = required.iter().find(|(k, _)| *k == n) {
            let got: Vec<&str> = s["required"]
                .as_array()
                .unwrap_or_else(|| panic!("{n}: required missing"))
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            assert_eq!(&got, want, "{n}");
        }
    }
}

#[test]
fn default_dir_is_project_root_found_from_cwd() {
    let p = project();
    let nested = p.root().join("a/b");
    std::fs::create_dir_all(&nested).unwrap();
    let mut c = McpClient::spawn_with(&["mcp"], Some(&nested));
    c.handshake();
    let (err, text) = c.call("atlas_config", json!({}));
    assert!(!err, "{text}");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["project"], "mcp-fixture");
}

#[test]
fn large_logs_response_is_one_valid_json_line() {
    let d = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(d.path().join(".logs")).unwrap();
    let mut body = String::new();
    for i in 0..3000 {
        body.push_str(&format!(
            "12:00:{:02} [stdout] line {i} {}\n",
            i % 60,
            "x".repeat(150)
        ));
    }
    std::fs::write(d.path().join(".logs/big.log"), body).unwrap();
    let mut c = McpClient::spawn(d.path());
    c.handshake();
    let (err, text) = c.call("atlas_logs", json!({"lines": 2000, "service": "big"}));
    assert!(!err);
    assert_eq!(text.lines().count(), 2000);
    assert!(text.len() > 300_000);
    assert!(text.contains("line 2999"));
    assert!(!text.contains("line 999 "), "must be the tail");
}
