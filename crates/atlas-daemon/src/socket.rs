use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::Serialize;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{broadcast, mpsc, Mutex};

use crate::app::AppState;
use crate::router;

/// Notification pushed from server to client (no id, no response expected)
#[derive(Debug, Serialize)]
struct Notification {
    method: String,
    params: serde_json::Value,
}

/// Per-connection state: tracks which terminal sessions are subscribed
struct ConnectionState {
    /// Channel to send outbound messages (responses + notifications) to the writer task
    outbound_tx: mpsc::UnboundedSender<String>,
    /// Terminal subscriptions: session_id → abort handle
    subscriptions: HashMap<String, tokio::task::JoinHandle<()>>,
}

pub async fn serve(socket_path: &Path, state: Arc<AppState>) -> anyhow::Result<()> {
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }

    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(socket_path)?;
    tracing::info!(path = %socket_path.display(), "unix socket listening");

    loop {
        let (stream, _addr) = listener.accept().await?;
        let state = Arc::clone(&state);

        tokio::spawn(async move {
            let (reader, mut writer) = stream.into_split();
            let mut reader = BufReader::new(reader);

            // Outbound channel: both responses and notifications go through here
            let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<String>();

            // Writer task: drains outbound channel → socket
            let writer_handle = tokio::spawn(async move {
                while let Some(msg) = outbound_rx.recv().await {
                    if writer.write_all(msg.as_bytes()).await.is_err() {
                        break;
                    }
                }
            });

            let conn_state = Arc::new(Mutex::new(ConnectionState {
                outbound_tx: outbound_tx.clone(),
                subscriptions: HashMap::new(),
            }));

            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        let trimmed = line.trim().to_string();
                        let response = router::dispatch(&state, &trimmed).await;

                        // Check if this is a terminal.attach — start subscription
                        if let Ok(req) = serde_json::from_str::<router::Request>(&trimmed) {
                            if req.method == "terminal.attach" {
                                if let Some(ref result) = response.result {
                                    if let Some(sid) = result.get("session_id").and_then(|v| v.as_str()) {
                                        start_terminal_subscription(
                                            &state,
                                            &conn_state,
                                            sid,
                                        )
                                        .await;
                                    }
                                }
                            } else if req.method == "terminal.detach" {
                                if let Ok(p) = serde_json::from_value::<DetachParams>(req.params) {
                                    stop_terminal_subscription(&conn_state, &p.session_id).await;
                                }
                            }
                        }

                        // Send response
                        let mut out = serde_json::to_string(&response).unwrap_or_default();
                        out.push('\n');
                        let _ = outbound_tx.send(out);
                    }
                    Err(_) => break,
                }
            }

            // Cleanup: abort all subscriptions
            let mut cs = conn_state.lock().await;
            for (_, handle) in cs.subscriptions.drain() {
                handle.abort();
            }
            drop(cs);
            writer_handle.abort();
        });
    }
}

#[derive(serde::Deserialize)]
struct DetachParams {
    session_id: String,
}

async fn start_terminal_subscription(
    state: &Arc<AppState>,
    conn_state: &Arc<Mutex<ConnectionState>>,
    session_id: &str,
) {
    let Ok((_, mut rx)) = state.pty_manager.attach(session_id).await else {
        return;
    };

    let sid = session_id.to_string();
    let conn = Arc::clone(conn_state);

    let handle = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(data) => {
                    let notification = Notification {
                        method: "terminal.output".to_string(),
                        params: json!({
                            "session_id": sid,
                            "data": BASE64.encode(&data),
                        }),
                    };
                    let mut msg = serde_json::to_string(&notification).unwrap_or_default();
                    msg.push('\n');

                    let cs = conn.lock().await;
                    if cs.outbound_tx.send(msg).is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::debug!(lagged = n, "terminal output subscriber lagged");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    let mut cs = conn_state.lock().await;
    // Abort previous subscription for same session if exists
    if let Some(old) = cs.subscriptions.insert(session_id.to_string(), handle) {
        old.abort();
    }
}

async fn stop_terminal_subscription(
    conn_state: &Arc<Mutex<ConnectionState>>,
    session_id: &str,
) {
    let mut cs = conn_state.lock().await;
    if let Some(handle) = cs.subscriptions.remove(session_id) {
        handle.abort();
    }
}
