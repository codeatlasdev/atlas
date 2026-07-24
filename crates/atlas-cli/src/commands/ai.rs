use clap::Subcommand;
use serde_json::json;

use crate::client::DaemonClient;

#[derive(Debug, Subcommand)]
pub enum AiCommands {
    Chat {
        #[arg(long)]
        message: String,
        #[arg(long, default_value = "claude-sonnet-4-20250514")]
        model: String,
    },
}

pub async fn handle(client: &mut DaemonClient, cmd: AiCommands) -> anyhow::Result<()> {
    let response = match cmd {
        AiCommands::Chat { message, model } => {
            client
                .call("ai.chat", json!({ "message": message, "model": model }))
                .await?
        }
    };

    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
