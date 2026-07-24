use thiserror::Error;

#[derive(Debug, Error)]
pub enum AtlasError {
    #[error("SSH error: {0}")]
    Ssh(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("AI provider error: {0}")]
    AiProvider(String),

    #[error("Server management error: {0}")]
    ServerManagement(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
