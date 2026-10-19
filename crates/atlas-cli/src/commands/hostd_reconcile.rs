//! Keeping the terminal host (`atlas hostd`) in step with the CLI: report it, hand it over
//! live, or restart it. Used by `atlas hostd --check|--reconcile|--restart` and, at the end of
//! `atlas update`, by [`reconcile_host`].

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use atlas_host::HostClient;
use atlas_host::upgrade::{self, Choice, Decision};
use serde_json::{Value, json};

use super::update_core::{self, Phase};

/// What the terminal host looks like right now.
#[derive(Debug, Clone)]
pub(crate) struct Snapshot {
    pub info: Value,
    /// Running terminals only (`pty_list` entries).
    pub terminals: Vec<Value>,
}

impl Snapshot {
    pub fn live(&self) -> usize {
        upgrade::live_ptys(&self.info, self.terminals.len())
    }
}

/// How the host ended up after [`reconcile_host`] / [`restart_host`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HostOutcome {
    NotRunning,
    Current,
    /// Handed over live; every terminal kept.
    Upgraded {
        kept: usize,
    },
    /// Replaced by a fresh daemon; `ended` terminals were ended.
    Restarted {
        ended: usize,
    },
    /// Left running (the user decides, or only the version differs); `live` terminals run.
    Kept {
        live: usize,
        decision: DecisionKind,
    },
}

/// [`Decision`] without the actionable variants, for reporting a daemon that was kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DecisionKind {
    Prompt,
    WaitIdle,
    Deferred,
    Passive,
    Declined,
}

/// Connect, read `info` and the running terminals. `None` when no daemon answers.
pub(crate) async fn snapshot(dir: &Path) -> Option<(HostClient, Snapshot)> {
    let c = HostClient::connect_dir(dir, "cli").await.ok()?;
    let info = c.call("info", json!({})).await.ok()?;
    let list = c.call("pty_list", json!({})).await.ok()?;
    let terminals = list["ptys"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["exited"].is_null())
        .cloned()
        .collect();
    Some((c, Snapshot { info, terminals }))
}

/// `"zsh"` / `"cargo test"` for a `pty_list` entry.
fn command_of(p: &Value) -> String {
    let argv: Vec<&str> = p["argv"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if argv.is_empty() {
        p["title"].as_str().unwrap_or("terminal").to_string()
    } else {
        argv.join(" ")
    }
}

/// One line per running terminal: `  <cwd>  <command>`.
pub(crate) fn terminal_lines(s: &Snapshot) -> Vec<String> {
    s.terminals
        .iter()
        .map(|p| format!("  {}  {}", p["cwd"].as_str().unwrap_or("?"), command_of(p)))
        .collect()
}

/// `40h`, `4d 4h`, `12m`, `45s` (hours up to 99h, as in "40h uptime").
pub(crate) fn fmt_uptime(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m", secs / 60),
        3600..=359_999 => format!("{}h", secs / 3600),
        _ => format!("{}d {}h", secs / 86_400, secs % 86_400 / 3600),
    }
}

fn plural(n: usize) -> String {
    format!("{n} terminal{}", if n == 1 { "" } else { "s" })
}

/// The `--check` report: `None` when the host is current. Pure, so it is unit-tested.
pub(crate) fn check_report(s: &Snapshot, bundled: &str, dir_hint: &str) -> Option<String> {
    let reason = upgrade::skew_reason(&s.info, bundled, &format!("Atlas {bundled}"))?;
    let version = s.info["version"].as_str().unwrap_or("unknown");
    let pid = s.info["pid"].as_u64().unwrap_or(0);
    let up = fmt_uptime(s.info["uptime_s"].as_u64().unwrap_or(0));
    let live = s.live();
    // "Terminal host 0.6.2 is older than Atlas 1.2.1; ..." -> insert "(pid, up, terminals)".
    let head = format!("Terminal host {version}");
    let detail = format!(" (pid {pid}, up {up}, {})", plural(live));
    let mut out = reason.replacen(&head, &format!("{head}{detail}"), 1);
    if live > 0 {
        out.push_str("\nRunning terminals:\n");
        out.push_str(&terminal_lines(s).join("\n"));
    }
    out.push_str("\nFix:\n");
    if upgrade::caps_of(&s.info).contains(&"upgrade") {
        out.push_str(&format!(
            "  atlas hostd{dir_hint} --reconcile   # hands the terminals over live, nothing is lost\n"
        ));
    } else {
        out.push_str(&format!(
            "  atlas hostd{dir_hint} --reconcile   # asks first{}\n",
            if live == 0 {
                ""
            } else {
                "; ends the terminals above"
            }
        ));
    }
    out.push_str(&format!(
        "  atlas hostd{dir_hint} --restart     # restart now (ends running terminals)"
    ));
    Some(out)
}

/// Ask `question [y/N]` on the terminal; anything but y/yes is no.
fn ask(question: &str) -> bool {
    print!("{question} [y/N] ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).is_ok()
        && matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// Stop the daemon at `c` (ends its terminals) and wait until its socket is gone.
async fn stop(dir: &Path, c: &HostClient) -> Result<()> {
    c.call("shutdown", json!({})).await?;
    for _ in 0..100 {
        if HostClient::connect_dir(dir, "cli").await.is_err() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(anyhow!("the old terminal host did not stop"))
}

/// Start a fresh daemon from this binary in `dir` and check it reports `bundled`.
async fn start_fresh(dir: &Path, bundled: &str) -> Result<()> {
    let exe = std::env::current_exe()?;
    let c = atlas_host::client::ensure_in(dir, &exe, "cli").await?;
    let v = c.call("info", json!({})).await?;
    if v["version"].as_str() != Some(bundled) {
        return Err(anyhow!(
            "the new terminal host reports {} (expected {bundled})",
            v["version"]
        ));
    }
    Ok(())
}

/// Shutdown (ends the terminals) then start fresh from this binary, then verify.
pub(crate) async fn restart_host(dir: &Path, bundled: &str) -> Result<HostOutcome> {
    update_core::phase(Phase::TerminalHost);
    let Some((c, snap)) = snapshot(dir).await else {
        // The app starts one (current) on demand.
        println!("Terminal host is not running.");
        return Ok(HostOutcome::NotRunning);
    };
    let ended = snap.live();
    stop(dir, &c).await?;
    start_fresh(dir, bundled).await?;
    update_core::host_event("restarted", ended);
    println!(
        "Terminal host restarted ({bundled}); {} ended.",
        plural(ended)
    );
    Ok(HostOutcome::Restarted { ended })
}

/// `upgrade {exe}` then verify: the new daemon reports `bundled` and no terminal was lost.
async fn handoff(dir: &Path, c: &HostClient, before: &Snapshot, bundled: &str) -> Result<usize> {
    let exe = std::env::current_exe()?;
    c.call("upgrade", json!({"exe": exe.to_string_lossy()}))
        .await
        .context("handoff")?;
    // The successor takes the socket over; wait until it answers with the new version.
    for _ in 0..200 {
        if let Some((_, after)) = snapshot(dir).await
            && after.info["version"].as_str() == Some(bundled)
            && after.info["pid"] != before.info["pid"]
        {
            if after.live() < before.live() {
                return Err(anyhow!(
                    "the upgraded terminal host lost terminals ({} -> {})",
                    before.live(),
                    after.live()
                ));
            }
            return Ok(after.live());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(anyhow!("the upgraded terminal host did not come up"))
}

/// Decide for the daemon in `dir` against `bundled` and act on it. Never ends a running
/// terminal without consent: that is `Choice` plus (`interactive` and a yes), or `--restart`.
pub(crate) async fn reconcile_host(
    dir: &Path,
    bundled: &str,
    choice: Choice,
    interactive: bool,
) -> Result<HostOutcome> {
    update_core::phase(Phase::TerminalHost);
    let Some((mut c, mut snap)) = snapshot(dir).await else {
        println!("Terminal host is not running.");
        return Ok(HostOutcome::NotRunning);
    };
    let mut decision = upgrade::decide(&snap.info, snap.terminals.len(), bundled, choice);
    if decision == Decision::Upgrade {
        match handoff(dir, &c, &snap, bundled).await {
            Ok(kept) => {
                update_core::host_event("upgraded", kept);
                println!(
                    "Terminal host upgraded to {bundled}; {} kept.",
                    plural(kept)
                );
                return Ok(HostOutcome::Upgraded { kept });
            }
            Err(e) => {
                eprintln!("Live upgrade of the terminal host failed: {e:#}");
                // The old daemon stays authoritative until the successor confirms, so it still
                // answers. Re-decide as one that cannot hand over.
                let Some((c2, again)) = snapshot(dir).await else {
                    update_core::host_event("failed", 0);
                    return Err(e);
                };
                (c, snap) = (c2, again);
                if let Some(caps) = snap.info["caps"].as_array_mut() {
                    caps.retain(|x| x != "upgrade");
                }
                decision = upgrade::decide(&snap.info, snap.terminals.len(), bundled, choice);
            }
        }
    }
    let live = snap.live();
    match decision {
        Decision::Current => {
            update_core::host_event("current", live);
            println!("Terminal host {bundled} is current.");
            Ok(HostOutcome::Current)
        }
        Decision::Upgrade => unreachable!("handled above"),
        Decision::RestartNow => {
            stop(dir, &c).await?;
            start_fresh(dir, bundled).await?;
            update_core::host_event("restarted", 0);
            println!("Terminal host restarted ({bundled}); no terminal was running.");
            Ok(HostOutcome::Restarted { ended: 0 })
        }
        Decision::Prompt => {
            let report = check_report(&snap, bundled, "").unwrap_or_default();
            println!("{report}");
            if interactive && ask(&format!("Restart now and end {}?", plural(live))) {
                stop(dir, &c).await?;
                start_fresh(dir, bundled).await?;
                update_core::host_event("restarted", live);
                println!(
                    "Terminal host restarted ({bundled}); {} ended.",
                    plural(live)
                );
                return Ok(HostOutcome::Restarted { ended: live });
            }
            update_core::host_event("needs-restart", live);
            println!(
                "Kept as is. Restart it later: atlas hostd --restart (or quit the terminals and run atlas hostd --reconcile)."
            );
            Ok(HostOutcome::Kept {
                live,
                decision: if interactive {
                    DecisionKind::Declined
                } else {
                    DecisionKind::Prompt
                },
            })
        }
        Decision::WaitIdle | Decision::Deferred | Decision::Passive => {
            let kind = match decision {
                Decision::WaitIdle => DecisionKind::WaitIdle,
                Decision::Deferred => DecisionKind::Deferred,
                _ => DecisionKind::Passive,
            };
            if let Some(r) = check_report(&snap, bundled, "") {
                println!("{r}");
            }
            // Passive: only the version differs, nothing the user would miss.
            let state = if decision == Decision::Passive {
                "current"
            } else {
                "needs-restart"
            };
            update_core::host_event(state, live);
            Ok(HostOutcome::Kept {
                live,
                decision: kind,
            })
        }
    }
}

/// True when both stdin and stdout are terminals (a prompt can be answered).
pub(crate) fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// `--choice` value.
pub(crate) fn parse_choice(s: &str) -> Choice {
    match s {
        "idle" => Choice::WhenIdle,
        "later" => Choice::Later,
        _ => Choice::Ask,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(version: &str, caps: &[&str], terminals: usize) -> Snapshot {
        Snapshot {
            info: json!({
                "version": version, "proto": 1, "caps": caps, "pid": 42,
                "uptime_s": 40 * 3600, "live_ptys": terminals,
            }),
            terminals: (0..terminals)
                .map(|i| json!({"cwd": format!("/w/{i}"), "argv": ["zsh"], "exited": null}))
                .collect(),
        }
    }

    #[test]
    fn current_host_has_no_report() {
        let s = snap("1.2.1", atlas_host::protocol::CAPS, 2);
        assert_eq!(check_report(&s, "1.2.1", ""), None);
    }

    #[test]
    fn stale_host_report_names_version_pid_uptime_terminals_and_remedies() {
        let s = snap("0.6.2", &[], 2);
        let r = check_report(&s, "1.2.1", " --dir /d").unwrap();
        assert!(
            r.starts_with("Terminal host 0.6.2 (pid 42, up 40h, 2 terminals) is older than Atlas 1.2.1; language features"),
            "{r}"
        );
        assert!(
            r.contains("  /w/0  zsh") && r.contains("  /w/1  zsh"),
            "{r}"
        );
        assert!(r.contains("atlas hostd --dir /d --reconcile"), "{r}");
        assert!(r.contains("atlas hostd --dir /d --restart"), "{r}");
        assert!(r.contains("ends the terminals above"), "{r}");
    }

    #[test]
    fn upgradable_host_report_promises_no_loss() {
        let s = snap("0.6.2", &["pty_attach_raw", "upgrade"], 1);
        let r = check_report(&s, "1.2.1", "").unwrap();
        assert!(r.contains("1 terminal)"), "{r}");
        assert!(r.contains("hands the terminals over live"), "{r}");
    }

    #[test]
    fn uptime_formats() {
        assert_eq!(fmt_uptime(5), "5s");
        assert_eq!(fmt_uptime(720), "12m");
        assert_eq!(fmt_uptime(40 * 3600), "40h");
        assert_eq!(fmt_uptime(99 * 3600), "99h");
        assert_eq!(fmt_uptime(4 * 86_400 + 4 * 3600), "4d 4h");
    }

    #[test]
    fn choice_parses() {
        assert_eq!(parse_choice("ask"), Choice::Ask);
        assert_eq!(parse_choice("idle"), Choice::WhenIdle);
        assert_eq!(parse_choice("later"), Choice::Later);
    }
}
