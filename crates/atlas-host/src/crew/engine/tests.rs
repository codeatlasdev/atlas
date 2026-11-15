//! What an engine's agent process is actually started with: its environment and argv, observed from
//! a fake CLI that dumps both. Each test runs the real engine module through `run_turn`.

use std::collections::BTreeMap;
use std::path::Path;

use super::{TEST_ENGINE_PROGRAM, TurnCancel, TurnCtx, run_turn};
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
            "#!/bin/sh\nenv > \"{d}/env.txt\"\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > \"{d}/argv.txt\"\n{reply}\n",
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
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_cli(tmp.path(), kind);
    let _lock = crate::crew::runner::TEST_BIN_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    *TEST_ENGINE_PROGRAM.lock() = Some((kind.into(), bin.to_string_lossy().into_owned()));
    let extra_env: BTreeMap<String, String> = secrets
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let work = tmp.path().join("work");
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
