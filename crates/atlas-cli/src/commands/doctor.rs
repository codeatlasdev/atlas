//! `atlas doctor --hygiene`: the processes Atlas tagged whose owner is gone, and the disk it
//! owns, per project. Looks by default (`--dry-run`); `--fix` ends the orphans and frees what
//! Atlas created (registered temp dirs and caches over their caps, rotated logs). Never removes
//! `target/`, `node_modules`, worktrees or any file Atlas did not create.

use anyhow::Result;
use atlas_core::hygiene::{self, Fixed, Registry, Report};
use clap::Args;

use super::AtlasCommand;

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Orphaned processes and disk use per project
    #[arg(long, required = true)]
    pub hygiene: bool,

    /// Show what would be freed and change nothing (the default)
    #[arg(long, conflicts_with = "fix")]
    pub dry_run: bool,

    /// End the orphaned processes and free the disk Atlas owns
    #[arg(long)]
    pub fix: bool,

    /// Machine-readable output
    #[arg(long)]
    pub json: bool,
}

impl AtlasCommand for DoctorArgs {
    async fn run(self) -> Result<()> {
        let registry = Registry::open();
        let base = atlas_core::paths::atlas_home();
        // The scan reads every process's environment and sizes directory trees: not on the runtime.
        let fix = self.fix;
        let (report, fixed) = tokio::task::spawn_blocking(move || {
            let report = hygiene::report(&registry, &base);
            let fixed = fix.then(|| hygiene::fix(&registry, &base));
            (report, fixed)
        })
        .await?;
        if self.json {
            let v = serde_json::json!({ "report": report, "fixed": fixed });
            println!("{}", serde_json::to_string_pretty(&v)?);
        } else {
            print!("{}", render(&report, fixed.as_ref()));
        }
        // A fix that could not end every orphan is an error the caller can script on.
        if fixed.as_ref().is_some_and(|f| !f.survivors.is_empty()) {
            std::process::exit(1);
        }
        Ok(())
    }
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// The text `atlas doctor --hygiene` prints.
pub fn render(r: &Report, fixed: Option<&Fixed>) -> String {
    let mut o = String::new();
    o.push_str(&format!(
        "Orphaned processes: {} ({} process(es) of live sessions left alone)\n",
        r.orphans.len(),
        r.live
    ));
    for p in &r.orphans {
        o.push_str(&format!(
            "  {:>7}  {}  {}{}\n",
            p.pid,
            p.name,
            p.label.as_deref().unwrap_or(""),
            p.project
                .as_deref()
                .map(|x| format!("  ({x})"))
                .unwrap_or_default(),
        ));
    }
    if !atlas_core::procs::ENV_READABLE {
        o.push_str(concat!(
            "  Note: this OS cannot read another process's environment, so these are the processes Atlas\n",
            "  saw under its terminals, services and actions while their owner ran. A process that forked and\n",
            "  detached within about 2 s of starting was never seen and is not listed.\n",
        ));
    }
    o.push_str("\nDisk per project:\n");
    if r.projects.is_empty() {
        o.push_str("  (no projects registered in Atlas Desktop)\n");
    }
    for p in &r.projects {
        let wt: u64 = p.worktrees.iter().map(|(_, b)| b).sum();
        o.push_str(&format!(
            "  {}\n    target {}  node_modules {}  .logs {}  worktrees {} ({})\n",
            p.root,
            human(p.target),
            human(p.node_modules),
            human(p.logs),
            human(wt),
            p.worktrees.len()
        ));
    }
    let indexed: Vec<&String> = r.projects.iter().flat_map(|p| &p.indexed).collect();
    if !indexed.is_empty() {
        o.push_str("\nIndexed by Spotlight (rewritten constantly: mdworker/corespotlightd burn CPU and disk):\n");
        for d in &indexed {
            o.push_str(&format!("  {d}\n"));
        }
    }
    if !r.dirs.is_empty() {
        o.push_str("\nAtlas temp and cache dirs:\n");
        for d in &r.dirs {
            o.push_str(&format!(
                "  {}  {} / cap {}{}\n",
                d.path,
                human(d.bytes),
                human(d.cap_bytes),
                if d.orphaned { "  (owner gone)" } else { "" }
            ));
        }
    }
    match fixed {
        None => {
            o.push_str(&format!(
                "\nDry run: --fix would end {} process(es) and free {} (registered dirs and rotated logs)",
                r.orphans.len(),
                human(r.freeable.bytes)
            ));
            if !indexed.is_empty() {
                o.push_str(&format!(
                    ", and add a .metadata_never_index marker to {} dir(s)",
                    indexed.len()
                ));
            }
            o.push_str(". target/, node_modules and worktrees are yours: never removed.\n");
        }
        Some(f) => {
            o.push_str(&format!(
                "\nEnded {} process(es), freed {}",
                f.reaped,
                human(f.freed.bytes)
            ));
            if !f.marked.is_empty() {
                o.push_str(&format!(", marked {} dir(s) never-index", f.marked.len()));
            }
            o.push_str(".\n");
            if !f.survivors.is_empty() {
                o.push_str(&format!(
                    "Still running after SIGKILL: {:?} (stuck in the kernel; they end when it lets go).\n",
                    f.survivors
                ));
            }
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use atlas_core::hygiene::{Freed, Orphan, ProjectDisk};

    #[test]
    fn sizes_are_human_readable_at_every_magnitude() {
        assert_eq!(human(0), "0 B");
        assert_eq!(human(1023), "1023 B");
        assert_eq!(human(1024), "1.0 KiB");
        assert_eq!(human(5 * 1024 * 1024 + 512 * 1024), "5.5 MiB");
        assert_eq!(human(3 << 30), "3.0 GiB");
        assert_eq!(human(5000 << 30), "5000.0 GiB");
    }

    fn report() -> Report {
        Report {
            orphans: vec![Orphan {
                pid: 4242,
                start: 1,
                name: "chrome".into(),
                session: "s".into(),
                kind: None,
                label: Some("agent".into()),
                project: Some("/p".into()),
            }],
            live: 3,
            projects: vec![ProjectDisk {
                root: "/p".into(),
                name: "p".into(),
                target: 2 << 20,
                node_modules: 0,
                logs: 10,
                worktrees: vec![("a".into(), 1 << 20)],
                indexed: vec!["/p/target".into(), "/p/node_modules".into()],
            }],
            dirs: vec![],
            freeable: Freed {
                bytes: 7 << 20,
                ..Freed::default()
            },
        }
    }

    #[test]
    fn a_dry_run_names_the_orphan_the_disk_and_the_promise_not_to_touch_user_files() {
        let t = render(&report(), None);
        assert!(
            t.contains("Orphaned processes: 1 (3 process(es) of live sessions"),
            "{t}"
        );
        assert!(t.contains("4242  chrome  agent  (/p)"), "{t}");
        assert!(t.contains("target 2.0 MiB"), "{t}");
        assert!(t.contains("worktrees 1.0 MiB (1)"), "{t}");
        assert!(
            t.contains("Dry run: --fix would end 1 process(es) and free 7.0 MiB (registered dirs and rotated logs)"),
            "{t}"
        );
        assert!(t.contains("Indexed by Spotlight"), "{t}");
        assert!(t.contains("  /p/target\n"), "{t}");
        assert!(t.contains("marker to 2 dir(s)"), "{t}");
        assert!(t.contains("never removed"), "{t}");
    }

    #[test]
    fn a_fix_reports_what_it_ended_and_what_would_not_die() {
        let fixed = Fixed {
            reaped: 2,
            survivors: vec![9],
            freed: Freed {
                bytes: 1024,
                ..Freed::default()
            },
            marked: vec!["/p/target".into()],
        };
        let t = render(&report(), Some(&fixed));
        assert!(
            t.contains("Ended 2 process(es), freed 1.0 KiB, marked 1 dir(s)"),
            "{t}"
        );
        assert!(t.contains("Still running after SIGKILL: [9]"), "{t}");
        assert!(!t.contains("Dry run"), "{t}");
    }

    #[test]
    fn the_json_output_carries_the_keys_the_desktop_panel_reads() {
        // The desktop deserializes this same value (`Report` is the one producer).
        let v = serde_json::to_value(report()).unwrap();
        for k in ["orphans", "live", "projects", "dirs", "freeable"] {
            assert!(v.get(k).is_some(), "missing {k}: {v}");
        }
        assert_eq!(v["orphans"][0]["pid"], 4242);
        assert_eq!(v["projects"][0]["node_modules"], 0);
        assert_eq!(v["freeable"]["bytes"], 7 << 20);
        assert_eq!(v["projects"][0]["indexed"][0], "/p/target");
    }
}
