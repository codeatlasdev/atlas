//! What an engine's agent process is actually started with: its environment and argv, observed from
//! a fake CLI that dumps both. Each test runs the real engine module through `run_turn`.

use std::collections::BTreeMap;
use std::path::Path;

use super::{Exec, TEST_ENGINE_PROGRAM, TurnCancel, TurnCtx, run_turn};
use crate::crew::store::{CrewmateSession, EngineConfig};

/// The hook endpoint the turn is given, and the values a hostile crewmate secret tries to put there.
const HOOK_PORT: u16 = 4711;
const HOOK_TOKEN: &str = "real-hook-token";
const PANE_KEY: &str = "crew:c1:t1";

/// Names hostd owns: a crewmate secret with one of these must never reach the agent.
const OWNED: [&str; 5] = [
    "ATLAS_HOOK_PORT",
    "ATLAS_HOOK_TOKEN",
    "ATLAS_PANE_KEY",
    "ATLAS_CREWMATE_ID",
    "ATLAS_HOOK_DECISION_SECS",
];

/// A fake CLI for `kind`: dumps `env` to `<dir>/env.txt` and its argv (one per line) to
/// `<dir>/argv.txt`, then answers the way `kind`'s module expects so the turn finishes.
fn fake_cli(dir: &Path, kind: &str) -> std::path::PathBuf {
    let reply = match kind {
        // omp: RPC on stdin/stdout. Read the prompt, finish it, answer get_state.
        "omp" => concat!(
            "read -r _prompt\n",
            r#"printf '{"type":"agent_end","messages":[{"role":"assistant","content":"ok"}]}\n'"#,
            "\n",
            r#"printf '{"type":"prompt_result","id":"r1"}\n'"#,
            "\n",
            "read -r _state\n",
            r#"printf '{"type":"response","id":"g","data":{"sessionFile":"/s.jsonl"}}\n'"#,
            "\n",
        )
        .to_string(),
        "claude" => {
            r#"printf '{"type":"result","result":"ok","session_id":"s1","is_error":false}\n'"#
                .to_string()
        }
        "codex" => {
            // The reply travels through `--output-last-message <file>`.
            "while [ $# -gt 0 ]; do [ \"$1\" = --output-last-message ] && echo ok > \"$2\"; shift; done\n\
             printf '{\"type\":\"thread.started\",\"thread_id\":\"th1\"}\\n'"
                .to_string()
        }
        _ => r#"printf '{"result":"ok","session_id":"s1"}\n'"#.to_string(),
    };
    let path = dir.join(format!("fake-{kind}"));
    std::fs::write(
        &path,
        format!(
            "#!/bin/sh\n/usr/bin/env > \"{d}/env.txt\"\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > \"{d}/argv.txt\"\n{reply}\n",
            d = dir.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// What the fake CLI saw: its environment as a map, and its argv.
struct Seen {
    env: BTreeMap<String, String>,
    argv: Vec<String>,
}

/// Run one turn of `kind` with `secrets` as the crewmate's env, approval `ask` (`yolo == false`)
/// unless `yolo`, and return what its process was started with.
async fn run_kind(kind: &str, secrets: &[(&str, &str)], yolo: bool) -> Seen {
    run_kind_on(kind, secrets, yolo, Where::Local).await
}

/// Where a test runs its turn: as a child of this process, or as the crewmate's user through a real
/// spawn helper on a socket (the rig runs it as the current user; everything between hostd's client and
/// the child is the real code).
#[derive(Clone, Copy)]
enum Where {
    Local,
    Helper,
}

async fn run_kind_on(kind: &str, secrets: &[(&str, &str)], yolo: bool, at: Where) -> Seen {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_cli(tmp.path(), kind);
    let _lock = crate::crew::runner::TEST_BIN_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    // Through the helper the program is the bare engine name found in the rig's engine directory (the
    // helper never takes a path from the request); locally it is the fake's path.
    let rig = matches!(at, Where::Helper).then(crate::crew::spawn::testing::rig);
    let exec = match &rig {
        Some(r) => {
            std::fs::copy(&bin, r.engines.join(kind)).unwrap();
            Exec::isolated(r.client.clone(), crate::crew::spawn::testing::USER.into())
        }
        None => {
            *TEST_ENGINE_PROGRAM.lock() = Some((kind.into(), bin.to_string_lossy().into_owned()));
            Exec::local()
        }
    };
    let extra_env: BTreeMap<String, String> = secrets
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let work = match &rig {
        Some(r) => r.work(),
        None => tmp.path().join("work"),
    };
    std::fs::create_dir_all(&work).unwrap();
    let engine = EngineConfig {
        kind: kind.into(),
        provider: None,
        base_url: None,
        model: None,
    };
    let crewmate_dir = tmp.path().join("crewmate");
    let out = run_turn(TurnCtx {
        engine: &engine,
        session: CrewmateSession::default(),
        prompt: "do it",
        crewmate_dir: &crewmate_dir,
        crewmate_id: "c1",
        hook_port: HOOK_PORT,
        hook_token: HOOK_TOKEN,
        pane_key: PANE_KEY,
        extra_env: &extra_env,
        work_dir: work.to_str().unwrap(),
        yolo,
        tools: None,
        cancel: &TurnCancel::new(),
        exec: &exec,
    })
    .await;
    *TEST_ENGINE_PROGRAM.lock() = None;
    out.unwrap_or_else(|e| panic!("{kind}: the turn failed: {e}"));
    let env = std::fs::read_to_string(tmp.path().join("env.txt"))
        .unwrap()
        .lines()
        .filter_map(|l| {
            l.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect();
    let argv = std::fs::read_to_string(tmp.path().join("argv.txt"))
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    Seen { env, argv }
}

// Regression: omp set ATLAS_HOOK_PORT/TOKEN/PANE_KEY BEFORE the crewmate's secrets, so a secret with
// one of those names pointed the approval extension at an endpoint of the crewmate's choosing (claude
// already set them after: "ATLAS_* always win"). Every engine must give hostd's values.
async fn hostd_env_beats_crewmate_secrets(kind: &str, yolo: bool) {
    let secrets: Vec<(&str, &str)> = OWNED.iter().map(|k| (*k, "from-a-secret")).collect();
    let seen = run_kind(kind, &secrets, yolo).await;
    let at = |k: &str| seen.env.get(k).map(String::as_str);
    assert_eq!(
        at("ATLAS_HOOK_PORT"),
        Some(HOOK_PORT.to_string().as_str()),
        "{kind} (yolo={yolo})"
    );
    assert_eq!(
        at("ATLAS_HOOK_TOKEN"),
        Some(HOOK_TOKEN),
        "{kind} (yolo={yolo})"
    );
    assert_eq!(at("ATLAS_PANE_KEY"), Some(PANE_KEY), "{kind} (yolo={yolo})");
    assert_eq!(at("ATLAS_CREWMATE_ID"), Some("c1"), "{kind} (yolo={yolo})");
    assert_eq!(
        at("ATLAS_HOOK_DECISION_SECS"),
        Some("1800"),
        "{kind} (yolo={yolo})"
    );
}

#[tokio::test]
async fn omp_ask_hostd_env_beats_crewmate_secrets() {
    hostd_env_beats_crewmate_secrets("omp", false).await;
}

#[tokio::test]
async fn omp_auto_hostd_env_beats_crewmate_secrets() {
    hostd_env_beats_crewmate_secrets("omp", true).await;
}

#[tokio::test]
async fn claude_hostd_env_beats_crewmate_secrets() {
    hostd_env_beats_crewmate_secrets("claude", false).await;
}

#[tokio::test]
async fn codex_hostd_env_beats_crewmate_secrets() {
    hostd_env_beats_crewmate_secrets("codex", true).await;
}

#[tokio::test]
async fn opencode_hostd_env_beats_crewmate_secrets() {
    hostd_env_beats_crewmate_secrets("opencode", true).await;
}

// ── The atlas MCP server reaches every engine ───────────────────────────────────────────────────────
// Gap: codex and opencode got no ATLAS_* env and no MCP config, so a headless codex/opencode crewmate
// had none of the crew tools (ask_user, set_ui, capture, locate), and a UI task there could never
// attach captures: the UI gate parked it forever.

/// The env the atlas MCP server must be started with (what `crew/mcp.rs` reads).
fn expected_mcp_env() -> BTreeMap<String, String> {
    [
        ("ATLAS_HOOK_PORT", HOOK_PORT.to_string()),
        ("ATLAS_HOOK_TOKEN", HOOK_TOKEN.to_string()),
        ("ATLAS_CREWMATE_ID", "c1".to_string()),
        ("ATLAS_PANE_KEY", PANE_KEY.to_string()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

#[tokio::test]
async fn codex_gets_the_atlas_mcp_server_with_its_env_and_auto_approval() {
    let seen = run_kind("codex", &[], true).await;
    // The overrides as codex parses them: `-c key=value`, value TOML.
    let overrides: BTreeMap<&str, &str> = seen
        .argv
        .windows(2)
        .filter(|w| w[0] == "-c")
        .filter_map(|w| w[1].split_once('='))
        .collect();
    let get = |k: &str| {
        *overrides
            .get(format!("mcp_servers.atlas.{k}").as_str())
            .unwrap_or_else(|| panic!("no -c mcp_servers.atlas.{k} in {:?}", seen.argv))
    };
    let exe = std::env::current_exe().unwrap();
    assert_eq!(get("command"), format!("{:?}", exe.to_string_lossy()));
    assert_eq!(get("args"), r#"["mcp","--crewmate","c1"]"#);
    // codex exec has nobody to ask: without this every MCP call is "user cancelled MCP tool call".
    assert_eq!(get("default_tools_approval_mode"), r#""approve""#);
    // The env, parsed back as TOML, is exactly what the MCP server reads.
    let env: BTreeMap<String, String> = get("env")
        .trim_matches(|c| c == '{' || c == '}')
        .split(',')
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap();
            (
                k.trim_matches('"').to_string(),
                v.trim_matches('"').to_string(),
            )
        })
        .collect();
    assert_eq!(env, expected_mcp_env());
    // The prompt is still the last argument, after every override.
    assert_eq!(seen.argv.last().map(String::as_str), Some("do it"));
}

#[tokio::test]
async fn opencode_gets_the_atlas_mcp_server_in_its_inline_config() {
    let seen = run_kind("opencode", &[], true).await;
    let config: serde_json::Value = serde_json::from_str(
        seen.env
            .get("OPENCODE_CONFIG_CONTENT")
            .unwrap_or_else(|| panic!("no OPENCODE_CONFIG_CONTENT in {:?}", seen.env.keys())),
    )
    .unwrap();
    // The shape both opencode majors load: V2 reads `mcp.<name>` natively (opencode.ai/docs/migrate-v1),
    // V1 cannot read `mcp.servers` (it would take "servers" for a server and run without the crew
    // tools, silently). `type: local`, `command` is one array, `environment` is a string map.
    assert!(
        config["mcp"].get("servers").is_none(),
        "V1 would read `servers` as a server name and start without the crew tools: {config}"
    );
    let server = &config["mcp"]["atlas"];
    assert_eq!(server["type"], "local", "{config}");
    let exe = std::env::current_exe().unwrap();
    assert_eq!(
        server["command"],
        serde_json::json!([exe.to_string_lossy(), "mcp", "--crewmate", "c1"]),
        "{config}"
    );
    let env: BTreeMap<String, String> =
        serde_json::from_value(server["environment"].clone()).unwrap();
    assert_eq!(env, expected_mcp_env());
}

// A crewmate secret named OPENCODE_CONFIG_CONTENT must not replace the config that carries the tools.
#[tokio::test]
async fn a_crewmate_secret_cannot_replace_opencodes_inline_config() {
    let seen = run_kind("opencode", &[("OPENCODE_CONFIG_CONTENT", "{}")], true).await;
    assert!(
        seen.env["OPENCODE_CONFIG_CONTENT"].contains("atlas"),
        "the secret replaced the atlas MCP config: {}",
        seen.env["OPENCODE_CONFIG_CONTENT"]
    );
}

// ── An isolated crewmate's turn is the same turn, started as its own user ────────────────────────────
// Issue #200: the helper is an executor behind `launch`, not a second launcher. Everything below runs the
// real engine module through `run_turn` with `Exec::isolated` against a real helper on a socket, so the
// env precedence, the MCP config and the cancel handling are the ones of the local path by construction.

async fn hostd_env_beats_secrets_through_the_helper(kind: &str, yolo: bool) {
    let secrets: Vec<(&str, &str)> = OWNED.iter().map(|k| (*k, "from-a-secret")).collect();
    let seen = run_kind_on(kind, &secrets, yolo, Where::Helper).await;
    let at = |k: &str| seen.env.get(k).map(String::as_str);
    assert_eq!(
        at("ATLAS_HOOK_PORT"),
        Some(HOOK_PORT.to_string().as_str()),
        "{kind}"
    );
    assert_eq!(at("ATLAS_HOOK_TOKEN"), Some(HOOK_TOKEN), "{kind}");
    assert_eq!(at("ATLAS_PANE_KEY"), Some(PANE_KEY), "{kind}");
    assert_eq!(at("ATLAS_CREWMATE_ID"), Some("c1"), "{kind}");
    assert_eq!(at("ATLAS_HOOK_DECISION_SECS"), Some("1800"), "{kind}");
}

// Regression guard for the duplicated launcher: #223 had its own env code in `proc.rs`, where a secret
// named ATLAS_HOOK_PORT could win. One `TurnCtx::command` now serves both executors.
#[tokio::test(flavor = "multi_thread")]
async fn an_isolated_claude_turn_still_gets_hostds_env_over_a_secret_of_the_same_name() {
    hostd_env_beats_secrets_through_the_helper("claude", false).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_isolated_omp_turn_still_gets_hostds_env_over_a_secret_of_the_same_name() {
    hostd_env_beats_secrets_through_the_helper("omp", false).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_isolated_codex_turn_still_gets_hostds_env_over_a_secret_of_the_same_name() {
    hostd_env_beats_secrets_through_the_helper("codex", true).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_isolated_opencode_turn_still_gets_hostds_env_over_a_secret_of_the_same_name() {
    hostd_env_beats_secrets_through_the_helper("opencode", true).await;
}

// #223 predates the MCP server for codex and opencode: an isolated codex/opencode crewmate had none of
// the crew tools (ask_user, set_ui, capture, locate), so a UI task could never attach captures.
#[tokio::test(flavor = "multi_thread")]
async fn an_isolated_codex_turn_gets_the_atlas_mcp_server_with_auto_approval() {
    let seen = run_kind_on("codex", &[], true, Where::Helper).await;
    let overrides: BTreeMap<&str, &str> = seen
        .argv
        .windows(2)
        .filter(|w| w[0] == "-c")
        .filter_map(|w| w[1].split_once('='))
        .collect();
    let get = |k: &str| {
        *overrides
            .get(format!("mcp_servers.atlas.{k}").as_str())
            .unwrap_or_else(|| panic!("no -c mcp_servers.atlas.{k} in {:?}", seen.argv))
    };
    assert_eq!(get("args"), r#"["mcp","--crewmate","c1"]"#);
    assert_eq!(get("default_tools_approval_mode"), r#""approve""#);
    assert!(get("env").contains(HOOK_TOKEN), "{}", get("env"));
    assert_eq!(seen.argv.last().map(String::as_str), Some("do it"));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_isolated_opencode_turn_gets_the_atlas_mcp_server_and_a_secret_cannot_replace_it() {
    let seen = run_kind_on(
        "opencode",
        &[("OPENCODE_CONFIG_CONTENT", "{}")],
        true,
        Where::Helper,
    )
    .await;
    let config: serde_json::Value =
        serde_json::from_str(&seen.env["OPENCODE_CONFIG_CONTENT"]).unwrap();
    assert_eq!(config["mcp"]["atlas"]["type"], "local", "{config}");
    assert_eq!(
        config["mcp"]["atlas"]["environment"]["ATLAS_HOOK_TOKEN"], HOOK_TOKEN,
        "{config}"
    );
}

/// Whether `pid` is gone (a zombie counts: its process is dead and only waits for a reaper).
fn gone(pid: u32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat
            .rsplit(')')
            .next()
            .is_some_and(|r| r.trim_start().starts_with('Z')),
        Err(_) => !atlas_core::sys::pid_alive(pid),
    }
}

/// An isolated `claude` turn whose fake starts a long-running grandchild (a tool), writes the
/// grandchild's pid to a file and waits. Returns the running turn, its cancel and the pid.
struct Hanging {
    turn: tokio::task::JoinHandle<Result<super::TurnResult, String>>,
    cancel: TurnCancel,
    pid: u32,
    _rig: crate::crew::spawn::testing::Rig,
    _tmp: tempfile::TempDir,
    // `program()` reads the process-wide fake: no other test may have one set while this runs.
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

/// `None` when `timeout` (a test's short turn timeout) passed before the engine got to start its tool.
async fn try_start_hanging_isolated_turn(timeout: Option<std::time::Duration>) -> Option<Hanging> {
    let lock = crate::crew::runner::TEST_BIN_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    // The turn reads its timeout when it starts waiting; the lock keeps other engine tests out meanwhile.
    *super::TEST_TURN_TIMEOUT.lock() = timeout;
    let rig = crate::crew::spawn::testing::rig();
    let tmp = tempfile::tempdir().unwrap();
    let pidfile = tmp.path().join("tool.pid");
    rig.engine(
        "claude",
        &format!(
            "(/bin/sleep 600 & echo $! > \"{}\"; wait) &\nwait",
            pidfile.display()
        ),
    );
    let exec = Exec::isolated(rig.client.clone(), crate::crew::spawn::testing::USER.into());
    let cancel = TurnCancel::new();
    let work = rig.work().to_string_lossy().into_owned();
    let dir = tmp.path().to_path_buf();
    let turn = {
        let cancel = cancel.clone();
        tokio::spawn(async move {
            let engine = EngineConfig {
                kind: "claude".into(),
                provider: None,
                base_url: None,
                model: None,
            };
            let env = BTreeMap::new();
            run_turn(TurnCtx {
                engine: &engine,
                session: CrewmateSession::default(),
                prompt: "go",
                crewmate_dir: &dir,
                crewmate_id: "c1",
                hook_port: HOOK_PORT,
                hook_token: HOOK_TOKEN,
                pane_key: PANE_KEY,
                extra_env: &env,
                work_dir: &work,
                yolo: true,
                tools: None,
                cancel: &cancel,
                exec: &exec,
            })
            .await
        })
    };
    let start = std::time::Instant::now();
    let pid = loop {
        if let Ok(s) = std::fs::read_to_string(&pidfile)
            && let Ok(pid) = s.trim().parse::<u32>()
        {
            break pid;
        }
        if turn.is_finished() {
            return None;
        }
        assert!(
            start.elapsed() < atlas_core::scaling::HANG,
            "the isolated engine never started its tool"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };
    // With a short turn timeout the turn may already be over: that is what the test then waits for.
    assert!(
        timeout.is_some() || !gone(pid),
        "the tool must be running before the cancel"
    );
    Some(Hanging {
        turn,
        cancel,
        pid,
        _rig: rig,
        _tmp: tmp,
        _lock: lock,
    })
}

/// [`try_start_hanging_isolated_turn`], again for as long as a short `timeout` beats a starved machine to the
/// start of the tool (the 3 s are for the turn, not a budget for the engine to start in).
async fn start_hanging_isolated_turn(timeout: Option<std::time::Duration>) -> Hanging {
    for _ in 0..20 {
        if let Some(h) = try_start_hanging_isolated_turn(timeout).await {
            return h;
        }
        assert!(
            timeout.is_some(),
            "the isolated turn ended before it started its tool"
        );
    }
    panic!("the isolated engine never started its tool within the {timeout:?} turn timeout");
}

async fn wait_gone(pid: u32, what: &str) {
    let start = std::time::Instant::now();
    while !gone(pid) {
        assert!(
            start.elapsed() < atlas_core::scaling::HANG,
            "{what}: the tool process {pid} outlived the turn"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

// #290 for the helper path: a cancel must end the whole tree of an isolated turn, not only the engine.
// The process tree lives in another uid on a real server, so hostd can only reach it through the helper.
#[tokio::test(flavor = "multi_thread")]
async fn cancelling_an_isolated_turn_kills_the_tool_process_the_engine_started() {
    let h = start_hanging_isolated_turn(None).await;
    h.cancel.fire();
    let err = h.turn.await.unwrap().err().expect("a cancelled turn fails");
    assert!(err.contains("cancelled"), "{err}");
    wait_gone(h.pid, "cancel").await;
}

// A turn future dropped by the task timeout must not leave an isolated agent running (and spending) either.
#[tokio::test(flavor = "multi_thread")]
async fn an_isolated_turn_that_times_out_kills_its_tree() {
    let h = start_hanging_isolated_turn(Some(std::time::Duration::from_secs(3))).await;
    let err = h.turn.await.unwrap().err().expect("a timed out turn fails");
    *super::TEST_TURN_TIMEOUT.lock() = None;
    assert!(err.contains("timed out"), "{err}");
    wait_gone(h.pid, "timeout").await;
}

// A turn future that is dropped (the whole task aborted) ends the tree through the session guard.
#[tokio::test(flavor = "multi_thread")]
async fn dropping_an_isolated_turn_kills_its_tree() {
    let h = start_hanging_isolated_turn(None).await;
    h.turn.abort();
    let _ = h.turn.await;
    wait_gone(h.pid, "abort").await;
}

// An isolated codex crewmate cannot write into `crew-runtime` (hostd's, read-only for the agent), so the reply file
// `--output-last-message` names must be where the crewmate can write: its work tree. The reply still reaches hostd,
// which reads it beneath the tree, and the file is gone afterwards.
#[tokio::test(flavor = "multi_thread")]
async fn an_isolated_codex_turn_writes_its_reply_file_where_the_crewmate_can_and_hostd_reads_it() {
    let _lock = crate::crew::runner::TEST_BIN_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    let rig = crate::crew::spawn::testing::rig();
    let tmp = tempfile::tempdir().unwrap();
    let seen = tmp.path().join("out-path");
    rig.engine(
        "codex",
        &format!(
            "while [ $# -gt 0 ]; do [ \"$1\" = --output-last-message ] && out=$2; shift; done\n\
             printf %s \"$out\" > \"{}\"\nprintf 'the codex reply' > \"$out\"",
            seen.display()
        ),
    );
    let exec = Exec::isolated(rig.client.clone(), crate::crew::spawn::testing::USER.into());
    let runtime = tmp.path().join("crew-runtime");
    let work = rig.work();
    let engine = EngineConfig {
        kind: "codex".into(),
        provider: None,
        base_url: None,
        model: None,
    };
    let env = BTreeMap::new();
    let out = run_turn(TurnCtx {
        engine: &engine,
        session: CrewmateSession::default(),
        prompt: "go",
        crewmate_dir: &runtime,
        crewmate_id: "c1",
        hook_port: HOOK_PORT,
        hook_token: HOOK_TOKEN,
        pane_key: PANE_KEY,
        extra_env: &env,
        work_dir: &work.to_string_lossy(),
        yolo: true,
        tools: None,
        cancel: &TurnCancel::new(),
        exec: &exec,
    })
    .await
    .unwrap();
    assert_eq!(out.reply, "the codex reply");
    let path = std::path::PathBuf::from(std::fs::read_to_string(&seen).unwrap());
    assert!(
        path.starts_with(&work) && path.parent() == Some(work.as_path()),
        "the reply file is {path:?}, not directly in the crewmate's work tree {work:?}"
    );
    assert!(!path.exists(), "the reply file is left behind");
}
