//! One phone connection: pairing (unpaired phones), then scoped NDJSON dispatch.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use iroh::endpoint::Connection;
use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

use super::{Scope, required_scope};
use crate::protocol::{self, HostCmd, MAX_LINE_BYTES, Request};
use crate::server::Hub;

async fn read_line<R: AsyncBufRead + Unpin>(r: &mut R, buf: &mut String) -> Option<String> {
    buf.clear();
    match r.read_line(buf).await {
        Ok(0) | Err(_) => None,
        Ok(n) if n > MAX_LINE_BYTES => Some(String::new()),
        Ok(_) => Some(buf.trim_end().to_string()),
    }
}

fn scope_name(s: Scope) -> String {
    serde_json::to_value(s)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

pub(super) async fn run(hub: Arc<Hub>, conn: Connection) {
    let node = conn.remote_id().to_string();
    let Ok((send, recv)) = conn.accept_bi().await else {
        return;
    };
    let mut lines = BufReader::new(recv);
    let (tx, rx) = mpsc::unbounded_channel::<String>();
    let writer = tokio::spawn(protocol::write_lines(rx, send));
    let m = hub.mobile.clone();
    let mut buf = String::new();

    if m.device(&node).is_none() {
        let line = read_line(&mut lines, &mut buf).await.unwrap_or_default();
        let v: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        let id = v["id"].as_u64().unwrap_or(0);
        let res = if v["cmd"] == "pair" {
            m.pair(
                &node,
                v["args"]["code"].as_str().unwrap_or_default(),
                v["args"]["name"].as_str().unwrap_or("phone"),
            )
            .map(|d| json!({"device": d, "hostname": crate::server::hostname()}))
        } else {
            Err("not paired".into())
        };
        let ok = res.is_ok();
        let _ = tx.send(protocol::reply(id, res));
        if !ok {
            drop(tx);
            if let Ok(mut send) = writer.await {
                let _ = send.finish();
            }
            // Let the reply reach the phone before the connection closes.
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            conn.close(2u32.into(), b"not paired");
            return;
        }
        m.audit(&node, "pair", &json!({}));
    }

    m.track(&node, &conn);
    hub.agents.mobile_live.fetch_add(1, Ordering::Relaxed);
    let conn_id = hub.add_subscriber(tx.clone());
    let mut attached = Vec::new();
    while let Some(line) = read_line(&mut lines, &mut buf).await {
        let raw: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        let id = raw["id"].as_u64().unwrap_or(0);
        // Re-checked per request: a revoke or scope change applies at once.
        let Some(dev) = m.device(&node) else { break };
        // A paired phone scanning a new code (e.g. it lost its local list): already trusted.
        if raw["cmd"] == "pair" {
            let data = json!({"device": dev, "hostname": crate::server::hostname()});
            let _ = tx.send(protocol::reply(id, Ok(data)));
            continue;
        }
        let req: Request = match serde_json::from_value(raw.clone()) {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(protocol::reply(id, Err(format!("bad request: {e}"))));
                continue;
            }
        };
        let scope = match required_scope(&req.cmd) {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(protocol::reply(id, Err(e.into())));
                continue;
            }
        };
        if !dev.scopes.contains(&scope) {
            let _ = tx.send(protocol::reply(
                id,
                Err(format!("forbidden: {}", scope_name(scope))),
            ));
            continue;
        }
        if scope != Scope::Read && scope != Scope::TermRead {
            m.audit(&node, raw["cmd"].as_str().unwrap_or_default(), &raw["args"]);
        }
        match req.cmd {
            HostCmd::PtyAttach {
                pty,
                replay,
                output,
            } => {
                let res = hub.get(&pty).map(|s| {
                    let info = s.attach(conn_id, tx.clone(), replay, output);
                    attached.push(s);
                    info
                });
                let _ = tx.send(protocol::reply(id, res));
            }
            cmd => {
                let (hub, tx) = (hub.clone(), tx.clone());
                tokio::spawn(async move {
                    let _ = tx.send(protocol::reply(
                        id,
                        crate::server::handle(&hub, cmd, conn_id).await,
                    ));
                });
            }
        }
    }
    for s in attached {
        s.detach(conn_id);
    }
    hub.remove_subscriber(conn_id);
    hub.agents.mobile_live.fetch_sub(1, Ordering::Relaxed);
    m.untrack(&node, &conn);
    drop(tx);
    let _ = writer.await;
    conn.close(0u32.into(), b"bye");
}
