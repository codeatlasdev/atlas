use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;

use atlas_core::ports::ai::{AiProvider, AiResponse, Conversation};
use atlas_core::{AtlasError, Result};

pub struct AiRouter {
    providers: HashMap<String, Arc<dyn AiProvider>>,
}

impl Default for AiRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl AiRouter {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: impl Into<String>, provider: Arc<dyn AiProvider>) {
        self.providers.insert(name.into(), provider);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn AiProvider>> {
        self.providers.get(name)
    }

    pub fn available_providers(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }
}

#[async_trait]
impl AiProvider for AiRouter {
    fn name(&self) -> &str {
        "router"
    }

    async fn chat(&self, conversation: &Conversation) -> Result<AiResponse> {
        let provider_name = extract_provider_from_model(&conversation.model);

        let provider = self.providers.get(provider_name).ok_or_else(|| {
            AtlasError::AiProvider(format!("unknown provider: {provider_name}"))
        })?;

        provider.chat(conversation).await
    }

    async fn is_available(&self) -> bool {
        !self.providers.is_empty()
    }
}

fn extract_provider_from_model(model: &str) -> &str {
    if model.starts_with("claude") || model.starts_with("anthropic") {
        "claude"
    } else if model.starts_with("gpt") || model.starts_with("o1") || model.starts_with("o3") {
        "openai"
    } else {
        "ollama"
    }
}
