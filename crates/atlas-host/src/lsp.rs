//! hostd's side of the `lsp_*` commands: thin glue between the wire protocol and
//! [`atlas_lsp::Manager`], plus the pump that fans manager pushes out to every connection.

use std::path::Path;
use std::sync::Weak;

use atlas_lsp::Push;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::protocol::HostCmd;
use crate::server::Hub;

fn rpc_err(e: atlas_lsp::rpc::RpcError) -> String {
    e.to_string()
}

fn abs(path: &str) -> Result<(), String> {
    if Path::new(path).is_absolute() {
        Ok(())
    } else {
        Err(format!("path must be absolute: {path}"))
    }
}

/// Handle one `lsp_*` command. `None`: not an LSP command.
pub(crate) async fn handle(hub: &Hub, cmd: HostCmd, conn_id: u64) -> Option<Result<Value, String>> {
    let lsp = &hub.lsp;
    Some(match cmd {
        HostCmd::LspOpen {
            path,
            language_id,
            text,
            version,
            overrides,
            root,
        } => {
            if let Err(e) = abs(&path) {
                return Some(Err(e));
            }
            let overrides: atlas_lsp::Overrides = match overrides {
                None | Some(Value::Null) => Default::default(),
                Some(v) => match serde_json::from_value(v) {
                    Ok(o) => o,
                    Err(e) => return Some(Err(format!("bad overrides: {e}"))),
                },
            };
            // `root` (the worktree) is the fallback when no marker is found and where dependency files are
            // served from; without it the file's own directory.
            if let Some(r) = &root
                && !Path::new(r).is_absolute()
            {
                return Some(Err(format!("root must be absolute: {r}")));
            }
            hub.lsp_docs.lock().insert(path.clone(), conn_id);
            let r = lsp
                .open_with(
                    &path,
                    &language_id,
                    text,
                    version,
                    root.as_deref().map(Path::new),
                    &overrides,
                )
                .await;
            Ok(json!({
                "server": r.server,
                "status": r.status,
                "capabilities": r.capabilities,
                "reason": r.reason,
                "supported": r.supported,
            }))
        }
        HostCmd::LspChange {
            path,
            version,
            changes,
        } => lsp
            .change(&path, version, changes)
            .await
            .map(|()| json!({}))
            .map_err(|e| format!("{e:#}")),
        HostCmd::LspClose { path } => {
            hub.lsp_docs.lock().remove(&path);
            lsp.close(&path).await;
            Ok(json!({}))
        }
        HostCmd::LspSave { path, text } => lsp
            .save(&path, text)
            .await
            .map(|()| json!({}))
            .map_err(|e| format!("{e:#}")),
        HostCmd::LspRequest {
            path,
            method,
            params,
            rid,
        } => lsp
            .request(&path, &method, params, rid)
            .await
            .map_err(rpc_err),
        HostCmd::LspCancel { id } => {
            lsp.cancel(&id).await;
            Ok(json!({}))
        }
        HostCmd::LspStatus {} => Ok(json!({"servers": lsp.status()})),
        HostCmd::LspCatalog {} => Ok(json!({
            "servers": lsp.catalog().await,
            "supported": atlas_lsp::registry::supported_languages(),
        })),
        HostCmd::LspInstall { name, reinstall } => match lsp.install(&name, reinstall).await {
            Ok(()) => Ok(json!({"ok": true})),
            Err(f) => Ok(json!({"ok": false, "reason": f})),
        },
        HostCmd::LspUninstall { name } => lsp
            .uninstall(&name)
            .await
            .map(|removed| json!({"removed": removed}))
            .map_err(|e| format!("{e:#}")),
        HostCmd::LspRestart { name, root } => {
            Ok(json!({"restarted": lsp.restart(&name, root.as_deref()).await}))
        }
        HostCmd::LspLog { name, lines } => {
            let (path, text) = lsp.log(&name, lines);
            Ok(json!({"path": path, "text": text}))
        }
        HostCmd::LspApplyEditResult { id, applied } => {
            lsp.apply_edit_result(&id, applied);
            Ok(json!({}))
        }
        _ => return None,
    })
}

/// A connection ended: close the documents it still owns (it never sent `lsp_close`).
pub(crate) async fn connection_closed(hub: &Hub, conn_id: u64) {
    let orphans: Vec<String> = {
        let mut docs = hub.lsp_docs.lock();
        let paths: Vec<String> = docs
            .iter()
            .filter(|(_, c)| **c == conn_id)
            .map(|(p, _)| p.clone())
            .collect();
        for p in &paths {
            docs.remove(p);
        }
        paths
    };
    for p in orphans {
        hub.lsp.close(&p).await;
    }
}

/// Forward manager pushes to every connection. Holds the hub weakly so it ends with it.
pub(crate) async fn pump(hub: Weak<Hub>, mut rx: mpsc::Receiver<Push>) {
    while let Some(p) = rx.recv().await {
        let Some(hub) = hub.upgrade() else { return };
        let line = match p {
            Push::Diagnostics {
                path,
                version,
                diagnostics,
            } => {
                json!({"ev": "lsp_diagnostics", "path": path, "version": version, "diagnostics": diagnostics})
            }
            Push::Status(servers) => json!({"ev": "lsp_status", "servers": servers}),
            Push::ApplyEdit { id, edit } => json!({"ev": "lsp_apply_edit", "id": id, "edit": edit}),
        };
        hub.broadcast(line.to_string());
    }
}

// The fake server is a shell script.
#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    use serde_json::{Value, json};

    use crate::client::HostClient;
    use crate::{paths, server};

    /// A shell "language server": answers `initialize`, then publishes one diagnostic per
    /// `didOpen` and echoes `test/echo`. Enough to exercise hostd's wire layer for real.
    const FAKE: &str = r#"#!/bin/sh
send() { printf 'Content-Length: %s\r\n\r\n%s' "${#1}" "$1"; }
while IFS= read -r line; do
  line=${line%?}
  case "$line" in
    Content-Length:*) len=${line#Content-Length: }; IFS= read -r _; body=$(dd bs=1 count="$len" 2>/dev/null)
      case "$body" in
        *'"method":"initialize"'*) id=${body#*\"id\":}; id=${id%%,*}
          send "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"capabilities\":{\"textDocumentSync\":2,\"hoverProvider\":true}}}";;
        *'"method":"textDocument/didOpen"'*)
          uri=${body#*\"uri\":\"}; uri=${uri%%\"*}
          send "{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/publishDiagnostics\",\"params\":{\"uri\":\"$uri\",\"version\":1,\"diagnostics\":[{\"message\":\"boom\",\"range\":{\"start\":{\"line\":0,\"character\":0},\"end\":{\"line\":0,\"character\":1}}}]}}";;
        *'"method":"test/echo"'*) id=${body#*\"id\":}; id=${id%%,*}
          send "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":\"pong\"}";;
        *'"method":"shutdown"'*) id=${body#*\"id\":}; id=${id%%,*}
          send "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":null}";;
        *'"method":"exit"'*) exit 0;;
      esac;;
  esac
done
"#;

    #[tokio::test]
    async fn lsp_commands_and_pushes_over_the_wire() {
        let tmp = tempfile::tempdir().unwrap();
        // The registry resolves `rust-analyzer` from PATH first (probing `--version`).
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let ra = bin.join("rust-analyzer");
        std::fs::write(
            &ra,
            format!(
                "#!/bin/sh\n[ \"$1\" = --version ] && {{ echo fake 1; exit 0; }}\n{}",
                &FAKE[FAKE.find('\n').unwrap() + 1..]
            ),
        )
        .unwrap();
        std::fs::set_permissions(&ra, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = format!("{}:/usr/bin:/bin", bin.display());
        // SAFETY: set before the server starts; this test owns the process env for PATH.
        unsafe { std::env::set_var("PATH", &path) };

        let dir = tmp.path().join("hostd");
        let d2 = dir.clone();
        let srv = tokio::spawn(async move { server::serve(&d2).await });
        let client = loop {
            if let Ok(c) = HostClient::connect_dir(&dir, "test").await {
                break c;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        let info = client.call("info", json!({})).await.unwrap();
        assert!(info["caps"].as_array().unwrap().iter().any(|c| c == "lsp"));

        let proj = tmp.path().join("p");
        std::fs::create_dir_all(proj.join("src")).unwrap();
        std::fs::write(proj.join("Cargo.toml"), "[package]\n").unwrap();
        let file = proj.join("src/lib.rs").to_string_lossy().into_owned();
        let mut events = client.events();

        let r = client
            .call(
                "lsp_open",
                json!({"path": file, "language_id": "rust", "text": "fn x(){}", "version": 1}),
            )
            .await
            .unwrap();
        assert_eq!(r["status"], "ready");
        assert_eq!(r["server"], "rust-analyzer");
        assert_eq!(r["capabilities"]["hoverProvider"], true);

        let mut diag = None;
        let mut statuses = 0;
        tokio::time::timeout(Duration::from_secs(10), async {
            while diag.is_none() {
                let ev = events.recv().await.unwrap();
                match ev["ev"].as_str() {
                    Some("lsp_diagnostics") => diag = Some(ev),
                    Some("lsp_status") => statuses += 1,
                    _ => {}
                }
            }
        })
        .await
        .expect("no diagnostics push");
        let d = diag.unwrap();
        assert_eq!(d["path"], file.as_str());
        assert_eq!(d["diagnostics"][0]["message"], "boom");
        assert!(statuses >= 1, "status push before ready");

        let pong = client
            .call(
                "lsp_request",
                json!({"path": file, "method": "test/echo", "params": {}}),
            )
            .await
            .unwrap();
        assert_eq!(pong, "pong");
        let caps = client
            .call(
                "lsp_request",
                json!({"path": file, "method": "$/capabilities"}),
            )
            .await
            .unwrap();
        assert_eq!(caps["textDocumentSync"], 2);
        let st = client.call("lsp_status", json!({})).await.unwrap();
        assert_eq!(st["servers"][0]["state"], "ready");

        // errors are replies, not disconnects
        let e = client
            .call(
                "lsp_request",
                json!({"path": "/not/open.rs", "method": "x"}),
            )
            .await
            .unwrap_err();
        assert!(e.to_string().contains("not open"), "{e}");
        let e = client
            .call(
                "lsp_open",
                json!({"path": "relative.rs", "language_id": "rust", "text": ""}),
            )
            .await
            .unwrap_err();
        assert!(e.to_string().contains("absolute"), "{e}");
        let none = client
            .call(
                "lsp_open",
                json!({"path": file, "language_id": "klingon", "text": ""}),
            )
            .await
            .unwrap();
        assert_eq!(
            (none["server"].clone(), none["status"].clone()),
            (Value::Null, json!("disabled"))
        );
        // No server for the language: the reply says which languages do have one.
        let sup: Vec<&str> = none["supported"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(
            sup.contains(&"TypeScript") && sup.contains(&"Rust"),
            "{sup:?}"
        );
        // A per-language `disabled` override never starts anything.
        let off = client
            .call(
                "lsp_open",
                json!({"path": file, "language_id": "rust", "text": "", "overrides": {"disabled": true}}),
            )
            .await
            .unwrap();
        assert_eq!(off["status"], "disabled");
        // Malformed overrides are an error reply, not a dropped connection.
        let bad = client
            .call(
                "lsp_open",
                json!({"path": file, "language_id": "rust", "text": "", "overrides": {"args": 5}}),
            )
            .await
            .unwrap_err();
        assert!(bad.to_string().contains("bad overrides"), "{bad}");
        // Catalog, log and restart commands answer over the wire.
        let cat = client.call("lsp_catalog", json!({})).await.unwrap();
        assert!(
            cat["servers"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["name"] == "gopls" && s["install"] == "go")
        );
        assert!(cat["supported"].as_array().unwrap().len() > 5);
        let log = client
            .call("lsp_log", json!({"name": "rust-analyzer", "lines": 5}))
            .await
            .unwrap();
        assert!(
            log["path"]
                .as_str()
                .unwrap()
                .ends_with("lsp/logs/rust-analyzer.log")
        );
        let rs = client
            .call("lsp_restart", json!({"name": "klingon"}))
            .await
            .unwrap();
        assert_eq!(rs["restarted"], false);
        let un = client
            .call("lsp_uninstall", json!({"name": "klingon"}))
            .await
            .unwrap_err();
        assert!(un.to_string().contains("unknown server"), "{un}");
        let inst = client
            .call("lsp_install", json!({"name": "klingon"}))
            .await
            .unwrap();
        assert_eq!(inst["ok"], false);
        assert_eq!(inst["reason"]["code"], "install_failed");
        let st = client.call("lsp_status", json!({})).await.unwrap();
        let s0 = &st["servers"][0];
        assert_eq!(s0["label"], "rust-analyzer");
        assert!(s0["log"].as_str().unwrap().ends_with("rust-analyzer.log"));

        // A dropped connection's documents are closed; the new owner of a re-opened one is
        // the connection that re-opened it.
        let other = proj.join("src/other.rs").to_string_lossy().into_owned();
        let c2 = HostClient::connect_dir(&dir, "test2").await.unwrap();
        for p in [&other, &file] {
            let open = json!({"path": p, "language_id": "rust", "text": "", "version": 1});
            c2.call("lsp_open", open).await.unwrap();
        }
        drop(c2); // no lsp_close: hostd must close both
        for p in [&other, &file] {
            let mut closed = false;
            for _ in 0..100 {
                let q = json!({"path": p, "method": "$/capabilities"});
                if client.call("lsp_request", q).await.is_err() {
                    closed = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
            }
            assert!(closed, "{p} still open after its connection dropped");
        }
        // re-opening on the first connection owns it again and survives
        let open = json!({"path": file, "language_id": "rust", "text": "", "version": 1});
        client.call("lsp_open", open).await.unwrap();
        let q = json!({"path": file, "method": "$/capabilities"});
        client.call("lsp_request", q).await.unwrap();
        client
            .call("lsp_close", json!({"path": file}))
            .await
            .unwrap();
        client.call("shutdown", json!({})).await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), srv)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(!paths::socket(&dir).exists());
    }
}
