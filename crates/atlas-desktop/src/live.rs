//! One background task per registered project: holds a `subscribe` connection to the
//! project's runner and forwards its lines as the Tauri event `project://<id>`. Service
//! output (`log` lines, up to 100k/s from a chatty service) is coalesced into `logs`
//! batches, one IPC message per frame instead of one per line.
//! While nothing is subscribable it polls the status file every 2s.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use atlas_tui::{control, status};
use notify::{RecursiveMode, Watcher};
use serde_json::{Value, json};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

const POLL: Duration = Duration::from_secs(2);
const NOTIFY_EVERY: Duration = Duration::from_secs(300);
/// Coalescing window for service output: about one display frame.
const BATCH_WINDOW: Duration = Duration::from_millis(16);
/// Lines per `logs` event; a burst past this is split so one message stays small.
const BATCH_MAX: usize = 2000;
/// A single line is cut here (on a char boundary): a minified blob must not stall rendering.
const LINE_MAX: usize = 16 * 1024;

/// Accumulates `log` events into `{"t":"logs","lines":[…]}` batches.
#[derive(Default)]
struct LogBatch {
    lines: Vec<Value>,
}

impl LogBatch {
    /// Add a runner `log` event. The runner stamps `seq`/`ts`/`level`/`kind`/`trunc` and
    /// caps line length; an older runner sends none of them, so `ts` falls back to arrival
    /// time `now` (ms since epoch) and an oversized line is cut here.
    /// Returns a full batch when the cap is reached.
    fn push(&mut self, ev: &Value, now: u64) -> Option<Value> {
        let mut ev = ev.clone();
        if ev["ts"].as_u64().is_none() {
            ev["ts"] = now.into();
        }
        if let Some(line) = ev["line"].as_str().filter(|l| l.len() > LINE_MAX) {
            let mut end = LINE_MAX;
            while !line.is_char_boundary(end) {
                end -= 1;
            }
            ev["line"] = format!("{}… [+{} bytes]", &line[..end], line.len() - end).into();
            ev["trunc"] = true.into();
        }
        if let Some(o) = ev.as_object_mut() {
            o.remove("t");
        }
        self.lines.push(ev);
        (self.lines.len() >= BATCH_MAX)
            .then(|| self.take())
            .flatten()
    }

    fn take(&mut self) -> Option<Value> {
        (!self.lines.is_empty())
            .then(|| json!({"t": "logs", "lines": std::mem::take(&mut self.lines)}))
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

pub struct LiveHub {
    tasks: Mutex<HashMap<String, JoinHandle<()>>>,
    /// Root → id, for routing file-watch events.
    roots: Mutex<HashMap<PathBuf, String>>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    /// Last native notification per (project root, message), for rate limiting.
    notified: Mutex<HashMap<(PathBuf, String), Instant>>,
}

impl LiveHub {
    pub fn new(app: &AppHandle) -> Self {
        let handle = app.clone();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(ev) = res else { return };
            if matches!(ev.kind, notify::EventKind::Access(_)) {
                return;
            }
            let hub = handle.state::<LiveHub>();
            let roots = hub.roots.lock();
            for p in &ev.paths {
                let is_cfg = p
                    .file_name()
                    .is_some_and(|n| n == "atlas.yaml" || n == "atlas.local.yaml");
                if let (true, Some(id)) = (is_cfg, p.parent().and_then(|d| roots.get(d))) {
                    let _ = handle.emit(&format!("project://{id}"), json!({"t": "config_changed"}));
                }
            }
        })
        .ok();
        Self {
            tasks: Mutex::default(),
            roots: Mutex::default(),
            watcher: Mutex::new(watcher),
            notified: Mutex::default(),
        }
    }

    /// Start watching a project (no-op when already watched).
    pub fn ensure(&self, app: &AppHandle, id: &str, root: &Path) {
        let mut tasks = self.tasks.lock();
        if tasks.contains_key(id) {
            return;
        }
        if let Some(w) = self.watcher.lock().as_mut() {
            let _ = w.watch(root, RecursiveMode::NonRecursive);
        }
        self.roots.lock().insert(root.to_path_buf(), id.to_string());
        let task =
            tauri::async_runtime::spawn(run(app.clone(), id.to_string(), root.to_path_buf()));
        tasks.insert(id.to_string(), task);
    }

    pub fn remove(&self, id: &str) {
        if let Some(t) = self.tasks.lock().remove(id) {
            t.abort();
        }
        let mut roots = self.roots.lock();
        if let Some(root) = roots.iter().find(|(_, v)| *v == id).map(|(k, _)| k.clone()) {
            roots.remove(&root);
            if let Some(w) = self.watcher.lock().as_mut() {
                let _ = w.unwatch(&root);
            }
        }
    }
}

async fn run(app: AppHandle, id: String, root: PathBuf) {
    let topic = format!("project://{id}");
    let mut running = false;
    let mut backoff = Duration::from_secs(1);
    loop {
        let live = status::live_snapshot(&root).ok();
        if live.is_some() != running {
            running = live.is_some();
            let _ = app.emit("projects://changed", ());
        }
        let Some(snap) = live else {
            tokio::time::sleep(POLL).await;
            continue;
        };
        match control::subscribe(&root).await {
            Ok(mut sub) => {
                let mut first = std::mem::take(&mut sub.snapshot);
                first["t"] = "snapshot".into();
                let _ = app.emit(&topic, first);
                let mut got_events = false;
                let mut batch = LogBatch::default();
                loop {
                    // While a batch is open, wait at most one window for the next line;
                    // an idle stream blocks without a timer.
                    let ev = if batch.lines.is_empty() {
                        sub.next().await
                    } else {
                        match tokio::time::timeout(BATCH_WINDOW, sub.next()).await {
                            Ok(ev) => ev,
                            Err(_) => {
                                if let Some(b) = batch.take() {
                                    let _ = app.emit(&topic, b);
                                }
                                continue;
                            }
                        }
                    };
                    let Some(ev) = ev else { break };
                    got_events = true;
                    if ev["t"] == "log" {
                        if let Some(full) = batch.push(&ev, now_ms()) {
                            let _ = app.emit(&topic, full);
                        }
                        continue;
                    }
                    // Keep order: pending output precedes the event that followed it.
                    if let Some(b) = batch.take() {
                        let _ = app.emit(&topic, b);
                    }
                    notify_if_needed(&app, &root, &ev);
                    let _ = app.emit(&topic, ev);
                }
                if let Some(b) = batch.take() {
                    let _ = app.emit(&topic, b);
                }
                if got_events {
                    backoff = Duration::from_secs(1);
                }
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(5));
            }
            // Live but not subscribable (an owner TUI): mirror the status file.
            Err(_) => {
                let _ = app.emit(&topic, poll_event(&snap));
                tokio::time::sleep(POLL).await;
            }
        }
    }
}

fn poll_event(s: &status::Snapshot) -> Value {
    let services: Vec<Value> = s
        .services
        .iter()
        .map(|r| {
            json!({
                "name": r.name, "state": r.state, "exit_code": r.exit_code,
                "restarts": r.restarts, "note": r.note, "deselected": r.off,
                "mem_mb": r.mem_mb, "cpu": r.cpu,
            })
        })
        .collect();
    let tunnel = s.tunnel.as_ref().map(|t| {
        json!({"state": t.state, "local_port": t.local_port, "remote": t.remote, "ssh_host": t.ssh_host})
    });
    json!({"t": "poll", "mode": s.mode, "pid": s.pid, "services": services, "tunnel": tunnel})
}

/// Native notification for failures/alerts, only while the window is not focused.
/// A crash-looping service sends `failed` every few seconds: each (project,
/// message) notifies at most once per [`NOTIFY_EVERY`].
fn notify_if_needed(app: &AppHandle, root: &Path, ev: &Value) {
    let body = match ev["t"].as_str() {
        Some("state") if ev["state"] == "failed" => {
            format!("{} failed", ev["name"].as_str().unwrap_or("service"))
        }
        Some("alert") => ev["message"].as_str().unwrap_or("alert").to_string(),
        _ => return,
    };
    {
        let hub = app.state::<LiveHub>();
        let mut last = hub.notified.lock();
        let key = (root.to_path_buf(), body.clone());
        let now = Instant::now();
        if last
            .get(&key)
            .is_some_and(|t| now.duration_since(*t) < NOTIFY_EVERY)
        {
            return;
        }
        last.retain(|_, t| now.duration_since(*t) < NOTIFY_EVERY);
        last.insert(key, now);
    }
    let focused = app
        .get_window("main")
        .and_then(|w| w.is_focused().ok())
        .unwrap_or(false);
    if focused {
        return;
    }
    let title = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Atlas".into());
    let _ = app.notification().builder().title(title).body(body).show();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(line: &str) -> Value {
        json!({"t": "log", "name": "web", "line": line, "stream": "stderr"})
    }

    #[test]
    fn batch_carries_service_stream_and_timestamp() {
        let mut b = LogBatch::default();
        assert!(b.push(&log("hello"), 42).is_none());
        let out = b.take().unwrap();
        assert_eq!(out["t"], "logs");
        assert_eq!(
            out["lines"][0],
            json!({"name": "web", "line": "hello", "stream": "stderr", "ts": 42})
        );
    }

    #[test]
    fn runner_fields_pass_through_and_its_timestamp_wins() {
        let mut b = LogBatch::default();
        let ev = json!({"t": "log", "name": "api", "line": "x", "stream": "stdout",
            "seq": 7, "ts": 1000, "level": "error", "kind": "restart", "trunc": true});
        b.push(&ev, 42);
        let out = b.take().unwrap();
        assert_eq!(
            out["lines"][0],
            json!({"name": "api", "line": "x", "stream": "stdout", "seq": 7, "ts": 1000,
                "level": "error", "kind": "restart", "trunc": true})
        );
        assert!(b.take().is_none(), "take() drains the batch");
    }

    #[test]
    fn batch_splits_at_the_cap_and_keeps_order() {
        let mut b = LogBatch::default();
        let mut full = None;
        for i in 0..BATCH_MAX {
            full = b.push(&log(&i.to_string()), 0);
        }
        let full = full.expect("cap reached");
        assert_eq!(full["lines"].as_array().unwrap().len(), BATCH_MAX);
        assert_eq!(full["lines"][0]["line"], "0");
        assert_eq!(
            full["lines"][BATCH_MAX - 1]["line"],
            (BATCH_MAX - 1).to_string()
        );
        assert!(b.lines.is_empty());
    }

    #[test]
    fn huge_line_is_cut_on_a_char_boundary() {
        let mut b = LogBatch::default();
        // 3-byte chars: LINE_MAX is not a multiple of 3, so the cut must back up.
        let blob = "€".repeat(LINE_MAX);
        b.push(&log(&blob), 0);
        let out = b.take().unwrap();
        let line = out["lines"][0]["line"].as_str().unwrap();
        assert!(line.len() < blob.len());
        assert!(line.contains("… [+"));
        assert_eq!(out["lines"][0]["trunc"], true);
        assert!(line.starts_with('€'));
    }

    #[test]
    fn short_lines_pass_through_untouched() {
        let mut b = LogBatch::default();
        let l = "x".repeat(LINE_MAX);
        b.push(&log(&l), 0);
        assert_eq!(b.take().unwrap()["lines"][0]["line"], l);
    }
}
