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

fn root_of(dir: &Path, id: &str) -> Result<PathBuf, String> {
    entries(dir)?
        .into_iter()
        .find(|e| e.id == id)
        .map(|e| PathBuf::from(e.root))
        .ok_or_else(|| format!("unknown project {id}"))
}

fn parse(s: String) -> Value {
    serde_json::from_str(&s).unwrap_or(Value::String(s))
}

pub async fn list(dir: &Path) -> Result<Value, String> {
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
        }));
    }
    Ok(json!({"projects": out}))
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
    let out = tokio::process::Command::new(exe)
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
        let v = list(tmp.path()).await.unwrap();
        let ps = v["projects"].as_array().unwrap();
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0]["id"], "a");
        assert_eq!(ps[0]["name"], "shop");
        assert_eq!(ps[0]["running"], false);
        assert_eq!(root_of(tmp.path(), "b").unwrap_err(), "unknown project b");
    }

    #[tokio::test]
    async fn missing_registry_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(list(tmp.path()).await.unwrap()["projects"], json!([]));
    }
}
