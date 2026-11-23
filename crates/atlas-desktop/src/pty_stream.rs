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
    let raw = open_raw(&app, &host, &pty).await?;
    let replayed = raw.info["replay"].as_u64().unwrap_or(0) as usize;
    let id = on_frame.id();
    let credit = Arc::new(Credit::default());
    let mut streams = STREAMS.lock();
    let task = spawn_feed(
        raw,
        replayed,
        hub,
        host,
        pty,
        ChannelSink(on_frame),
        credit.clone(),
        move || {
            STREAMS.lock().remove(&id);
        },
    );
    streams.insert(id, Stream { credit, task });
    Ok(id)
}

/// Feed `sink` from `raw` until it ends, then say why (`ended` runs last, whatever the reason).
#[allow(clippy::too_many_arguments)]
fn spawn_feed<S: FrameSink + Send + Sync + 'static>(
    raw: atlas_host::client::RawPty,
    replayed: usize,
    hub: Arc<HostHub>,
    host: String,
    pty: String,
    sink: S,
    credit: Arc<Credit>,
    ended: impl FnOnce() + Send + 'static,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        // The whole connection moves in and lives as long as the stream: `raw._w`, its write half, closing tells hostd
        // the client is gone. (A task that only names `raw.reader` captures just that field, drops the rest when this
        // function returns, and every stream ended in EOF right after its replay.)
        let mut raw = raw;
        feed(&mut raw.reader, replayed, &sink, &credit, async {
            why_ended(&hub, &host, &pty).await
        })
        .await;
        ended();
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty_feed::{DROPPED, ENDED};
    use crate::test_hostd::real_hostd;
    use std::time::Duration;

    #[derive(Default, Clone)]
    struct Frames(Arc<Mutex<Vec<Vec<u8>>>>);

    impl FrameSink for Frames {
        fn send(&self, frame: Vec<u8>) -> bool {
            self.0.lock().push(frame);
            true
        }
    }

    impl Frames {
        /// Kind byte and text of every frame so far.
        fn seen(&self) -> Vec<(u8, String)> {
            self.0
                .lock()
                .iter()
                .map(|f| (f[0], String::from_utf8_lossy(&f[1..]).into_owned()))
                .collect()
        }
    }

    async fn until(what: &str, mut ok: impl FnMut() -> bool) {
        tokio::time::timeout(atlas_core::scaling::HANG, async {
            while !ok() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
    }

    // Incident (#734, release, chat-stream): every stream of a chat tab ended in EOF right after hostd's replay and
    // the page re-attached ~10 times a second, 0.4-1.4 MB of replay each. The raw connection's write half was dropped
    // when `pty_stream` returned (only `raw.reader` moved into the feed task), which tells hostd the client is gone.
    // A stream must stay open for as long as the program runs: its output after the replay arrives, nothing ends it.
    #[tokio::test]
    async fn a_stream_stays_open_after_the_replay_and_carries_live_output() {
        let (tmp, client) = real_hostd().await;
        let dir = tmp.path().join("hostd");
        let made = client
            .call(
                "pty_create",
                json!({"cwd": tmp.path(), "argv": ["sh", "-c", "echo first; sleep 1; echo second; sleep 30"]}),
            )
            .await
            .unwrap();
        let pty = made["pty"].as_str().unwrap().to_string();
        let hub = Arc::new(HostHub::default());
        hub.set(LOCAL, Some(client)).await;

        let raw = atlas_host::client::attach_raw(&dir, &pty, true)
            .await
            .unwrap();
        let replayed = raw.info["replay"].as_u64().unwrap_or(0) as usize;
        let frames = Frames::default();
        let task = spawn_feed(
            raw,
            replayed,
            hub,
            LOCAL.into(),
            pty,
            frames.clone(),
            Arc::new(Credit::default()),
            || {},
        );

        let over = |f: &Frames| {
            let seen = f.seen();
            seen.iter().any(|(k, _)| matches!(*k, DROPPED | ENDED))
                || seen.iter().any(|(_, t)| t.contains("second"))
        };
        until(
            "the output the program prints after the replay, or the stream's end",
            || over(&frames),
        )
        .await;
        let kinds: Vec<u8> = frames.seen().iter().map(|(k, _)| *k).collect();
        assert!(
            !kinds.contains(&DROPPED) && !kinds.contains(&ENDED),
            "the stream ended while the program runs: frame kinds {kinds:?}"
        );
        task.abort();
    }
}
