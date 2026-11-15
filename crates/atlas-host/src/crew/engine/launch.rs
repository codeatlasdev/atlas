//! How an agent turn is launched, in one place: what every engine gets (`TurnCtx`), the process it is
//! started as (`command`: cwd, crewmate secrets, then hostd's own `ATLAS_*` set), how it is waited for
//! (`output`), and the one description of the atlas MCP server that each engine renders in its own
//! config language (`McpServer`: claude JSON file, codex `-c` overrides, opencode inline config).

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

use super::{ToolSink, TurnCancel, TurnProc};
use crate::crew::store::{CrewmateSession, EngineConfig};

/// Everything one turn needs, whatever the engine.
pub struct TurnCtx<'a> {
    pub engine: &'a EngineConfig,
    pub session: CrewmateSession,
    pub prompt: &'a str,
    /// `<store>/crew/<id>/`: scratch space (mcp config, codex output file, omp extension).
    pub crewmate_dir: &'a Path,
    pub crewmate_id: &'a str,
    pub hook_port: u16,
    pub hook_token: &'a str,
    /// PTY pane key for permission prompts (`crew:<id>:<task_id>`).
    pub pane_key: &'a str,
    /// Crewmate-scoped secrets, forwarded as env vars (hostd's `ATLAS_*` always win over them).
    pub extra_env: &'a BTreeMap<String, String>,
    pub work_dir: &'a str,
    /// Skip interactive permission prompts (auto-approve).
    pub yolo: bool,
    /// Told of every finished tool call, for engines whose stream carries them (omp RPC).
    pub tools: Option<&'a ToolSink<'a>>,
    /// Fires when the task is cancelled or the crew paused: the turn's process tree ends.
    pub cancel: &'a TurnCancel,
}

impl TurnCtx<'_> {
    /// The env hostd owns for this turn. The one list of these names and values: the agent, the
    /// approval extension and the MCP server all read them, so a crewmate secret must never replace them.
    pub fn atlas_env(&self) -> [(&'static str, String); 5] {
        [
            ("ATLAS_HOOK_PORT", self.hook_port.to_string()),
            ("ATLAS_HOOK_TOKEN", self.hook_token.to_string()),
            ("ATLAS_CREWMATE_ID", self.crewmate_id.to_string()),
            ("ATLAS_PANE_KEY", self.pane_key.to_string()),
            ("ATLAS_HOOK_DECISION_SECS", "1800".to_string()),
        ]
    }

    /// `program` started in the work dir with the crewmate's secrets, then hostd's `ATLAS_*` set on
    /// top (set last, so they win). Stdio is the caller's.
    pub fn command(&self, program: &str) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(program);
        cmd.current_dir(self.work_dir);
        for (k, v) in self.extra_env {
            cmd.env(k, v);
        }
        for (k, v) in self.atlas_env() {
            cmd.env(k, v);
        }
        cmd
    }

    /// The atlas MCP server for this crewmate (`atlas mcp --crewmate <id>`), described once.
    pub fn mcp_server(&self) -> McpServer {
        McpServer {
            command: std::env::current_exe()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "atlas".into()),
            args: vec!["mcp".into(), "--crewmate".into(), self.crewmate_id.into()],
            // What `crew/mcp.rs` reads; the decision timeout is the agent's, not the server's.
            env: self
                .atlas_env()
                .into_iter()
                .filter(|(k, _)| *k != "ATLAS_HOOK_DECISION_SECS")
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        }
    }
}

/// Run `cmd` as a turn (piped stdout and stderr, own process group, tree killed on drop, cancel and
/// timeout) and return what it printed. A process that cannot start is `"<name>: spawn failed: …"`.
/// Whether a non-zero exit is an error is the engine's call (claude still parses a result it printed).
pub async fn output(
    mut cmd: tokio::process::Command,
    cancel: &TurnCancel,
    name: &str,
) -> Result<std::process::Output, String> {
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    TurnProc::spawn(&mut cmd)
        .map_err(|e| format!("{name}: spawn failed: {e}"))?
        .output(cancel, name)
        .await
}

/// The error for a turn whose process exited non-zero: `"<name>: exited <code>: <stderr>"`.
pub fn exit_error(name: &str, out: &std::process::Output) -> String {
    format!(
        "{name}: exited {}: {}",
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// One MCP stdio server, engine-neutral. Each engine renders it in its own config language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServer {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

impl McpServer {
    /// claude's `--mcp-config` file: `{"mcpServers": {"atlas": {command, args, env}}}`.
    pub fn claude_json(&self) -> Value {
        json!({ "mcpServers": { "atlas": {
            "command": self.command, "args": self.args, "env": self.env,
        } } })
    }

    /// codex's per-run `-c key=value` overrides (values are TOML). `default_tools_approval_mode =
    /// "approve"` is required: `codex exec` has nobody to ask, so without it every call of this
    /// server is cancelled with "user cancelled MCP tool call" (openai/codex#29857, #24135).
    pub fn codex_overrides(&self) -> Vec<String> {
        let toml = |s: &str| Value::String(s.to_string()).to_string();
        let args: Vec<String> = self.args.iter().map(|a| toml(a)).collect();
        let env: Vec<String> = self
            .env
            .iter()
            .map(|(k, v)| format!("{}={}", toml(k), toml(v)))
            .collect();
        let mut out = vec![];
        for (key, value) in [
            ("command", toml(&self.command)),
            ("args", format!("[{}]", args.join(","))),
            ("env", format!("{{{}}}", env.join(","))),
            ("default_tools_approval_mode", toml("approve")),
        ] {
            out.push("-c".to_string());
            out.push(format!("mcp_servers.atlas.{key}={value}"));
        }
        out
    }

    /// opencode's config with the atlas server as `mcp.atlas`: `type: "local"`, `command: [..]`,
    /// `environment: {..}`. This is the V1 shape on purpose: V2 reads it natively ("Supported V1
    /// syntax remains quiet", opencode.ai/docs/migrate-v1), while V1 cannot read V2's `mcp.servers`
    /// (it would see a server named "servers" and run without the crew tools, silently). One shape
    /// that both majors load needs no version probe. Passed as `OPENCODE_CONFIG_CONTENT`, which
    /// opencode merges after the project's own config and never writes into the user's repo.
    pub fn opencode_config(&self) -> Value {
        let mut command = vec![self.command.clone()];
        command.extend(self.args.iter().cloned());
        json!({ "mcp": { "atlas": {
            "type": "local", "command": command, "environment": self.env,
        } } })
    }
}
