use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub messages: Vec<Message>,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiResponse {
    pub content: String,
    pub model: String,
    pub tokens_used: Option<u32>,
}

#[async_trait]
pub trait AiProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn chat(&self, conversation: &Conversation) -> Result<AiResponse>;
    async fn is_available(&self) -> bool;
}
