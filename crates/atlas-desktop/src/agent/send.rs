//! Submitting text to an agent terminal, in Rust: one implementation for the main window (`agents.send`), the CLI
//! (`terminal_send`) and the dock, which has to work while no webview is focused (or the main window is closed).
//!
//! The text goes in as a bracketed paste (keeps newlines, no `@` or `/` autocomplete popups in the agent's TUI),
//! then Enter once the TUI has taken the paste. Writes to one pty are serialized: two sends racing would
//! interleave one's Enter into the other's paste.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use serde_json::json;
use tauri::{AppHandle, Manager};
use tokio::sync::Mutex as AsyncMutex;

use crate::host::HostHub;

/// How long the TUI gets to take a paste before Enter (was `PASTE_SETTLE_MS` in `agents.svelte.ts`).
pub const PASTE_SETTLE: Duration = Duration::from_millis(150);

/// The two writes of one send: the bracketed paste (ESC removed from the text, it would end the paste early and
/// let the rest run as keystrokes), then the Enter.
pub fn paste_frames(text: &str) -> (Vec<u8>, &'static [u8]) {
    let mut paste = Vec::with_capacity(text.len() + 12);
    paste.extend_from_slice(b"\x1b[200~");
    paste.extend(text.bytes().filter(|b| *b != 0x1b));
    paste.extend_from_slice(b"\x1b[201~");
    (paste, b"\r")
}

/// One lock per pty, so sends to the same terminal run one after the other.
#[derive(Default)]
pub struct Sends {
    locks: parking_lot::Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl Sends {
    fn lock_of(&self, pty: &str) -> Arc<AsyncMutex<()>> {
        self.locks
            .lock()
            .entry(pty.to_string())
            .or_default()
            .clone()
    }
}

async fn write(hub: &HostHub, pty: &str, bytes: &[u8]) -> Result<(), String> {
    let data = base64::engine::general_purpose::STANDARD.encode(bytes);
    hub.call("pty_write", json!({"pty": pty, "data": data}))
        .await
        .map(|_| ())
}

/// Paste `text` into `pty` and press Enter. Errors are hostd's (terminal gone, host not reachable).
pub async fn send_text(app: &AppHandle, pty: &str, text: &str) -> Result<(), String> {
    let hub = app.state::<Arc<HostHub>>().inner().clone();
    let lock = app.state::<Sends>().lock_of(pty);
    let _turn = lock.lock().await;
    let (paste, enter) = paste_frames(text);
    write(&hub, pty, &paste).await?;
    tokio::time::sleep(PASTE_SETTLE).await;
    write(&hub, pty, enter).await
}

/// `agents.send(tabId, text)` and the CLI's `terminal_send`: the main window resolves the tab's pty.
#[tauri::command]
pub async fn agent_send(app: AppHandle, pty: String, text: String) -> Result<(), String> {
    send_text(&app, &pty, &text).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden test: the exact bytes `agents.svelte.ts` used to write (`\x1b[200~…\x1b[201~`, then `\r`), so moving the
    /// framing into Rust changed nothing the agents see.
    #[test]
    fn the_frames_are_the_bracketed_paste_then_enter_the_old_typescript_wrote() {
        let (paste, enter) = paste_frames("fix the 500\non /login");
        assert_eq!(paste, b"\x1b[200~fix the 500\non /login\x1b[201~");
        assert_eq!(enter, b"\r");
    }

    /// An ESC in the text would close the paste early (`ESC [ 2 0 1 ~`) and the rest would run as keystrokes.
    #[test]
    fn escape_bytes_in_the_text_cannot_end_the_paste_early() {
        let (paste, _) = paste_frames("a\x1b[201~rm -rf ~\x1b");
        assert_eq!(paste, b"\x1b[200~a[201~rm -rf ~\x1b[201~");
        assert_eq!(
            paste.iter().filter(|b| **b == 0x1b).count(),
            2,
            "only our own two"
        );
    }

    #[test]
    fn multibyte_text_is_kept_whole() {
        let (paste, _) = paste_frames("olá, agente ✓");
        assert_eq!(&paste[6..paste.len() - 6], "olá, agente ✓".as_bytes());
    }

    /// Two sends to one terminal must queue (paste, Enter, paste, Enter), never interleave; different terminals
    /// do not wait for each other.
    #[tokio::test]
    async fn sends_to_one_pty_are_serialized_and_other_ptys_are_free() {
        let sends = Sends::default();
        let a1 = sends.lock_of("a");
        let held = a1.lock().await;
        assert!(sends.lock_of("a").try_lock().is_err(), "same pty waits");
        assert!(
            sends.lock_of("b").try_lock().is_ok(),
            "another pty does not"
        );
        drop(held);
        assert!(sends.lock_of("a").try_lock().is_ok());
    }
}
