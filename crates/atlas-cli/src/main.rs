use clap::{Parser, Subcommand};

mod client;
mod commands;

#[derive(Debug, Parser)]
#[command(name = "atlas", about = "Atlas daemon CLI client")]
struct Cli {
    #[arg(long, default_value = "~/.local/share/atlas/atlas.sock")]
    socket: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(subcommand)]
    Server(commands::server::ServerCommands),

    #[command(subcommand)]
    Service(commands::service::ServiceCommands),

    #[command(subcommand)]
    Deploy(commands::deploy::DeployCommands),

    #[command(subcommand)]
    Ai(commands::ai::AiCommands),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let socket_path = shellexpand(&cli.socket);
    let mut client = client::DaemonClient::connect(&socket_path).await?;

    match cli.command {
        Commands::Server(cmd) => commands::server::handle(&mut client, cmd).await?,
        Commands::Service(cmd) => commands::service::handle(&mut client, cmd).await?,
        Commands::Deploy(cmd) => commands::deploy::handle(&mut client, cmd).await?,
        Commands::Ai(cmd) => commands::ai::handle(&mut client, cmd).await?,
    }

    Ok(())
}

fn shellexpand(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    path.to_string()
}
