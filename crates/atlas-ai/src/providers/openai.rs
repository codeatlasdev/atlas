use async_trait::async_trait;
use serde::Deserialize;

use atlas_core::ports::ai::{AiProvider, AiResponse, Conversation, Role};
use atlas_core::{AtlasError, Result};

pub struct OpenAiProvider {
    api_key: String,
    client: reqwest::Client,
}

impl OpenAiProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl AiProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    async fn chat(&self, conversation: &Conversation) -> Result<AiResponse> {
        if self.api_key.is_empty() {
            return Err(AtlasError::AiProvider(
                "OpenAI API key not configured".to_string(),
            ));
        }

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
        });

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AtlasError::AiProvider(format!("OpenAI request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(AtlasError::AiProvider(format!(
                "OpenAI API error {status}: {text}"
            )));
        }

        let resp: OpenAiResponse = response
            .json()
            .await
            .map_err(|e| AtlasError::AiProvider(format!("OpenAI parse error: {e}")))?;

        let choice = resp
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| AtlasError::AiProvider("No choices in OpenAI response".to_string()))?;

        Ok(AiResponse {
            content: choice.message.content,
            model: resp.model,
            tokens_used: resp.usage.map(|u| u.total_tokens),
        })
    }

    async fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    model: String,
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    total_tokens: u32,
}
