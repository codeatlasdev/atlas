//! ACP (Agent Client Protocol) transport layer.
//!
//! Manages a subprocess running `<agent> acp` and provides bidirectional
//! JSON-RPC 2.0 communication over stdin/stdout.
//!
//! Architecture:
//! - Spawns agent binary as child process with piped stdin/stdout
//! - Reader task: reads stdout line-by-line, dispatches responses and notifications
//! - Writer: serializes JSON-RPC messages to stdin
//! - Notifications (session/update) are broadcast to subscribers via tokio::broadcast
//! - Requests use oneshot channels for response delivery

pub mod events;
pub mod transport;

pub use events::*;
pub use transport::{AcpClientHandler, AcpSpawnConfig, AcpTransport, DirectClientHandler};
