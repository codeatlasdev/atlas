use anyhow::Result;

pub mod config;
pub mod deploy;
pub mod desktop;
#[cfg(unix)]
pub mod desktop_agent;
pub mod desktop_capture;
pub mod desktop_debug;
#[cfg(target_os = "linux")]
pub(crate) mod desktop_linux;
#[cfg(windows)]
pub(crate) mod desktop_windows;
pub mod dev;
pub mod down;
pub mod hook;
pub mod hostd;
pub(crate) mod hostd_reconcile;
pub mod init;
pub mod install;
pub mod link;
pub mod mcp;
pub mod release;
pub mod self_update;
pub mod setup;
pub mod skills;
pub mod uninstall;
pub(crate) mod update_channel;
pub mod update_core;
pub mod update_prompt;
pub mod version;
pub mod wt;

/// Every top-level atlas subcommand implements this trait.
/// Parsing is handled by clap's derive macro on the args struct;
/// `run` receives the already-parsed, owned args.
#[allow(async_fn_in_trait)]
pub trait AtlasCommand {
    async fn run(self) -> Result<()>;
}
