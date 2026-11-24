//! What the tools answer when no `atlas dev` run is up: read tools work, control tools say so.

use super::*;

#[test]
fn not_running_read_tools_work_and_control_tools_error() {
    let p = project();
    std::fs::create_dir_all(p.logs()).unwrap();
    std::fs::write(
        p.logs().join("api.log"),
        "10:00:01 [stdout] boot ok\n10:00:05 [stderr] ERROR db down\n10:00:06 [stdout] WARN slow query\n10:00:09 [stdout] ready\n",
    )
    .unwrap();
    std::fs::write(
        p.logs().join("api.prev.log"),
        "09:00:00 [stdout] old ERROR boom\n",
    )
    .unwrap();
    std::fs::write(p.logs().join("web.log"), "10:00:03 [stdout] vite up\n").unwrap();

    let mut c = McpClient::spawn(p.root());
    c.handshake();

    let st = status(&mut c);
    assert_eq!(st["running"], false);
    assert!(st["hint"].as_str().unwrap().contains("not running"));

    let (err, text) = c.call("atlas_logs", json!({}));
    assert!(!err);
    assert!(
        text.contains("[api:2]") && text.contains("[web:1]"),
        "{text}"
    );
    assert!(
        !text.contains("old ERROR"),
        "prev log must not leak into tail"
    );
    let (_, text) = c.call("atlas_logs", json!({"level": "error"}));
    assert!(
        text.contains("db down") && !text.contains("boot ok"),
        "{text}"
    );
    let (_, text) = c.call("atlas_logs", json!({"level": "warn"}));
    assert!(
        text.contains("db down") && text.contains("slow query"),
        "{text}"
    );
    let (_, text) = c.call("atlas_logs", json!({"service": "api", "lines": 1}));
    assert_eq!(text.lines().count(), 1);
    assert!(text.contains("ready"));
    let (err, text) = c.call("atlas_logs", json!({"service": "nope"}));
    assert!(
        err && text.contains("api") && text.contains("web"),
        "{text}"
    );

    let (_, text) = c.call("atlas_logs_search", json!({"query": "error"}));
    assert!(
        text.contains("db down") && !text.contains("old ERROR"),
        "{text}"
    );
    let (_, text) = c.call(
        "atlas_logs_search",
        json!({"query": "/db (down|up)/", "service": "api"}),
    );
    assert!(text.contains("db down"), "{text}");
    let (_, text) = c.call(
        "atlas_logs_search",
        json!({"query": "boom", "include_previous": true}),
    );
    assert!(
        text.contains("old ERROR boom") && text.contains("prev run"),
        "{text}"
    );
    let (_, text) = c.call("atlas_logs_search", json!({"query": "boom"}));
    assert_eq!(text, "(no matches)");
    let (err, text) = c.call(
        "atlas_logs_search",
        json!({"query": "x", "service": "nope"}),
    );
    assert!(err && text.contains("api"), "{text}");

    let (err, text) = c.call("atlas_config", json!({}));
    assert!(!err, "{text}");
    let cfg: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(cfg["project"], "mcp-fixture");
    let svcs: Vec<&str> = cfg["services"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(svcs, ["ticker", "web"]);
    let wipe = cfg["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["label"] == "Wipe")
        .unwrap();
    assert_eq!(wipe["confirm"], "Wipe all data?");

    for (tool, args) in [
        ("atlas_restart_service", json!({"service":"web"})),
        ("atlas_stop_service", json!({"service":"web"})),
        ("atlas_start_service", json!({"service":"web"})),
        ("atlas_run_action", json!({"action":"Hello"})),
    ] {
        let (err, text) = c.call(tool, args);
        assert!(err, "{tool}");
        assert_eq!(text, "atlas dev is not running; start it in a terminal");
    }
    let (err, text) = c.call("atlas_wait_ready", json!({"timeout_s": 1}));
    assert!(err && text.contains("not running"), "{text}");
}

/// No atlas.yaml is not an error any more (the stack is detected in memory and the tool says so); a broken
/// atlas.yaml still is: detection never papers over a file the user wrote.
#[test]
fn config_tool_detects_without_a_file_and_reports_invalid_yaml_as_tool_error() {
    let d = tempfile::TempDir::new().unwrap();
    let mut c = McpClient::spawn(d.path());
    c.handshake();
    let (err, text) = c.call("atlas_config", json!({}));
    assert!(!err, "{text}");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["source"], "detected", "{text}");
    std::fs::write(d.path().join("atlas.yaml"), "name: [unclosed").unwrap();
    let (err, _) = c.call("atlas_config", json!({}));
    assert!(err);
}

// Issue #128: an agent decides what to run without starting a process. End to end over the real
// stdio server: the project's `affected:` rules answer `atlas_affected`, and `atlas_estimate`
// answers from a cache entry and a heavy history recorded by the real `atlas run --cache`, with
// the cache and the queue isolated in temp dirs.
#[cfg(unix)]
#[test]
fn affected_and_estimate_answer_from_data_without_running_anything() {
    let d = tempfile::TempDir::new().unwrap();
    let root = d.path().join("proj");
    let (cache, heavy) = (d.path().join("cache"), d.path().join("heavy"));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(&heavy).unwrap();
    let marker = root.join("ran-by-estimate");
    std::fs::write(root.join("src/lib.rs"), "1").unwrap();
    std::fs::write(
        root.join("atlas.yaml"),
        format!(
            "name: proj\nservices:\n  api: {{ command: run }}\nactions:\n  - key: u\n    label: Unit tests\n    command: \"echo unit && touch {m}\"\n    cache: {{ inputs: ['src/**'] }}\naffected:\n  rules:\n    - {{ match: ['src/**'], tasks: [Unit tests], tests: ['cargo test'], routes: [/api], builds: [core] }}\n",
            m = marker.display()
        ),
    )
    .unwrap();
    let envs = [
        ("ATLAS_CACHE_DIR", cache.to_str().unwrap()),
        ("ATLAS_HEAVY_DIR", heavy.to_str().unwrap()),
        // `run_with` names the CLI this machine uses: the one under test, never an installed one.
        ("ATLAS_BIN", BIN),
    ];
    let mut c = McpClient::spawn_with_env(&["mcp", "--dir", root.to_str().unwrap()], None, &envs);
    c.handshake();

    // 1. what does editing src/lib.rs affect? Declared rules answer; nothing is started.
    let (err, text) = c.call("atlas_affected", json!({"files": ["src/lib.rs"]}));
    assert!(!err, "{text}");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["source"], "rules");
    assert_eq!(v["tasks"], json!(["Unit tests"]));
    assert_eq!(v["tests"], json!(["cargo test"]));
    assert_eq!(v["routes"], json!(["/api"]));
    assert_eq!(v["builds"], json!(["core"]));

    // 2. cost of the task: never ran, so no number is invented, and it is not a hit.
    let (err, text) = c.call("atlas_estimate", json!({"action": "Unit tests"}));
    assert!(!err, "{text}");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["runs"], 0);
    assert!(v["median_ms"].is_null());
    assert_eq!(v["would_hit"], false);
    assert!(!marker.exists(), "estimate must not run the action");

    // 3. run it for real through the cache (what the estimate's `run_with` says), then ask again.
    let line = v["run_with"].as_str().unwrap().to_string();
    let mut run = std::process::Command::new("sh");
    run.args(["-c", &line])
        .current_dir(&root)
        .envs(envs.iter().copied());
    assert!(run.status().unwrap().success(), "{line}");
    assert!(marker.exists(), "the run really ran");
    std::fs::remove_file(&marker).unwrap();

    let (err, text) = c.call("atlas_estimate", json!({"action": "Unit tests"}));
    assert!(!err, "{text}");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["would_hit"], true, "{v}");
    assert_eq!(v["runs"], 1);
    assert!(v["median_ms"].as_u64().is_some());
    assert!(v["saved_ms"].as_u64().is_some());
    assert!(!marker.exists(), "asking is not running");

    // 4. an edit to an input flips it to a miss.
    std::fs::write(root.join("src/lib.rs"), "2").unwrap();
    let (_, text) = c.call("atlas_estimate", json!({"action": "Unit tests"}));
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["would_hit"], false);

    // 5. errors are tool errors the agent can act on.
    let (err, text) = c.call("atlas_estimate", json!({}));
    assert!(err && text.contains("action"), "{text}");
    let (err, text) = c.call("atlas_affected", json!({"files": ["../x"]}));
    assert!(err && text.contains("outside"), "{text}");
}
