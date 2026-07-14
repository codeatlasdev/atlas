use async_trait::async_trait;

use atlas_core::ports::ai::{AiProvider, AiResponse, Conversation};
use atlas_core::{AtlasError, Result};

pub struct OllamaProvider {
    base_url: String,
    #[allow(dead_code)]
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
        // TODO: implement Ollama /api/chat endpoint
        let _ = &self.base_url;
        let _ = conversation;
        Err(AtlasError::AiProvider(
            "Ollama provider not yet implemented".to_string(),
        ))
    }

    async fn is_available(&self) -> bool {
        // TODO: ping /api/tags to check availability
        true
    }
}
