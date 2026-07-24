use async_trait::async_trait;
use serde::Deserialize;

use atlas_core::ports::ai::{AiProvider, AiResponse, Conversation, Role};
use atlas_core::{AtlasError, Result};

pub struct OllamaProvider {
    base_url: String,
    client: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(base_url: Option<String>) -> Self {
        Self {
            base_url: base_url.unwrap_or_else(|| "http://localhost:11434".to_string()),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl AiProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn chat(&self, conversation: &Conversation) -> Result<AiResponse> {
        let messages: Vec<serde_json::Value> = conversation
            .messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                };
                serde_json::json!({
                    "role": role,
                    "content": &msg.content,
                })
            })
            .collect();

        let body = serde_json::json!({
            "model": &conversation.model,
            "messages": messages,
            "stream": false,
        });

        let url = format!("{}/api/chat", self.base_url);
        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AtlasError::AiProvider(format!("Ollama request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AtlasError::AiProvider(format!(
                "Ollama API error {status}: {text}"
            )));
        }

        let resp: OllamaResponse = response
            .json()
            .await
            .map_err(|e| AtlasError::AiProvider(format!("Ollama parse error: {e}")))?;

        Ok(AiResponse {
            content: resp.message.content,
            model: conversation.model.clone(),
            tokens_used: resp.eval_count,
        })
    }

    async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        self.client.get(&url).send().await.is_ok()
    }
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
    eval_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OllamaMessage {
    content: String,
}
