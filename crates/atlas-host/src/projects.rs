//! Projects for remote clients (Atlas Mobile): the desktop's registry
//! (`<dir>/desktop/projects.json`) joined with each project's live `atlas dev` state.
//! Service, log and action operations go through the same functions as the MCP tools
//! (`atlas_tui::mcp::tools`), so they behave exactly like an agent's `atlas_*` calls.
//! Stacks run as detached `atlas dev` processes, so all of this works with the app closed.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value, json};

use atlas_tui::mcp::tools;

#[derive(Deserialize)]
struct Entry {
    id: String,
    root: String,
    /// SSH host: not reachable from this host's runner, skipped.
    #[serde(default)]
    host: Option<String>,
}

fn entries(dir: &Path) -> Result<Vec<Entry>, String> {
    let p = dir.join("desktop").join("projects.json");
    match std::fs::read_to_string(&p) {
        Ok(t) => serde_json::from_str::<Vec<Entry>>(&t)
            .map(|v| v.into_iter().filter(|e| e.host.is_none()).collect())
            .map_err(|e| format!("{}: {e}", p.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

pub(crate) fn root_of(dir: &Path, id: &str) -> Result<PathBuf, String> {
    entries(dir)?
        .into_iter()
        .find(|e| e.id == id)
        .map(|e| PathBuf::from(e.root))
        .ok_or_else(|| format!("unknown project {id}"))
}

fn parse(s: String) -> Value {
    serde_json::from_str(&s).unwrap_or(Value::String(s))
}

/// Every local project with its live services and the agents whose terminal runs in it
/// (`panes`/`agent_status` as for [`detail`]): enough for the phone's home screen.
pub async fn list(
    dir: &Path,
    panes: &[(String, String)],
    agent_status: &Value,
) -> Result<Value, String> {
    let mut out = Vec::new();
    for e in entries(dir)? {
        let root = Path::new(&e.root);
        let st = tools::call(root, "atlas_status", &json!({}))
            .await
            .map(parse)
            .unwrap_or(Value::Null);
        out.push(json!({
            "id": e.id,
            "root": e.root,
            "name": root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            "running": st["running"] == true,
            "services": st["services"].as_array().cloned().unwrap_or_default(),
            "agents": agents_in(root, panes, agent_status),
        }));
    }
    Ok(json!({"projects": out}))
}

/// Agent panes whose terminal runs inside `root` (a pane belongs to the project of its
/// terminal's folder). `panes`: `(pane_key, terminal cwd)` of the live terminals.
pub fn agents_in(root: &Path, panes: &[(String, String)], status: &Value) -> Vec<Value> {
    let mut out: Vec<Value> = panes
        .iter()
        .filter(|(_, cwd)| Path::new(cwd).starts_with(root))
        .filter_map(|(pane, _)| {
            let st = status.get(pane)?;
            Some(json!({"pane_key": pane, "status": st}))
        })
        .collect();
    out.sort_by(|a, b| a["pane_key"].as_str().cmp(&b["pane_key"].as_str()));
    out
}

/// Everything the phone's project screen needs in one call (polled every few seconds; the
/// phone keeps the history for the charts): live status, config (ports, dependencies,
/// actions), the last log line of each service and the error lines of the last 5 minutes.
pub async fn detail(
    dir: &Path,
    id: &str,
    panes: &[(String, String)],
    agent_status: &Value,
) -> Result<Value, String> {
    let root = root_of(dir, id)?;
    let status = tools::call(&root, "atlas_status", &json!({}))
        .await
        .map(parse)
        .unwrap_or(Value::Null);
    let config = tools::call(&root, "atlas_config", &json!({}))
        .await
        .map(parse)
        .unwrap_or(Value::Null);
    let mut last_lines = serde_json::Map::new();
    for s in config["services"].as_array().into_iter().flatten() {
        let Some(name) = s["name"].as_str() else {
            continue;
        };
        if let Ok(t) = tools::call(&root, "atlas_logs", &json!({"service": name, "lines": 1})).await
        {
            let line = t.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
            last_lines.insert(name.to_string(), json!(line));
        }
    }
    let errors_5m = tools::call(
        &root,
        "atlas_logs",
        &json!({"level": "error", "since_minutes": 5, "lines": 500}),
    )
    .await
    .map(|t| t.lines().filter(|l| l.starts_with('[')).count())
    .unwrap_or(0);
    Ok(json!({
        "id": id,
        "root": root,
        "name": root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        "status": status,
        "config": config,
        "last_lines": last_lines,
        "errors_5m": errors_5m,
        "agents": agents_in(&root, panes, agent_status),
    }))
}

pub async fn logs(
    dir: &Path,
    id: &str,
    service: Option<String>,
    lines: Option<u64>,
    query: Option<String>,
) -> Result<Value, String> {
    let root = root_of(dir, id)?;
    let text = match query {
        Some(q) => {
            tools::call(
                &root,
                "atlas_logs_search",
                &json!({"query": q, "service": service}),
            )
            .await?
        }
        None => {
            tools::call(
                &root,
                "atlas_logs",
                &json!({"service": service, "lines": lines}),
            )
            .await?
        }
    };
    Ok(json!({"text": text}))
}

pub async fn service(
    dir: &Path,
    id: &str,
    service: Option<String>,
    op: &str,
) -> Result<Value, String> {
    let root = root_of(dir, id)?;
    if op == "up" {
        return up(&root).await;
    }
    let tool = match op {
        "restart" => "atlas_restart_service",
        "start" => "atlas_start_service",
        "stop" => "atlas_stop_service",
        _ => return Err(format!("unknown op {op}")),
    };
    let service = service.ok_or("service is required")?;
    let text = tools::call(&root, tool, &json!({"service": service})).await?;
    Ok(json!({"text": text}))
}

/// Start the whole stack the way the desktop does (`atlas dev --detach --dir <root>`).
async fn up(root: &Path) -> Result<Value, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = tokio::process::Command::new(exe);
    let out = atlas_core::sys::CommandExt::no_window(&mut cmd)
        .args(["dev", "--detach", "--dir"])
        .arg(root)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|e| format!("cannot run atlas: {e}"))?;
    if out.status.success() {
        Ok(json!({}))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub async fn action(dir: &Path, id: &str, key: &str, confirmed: bool) -> Result<Value, String> {
    let root = root_of(dir, id)?;
    let text = tools::call(
        &root,
        "atlas_run_action",
        &json!({"action": key, "confirmed": confirmed}),
    )
    .await?;
    Ok(json!({"text": text}))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SSH projects run on another host: listing them here would show a stack this
    /// host can neither see nor control.
    #[tokio::test]
    async fn lists_only_local_projects_and_reports_stopped_stacks() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("shop");
        std::fs::create_dir_all(&proj).unwrap();
        std::fs::create_dir_all(tmp.path().join("desktop")).unwrap();
        std::fs::write(
            tmp.path().join("desktop/projects.json"),
            json!([
                {"id": "a", "root": proj, "pinned": false, "added": "x"},
                {"id": "b", "root": "/srv/x", "host": "ssh:box"},
            ])
            .to_string(),
        )
        .unwrap();
        // Real AgentHub output, the same serialization hostd sends.
        let hub = crate::agents::AgentHub::default();
        hub.apply("claude", &json!({"pane_key": "p1", "payload": {"event": "permission_request", "tool_name": "Bash"}}));
        let status = serde_json::to_value(hub.all()).unwrap();
        let panes = vec![(
            "p1".to_string(),
            proj.join("src").to_string_lossy().into_owned(),
        )];
        let v = list(tmp.path(), &panes, &status).await.unwrap();
        let ps = v["projects"].as_array().unwrap();
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0]["id"], "a");
        assert_eq!(ps[0]["name"], "shop");
        assert_eq!(ps[0]["running"], false);
        // The home screen's "needs you" comes from here: agents ride along per project.
        assert_eq!(ps[0]["agents"][0]["pane_key"], "p1");
        assert_eq!(ps[0]["agents"][0]["status"]["state"], "blocked");
        // The phone's home parses this payload (apps/mobile/test/project_list_test.dart).
        check_fixture(
            "project_list.json",
            &serde_json::to_string_pretty(&v)
                .unwrap()
                .replace(&json_path(&proj), "/Users/me/shop"),
        );
        assert_eq!(root_of(tmp.path(), "b").unwrap_err(), "unknown project b");
    }

    #[tokio::test]
    async fn missing_registry_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            list(tmp.path(), &[], &json!({})).await.unwrap()["projects"],
            json!([])
        );
    }

    /// The phone's project screen reads one call: config services, the last line of
    /// each service log and how many error lines the last 5 minutes had.
    #[tokio::test]
    async fn detail_has_config_last_lines_and_recent_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let proj = tmp.path().join("shop");
        std::fs::create_dir_all(proj.join(".logs")).unwrap();
        std::fs::write(
            proj.join("atlas.yaml"),
            "name: shop\nservices:\n  api: { command: 'x', port: 3000 }\n  db: { command: 'y', depends_on: [api] }\nactions:\n  - { key: x, label: Migrate, command: 'true', confirm: 'sure?' }\n",
        )
        .unwrap();
        let now = chrono::Local::now().format("%H:%M:%S");
        std::fs::write(
            proj.join(".logs/api.log"),
            format!("{now} [stdout] booting\n{now} [stderr] Error: ECONNREFUSED db\n{now} [stdout] listening on 3000\n"),
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join("desktop")).unwrap();
        std::fs::write(
            tmp.path().join("desktop/projects.json"),
            json!([{"id": "a", "root": proj}]).to_string(),
        )
        .unwrap();
        // Real AgentHub output: the same serialization hostd sends to clients.
        let hub = crate::agents::AgentHub::default();
        for (pane, ev) in [("p-in", "agent_start"), ("p-out", "agent_start")] {
            hub.apply(
                "omp",
                &json!({"pane_key": pane, "payload": {"event": ev, "prompt": "diga oi"}}),
            );
        }
        hub.apply(
            "omp",
            &json!({"pane_key": "p-in", "payload": {"event": "message_end", "text": "oi"}}),
        );
        hub.apply(
            "omp",
            &json!({"pane_key": "p-in", "payload": {"event": "agent_end"}}),
        );
        let status = serde_json::to_value(hub.all()).unwrap();
        let panes = vec![
            (
                "p-in".to_string(),
                proj.join("sub").to_string_lossy().into_owned(),
            ),
            ("p-out".to_string(), "/elsewhere".to_string()),
            ("no-status".to_string(), proj.to_string_lossy().into_owned()),
        ];
        let d = detail(tmp.path(), "a", &panes, &status).await.unwrap();
        // Only agents whose terminal runs inside the project, and only with a status.
        let agents = d["agents"].as_array().unwrap();
        assert_eq!(agents.len(), 1, "{d}");
        assert_eq!(agents[0]["pane_key"], "p-in");
        let names: Vec<_> = d["config"]["services"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["api", "db"]);
        assert!(
            d["last_lines"]["api"]
                .as_str()
                .unwrap()
                .contains("listening on 3000"),
            "{d}"
        );
        assert_eq!(d["errors_5m"], 1, "{d}");
        assert_eq!(d["status"]["running"], false);

        // The phone parses this exact payload (apps/mobile/test/project_detail_test.dart):
        // the fixture is this producer's output, so a shape change here fails until the
        // fixture is regenerated with ATLAS_UPDATE_FIXTURES=1 and the Dart side follows.
        let text = serde_json::to_string_pretty(&d)
            .unwrap()
            .replace(&json_path(&proj), "/Users/me/shop")
            .replace(&now.to_string(), "12:00:00");
        check_fixture("project_detail.json", &text);
    }

    /// `p` as it appears inside a JSON string (Windows backslashes are escaped there).
    fn json_path(p: &std::path::Path) -> String {
        let quoted = serde_json::to_string(&p.to_string_lossy()).unwrap();
        quoted[1..quoted.len() - 1].to_string()
    }

    /// Compare `text` (pretty JSON) with `apps/mobile/test/fixtures/<name>`, the payload the
    /// phone's tests parse; `ATLAS_UPDATE_FIXTURES=1` rewrites it. Compared as JSON: key
    /// order depends on serde_json's `preserve_order` being on somewhere in the build.
    fn check_fixture(name: &str, text: &str) {
        let text = regex_lite_ts(text);
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/mobile/test/fixtures")
            .join(name);
        if std::env::var_os("ATLAS_UPDATE_FIXTURES").is_some() {
            std::fs::create_dir_all(fixture.parent().unwrap()).unwrap();
            std::fs::write(&fixture, format!("{text}\n")).unwrap();
        }
        let want: Value =
            serde_json::from_str(&std::fs::read_to_string(&fixture).unwrap_or_default())
                .unwrap_or(Value::Null);
        let got: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            want, got,
            "{name} changed shape: rerun with ATLAS_UPDATE_FIXTURES=1 and update the phone"
        );
    }

    /// Millisecond timestamps vary per run: pin them so the fixture is stable.
    fn regex_lite_ts(text: &str) -> String {
        text.lines()
            .map(|l| {
                let t = l.trim_start();
                if t.starts_with("\"updated_at\"") || t.starts_with("\"state_started_at\"") {
                    let indent = &l[..l.len() - t.len()];
                    let key = t.split(':').next().unwrap();
                    let comma = if t.ends_with(',') { "," } else { "" };
                    format!("{indent}{key}: 1700000000000{comma}")
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
