use clap::Subcommand;
use serde_json::json;

use crate::client::DaemonClient;

#[derive(Debug, Subcommand)]
pub enum ServerCommands {
    List,
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        host: String,
        #[arg(long, default_value = "root")]
        user: String,
        #[arg(long, default_value_t = 22)]
        port: u16,
    },
    Remove {
        #[arg(long)]
        id: String,
    },
    Status {
        #[arg(long)]
        id: String,
    },
}

pub async fn handle(client: &mut DaemonClient, cmd: ServerCommands) -> anyhow::Result<()> {
    let response = match cmd {
        ServerCommands::List => client.call("servers.list", json!({})).await?,
        ServerCommands::Add {
            name,
            host,
            user,
            port,
        } => {
            client
                .call("servers.add", json!({ "name": name, "host": host, "user": user, "port": port }))
                .await?
        }
        ServerCommands::Remove { id } => {
            client.call("servers.remove", json!({ "id": id })).await?
        }
        ServerCommands::Status { id } => {
            client.call("servers.status", json!({ "id": id })).await?
        }
    };

    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}
