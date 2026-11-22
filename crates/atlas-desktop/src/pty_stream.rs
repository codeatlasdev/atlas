//! The commands behind a terminal view's output (`apps/desktop/src/lib/ptyChannel.ts`), on top of [`crate::pty_feed`].
//!
//! `pty_stream(pty, on_frame)` attaches the pty for its title/exit events (JSON, on the app's connection), opens a
//! dedicated raw connection to the host that owns it (`pty_attach_raw`: the local hostd, or an SSH host through its
//! tunnel; the same on every OS) and feeds it to `on_frame` as binary frames. It answers with the stream's id.
//! `pty_stream_ack(id, bytes)` is the page saying it parsed that many bytes; `pty_stream_stop(id)` ends the stream.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use parking_lot::Mutex;
use serde_json::json;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Emitter};

use crate::host::{HostFail, HostHub, LOCAL};
use crate::pty_feed::{Credit, Ending, FrameSink, feed};

struct Stream {
    credit: Arc<Credit>,
    task: tauri::async_runtime::JoinHandle<()>,
}

/// Live streams by channel id.
static STREAMS: LazyLock<Mutex<HashMap<u32, Stream>>> = LazyLock::new(Default::default);

struct ChannelSink(Channel<InvokeResponseBody>);

impl FrameSink for ChannelSink {
    fn send(&self, frame: Vec<u8>) -> bool {
        self.0.send(InvokeResponseBody::Raw(frame)).is_ok()
    }
}

/// A raw connection to the host `host` owns `pty` on.
async fn open_raw(
    app: &AppHandle,
    host: &str,
    pty: &str,
) -> Result<atlas_host::client::RawPty, HostFail> {
    let raw = if host == LOCAL {
        atlas_host::client::attach_raw(&atlas_host::paths::base(), pty, true).await
    } else {
        let (sock, token) = crate::remote::endpoint(app, host)
            .ok_or_else(|| HostFail::Text(format!("{host} is not connected")))?;
        atlas_host::client::attach_raw_at(&sock, &token, pty, true).await
    };
    raw.map_err(|e| HostFail::Text(format!("{e:#}")))
}

/// Why a stream that ended did: the program exited (or its host is gone, and `host://reconnected` brings the page
/// back), or hostd cut this reader while the program runs.
async fn why_ended(hub: &HostHub, host: &str, pty: &str) -> Ending {
    let list = hub.call("pty_list", json!({"host": host})).await;
    let running = list.is_ok_and(|l| {
        l["ptys"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|p| p["pty"] == pty && p["exited"].is_null())
    });
    if running {
        Ending::Dropped
    } else {
        Ending::Exited
    }
}

/// Start feeding `on_frame` with the output of `pty`, history first. Returns the stream's id (the channel's).
#[tauri::command]
pub async fn pty_stream(
    app: AppHandle,
    hub: tauri::State<'_, Arc<HostHub>>,
    pty: String,
    on_frame: Channel<InvokeResponseBody>,
) -> Result<u32, HostFail> {
    let hub = hub.inner().clone();
    let host = hub.route(&json!({"pty": pty}));
    // Title and exit are pushed to attached connections only, and the raw stream carries bytes alone: this app's
    // own connection listens (`output: false`). The answer's title is sent as `pty_title`, as `host_call` does.
    let attached = hub
        .call_coded("pty_attach", json!({"pty": pty, "output": false}))
        .await?;
    if let Some(ev) = atlas_host::protocol::title_event(&pty, &attached) {
        let _ = app.emit("host://event", ev);
    }
    let mut raw = open_raw(&app, &host, &pty).await?;
    let replayed = raw.info["replay"].as_u64().unwrap_or(0) as usize;
    let id = on_frame.id();
    let credit = Arc::new(Credit::default());
    let sink = ChannelSink(on_frame);
    let mut streams = STREAMS.lock();
    let task = {
        let credit = credit.clone();
        tauri::async_runtime::spawn(async move {
            feed(&mut raw.reader, replayed, &sink, &credit, async {
                why_ended(&hub, &host, &pty).await
            })
            .await;
            STREAMS.lock().remove(&id);
        })
    };
    streams.insert(id, Stream { credit, task });
    Ok(id)
}

/// The page parsed `bytes` of what stream `id` sent: room for that much more.
#[tauri::command]
pub fn pty_stream_ack(id: u32, bytes: usize) {
    if let Some(s) = STREAMS.lock().get(&id) {
        s.credit.ack(bytes);
    }
}

/// End stream `id` (its raw connection closes, hostd stops feeding it).
#[tauri::command]
pub fn pty_stream_stop(id: u32) {
    if let Some(s) = STREAMS.lock().remove(&id) {
        s.task.abort();
    }
}
