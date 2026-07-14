use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

pub struct DaemonBridge {
    socket_path: PathBuf,
    request_id: AtomicU64,
}

impl DaemonBridge {
    pub fn new() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"));

        let socket_path = home.join(".atlas").join("atlas.sock");

        Self {
            socket_path,
            request_id: AtomicU64::new(1),
        }
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let stream = UnixStream::connect(&self.socket_path)
            .await
            .with_context(|| {
                format!(
                    "failed to connect to daemon at {}",
                    self.socket_path.display()
                )
            })?;

        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        let id = self.request_id.fetch_add(1, Ordering::Relaxed);

        let request = json!({
            "method": method,
            "params": params,
            "id": id.to_string()
        });

        let mut msg = serde_json::to_string(&request)?;
        msg.push('\n');
        writer.write_all(msg.as_bytes()).await?;

        let mut response_line = String::new();
        reader.read_line(&mut response_line).await?;

        let response: Value = serde_json::from_str(response_line.trim())
            .context("failed to parse daemon response")?;

        if let Some(error) = response.get("error") {
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown daemon error");
            anyhow::bail!("daemon error: {message}");
        }

        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }
}
