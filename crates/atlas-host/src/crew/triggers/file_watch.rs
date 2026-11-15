//! File-change trigger watcher using `notify`.
//!
//! One watcher task is spawned per `File` trigger. Falls back to 30 s polling
//! if `notify` fails to initialise or the path does not yet exist.
//! New triggers are picked up every 2 minutes via a rescan loop.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::{Duration, SystemTime};

use notify::{RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::crew::store::{Trigger, TriggerKind};
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
    let crewmates = hub.crew.store.list_crewmates();
    for crewmate in crewmates {
        for trigger in &crewmate.triggers {
            if !trigger.enabled {
                continue;
            }
            if let TriggerKind::File { path, glob } = &trigger.kind {
                let key = format!("{}:{}", crewmate.id, trigger.id);
                if registered.contains(&key) {
                    continue;
                }
                registered.insert(key);
                let filter = Filter::new(glob.as_deref(), path, own_trees(hub, &crewmate));
                spawn_watcher(
                    hub_weak.clone(),
                    crewmate.id.clone(),
                    trigger.clone(),
                    path.clone(),
                    filter,
                );
            }
        }
    }
}

/// Which changes under a watched path start a task: the ones that match the trigger's `glob` (against
/// the path below the watched one; a glob without `/` matches the file name at any depth, so `*.md`
/// is every Markdown file), and never one inside the crewmate's own work tree: its own edits would
/// start the task that makes the next edit.
pub(super) struct Filter {
    root: PathBuf,
    glob: Option<Glob>,
    own: Vec<PathBuf>,
}

struct Glob {
    matcher: globset::GlobMatcher,
    by_name: bool,
}

impl Filter {
    /// A `glob` that does not parse matches nothing: a typo must not turn into "fire on everything".
    pub(super) fn new(glob: Option<&str>, root: &str, own: Vec<PathBuf>) -> Self {
        let glob = glob.filter(|g| !g.is_empty()).map(|g| {
            let matcher = globset::GlobBuilder::new(g)
                .literal_separator(true)
                .build()
                .inspect_err(|e| eprintln!("triggers/file: glob {g:?} is not valid ({e})"))
                .ok();
            (g, matcher)
        });
        Self {
            root: PathBuf::from(root),
            glob: glob.map(|(g, m)| Glob {
                // No match is possible for an invalid glob: the empty set.
                matcher: m.map(|m| m.compile_matcher()).unwrap_or_else(never),
                by_name: !g.contains('/'),
            }),
            own,
        }
    }

    pub(super) fn accepts(&self, changed: &Path) -> bool {
        if self.own.iter().any(|t| changed.starts_with(t)) {
            return false;
        }
        let Some(g) = &self.glob else { return true };
        let below = changed.strip_prefix(&self.root).unwrap_or(changed);
        g.matcher.is_match(below)
            || (g.by_name
                && changed
                    .file_name()
                    .is_some_and(|n| g.matcher.is_match(Path::new(n))))
    }
}

/// A matcher that matches no path.
fn never() -> globset::GlobMatcher {
    // A path never contains a NUL, so this literal matches nothing.
    globset::Glob::new("\0")
        .expect("a literal pattern")
        .compile_matcher()
}

/// The folders the crewmate's agent writes in: its `work/` (a Git project's worktree) and the
/// directory it runs in, resolved the way the runner does it.
fn own_trees(hub: &Hub, crewmate: &crate::crew::store::Crewmate) -> Vec<PathBuf> {
    let mut trees = vec![crate::crew::project::work_dir(&hub.crew.dir, &crewmate.id)];
    if let Ok(dir) = crate::crew::project::current_work_dir(&hub.crew.dir, crewmate) {
        trees.push(dir);
    }
    trees
}

fn spawn_watcher(
    hub_weak: Weak<Hub>,
    crewmate_id: String,
    trigger: Trigger,
    path: String,
    filter: Filter,
) {
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
                    poll_fallback(hub_weak, crewmate_id, trigger, path, filter).await;
                    return;
                }
                // Keep watcher alive for the duration of the task.
                let _watcher = watcher;
                notify_loop(hub_weak, crewmate_id, trigger, rx, filter).await;
            }
            Err(e) => {
                eprintln!("triggers/file: notify init failed ({e}), using 30 s poll");
                poll_fallback(hub_weak, crewmate_id, trigger, path, filter).await;
            }
        }
    });
}

/// Receive change events, debounce 5 s, then fire.
async fn notify_loop(
    hub_weak: Weak<Hub>,
    crewmate_id: String,
    trigger: Trigger,
    mut rx: mpsc::UnboundedReceiver<String>,
    filter: Filter,
) {
    loop {
        // Wait for the first change event that counts.
        let changed = match rx.recv().await {
            Some(p) if filter.accepts(Path::new(&p)) => p,
            Some(_) => continue,
            None => return,
        };
        // Debounce: absorb further events for 5 s.
        tokio::time::sleep(Duration::from_secs(5)).await;
        let mut last = changed;
        while let Ok(p) = rx.try_recv() {
            if filter.accepts(Path::new(&p)) {
                last = p;
            }
        }
        let Some(hub) = hub_weak.upgrade() else {
            return;
        };
        super::fire(
            &hub,
            &crewmate_id,
            &trigger.id,
            &trigger.prompt,
            super::Event::Local(&last),
        );
    }
}

/// Fall back to polling the path's mtime every 30 s.
async fn poll_fallback(
    hub_weak: Weak<Hub>,
    crewmate_id: String,
    trigger: Trigger,
    path: String,
    filter: Filter,
) {
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
            // The poll only sees the watched path itself, so it is the path the filter judges.
            if !filter.accepts(Path::new(&path)) {
                continue;
            }
            super::fire(
                &hub,
                &crewmate_id,
                &trigger.id,
                &trigger.prompt,
                super::Event::Local(&path),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(glob: Option<&str>, own: &[&str]) -> Filter {
        Filter::new(glob, "/watched", own.iter().map(PathBuf::from).collect())
    }

    // Regression (#294): the `glob` field of a file trigger was never read, so `*.md` fired on a `.rs` change.
    #[test]
    fn a_glob_without_a_slash_matches_the_file_name_at_any_depth() {
        let f = filter(Some("*.md"), &[]);
        assert!(f.accepts(Path::new("/watched/README.md")));
        assert!(f.accepts(Path::new("/watched/docs/deep/guide.md")));
        assert!(!f.accepts(Path::new("/watched/src/main.rs")));
        assert!(!f.accepts(Path::new("/watched/README.md.bak")));
    }

    #[test]
    fn a_glob_with_a_slash_is_relative_to_the_watched_path() {
        let f = filter(Some("docs/*.md"), &[]);
        assert!(f.accepts(Path::new("/watched/docs/a.md")));
        assert!(
            !f.accepts(Path::new("/watched/docs/deep/a.md")),
            "* stops at /"
        );
        assert!(!f.accepts(Path::new("/watched/a.md")));
    }

    #[test]
    fn no_glob_or_an_empty_one_accepts_everything_and_a_broken_one_nothing() {
        for g in [None, Some("")] {
            assert!(filter(g, &[]).accepts(Path::new("/watched/anything.rs")));
        }
        let broken = filter(Some("[unclosed"), &[]);
        assert!(!broken.accepts(Path::new("/watched/anything.rs")));
        assert!(!broken.accepts(Path::new("/watched/[unclosed")));
    }

    // Regression (#294): a trigger on the crewmate's own work tree fired on the edits its own task made.
    #[test]
    fn a_change_inside_the_crewmates_own_work_tree_never_fires() {
        let f = filter(None, &["/watched/crew/me/work"]);
        assert!(!f.accepts(Path::new("/watched/crew/me/work/src/lib.rs")));
        assert!(f.accepts(Path::new("/watched/crew/other/work/src/lib.rs")));
        assert!(
            f.accepts(Path::new("/watched/crew/me/work-notes.md")),
            "a sibling name is not inside it"
        );
        // Also with a glob that would otherwise match.
        let f = filter(Some("*.rs"), &["/watched/crew/me/work"]);
        assert!(!f.accepts(Path::new("/watched/crew/me/work/src/lib.rs")));
    }
}
