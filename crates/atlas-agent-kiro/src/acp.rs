#![allow(unused)]

use std::path::Path;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tracing::{debug, error};

#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    id: Option<u64>,
    result: Option<serde_json::Value>,
    error: Option<serde_json::Value>,
}

pub struct AcpClient {
    child: Child,
    next_id: u64,
}

impl AcpClient {
    pub async fn spawn(binary: &Path) -> std::io::Result<Self> {
        let child = Command::new(binary)
            .arg("acp")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()?;

        Ok(Self { child, next_id: 1 })
    }

    pub async fn initialize(&mut self) -> Result<serde_json::Value, String> {
        self.send_request("initialize", serde_json::json!({})).await
    }

    pub async fn new_session(&mut self) -> Result<String, String> {
        let result = self
            .send_request("session/new", serde_json::json!({}))
            .await?;
        result
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "missing session_id in response".to_string())
    }

    pub async fn send_prompt(
        &mut self,
        session_id: &str,
        prompt: &str,
    ) -> Result<serde_json::Value, String> {
        self.send_request(
            "session/prompt",
            serde_json::json!({
                "session_id": session_id,
                "prompt": prompt,
            }),
        )
        .await
    }

    pub async fn cancel(&mut self, session_id: &str) -> Result<serde_json::Value, String> {
        self.send_request(
            "session/cancel",
            serde_json::json!({ "session_id": session_id }),
        )
        .await
    }

    async fn send_request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_id,
            method: method.to_string(),
            params,
        };
        self.next_id += 1;

        let stdin = self
            .child
            .stdin
            .as_mut()
            .ok_or_else(|| "stdin not available".to_string())?;

        let mut line = serde_json::to_string(&request).map_err(|e| e.to_string())?;
        line.push('\n');
        stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        stdin.flush().await.map_err(|e| e.to_string())?;

        self.read_response().await
    }

    async fn read_response(&mut self) -> Result<serde_json::Value, String> {
        let stdout = self
            .child
            .stdout
            .as_mut()
            .ok_or_else(|| "stdout not available".to_string())?;

        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .map_err(|e| e.to_string())?;

        let response: JsonRpcResponse =
            serde_json::from_str(&line).map_err(|e| e.to_string())?;

        if let Some(error) = response.error {
            return Err(format!("RPC error: {error}"));
        }

        Ok(response.result.unwrap_or(serde_json::Value::Null))
    }

    pub async fn kill(&mut self) {
        let _ = self.child.kill().await;
    }
}

impl Drop for AcpClient {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}
