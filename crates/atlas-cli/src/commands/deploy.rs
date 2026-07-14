use clap::Subcommand;
use serde_json::json;

use crate::client::DaemonClient;

#[derive(Debug, Subcommand)]
pub enum DeployCommands {
    Run {
        #[arg(long)]
        server_id: String,
        #[arg(long)]
        service: String,
    },
}

pub async fn handle(client: &mut DaemonClient, cmd: DeployCommands) -> anyhow::Result<()> {
    let response = match cmd {
        DeployCommands::Run { server_id, service } => {
            client
                .call("deploy.run", json!({ "server_id": server_id, "service": service }))
                .await?
        }
    };

    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
