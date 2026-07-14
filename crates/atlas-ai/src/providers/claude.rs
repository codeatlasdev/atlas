use async_trait::async_trait;
use serde::Deserialize;

use atlas_core::ports::ai::{AiProvider, AiResponse, Conversation, Role};
use atlas_core::{AtlasError, Result};

pub struct ClaudeProvider {
    api_key: String,
    client: reqwest::Client,
}

impl ClaudeProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl AiProvider for ClaudeProvider {
    fn name(&self) -> &str {
        "claude"
    }

    async fn chat(&self, conversation: &Conversation) -> Result<AiResponse> {
        if self.api_key.is_empty() {
            return Err(AtlasError::AiProvider(
                "Claude API key not configured".to_string(),
            ));
        }

        let (system, messages) = build_claude_messages(conversation);

        let mut body = serde_json::json!({
            "model": &conversation.model,
            "max_tokens": 4096,
            "messages": messages,
        });
        if let Some(sys) = system {
            body["system"] = serde_json::Value::String(sys);
        }

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AtlasError::AiProvider(format!("Claude request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AtlasError::AiProvider(format!(
                "Claude API error {status}: {text}"
            )));
        }

        let resp: ClaudeResponse = response
            .json()
            .await
            .map_err(|e| AtlasError::AiProvider(format!("Claude parse error: {e}")))?;

        let content = resp
            .content
            .into_iter()
            .filter_map(|block| {
                if block.block_type == "text" {
                    Some(block.text)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");

        Ok(AiResponse {
            content,
            model: resp.model,
            tokens_used: resp.usage.map(|u| u.input_tokens + u.output_tokens),
        })
    }

    async fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}

fn build_claude_messages(conversation: &Conversation) -> (Option<String>, Vec<serde_json::Value>) {
    let mut system = None;
    let mut messages = Vec::new();

    for msg in &conversation.messages {
        match msg.role {
            Role::System => {
                system = Some(msg.content.clone());
            }
            Role::User => {
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": &msg.content,
                }));
            }
            Role::Assistant => {
                messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": &msg.content,
                }));
            }
        }
    }

    (system, messages)
}

#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    content: Vec<ContentBlock>,
    model: String,
    usage: Option<ClaudeUsage>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    text: String,
}

#[derive(Debug, Deserialize)]
struct ClaudeUsage {
    input_tokens: u32,
    output_tokens: u32,
}
