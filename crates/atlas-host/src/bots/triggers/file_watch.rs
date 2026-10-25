//! File-change trigger watcher using `notify`.
//!
//! One watcher task is spawned per `File` trigger. Falls back to 30 s polling
//! if `notify` fails to initialise or the path does not yet exist.
//! New triggers are picked up every 2 minutes via a rescan loop.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use std::time::{Duration, SystemTime};

use notify::{RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::bots::store::{Trigger, TriggerKind};
use crate::server::Hub;

pub(super) async fn run(hub_weak: Weak<Hub>) {
    let mut registered: HashSet<String> = HashSet::new();
    // Initial pass.
    if let Some(hub) = hub_weak.upgrade() {
        register_new(&hub, &hub_weak, &mut registered);
    }
    // Rescan every 2 minutes to pick up triggers added after startup.
    let mut interval = tokio::time::interval(Duration::from_secs(120));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let Some(hub) = hub_weak.upgrade() else { break };
        register_new(&hub, &hub_weak, &mut registered);
    }
}

fn register_new(hub: &Arc<Hub>, hub_weak: &Weak<Hub>, registered: &mut HashSet<String>) {
    let bots = hub.bots.store.list_bots();
    for bot in bots {
        for trigger in &bot.triggers {
            if !trigger.enabled {
                continue;
            }
            if let TriggerKind::File { path, .. } = &trigger.kind {
                let key = format!("{}:{}", bot.id, trigger.id);
                if registered.contains(&key) {
                    continue;
                }
                registered.insert(key);
                spawn_watcher(
                    hub_weak.clone(),
                    bot.id.clone(),
                    trigger.clone(),
                    path.clone(),
                );
            }
        }
    }
}

fn spawn_watcher(hub_weak: Weak<Hub>, bot_id: String, trigger: Trigger, path: String) {
    let watch_path = PathBuf::from(&path);
    let (tx_cb, rx) = mpsc::unbounded_channel::<String>();
    let watcher_result = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            for p in ev.paths {
                let _ = tx_cb.send(p.to_string_lossy().to_string());
            }
        }
    });

    tokio::spawn(async move {
        match watcher_result {
            Ok(mut watcher) => {
                if watcher
                    .watch(&watch_path, RecursiveMode::Recursive)
                    .is_err()
                {
                    // Path not accessible yet — use poll fallback.
                    poll_fallback(hub_weak, bot_id, trigger, path).await;
                    return;
                }
                // Keep watcher alive for the duration of the task.
                let _watcher = watcher;
                notify_loop(hub_weak, bot_id, trigger, rx).await;
            }
            Err(e) => {
                eprintln!("triggers/file: notify init failed ({e}), using 30 s poll");
                poll_fallback(hub_weak, bot_id, trigger, path).await;
            }
        }
    });
}

/// Receive change events, debounce 5 s, then fire.
async fn notify_loop(
    hub_weak: Weak<Hub>,
    bot_id: String,
    trigger: Trigger,
    mut rx: mpsc::UnboundedReceiver<String>,
) {
    loop {
        // Wait for the first change event.
        let changed = match rx.recv().await {
            Some(p) => p,
            None => return,
        };
        // Debounce: absorb further events for 5 s.
        tokio::time::sleep(Duration::from_secs(5)).await;
        let mut last = changed;
        while let Ok(p) = rx.try_recv() {
            last = p;
        }
        let Some(hub) = hub_weak.upgrade() else {
            return;
        };
        super::fire(&hub, &bot_id, &trigger.id, &trigger.prompt, &last);
    }
}

/// Fall back to polling the path's mtime every 30 s.
async fn poll_fallback(hub_weak: Weak<Hub>, bot_id: String, trigger: Trigger, path: String) {
    let mut last_mtime: Option<SystemTime> = std::fs::metadata(&path)
        .ok()
        .and_then(|m| m.modified().ok());
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let Some(hub) = hub_weak.upgrade() else {
            return;
        };
        let mtime = std::fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok());
        if mtime.is_some() && mtime != last_mtime {
            last_mtime = mtime;
            super::fire(&hub, &bot_id, &trigger.id, &trigger.prompt, &path);
        }
    }
}
