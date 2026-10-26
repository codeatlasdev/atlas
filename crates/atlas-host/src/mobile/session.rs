//! One phone connection: pairing (unpaired phones), then scoped NDJSON dispatch.

use std::sync::Arc;

use iroh::endpoint::Connection;
use serde_json::{Value, json};
use tokio::io::BufReader;

use super::{Scope, push_scope, required_scope};
use crate::protocol::{self, HostCmd, Request};
use crate::server::{Hub, LongLine, read_line};

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
    let (tx, rx) = crate::pty::out_channel();
    let writer = tokio::spawn(protocol::write_lines(rx, send));
    let m = hub.mobile.clone();

    if m.device(&node).is_none() {
        let line = read_line(&mut lines, LongLine::Close)
            .await
            .unwrap_or_default();
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
        let _ = tx.force(protocol::reply(id, res));
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
    // Pushes (broadcasts, terminal output) pass this phone's scopes as they are now,
    // so a scope taken away on the desktop stops them at once; replies go straight out.
    let (push, mut pushes) = crate::pty::out_channel();
    let filter = {
        let (m, node, tx) = (m.clone(), node.clone(), tx.clone());
        tokio::spawn(async move {
            #[derive(serde::Deserialize)]
            struct Ev<'a> {
                #[serde(borrow)]
                ev: std::borrow::Cow<'a, str>,
            }
            while let Some(line) = pushes.recv().await {
                let Some(dev) = m.device(&node) else { break };
                let allowed = serde_json::from_str::<Ev>(&line)
                    .ok()
                    .and_then(|e| push_scope(&e.ev))
                    .is_some_and(|s| dev.scopes.contains(&s));
                if !allowed {
                    continue;
                }
                // The phone's own queue is full: it is not reading. Stop draining `push`, so
                // that fills to its cap too and the pty drops this phone (`pty_dropped`; the
                // phone reattaches with replay) instead of hostd buffering for it.
                while tx.send(line.clone()).is_err() {
                    if tx.is_closed() {
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
            }
        })
    };
    let conn_id = hub.add_subscriber(push.clone());
    let mut attached = Vec::new();
    let mut link = crate::lab::Link::default();
    while let Some(line) = read_line(&mut lines, LongLine::Close).await {
        let raw: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        let id = raw["id"].as_u64().unwrap_or(0);
        // Re-checked per request: a revoke or scope change applies at once.
        let Some(dev) = m.device(&node) else { break };
        // A paired phone scanning a new code (e.g. it lost its local list): already trusted.
        if raw["cmd"] == "pair" {
            let data = json!({"device": dev, "hostname": crate::server::hostname()});
            let _ = tx.force(protocol::reply(id, Ok(data)));
            continue;
        }
        let req: Request = match serde_json::from_value(raw.clone()) {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.force(protocol::reply(id, Err(format!("bad request: {e}"))));
                continue;
            }
        };
        let scope = match required_scope(&req.cmd) {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.force(protocol::reply(id, Err(e.into())));
                continue;
            }
        };
        if !dev.scopes.contains(&scope) {
            let _ = tx.force(protocol::reply(
                id,
                Err(format!("forbidden: {}", scope_name(scope))),
            ));
            continue;
        }
        if scope != Scope::Read && scope != Scope::TermRead {
            m.audit(&node, raw["cmd"].as_str().unwrap_or_default(), &raw["args"]);
        }
        // The lab lane (`atlas hostd --lab`): terminals and agent actions belong to the main
        // hostd, reached through this phone's own link; what `Route::Local` leaves runs here.
        if let Some(lab) = &hub.lab {
            let args = &raw["args"];
            match &req.cmd {
                HostCmd::PtyAttach { .. } => {
                    let res = link.attach(lab, args.clone(), push.clone()).await;
                    let _ = tx.send(protocol::reply(id, res));
                    continue;
                }
                HostCmd::PtyDetach { .. } => {
                    let _ = tx.send(protocol::reply(id, link.detach(args.clone()).await));
                    continue;
                }
                cmd if !matches!(crate::lab::route(cmd), crate::lab::Route::Local) => {
                    let (lab, tx) = (lab.clone(), tx.clone());
                    let (name, args) = (
                        raw["cmd"].as_str().unwrap_or_default().to_string(),
                        args.clone(),
                    );
                    let cmd = req.cmd;
                    tokio::spawn(async move {
                        if let Some(res) = crate::lab::dispatch(&lab, &cmd, &name, &args).await {
                            let _ = tx.send(protocol::reply(id, res));
                        }
                    });
                    continue;
                }
                _ => {}
            }
        }
        match req.cmd {
            HostCmd::PtyAttach {
                pty,
                replay,
                output,
            } => {
                let res = hub.get(&pty).map(|s| {
                    let info = s.attach(conn_id, push.clone(), replay, output);
                    attached.push(s);
                    info
                });
                let _ = tx.force(protocol::reply(id, res));
            }
            cmd => {
                let (hub, tx) = (hub.clone(), tx.clone());
                tokio::spawn(async move {
                    let _ = tx.force(protocol::reply(
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
    m.untrack(&node, &conn);
    // Detached and unsubscribed: nothing more for this phone.
    filter.abort();
    drop(tx);
    let _ = writer.await;
    conn.close(0u32.into(), b"bye");
}
