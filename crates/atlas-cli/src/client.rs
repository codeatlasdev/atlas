use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

pub struct DaemonClient {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
}

impl DaemonClient {
    pub async fn connect(socket_path: &str) -> Result<Self> {
        let stream = UnixStream::connect(socket_path)
            .await
            .with_context(|| format!("failed to connect to daemon at {socket_path}"))?;

        let (reader, writer) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(reader),
            writer,
        })
    }

    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let request = serde_json::json!({
            "method": method,
            "params": params,
            "id": 1
        });

        let mut msg = serde_json::to_string(&request)?;
        msg.push('\n');
        self.writer.write_all(msg.as_bytes()).await?;

        let mut line = String::new();
        self.reader.read_line(&mut line).await?;

        let response: Value = serde_json::from_str(line.trim())?;

        if let Some(error) = response.get("error") {
            anyhow::bail!("daemon error: {}", error);
        }

        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }
}
