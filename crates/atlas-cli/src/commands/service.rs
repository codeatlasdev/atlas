use clap::Subcommand;
use serde_json::json;

use crate::client::DaemonClient;

#[derive(Debug, Subcommand)]
pub enum ServiceCommands {
    List {
        #[arg(long)]
        server_id: String,
    },
    Restart {
        #[arg(long)]
        server_id: String,
        #[arg(long)]
        unit: String,
    },
    Logs {
        #[arg(long)]
        server_id: String,
        #[arg(long)]
        unit: String,
        #[arg(long, default_value_t = 50)]
        lines: u32,
    },
}

pub async fn handle(client: &mut DaemonClient, cmd: ServiceCommands) -> anyhow::Result<()> {
    let response = match cmd {
        ServiceCommands::List { server_id } => {
            client
                .call("services.list", json!({ "server_id": server_id }))
                .await?
        }
        ServiceCommands::Restart { server_id, unit } => {
            client
                .call("services.restart", json!({ "server_id": server_id, "unit": unit }))
                .await?
        }
        ServiceCommands::Logs {
            server_id,
            unit,
            lines,
        } => {
            client
                .call("services.logs", json!({ "server_id": server_id, "unit": unit, "lines": lines }))
                .await?
        }
    };

    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
