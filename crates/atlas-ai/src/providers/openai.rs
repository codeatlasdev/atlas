use async_trait::async_trait;

use atlas_core::ports::ai::{AiProvider, AiResponse, Conversation};
use atlas_core::{AtlasError, Result};

pub struct OpenAiProvider {
    api_key: String,
    #[allow(dead_code)]
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

        // TODO: implement actual OpenAI Chat Completions API call
        let _ = conversation;
        Err(AtlasError::AiProvider(
            "OpenAI provider not yet implemented".to_string(),
        ))
    }

    async fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }
}
