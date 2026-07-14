use serde::{Deserialize, Serialize};

pub use atlas_core::ports::ai::{AiResponse, Conversation, Message, Role};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
}

impl ChatRequest {
    pub fn into_conversation(self) -> Conversation {
        Conversation {
            messages: self.messages,
            model: self.model,
        }
    }
}
