//! `atlas doctor`: the install checks (plain), `--hygiene` (the processes Atlas tagged whose owner
//! is gone, and the disk it owns, per project) and `--bundle` (`doctor_bundle`).
//! `--hygiene` looks by default (`--dry-run`); `--fix` ends the orphans and frees what
//! Atlas created (registered temp dirs and caches over their caps, rotated logs). Never removes
//! `target/`, `node_modules`, worktrees or any file Atlas did not create.

use anyhow::Result;
use atlas_core::hygiene::{self, Fixed, Registry, Report};
use atlas_i18n::t_in;
use clap::Args;

use super::AtlasCommand;

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Orphaned processes and disk use per project (without it: the install checks)
    #[arg(long)]
    pub hygiene: bool,

    /// Show what would be freed and change nothing (the default)
    #[arg(long, conflicts_with = "fix", requires = "hygiene")]
    pub dry_run: bool,

    /// End the orphaned processes and free the disk Atlas owns
    #[arg(long, requires = "hygiene")]
    pub fix: bool,

    /// The diagnostics bundle (what Atlas Desktop's "Copy details" copies): distro, session, GPU,
    /// packaging, crashes, versions and the log tails, redacted. Attach it to a bug report.
    /// With `--json`, the app's payload.
    #[arg(long, conflicts_with_all = ["hygiene", "dry_run", "fix"])]
    pub bundle: bool,
    /// Machine-readable output
    #[arg(long)]
    pub json: bool,
}

impl AtlasCommand for DoctorArgs {
    async fn run(self) -> Result<()> {
        if self.bundle {
            return super::doctor_bundle::run(self.json).await;
        }
        if !self.hygiene {
            use super::doctor_checks as dc;
            let facts = dc::Facts::gather().await;
            // `--json` is machine output: English whatever the display language.
            let checks = if self.json {
                dc::evaluate(&facts)
            } else {
                dc::evaluate_in(atlas_i18n::lang(), &facts)
            };
            if self.json {
                println!("{}", serde_json::to_string_pretty(&checks)?);
            } else {
                print!("{}", dc::render(atlas_i18n::lang(), &checks));
            }
            if checks.iter().any(|c| c.status == dc::Status::Fail) {
                std::process::exit(1);
            }
            return Ok(());
        }
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
            print!("{}", render(atlas_i18n::lang(), &report, fixed.as_ref()));
        }
        // A fix that could not end every orphan is an error the caller can script on.
        if fixed.as_ref().is_some_and(|f| !f.survivors.is_empty()) {
            std::process::exit(1);
        }
        Ok(())
    }
}

fn human(tag: &str, bytes: u64) -> String {
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
        format!("{} {}", crate::numfmt::fixed(tag, v, 1), UNITS[i])
    }
}

/// The text `atlas doctor --hygiene` prints, in `tag`.
pub fn render(tag: &str, r: &Report, fixed: Option<&Fixed>) -> String {
    let mut o = String::new();
    o.push_str(&t_in!(
        tag,
        "cmd_misc_doctor_orphans",
        orphans = r.orphans.len(),
        live = r.live
    ));
    o.push('\n');
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
        o.push_str(&t_in!(tag, "cmd_misc_doctor_env_unreadable"));
        o.push('\n');
    }
    o.push('\n');
    o.push_str(&t_in!(tag, "cmd_misc_doctor_disk_heading"));
    o.push('\n');
    if r.projects.is_empty() {
        o.push_str(&t_in!(tag, "cmd_misc_doctor_no_projects"));
        o.push('\n');
    }
    for p in &r.projects {
        let wt: u64 = p.worktrees.iter().map(|(_, b)| b).sum();
        // The folder names and the worktree term are what the person looks for on disk: not translated.
        o.push_str(&format!(
            "  {}\n    target {}  node_modules {}  .logs {}  worktrees {} ({})\n",
            p.root,
            human(tag, p.target),
            human(tag, p.node_modules),
            human(tag, p.logs),
            human(tag, wt),
            p.worktrees.len()
        ));
    }
    let indexed: Vec<&String> = r.projects.iter().flat_map(|p| &p.indexed).collect();
    if !indexed.is_empty() {
        o.push('\n');
        o.push_str(&t_in!(tag, "cmd_misc_doctor_indexed_heading"));
        o.push('\n');
        for d in &indexed {
            o.push_str(&format!("  {d}\n"));
        }
    }
    if !r.dirs.is_empty() {
        o.push('\n');
        o.push_str(&t_in!(tag, "cmd_misc_doctor_dirs_heading"));
        o.push('\n');
        for d in &r.dirs {
            let (path, size, cap) = (&d.path, human(tag, d.bytes), human(tag, d.cap_bytes));
            o.push_str(&if d.orphaned {
                t_in!(
                    tag,
                    "cmd_misc_doctor_dir_row_orphaned",
                    path = path,
                    size = size,
                    cap = cap
                )
            } else {
                t_in!(
                    tag,
                    "cmd_misc_doctor_dir_row",
                    path = path,
                    size = size,
                    cap = cap
                )
            });
            o.push('\n');
        }
    }
    o.push('\n');
    match fixed {
        None => {
            let mut sentences = vec![t_in!(
                tag,
                "cmd_misc_doctor_dry_run",
                count = r.orphans.len(),
                size = human(tag, r.freeable.bytes)
            )];
            if !indexed.is_empty() {
                sentences.push(t_in!(
                    tag,
                    "cmd_misc_doctor_dry_run_marker",
                    count = indexed.len()
                ));
            }
            sentences.push(t_in!(tag, "cmd_misc_doctor_dry_run_yours"));
            o.push_str(&sentences.join(" "));
            o.push('\n');
        }
        Some(f) => {
            let mut sentences = vec![t_in!(
                tag,
                "cmd_misc_doctor_fixed",
                count = f.reaped,
                size = human(tag, f.freed.bytes)
            )];
            if !f.marked.is_empty() {
                sentences.push(t_in!(
                    tag,
                    "cmd_misc_doctor_fixed_marked",
                    count = f.marked.len()
                ));
            }
            o.push_str(&sentences.join(" "));
            o.push('\n');
            if !f.survivors.is_empty() {
                o.push_str(&t_in!(
                    tag,
                    "cmd_misc_doctor_survivors",
                    pids = format!("{:?}", f.survivors)
                ));
                o.push('\n');
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
        assert_eq!(human("en", 0), "0 B");
        assert_eq!(human("en", 1023), "1023 B");
        assert_eq!(human("en", 1024), "1.0 KiB");
        assert_eq!(human("en", 5 * 1024 * 1024 + 512 * 1024), "5.5 MiB");
        assert_eq!(human("pt-BR", 5 * 1024 * 1024 + 512 * 1024), "5,5 MiB");
        assert_eq!(human("en", 3 << 30), "3.0 GiB");
        assert_eq!(human("en", 5000 << 30), "5000.0 GiB");
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
        let t = render("en", &report(), None);
        assert!(
            t.contains("Orphaned processes: 1 (3 processes of live sessions"),
            "{t}"
        );
        assert!(t.contains("4242  chrome  agent  (/p)"), "{t}");
        assert!(t.contains("target 2.0 MiB"), "{t}");
        assert!(t.contains("worktrees 1.0 MiB (1)"), "{t}");
        assert!(
            t.contains("Dry run: --fix would end 1 process and free 7.0 MiB (registered dirs and rotated logs)."),
            "{t}"
        );
        assert!(t.contains("Indexed by Spotlight"), "{t}");
        assert!(t.contains("  /p/target\n"), "{t}");
        assert!(t.contains("marker to 2 folders"), "{t}");
        assert!(t.contains("they are never removed"), "{t}");
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
        let t = render("en", &report(), Some(&fixed));
        assert!(
            t.contains("Ended 2 processes, freed 1.0 KiB. Added a never-index marker to 1 folder."),
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
