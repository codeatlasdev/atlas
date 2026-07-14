use clap::{Parser, Subcommand};

mod client;
mod commands;

#[derive(Debug, Parser)]
#[command(name = "atlas", about = "Atlas daemon CLI client")]
struct Cli {
    #[arg(long, default_value = "~/.atlas/atlas.sock")]
    socket: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Server management
    #[command(subcommand)]
    Server(commands::server::ServerCommands),

    /// Service management
    #[command(subcommand)]
    Service(commands::service::ServiceCommands),

    /// Deploy commands
    #[command(subcommand)]
    Deploy(commands::deploy::DeployCommands),

    /// AI chat
    #[command(subcommand)]
    Ai(commands::ai::AiCommands),

    /// Terminal session management
    #[command(subcommand)]
    Terminal(TerminalCommands),

    /// Agent session management
    #[command(subcommand)]
    Agent(AgentCommands),

    /// Check daemon connection
    Ping,
}

#[derive(Debug, Subcommand)]
enum TerminalCommands {
    /// Create a new terminal session
    Create {
        #[arg(short, long, default_value = "/bin/zsh")]
        shell: String,
        #[arg(long, default_value = ".")]
        cwd: String,
    },
    /// List terminal sessions
    List,
    /// Kill a terminal session
    Kill { session_id: String },
}

#[derive(Debug, Subcommand)]
enum AgentCommands {
    /// Spawn a new agent session
    Spawn {
        #[arg(short, long, default_value = "kiro")]
        adapter: String,
        /// The prompt to send to the agent
        prompt: String,
        #[arg(long, default_value = ".")]
        cwd: String,
    },
    /// List agent sessions
    List,
    /// Stop an agent session
    Stop { session_id: String },
    /// Send a prompt to an agent
    Prompt { session_id: String, prompt: String },
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
        Commands::Terminal(cmd) => handle_terminal(&mut client, cmd).await?,
        Commands::Agent(cmd) => handle_agent(&mut client, cmd).await?,
        Commands::Ping => {
            let result = client.call("servers.list", serde_json::json!({})).await?;
            println!("✓ Connected to daemon. Response: {result}");
        }
    }

    Ok(())
}

async fn handle_terminal(
    client: &mut client::DaemonClient,
    cmd: TerminalCommands,
) -> anyhow::Result<()> {
    match cmd {
        TerminalCommands::Create { shell, cwd } => {
            let cwd = std::fs::canonicalize(shellexpand(&cwd))
                .unwrap_or_else(|_| std::path::PathBuf::from(&cwd));
            let result = client
                .call(
                    "terminal.create",
                    serde_json::json!({
                        "shell": shell,
                        "rows": 24,
                        "cols": 80,
                        "cwd": cwd.to_string_lossy(),
                    }),
                )
                .await?;
            println!("{result}");
        }
        TerminalCommands::List => {
            let result = client.call("terminal.list", serde_json::json!({})).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        TerminalCommands::Kill { session_id } => {
            let result = client
                .call("terminal.kill", serde_json::json!({"session_id": session_id}))
                .await?;
            println!("{result}");
        }
    }
    Ok(())
}

async fn handle_agent(
    client: &mut client::DaemonClient,
    cmd: AgentCommands,
) -> anyhow::Result<()> {
    match cmd {
        AgentCommands::Spawn {
            adapter,
            prompt,
            cwd,
        } => {
            let cwd = std::fs::canonicalize(shellexpand(&cwd))
                .unwrap_or_else(|_| std::path::PathBuf::from(&cwd));
            let result = client
                .call(
                    "agent.spawn",
                    serde_json::json!({
                        "adapter": adapter,
                        "prompt": prompt,
                        "cwd": cwd.to_string_lossy(),
                        "permission": "autonomous",
                    }),
                )
                .await?;
            println!("{result}");
        }
        AgentCommands::List => {
            let result = client.call("agent.list", serde_json::json!({})).await?;
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        AgentCommands::Stop { session_id } => {
            let result = client
                .call("agent.stop", serde_json::json!({"session_id": session_id}))
                .await?;
            println!("{result}");
        }
        AgentCommands::Prompt { session_id, prompt } => {
            let result = client
                .call(
                    "agent.prompt",
                    serde_json::json!({"session_id": session_id, "prompt": prompt}),
                )
                .await?;
            println!("{result}");
        }
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
