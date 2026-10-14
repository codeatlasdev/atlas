//! End-to-end tests for `atlas setup omp|claude-code|codex` and `atlas hook` against the real
//! binary, fully sandboxed: HOME, PI_CODING_AGENT_DIR, CLAUDE_CONFIG_DIR and CODEX_HOME point
//! into a temp dir, never the user's real config.

// The fixtures are sh scripts and Unix sockets.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_atlas");

struct Sandbox {
    _tmp: tempfile::TempDir,
    home: PathBuf,
    agent: PathBuf,
    claude: PathBuf,
    codex: PathBuf,
    proj: PathBuf,
    fake_bin: PathBuf,
    omp_log: PathBuf,
    omp_state: PathBuf,
}

/// Stand-in for `omp`: logs every invocation to `$OMP_FAKE_LOG` and models the plugin registry
/// in `$OMP_FAKE_STATE` (`enabled` / `disabled`; absent = not linked).
const FAKE_OMP: &str = r#"#!/bin/sh
echo "$@" >> "$OMP_FAKE_LOG"
case "$1 $2" in
  "plugin list")
    if [ -f "$OMP_FAKE_STATE" ]; then
      if [ "$(cat "$OMP_FAKE_STATE")" = enabled ]; then en=true; else en=false; fi
      echo "{\"npm\":[{\"name\":\"atlas-omp\",\"version\":\"0\",\"path\":\"x\",\"enabled\":$en}],\"marketplace\":[]}"
    else
      echo '{"npm":[],"marketplace":[]}'
    fi ;;
  "plugin link") echo enabled > "$OMP_FAKE_STATE" ;;
  "plugin enable") echo enabled > "$OMP_FAKE_STATE" ;;
  "plugin disable") echo disabled > "$OMP_FAKE_STATE" ;;
  "plugin uninstall") rm -f "$OMP_FAKE_STATE" ;;
esac
exit 0
"#;

impl Sandbox {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let (home, agent, proj) = (root.join("home"), root.join("agent"), root.join("proj"));
        let (claude, codex) = (root.join("claude"), root.join("codex"));
        for d in [&home, &agent, &proj, &claude, &codex] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(
            proj.join("atlas.yaml"),
            "name: demo\nservices:\n  t:\n    command: sleep 1\n",
        )
        .unwrap();
        // Deterministic `omp` first on PATH so tests never reach the real binary.
        let fake_bin = root.join("fakebin");
        std::fs::create_dir_all(&fake_bin).unwrap();
        let omp = fake_bin.join("omp");
        std::fs::write(&omp, FAKE_OMP).unwrap();
        std::fs::set_permissions(&omp, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            _tmp: tmp,
            home,
            agent,
            claude,
            codex,
            proj,
            fake_bin,
            omp_log: root.join("omp.log"),
            omp_state: root.join("omp.state"),
        }
    }

    /// `~/.claude.json` equivalent: with CLAUDE_CONFIG_DIR set Claude Code keeps it inside the dir.
    fn claude_json(&self) -> PathBuf {
        self.claude.join(".claude.json")
    }

    /// The atlas-owned omp plugin directory.
    fn plugin(&self) -> PathBuf {
        self.home.join(".atlas/omp-plugin")
    }

    /// Lines the fake `omp` was invoked with so far.
    fn omp_calls(&self) -> Vec<String> {
        std::fs::read_to_string(&self.omp_log)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn run(&self, cwd: &Path, args: &[&str]) -> Output {
        // The binary's dir on PATH makes `atlas` resolve (bare command in the plugin .mcp.json).
        let bin_dir = Path::new(BIN).parent().unwrap();
        let path = format!(
            "{}:{}:{}",
            self.fake_bin.display(),
            bin_dir.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        Command::new(BIN)
            .args(args)
            .current_dir(cwd)
            .env("HOME", &self.home)
            .env("PI_CODING_AGENT_DIR", &self.agent)
            .env("CLAUDE_CONFIG_DIR", &self.claude)
            .env("CODEX_HOME", &self.codex)
            .env("OMP_FAKE_LOG", &self.omp_log)
            .env("OMP_FAKE_STATE", &self.omp_state)
            .env("PATH", path)
            .output()
            .unwrap()
    }

    /// Run `atlas hook <harness> <event>` with `stdin`; returns (exit ok, stdout, stderr).
    fn hook(&self, harness: &str, event: &str, stdin: &str) -> (bool, String, String) {
        use std::io::Write;
        use std::process::Stdio;
        let mut child = Command::new(BIN)
            .args(["hook", harness, event])
            .current_dir(&self.home)
            .env("HOME", &self.home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let o = child.wait_with_output().unwrap();
        (
            o.status.success(),
            String::from_utf8_lossy(&o.stdout).trim().to_string(),
            String::from_utf8_lossy(&o.stderr).to_string(),
        )
    }
}

fn out(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn dry_run_writes_nothing_apply_is_idempotent_uninstall_is_clean() {
    let sb = Sandbox::new();
    let plugin = sb.plugin();

    let dry = sb.run(&sb.proj, &["setup", "omp"]);
    assert!(dry.status.success(), "{}", out(&dry));
    let text = out(&dry);
    assert!(text.contains("Dry run"), "{text}");
    assert!(text.contains("omp plugin link"), "{text}");
    assert!(!plugin.exists());
    assert!(!sb.home.join(".agents").exists());
    assert!(
        sb.omp_calls().iter().all(|c| !c.starts_with("plugin link")),
        "dry-run must not link: {:?}",
        sb.omp_calls()
    );

    let apply = sb.run(&sb.proj, &["setup", "omp", "--apply"]);
    assert!(apply.status.success(), "{}", out(&apply));
    for rel in [
        "package.json",
        ".mcp.json",
        "hooks/pre/atlas.ts",
        "rules/atlas-no-dev-tui.md",
        "agents/atlas-ops.md",
        "skills/atlas/SKILL.md",
        "skills/atlas/yaml.md",
    ] {
        assert!(plugin.join(rel).is_file(), "{rel}");
    }
    let mcp = read_json(&plugin.join(".mcp.json"));
    assert_eq!(mcp["mcpServers"]["atlas"]["command"], "atlas");
    assert_eq!(mcp["mcpServers"]["atlas"]["args"][0], "mcp");
    assert_eq!(read_json(&plugin.join("package.json"))["name"], "atlas-omp");
    let hook = std::fs::read_to_string(plugin.join("hooks/pre/atlas.ts")).unwrap();
    assert!(
        hook.lines()
            .next()
            .unwrap()
            .contains("managed by atlas setup")
    );
    assert!(
        !sb.agent.join("mcp.json").exists() && !sb.agent.join("hooks").exists(),
        "nothing is written into omp's own config any more"
    );
    assert!(sb.home.join(".agents/skills/atlas/SKILL.md").is_file());
    let link = format!("plugin link {}", plugin.display());
    assert_eq!(
        sb.omp_calls().iter().filter(|c| **c == link).count(),
        1,
        "{:?}",
        sb.omp_calls()
    );

    let again = sb.run(&sb.proj, &["setup", "omp", "--apply"]);
    assert!(again.status.success());
    let text = out(&again);
    assert!(text.contains("up to date"), "{text}");
    assert!(
        !text.contains("created") && !text.contains("updated"),
        "{text}"
    );
    assert_eq!(
        sb.omp_calls().iter().filter(|c| **c == link).count(),
        1,
        "second apply must not link again: {:?}",
        sb.omp_calls()
    );

    // A disabled plugin is re-enabled, not re-linked.
    std::fs::write(&sb.omp_state, "disabled").unwrap();
    let re = sb.run(&sb.proj, &["setup", "omp", "--apply"]);
    assert!(re.status.success(), "{}", out(&re));
    assert!(
        sb.omp_calls()
            .iter()
            .any(|c| c == "plugin enable atlas-omp"),
        "{:?}",
        sb.omp_calls()
    );

    let un = sb.run(&sb.proj, &["setup", "omp", "--uninstall", "--apply"]);
    assert!(un.status.success(), "{}", out(&un));
    assert!(
        sb.omp_calls()
            .iter()
            .any(|c| c == "plugin uninstall atlas-omp"),
        "{:?}",
        sb.omp_calls()
    );
    assert!(!plugin.exists());
    assert!(!sb.home.join(".agents/skills/atlas").exists());
    let gone = sb.run(&sb.proj, &["setup", "omp", "--uninstall", "--apply"]);
    assert!(out(&gone).contains("nothing to remove"), "{}", out(&gone));
}

#[test]
fn missing_omp_still_writes_the_plugin_and_prints_the_link_command() {
    let sb = Sandbox::new();
    // PATH without any omp: only the atlas binary dir and system dirs.
    let path = format!(
        "{}:/usr/bin:/bin",
        Path::new(BIN).parent().unwrap().display()
    );
    let o = Command::new(BIN)
        .args(["setup", "omp", "--apply"])
        .current_dir(&sb.proj)
        .env("HOME", &sb.home)
        .env("PI_CODING_AGENT_DIR", &sb.agent)
        .env("PATH", path)
        .output()
        .unwrap();
    let text = out(&o);
    assert!(o.status.success(), "{text}");
    assert!(
        text.contains("omp not found on PATH") && text.contains("omp plugin link"),
        "{text}"
    );
    assert!(sb.plugin().join("package.json").is_file());
}

#[test]
fn legacy_hook_and_mcp_entry_are_removed_on_migration() {
    let sb = Sandbox::new();
    let hook_dir = sb.agent.join("hooks/pre");
    std::fs::create_dir_all(&hook_dir).unwrap();
    // The legacy hook is whatever an older atlas wrote: first line carries the marker.
    std::fs::write(
        hook_dir.join("atlas.ts"),
        "// managed by atlas setup — atlas 0.5.0 — do not edit\nexport default () => {}\n",
    )
    .unwrap();
    let foreign = hook_dir.join("mine.ts");
    std::fs::write(&foreign, "export default () => {}").unwrap();
    let mcp = sb.agent.join("mcp.json");
    std::fs::write(
        &mcp,
        r#"{"mcpServers":{"atlas":{"type":"stdio","command":"atlas","args":["mcp"]},"keep":{"command":"k"}}}"#,
    )
    .unwrap();

    let ok = sb.run(&sb.proj, &["setup", "omp", "--apply"]);
    assert!(ok.status.success(), "{}", out(&ok));
    assert!(!hook_dir.join("atlas.ts").exists(), "legacy hook removed");
    assert!(foreign.exists(), "foreign hook sibling kept");
    let v = read_json(&mcp);
    assert!(v["mcpServers"].get("atlas").is_none());
    assert_eq!(v["mcpServers"]["keep"]["command"], "k");
    assert!(sb.plugin().join("hooks/pre/atlas.ts").is_file());
}

#[test]
fn foreign_legacy_atlas_entry_is_ignored_by_install_and_kept_by_uninstall() {
    let sb = Sandbox::new();
    let mcp = sb.agent.join("mcp.json");
    std::fs::write(
        &mcp,
        r#"{"mcpServers":{"atlas":{"command":"node","args":["x.js"]},"keep":{"command":"k"}}}"#,
    )
    .unwrap();

    let ok = sb.run(&sb.proj, &["setup", "omp", "--apply"]);
    assert!(ok.status.success(), "{}", out(&ok));
    assert!(std::fs::read_to_string(&mcp).unwrap().contains("x.js"));

    let un = sb.run(&sb.proj, &["setup", "omp", "--uninstall", "--apply"]);
    assert!(!un.status.success(), "foreign entry is a conflict");
    assert!(out(&un).contains("conflicts"), "{}", out(&un));
    assert!(sb.plugin().exists(), "nothing applied on conflict");

    let forced = sb.run(
        &sb.proj,
        &["setup", "omp", "--uninstall", "--apply", "--force"],
    );
    assert!(forced.status.success(), "{}", out(&forced));
    let v = read_json(&mcp);
    assert!(v["mcpServers"].get("atlas").is_none());
    assert_eq!(v["mcpServers"]["keep"]["command"], "k");
}

#[test]
fn project_scope_uses_nearest_atlas_yaml_and_errors_outside() {
    let sb = Sandbox::new();
    let nested = sb.proj.join("a/b");
    std::fs::create_dir_all(&nested).unwrap();
    let ok = sb.run(&nested, &["setup", "omp", "--scope", "project", "--apply"]);
    assert!(ok.status.success(), "{}", out(&ok));
    assert!(
        out(&ok).contains("omp plugins are user-level"),
        "{}",
        out(&ok)
    );
    assert!(sb.plugin().join("package.json").is_file());
    assert!(!sb.proj.join(".omp/mcp.json").exists());
    assert!(!sb.proj.join(".omp/hooks").exists());
    assert!(!sb.agent.join("mcp.json").exists(), "user file untouched");

    let outside = sb.home.join("elsewhere");
    std::fs::create_dir_all(&outside).unwrap();
    let bad = sb.run(&outside, &["setup", "omp", "--scope", "project"]);
    assert!(!bad.status.success());
    assert!(out(&bad).contains("atlas.yaml"), "{}", out(&bad));
}

#[test]
fn skills_commands_use_the_embedded_copy() {
    let sb = Sandbox::new();
    let install = sb.run(&sb.proj, &["skills", "install"]);
    assert!(install.status.success(), "{}", out(&install));
    assert!(
        sb.home
            .join(".agents/skills/atlas-install/SKILL.md")
            .is_file()
    );

    let skill = sb.home.join(".agents/skills/atlas-install/SKILL.md");
    std::fs::write(&skill, "stale").unwrap();
    let status = sb.run(&sb.proj, &["skills", "status"]);
    assert!(out(&status).contains("outdated"), "{}", out(&status));
    let update = sb.run(&sb.proj, &["skills", "update"]);
    assert!(update.status.success());
    assert_ne!(std::fs::read_to_string(&skill).unwrap(), "stale");
}

#[test]
fn setup_without_harness_reports_status() {
    let sb = Sandbox::new();
    let o = sb.run(&sb.proj, &["setup"]);
    assert!(o.status.success(), "{}", out(&o));
    let text = out(&o);
    for agent in ["omp", "claude-code", "codex"] {
        assert!(text.contains(agent), "{text}");
    }
}

/// Fake agent binaries on PATH make detection report them and print per-agent commands.
#[test]
fn setup_detection_lists_all_three_agents_when_present() {
    use std::os::unix::fs::PermissionsExt;
    let sb = Sandbox::new();
    let bin = sb.home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for b in ["claude", "codex"] {
        let p = bin.join(b);
        std::fs::write(&p, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = format!(
        "{}:{}:{}:{}",
        sb.fake_bin.display(),
        bin.display(),
        Path::new(BIN).parent().unwrap().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let o = Command::new(BIN)
        .arg("setup")
        .current_dir(&sb.proj)
        .env("HOME", &sb.home)
        .env("PI_CODING_AGENT_DIR", &sb.agent)
        .env("CLAUDE_CONFIG_DIR", &sb.claude)
        .env("CODEX_HOME", &sb.codex)
        .env("OMP_FAKE_LOG", &sb.omp_log)
        .env("OMP_FAKE_STATE", &sb.omp_state)
        .env("PATH", path)
        .output()
        .unwrap();
    let text = out(&o);
    assert!(o.status.success(), "{text}");
    for cmd in [
        "atlas setup omp --apply",
        "atlas setup claude-code --apply",
        "atlas setup codex --apply",
    ] {
        assert!(text.contains(cmd), "{text}");
    }
    assert_eq!(
        text.matches("not set up (or outdated)").count(),
        3,
        "{text}"
    );
    // Applying one of them flips only that one.
    let ok = sb.run(&sb.proj, &["setup", "codex", "--apply"]);
    assert!(ok.status.success(), "{}", out(&ok));
    let o = Command::new(BIN)
        .arg("setup")
        .current_dir(&sb.proj)
        .env("HOME", &sb.home)
        .env("PI_CODING_AGENT_DIR", &sb.agent)
        .env("CLAUDE_CONFIG_DIR", &sb.claude)
        .env("CODEX_HOME", &sb.codex)
        .env("OMP_FAKE_LOG", &sb.omp_log)
        .env("OMP_FAKE_STATE", &sb.omp_state)
        .env(
            "PATH",
            format!(
                "{}:{}:{}",
                sb.fake_bin.display(),
                bin.display(),
                Path::new(BIN).parent().unwrap().display()
            ),
        )
        .output()
        .unwrap();
    let text = out(&o);
    assert_eq!(text.matches("set up, up to date").count(), 1, "{text}");
    assert_eq!(
        text.matches("not set up (or outdated)").count(),
        2,
        "{text}"
    );
}

/// Runs the *installed* hook under bun against a table of commands. Skipped (loudly)
/// without bun: it is the only way to exercise the TypeScript for real.
#[test]
fn installed_hook_blocks_interactive_atlas_dev_and_allows_the_rest() {
    if Command::new("bun").arg("--version").output().is_err() {
        eprintln!("SKIP hook guard: bun not available");
        return;
    }
    let sb = Sandbox::new();
    std::fs::write(
        sb.proj.join("package.json"),
        r#"{"scripts":{"dev":"atlas dev"}}"#,
    )
    .unwrap();
    let ok = sb.run(&sb.proj, &["setup", "omp", "--apply"]);
    assert!(ok.status.success(), "{}", out(&ok));

    let script = sb.home.join("t.ts");
    let hook = sb.plugin().join("hooks/pre/atlas.ts");
    let other = sb.home.join("plain");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(
        &script,
        format!(
            r#"import hook from "{hook}";
type H = (e: unknown, c: unknown) => Promise<unknown>;
const h: Record<string, H> = {{}};
(hook as (p: unknown) => void)({{ on: (n: string, f: H) => {{ h[n] = f; }}, exec: async () => ({{ stdout: "", code: 3 }}) }});
const blocked = async (command: string, cwd: string) =>
  !!((await h.tool_call({{ toolName: "bash", input: {{ command }} }}, {{ cwd }})) as {{ block?: boolean }} | undefined)?.block;
const P = "{proj}", Q = "{other}";
const cases: [string, string, boolean][] = [
  ["atlas dev", P, true], ["cd x && atlas dev", P, true], ["FOO=1 atlas dev -d .", P, true],
  ["/Users/a/.atlas/bin/atlas dev", P, true], ["bun run dev", P, true], ["pnpm dev", P, true], ["npm run dev", P, true],
  ["atlas dev --status", P, false], ["atlas dev --ci", P, false], ["atlas dev --headless", P, false],
  ["echo atlas dev is cool", P, false], ["bun run build", P, false], ["atlas mcp", P, false],
  ["atlas dev", Q, false], ["bun run dev", Q, false],
];
const hubBlocked = async (input: unknown, cwd: string) =>
  !!((await h.tool_call({{ toolName: "hub", input }}, {{ cwd }})) as {{ block?: boolean }} | undefined)?.block;
const hubCases: [unknown, string, boolean][] = [
  [{{ op: "start", name: "d", application: "atlas", args: ["dev"] }}, P, true],
  [{{ op: "start", name: "d", application: "/Users/a/.atlas/bin/atlas", args: ["dev", "-d", "."] }}, P, true],
  [{{ op: "start", name: "d", application: "atlas", args: ["dev", "--status"] }}, P, false],
  [{{ op: "start", name: "d", application: "bun", args: ["run", "dev"] }}, P, true],
  [{{ op: "start", name: "d", application: "cargo", args: ["tauri", "dev"] }}, P, false],
  [{{ op: "logs", name: "d" }}, P, false],
  [{{ op: "start", name: "d", application: "atlas", args: ["dev"] }}, Q, false],
];
let bad = 0;
for (const [c, d, want] of cases) if ((await blocked(c, d)) !== want) {{ bad++; console.log("FAIL", c, d, want); }}
for (const [i, d, want] of hubCases) if ((await hubBlocked(i, d)) !== want) {{ bad++; console.log("FAIL hub", JSON.stringify(i), d, want); }}
const reason = (await h.tool_call({{ toolName: "bash", input: {{ command: "atlas dev" }} }}, {{ cwd: P }})) as {{ reason?: string }};
if (!reason.reason?.includes("atlas_restart_service") || reason.reason.includes("atlas_restart,")) {{ bad++; console.log("FAIL: stale tool names in block reason"); }}
for (const e of [null, undefined, {{}}, {{ toolName: "bash" }}, {{ toolName: "bash", input: null }}]) await h.tool_call(e, undefined);
const first = (await h.before_agent_start({{}}, {{ cwd: Q }})) as unknown;
if (first !== undefined) {{ bad++; console.log("FAIL: injected outside an atlas project"); }}
const msg = (await h.before_agent_start({{}}, {{ cwd: P }})) as {{ message?: {{ content?: string }} }} | undefined;
if (!msg?.message?.content?.includes("NOT running")) {{ bad++; console.log("FAIL: no not-running message", JSON.stringify(msg)); }}
const again = (await h.before_agent_start({{}}, {{ cwd: P }})) as unknown;
if (again !== undefined) {{ bad++; console.log("FAIL: injected twice in one session"); }}
console.log(bad === 0 ? "OK" : "BAD " + bad);
"#,
            hook = hook.display(),
            proj = sb.proj.display(),
            other = other.display()
        ),
    )
    .unwrap();
    let r = Command::new("bun")
        .arg("run")
        .arg(&script)
        .output()
        .unwrap();
    let text = out(&r);
    assert!(text.trim_end().ends_with("OK"), "{text}");
}

// ── claude-code ──────────────────────────────────────────────────────────────

const CLAUDE_SS: &str = "atlas hook claude-code session-start 2>/dev/null || true";
const CLAUDE_PRE: &str = "atlas hook claude-code pre-tool-use 2>/dev/null || true";

fn read_json(p: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

#[test]
fn claude_code_dry_run_apply_idempotent_uninstall() {
    let sb = Sandbox::new();

    let dry = sb.run(&sb.proj, &["setup", "claude-code"]);
    assert!(dry.status.success(), "{}", out(&dry));
    assert!(out(&dry).contains("Dry run"), "{}", out(&dry));
    assert!(!sb.claude_json().exists() && !sb.claude.join("settings.json").exists());
    assert!(!sb.claude.join("skills").exists());

    let apply = sb.run(&sb.proj, &["setup", "claude-code", "--apply"]);
    assert!(apply.status.success(), "{}", out(&apply));
    let mcp = read_json(&sb.claude_json());
    assert_eq!(
        mcp["mcpServers"]["atlas"],
        serde_json::json!({"type":"stdio","command":"atlas","args":["mcp"],"env":{}})
    );
    let settings = read_json(&sb.claude.join("settings.json"));
    assert_eq!(
        settings["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        CLAUDE_SS
    );
    assert_eq!(settings["hooks"]["PreToolUse"][0]["matcher"], "Bash");
    assert_eq!(
        settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        CLAUDE_PRE
    );
    assert!(sb.claude.join("skills/atlas/SKILL.md").is_file());
    assert!(
        !sb.home.join(".agents").exists(),
        "Claude Code does not read ~/.agents/skills"
    );
    assert!(
        !sb.home.join(".claude.json").exists(),
        "never the real home with CLAUDE_CONFIG_DIR set"
    );

    let again = sb.run(&sb.proj, &["setup", "claude-code", "--apply"]);
    let text = out(&again);
    assert!(
        again.status.success() && text.contains("up to date"),
        "{text}"
    );
    assert!(
        !text.contains("created") && !text.contains("updated"),
        "{text}"
    );

    let un = sb.run(
        &sb.proj,
        &["setup", "claude-code", "--uninstall", "--apply"],
    );
    assert!(un.status.success(), "{}", out(&un));
    assert!(!sb.claude.join("settings.json").exists());
    assert!(!sb.claude.join("skills/atlas").exists());
    let leftover = std::fs::read_to_string(sb.claude_json()).ok();
    assert!(
        leftover.is_none(),
        "atlas-only .claude.json is deleted: {leftover:?}"
    );
}

#[test]
fn claude_code_preserves_user_state_and_refuses_foreign_entry() {
    let sb = Sandbox::new();
    std::fs::write(
        sb.claude_json(),
        r#"{"numStartups":3,"mcpServers":{"atlas":{"type":"stdio","command":"node","args":["x.js"],"env":{}},"keep":{"type":"stdio","command":"k","args":[],"env":{}}},"projects":{}}"#,
    )
    .unwrap();
    std::fs::write(
        sb.claude.join("settings.json"),
        r#"{"theme":"dark","hooks":{"PreToolUse":[{"matcher":"Edit","hooks":[{"type":"command","command":"echo mine"}]}]}}"#,
    )
    .unwrap();

    let refused = sb.run(&sb.proj, &["setup", "claude-code", "--apply"]);
    assert!(!refused.status.success());
    assert!(out(&refused).contains("conflicts"), "{}", out(&refused));
    assert!(
        std::fs::read_to_string(sb.claude_json())
            .unwrap()
            .contains("x.js")
    );
    assert!(
        !std::fs::read_to_string(sb.claude.join("settings.json"))
            .unwrap()
            .contains("atlas hook")
    );
    assert!(
        !sb.claude.join("skills").exists(),
        "nothing applied on conflict"
    );

    let forced = sb.run(&sb.proj, &["setup", "claude-code", "--apply", "--force"]);
    assert!(forced.status.success(), "{}", out(&forced));
    let v = read_json(&sb.claude_json());
    assert_eq!(v["mcpServers"]["atlas"]["args"][0], "mcp");
    assert_eq!(v["mcpServers"]["keep"]["command"], "k");
    assert_eq!(v["numStartups"], 3);
    let settings = read_json(&sb.claude.join("settings.json"));
    assert_eq!(settings["theme"], "dark");
    assert_eq!(
        settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        "echo mine"
    );

    let un = sb.run(
        &sb.proj,
        &["setup", "claude-code", "--uninstall", "--apply"],
    );
    assert!(un.status.success(), "{}", out(&un));
    let v = read_json(&sb.claude_json());
    assert!(v["mcpServers"].get("atlas").is_none());
    assert_eq!(v["mcpServers"]["keep"]["command"], "k");
    let settings = read_json(&sb.claude.join("settings.json"));
    assert_eq!(settings["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
    assert!(settings["hooks"].get("SessionStart").is_none());
}

#[test]
fn claude_code_project_scope_writes_mcp_json_and_dot_claude_settings() {
    let sb = Sandbox::new();
    let nested = sb.proj.join("a/b");
    std::fs::create_dir_all(&nested).unwrap();
    let ok = sb.run(
        &nested,
        &["setup", "claude-code", "--scope", "project", "--apply"],
    );
    assert!(ok.status.success(), "{}", out(&ok));
    assert_eq!(
        read_json(&sb.proj.join(".mcp.json"))["mcpServers"]["atlas"]["args"][0],
        "mcp"
    );
    let settings = read_json(&sb.proj.join(".claude/settings.json"));
    assert_eq!(
        settings["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        CLAUDE_SS
    );
    assert!(!sb.claude_json().exists(), "user config untouched");
    assert!(!sb.claude.join("settings.json").exists());

    let un = sb.run(
        &nested,
        &[
            "setup",
            "claude-code",
            "--scope",
            "project",
            "--uninstall",
            "--apply",
        ],
    );
    assert!(un.status.success(), "{}", out(&un));
    assert!(!sb.proj.join(".mcp.json").exists() && !sb.proj.join(".claude").exists());
    assert!(sb.proj.join("atlas.yaml").exists());

    let outside = sb.home.join("elsewhere");
    std::fs::create_dir_all(&outside).unwrap();
    let bad = sb.run(&outside, &["setup", "claude-code", "--scope", "project"]);
    assert!(
        !bad.status.success() && out(&bad).contains("atlas.yaml"),
        "{}",
        out(&bad)
    );
}

// ── codex ────────────────────────────────────────────────────────────────────

const CODEX_SS: &str = "atlas hook codex session-start 2>/dev/null || true";
const CODEX_PRE: &str = "atlas hook codex pre-tool-use 2>/dev/null || true";

#[test]
fn codex_dry_run_apply_idempotent_uninstall_with_shared_skills() {
    let sb = Sandbox::new();

    let dry = sb.run(&sb.proj, &["setup", "codex"]);
    assert!(dry.status.success(), "{}", out(&dry));
    assert!(out(&dry).contains("Dry run"), "{}", out(&dry));
    assert!(out(&dry).contains("Skills (shared)"), "{}", out(&dry));
    assert!(!sb.codex.join("config.toml").exists() && !sb.home.join(".agents").exists());

    let apply = sb.run(&sb.proj, &["setup", "codex", "--apply"]);
    assert!(apply.status.success(), "{}", out(&apply));
    assert_eq!(
        std::fs::read_to_string(sb.codex.join("config.toml")).unwrap(),
        "[mcp_servers.atlas]\ncommand = \"atlas\"\nargs = [\"mcp\"]\n"
    );
    let hooks = read_json(&sb.codex.join("hooks.json"));
    assert_eq!(
        hooks["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        CODEX_SS
    );
    assert_eq!(hooks["hooks"]["PreToolUse"][0]["matcher"], "Bash");
    assert_eq!(
        hooks["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        CODEX_PRE
    );
    assert!(
        sb.home.join(".agents/skills/atlas/SKILL.md").is_file(),
        "shared skills dir"
    );
    assert!(
        out(&apply).contains("/hooks"),
        "next steps mention hook trust: {}",
        out(&apply)
    );

    let again = sb.run(&sb.proj, &["setup", "codex", "--apply"]);
    let text = out(&again);
    assert!(
        again.status.success() && text.contains("up to date"),
        "{text}"
    );
    assert!(
        !text.contains("created") && !text.contains("updated"),
        "{text}"
    );

    let un = sb.run(&sb.proj, &["setup", "codex", "--uninstall", "--apply"]);
    assert!(un.status.success(), "{}", out(&un));
    assert!(!sb.codex.join("config.toml").exists() && !sb.codex.join("hooks.json").exists());
    assert!(!sb.home.join(".agents/skills/atlas").exists());
}

#[test]
fn codex_preserves_config_comments_and_user_hooks_and_refuses_foreign_server() {
    let sb = Sandbox::new();
    let cfg = "# my config\nmodel = \"gpt-5\" # fast\n\n[mcp_servers.atlas]\ncommand = \"node\"\nargs = [\"x.js\"]\n";
    std::fs::write(sb.codex.join("config.toml"), cfg).unwrap();
    std::fs::write(
        sb.codex.join("hooks.json"),
        r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"echo stop"}]}]}}"#,
    )
    .unwrap();

    let refused = sb.run(&sb.proj, &["setup", "codex", "--apply"]);
    assert!(!refused.status.success());
    assert!(out(&refused).contains("conflicts"), "{}", out(&refused));
    assert_eq!(
        std::fs::read_to_string(sb.codex.join("config.toml")).unwrap(),
        cfg
    );
    assert!(
        !std::fs::read_to_string(sb.codex.join("hooks.json"))
            .unwrap()
            .contains("atlas hook")
    );

    let forced = sb.run(&sb.proj, &["setup", "codex", "--apply", "--force"]);
    assert!(forced.status.success(), "{}", out(&forced));
    let written = std::fs::read_to_string(sb.codex.join("config.toml")).unwrap();
    assert!(
        written.starts_with("# my config\nmodel = \"gpt-5\" # fast\n"),
        "{written}"
    );
    assert!(
        written.contains("args = [\"mcp\"]") && !written.contains("x.js"),
        "{written}"
    );
    let hooks = read_json(&sb.codex.join("hooks.json"));
    assert_eq!(
        hooks["hooks"]["Stop"][0]["hooks"][0]["command"],
        "echo stop"
    );

    let un = sb.run(&sb.proj, &["setup", "codex", "--uninstall", "--apply"]);
    assert!(un.status.success(), "{}", out(&un));
    assert!(
        !std::fs::read_to_string(sb.codex.join("config.toml"))
            .unwrap()
            .contains("atlas")
    );
    let hooks = read_json(&sb.codex.join("hooks.json"));
    assert_eq!(
        hooks["hooks"]["Stop"][0]["hooks"][0]["command"],
        "echo stop"
    );
    assert!(hooks["hooks"].get("PreToolUse").is_none());
}

#[test]
fn codex_project_scope_uses_dot_codex_and_omp_plus_codex_share_skills() {
    let sb = Sandbox::new();
    let ok = sb.run(
        &sb.proj,
        &["setup", "codex", "--scope", "project", "--apply"],
    );
    assert!(ok.status.success(), "{}", out(&ok));
    assert!(
        sb.proj.join(".codex/config.toml").is_file() && sb.proj.join(".codex/hooks.json").is_file()
    );
    assert!(
        !sb.codex.join("config.toml").exists(),
        "user config untouched"
    );
    let un = sb.run(
        &sb.proj,
        &[
            "setup",
            "codex",
            "--scope",
            "project",
            "--uninstall",
            "--apply",
        ],
    );
    assert!(un.status.success(), "{}", out(&un));
    assert!(!sb.proj.join(".codex").exists());

    // omp + codex at user scope: removing one keeps the shared skills for the other.
    for agent in ["omp", "codex"] {
        let o = sb.run(&sb.proj, &["setup", agent, "--apply"]);
        assert!(o.status.success(), "{}", out(&o));
    }
    let skill = sb.home.join(".agents/skills/atlas/SKILL.md");
    assert!(skill.is_file());
    let un = sb.run(&sb.proj, &["setup", "codex", "--uninstall", "--apply"]);
    assert!(un.status.success(), "{}", out(&un));
    assert!(skill.is_file(), "OMP still uses the shared skills");
    assert!(sb.plugin().join("package.json").is_file());
    let un = sb.run(&sb.proj, &["setup", "omp", "--uninstall", "--apply"]);
    assert!(un.status.success(), "{}", out(&un));
    assert!(
        !skill.exists(),
        "last harness out removes the shared skills"
    );
}

// ── atlas hook (real binary, JSON in → JSON out) ─────────────────────────────

#[test]
fn hook_pre_tool_use_fixtures_for_both_harnesses() {
    let sb = Sandbox::new();
    std::fs::write(
        sb.proj.join("package.json"),
        r#"{"scripts":{"dev":"atlas dev"}}"#,
    )
    .unwrap();
    let cwd = sb.proj.display().to_string();
    let pre = |harness: &str, tool: &str, command: &str| {
        let input = serde_json::json!({
            "session_id": "s1", "cwd": cwd, "hook_event_name": "PreToolUse",
            "tool_name": tool, "tool_input": {"command": command}, "tool_use_id": "t1",
        });
        sb.hook(harness, "pre-tool-use", &input.to_string())
    };
    for harness in ["claude-code", "codex"] {
        for cmd in [
            "atlas dev",
            "cd x && atlas dev -d .",
            "bun run dev",
            "pnpm dev",
        ] {
            let (ok, stdout, stderr) = pre(harness, "Bash", cmd);
            assert!(ok, "{harness} {cmd}: {stderr}");
            let v: Value = serde_json::from_str(&stdout)
                .unwrap_or_else(|_| panic!("{harness} {cmd}: {stdout:?}"));
            assert_eq!(
                v,
                serde_json::json!({"hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": v["hookSpecificOutput"]["permissionDecisionReason"].clone(),
                }}),
                "{harness} {cmd}"
            );
            assert!(
                v["hookSpecificOutput"]["permissionDecisionReason"]
                    .as_str()
                    .unwrap()
                    .contains("atlas")
            );
        }
        for cmd in [
            "atlas dev --status",
            "atlas dev --ci",
            "atlas dev --headless",
            "ls",
            "atlas mcp",
        ] {
            let (ok, stdout, _) = pre(harness, "Bash", cmd);
            assert!(ok && stdout.is_empty(), "{harness} {cmd}: {stdout:?}");
        }
        let (ok, stdout, _) = pre(harness, "Edit", "atlas dev");
        assert!(ok && stdout.is_empty(), "non-Bash tools are ignored");
    }
}

#[test]
fn hook_session_start_fixtures_and_never_fails_on_bad_input() {
    let sb = Sandbox::new();
    let in_proj =
        serde_json::json!({"hook_event_name":"SessionStart","source":"startup","cwd":sb.proj})
            .to_string();
    let outside =
        serde_json::json!({"hook_event_name":"SessionStart","source":"startup","cwd":sb.home})
            .to_string();
    for harness in ["claude-code", "codex"] {
        let (ok, stdout, stderr) = sb.hook(harness, "session-start", &in_proj);
        assert!(ok && stderr.is_empty(), "{harness}: {stderr}");
        let v: Value = serde_json::from_str(&stdout).unwrap();
        let h = &v["hookSpecificOutput"];
        assert_eq!(h["hookEventName"], "SessionStart");
        let ctx = h["additionalContext"].as_str().unwrap();
        assert!(ctx.contains("atlas dev is NOT running"), "{ctx}");
        assert!(ctx.contains("Never run `atlas dev` yourself"), "{ctx}");
        assert_eq!(v.as_object().unwrap().len(), 1);

        let (ok, stdout, _) = sb.hook(harness, "session-start", &outside);
        assert!(ok && stdout.is_empty(), "silent outside atlas projects");

        for bad in ["", "not json", "[]", "{"] {
            for event in ["session-start", "pre-tool-use"] {
                let (ok, stdout, stderr) = sb.hook(harness, event, bad);
                assert!(
                    ok && stdout.is_empty() && stderr.is_empty(),
                    "{harness} {event} {bad:?}: {stdout:?} {stderr:?}"
                );
            }
        }
    }
}

#[test]
fn registered_hook_command_line_is_safe_with_an_older_atlas_or_none() {
    // The registered command ends in `|| true`: an `atlas` without the `hook` subcommand (clap
    // exit 2 = "blocking error" for both agents) or no atlas at all must never block a tool call.
    let sb = Sandbox::new();
    let ok = sb.run(&sb.proj, &["setup", "claude-code", "--apply"]);
    assert!(ok.status.success(), "{}", out(&ok));
    let settings = read_json(&sb.claude.join("settings.json"));
    let command = settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();

    use std::os::unix::fs::PermissionsExt;
    let old = sb.home.join("old-bin");
    std::fs::create_dir_all(&old).unwrap();
    let fake = old.join("atlas");
    std::fs::write(
        &fake,
        "#!/bin/sh\necho \"error: unrecognized subcommand 'hook'\" >&2\nexit 2\n",
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    for path in [old.display().to_string(), "/nonexistent".to_string()] {
        let o = Command::new("sh")
            .args(["-c", command])
            .env("PATH", format!("{path}:/usr/bin:/bin"))
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(0), "PATH={path}: {}", out(&o));
        assert!(o.stdout.is_empty());
    }
    // And the real binary through the registered command line produces the real output.
    let input =
        serde_json::json!({"tool_name":"Bash","tool_input":{"command":"atlas dev"},"cwd":sb.proj})
            .to_string();
    let bin_dir = Path::new(BIN).parent().unwrap();
    let mut child = Command::new("sh")
        .args(["-c", command])
        .env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::io::Write::write_all(&mut child.stdin.take().unwrap(), input.as_bytes()).unwrap();
    let o = child.wait_with_output().unwrap();
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
}

#[test]
fn update_refresh_rewrites_claude_and_codex_only_when_already_set_up() {
    let sb = Sandbox::new();
    // Not set up: silent no-op, nothing created.
    for agent in ["claude-code", "codex"] {
        let o = sb.run(&sb.proj, &["setup", agent, "--refresh"]);
        assert!(
            o.status.success() && !out(&o).contains("refreshed"),
            "{}",
            out(&o)
        );
    }
    assert!(!sb.claude.join("settings.json").exists() && !sb.codex.join("hooks.json").exists());

    // Set up with an old absolute atlas path, then refresh rewrites it to the PATH one.
    for agent in ["claude-code", "codex"] {
        let ok = sb.run(&sb.proj, &["setup", agent, "--apply"]);
        assert!(ok.status.success(), "{}", out(&ok));
    }
    let hooks = sb.codex.join("hooks.json");
    let stale = std::fs::read_to_string(&hooks)
        .unwrap()
        .replace("atlas hook", "/old/bin/atlas hook");
    std::fs::write(&hooks, stale).unwrap();
    let o = sb.run(&sb.proj, &["setup", "codex", "--refresh"]);
    assert!(
        o.status.success() && out(&o).contains("refreshed Codex"),
        "{}",
        out(&o)
    );
    assert!(
        !std::fs::read_to_string(&hooks)
            .unwrap()
            .contains("/old/bin")
    );
}
