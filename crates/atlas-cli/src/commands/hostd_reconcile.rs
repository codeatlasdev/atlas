//! Keeping the terminal host (`atlas hostd`) in step with the CLI: report it, hand it over
//! live, or restart it. Used by `atlas hostd --check|--reconcile|--restart` and, at the end of
//! `atlas update`, by [`reconcile_host`].

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use atlas_host::HostClient;
use atlas_host::upgrade::{self, Choice, Decision};
use atlas_i18n::{t, t_in};
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
fn command_of(tag: &str, p: &Value) -> String {
    let argv: Vec<&str> = p["argv"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    if argv.is_empty() {
        p["title"]
            .as_str()
            .map_or_else(|| t_in!(tag, "cmd_hostd_terminal_untitled"), str::to_string)
    } else {
        argv.join(" ")
    }
}

/// One line per running terminal: `  <cwd>  <command>`.
pub(crate) fn terminal_lines(tag: &str, s: &Snapshot) -> Vec<String> {
    s.terminals
        .iter()
        .map(|p| {
            format!(
                "  {}  {}",
                p["cwd"].as_str().unwrap_or("?"),
                command_of(tag, p)
            )
        })
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

/// What a stale host cannot offer, by the capability it lacks; an id this build does not know is shown as it is.
fn feature_name(tag: &str, cap: &str) -> String {
    match cap {
        "lsp" => t_in!(tag, "cmd_hostd_feature_lsp"),
        "pty_attach_raw" => t_in!(tag, "cmd_hostd_feature_pty_attach_raw"),
        "agents" => t_in!(tag, "cmd_hostd_feature_agents"),
        other => other.to_string(),
    }
}

/// The lost features as one list: one, two or three names (`upgrade::NEEDED` has three), always in the
/// order a person reads them (what the editor loses first), whatever order hostd lists its capabilities in.
fn features_list(tag: &str, caps: &[&str]) -> String {
    const ORDER: [&str; 3] = ["lsp", "pty_attach_raw", "agents"];
    let mut caps = caps.to_vec();
    caps.sort_by_key(|c| ORDER.iter().position(|o| o == c).unwrap_or(ORDER.len()));
    let names: Vec<String> = caps.iter().map(|c| feature_name(tag, c)).collect();
    let name = |i: usize| names.get(i).cloned().unwrap_or_default();
    t_in!(
        tag,
        "cmd_hostd_features_list",
        count = names.len(),
        first = name(0),
        second = name(1),
        third = name(2)
    )
}

/// The `--check` report in the display language: `None` when the host is current.
pub(crate) fn check_report(s: &Snapshot, bundled: &str, dir_hint: &str) -> Option<String> {
    check_report_in(atlas_i18n::lang(), s, bundled, dir_hint)
}

/// [`check_report`] in `tag`. Pure, so it is unit-tested.
pub(crate) fn check_report_in(
    tag: &str,
    s: &Snapshot,
    bundled: &str,
    dir_hint: &str,
) -> Option<String> {
    // `skew_reason` only decides whether the host is current; its English sentence is the app's (and the
    // logs'), the report is written from the catalog.
    upgrade::skew_reason(&s.info, bundled, "")?;
    let version = s.info["version"]
        .as_str()
        .map_or_else(|| t_in!(tag, "cmd_hostd_version_unknown"), str::to_string);
    let pid = s.info["pid"].as_u64().unwrap_or(0);
    let up = fmt_uptime(s.info["uptime_s"].as_u64().unwrap_or(0));
    let live = s.live();
    let lost = upgrade::missing_caps(&s.info);
    let mut out = if lost.is_empty() {
        t_in!(
            tag,
            "cmd_hostd_check_older",
            version = version,
            pid = pid,
            uptime = up,
            count = live,
            bundled = bundled
        )
    } else {
        t_in!(
            tag,
            "cmd_hostd_check_older_features",
            version = version,
            pid = pid,
            uptime = up,
            count = live,
            bundled = bundled,
            features = features_list(tag, &lost)
        )
    };
    if live > 0 {
        out.push('\n');
        out.push_str(&t_in!(tag, "cmd_hostd_check_running_terminals"));
        out.push('\n');
        out.push_str(&terminal_lines(tag, s).join("\n"));
    }
    out.push('\n');
    out.push_str(&t_in!(tag, "cmd_hostd_check_fix"));
    out.push('\n');
    let reconcile = format!("atlas hostd{dir_hint} --reconcile");
    let restart = format!("atlas hostd{dir_hint} --restart");
    let width = reconcile.len().max(restart.len());
    let reconcile_note = if upgrade::caps_of(&s.info).contains(&"upgrade") {
        t_in!(tag, "cmd_hostd_check_fix_live")
    } else if live == 0 {
        t_in!(tag, "cmd_hostd_check_fix_asks")
    } else {
        t_in!(tag, "cmd_hostd_check_fix_asks_ends")
    };
    out.push_str(&format!("  {reconcile:<width$}   # {reconcile_note}\n"));
    out.push_str(&format!(
        "  {restart:<width$}   # {}",
        t_in!(tag, "cmd_hostd_check_fix_restart")
    ));
    Some(out)
}

/// Ask `question [y/N]` on the terminal; anything but yes (`y`, `yes`, `s`, `sim`) is no.
fn ask(question: &str) -> bool {
    print!("{question} {} ", t!("cmd_hostd_yes_no"));
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).is_ok() && is_yes(&line)
}

/// `y`/`yes` in English, `s`/`sim` in Portuguese (the hint says `[s/N]`); anything else is no.
fn is_yes(answer: &str) -> bool {
    matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes" | "s" | "sim"
    )
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
    Err(anyhow!(t!("cmd_hostd_stop_timeout")))
}

/// Start a fresh daemon from this binary in `dir` and check it reports `bundled`.
async fn start_fresh(dir: &Path, bundled: &str) -> Result<()> {
    let exe = std::env::current_exe()?;
    let c = atlas_host::client::ensure_in(dir, &exe, "cli").await?;
    let v = c.call("info", json!({})).await?;
    if v["version"].as_str() != Some(bundled) {
        return Err(anyhow!(t!(
            "cmd_hostd_version_mismatch",
            reported = v["version"].to_string(),
            expected = bundled
        )));
    }
    Ok(())
}

/// Shutdown (ends the terminals) then start fresh from this binary, then verify.
pub(crate) async fn restart_host(dir: &Path, bundled: &str) -> Result<HostOutcome> {
    update_core::phase(Phase::TerminalHost);
    let Some((c, snap)) = snapshot(dir).await else {
        // The app starts one (current) on demand.
        println!("{}", t!("cmd_hostd_not_running"));
        return Ok(HostOutcome::NotRunning);
    };
    let ended = snap.live();
    stop(dir, &c).await?;
    start_fresh(dir, bundled).await?;
    update_core::host_event("restarted", ended);
    println!(
        "{}",
        t!(
            "cmd_hostd_restarted_ended",
            version = bundled,
            count = ended
        )
    );
    Ok(HostOutcome::Restarted { ended })
}

/// `upgrade {exe}` then verify: the new daemon reports `bundled` and no terminal was lost.
async fn handoff(dir: &Path, c: &HostClient, before: &Snapshot, bundled: &str) -> Result<usize> {
    let exe = std::env::current_exe()?;
    c.call("upgrade", json!({"exe": exe.to_string_lossy()}))
        .await
        .with_context(|| t!("cmd_hostd_ctx_handoff"))?;
    // The successor takes the socket over; wait until it answers with the new version.
    for _ in 0..200 {
        if let Some((_, after)) = snapshot(dir).await
            && after.info["version"].as_str() == Some(bundled)
            && after.info["pid"] != before.info["pid"]
        {
            if after.live() < before.live() {
                return Err(anyhow!(t!(
                    "cmd_hostd_upgrade_lost_terminals",
                    before = before.live(),
                    after = after.live()
                )));
            }
            return Ok(after.live());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(anyhow!(t!("cmd_hostd_upgrade_no_start")))
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
        println!("{}", t!("cmd_hostd_not_running"));
        return Ok(HostOutcome::NotRunning);
    };
    let mut decision = upgrade::decide(&snap.info, snap.terminals.len(), bundled, choice);
    if decision == Decision::Upgrade {
        match handoff(dir, &c, &snap, bundled).await {
            Ok(kept) => {
                update_core::host_event("upgraded", kept);
                println!(
                    "{}",
                    t!(
                        "cmd_hostd_reconcile_upgraded",
                        version = bundled,
                        count = kept
                    )
                );
                return Ok(HostOutcome::Upgraded { kept });
            }
            Err(e) => {
                eprintln!(
                    "{}",
                    t!("cmd_hostd_upgrade_failed", error = format!("{e:#}"))
                );
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
            println!("{}", t!("cmd_hostd_current", version = bundled));
            Ok(HostOutcome::Current)
        }
        Decision::Upgrade => unreachable!("handled above"),
        Decision::RestartNow => {
            stop(dir, &c).await?;
            start_fresh(dir, bundled).await?;
            update_core::host_event("restarted", 0);
            println!("{}", t!("cmd_hostd_restarted_idle", version = bundled));
            Ok(HostOutcome::Restarted { ended: 0 })
        }
        Decision::Prompt => {
            let report = check_report(&snap, bundled, "").unwrap_or_default();
            println!("{report}");
            if interactive && ask(&t!("cmd_hostd_ask_restart", count = live)) {
                stop(dir, &c).await?;
                start_fresh(dir, bundled).await?;
                update_core::host_event("restarted", live);
                println!(
                    "{}",
                    t!("cmd_hostd_restarted_ended", version = bundled, count = live)
                );
                return Ok(HostOutcome::Restarted { ended: live });
            }
            update_core::host_event("needs-restart", live);
            println!("{}", t!("cmd_hostd_kept"));
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
        let s = snap("1.2.1", &["pty_attach_raw", "upgrade", "lsp", "agents"], 2);
        assert_eq!(check_report_in("en", &s, "1.2.1", ""), None);
    }

    #[test]
    fn stale_host_report_names_version_pid_uptime_terminals_and_remedies() {
        let s = snap("0.6.2", &[], 2);
        let r = check_report_in("en", &s, "1.2.1", " --dir /d").unwrap();
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
        let r = check_report_in("en", &s, "1.2.1", "").unwrap();
        assert!(r.contains("1 terminal)"), "{r}");
        assert!(r.contains("hands the terminals over live"), "{r}");
    }

    #[test]
    fn every_capability_a_host_can_lack_has_a_name_in_every_language() {
        // The list message holds three names at most.
        assert!(upgrade::NEEDED.len() <= 3, "{:?}", upgrade::NEEDED);
        for tag in atlas_i18n::locales() {
            for cap in upgrade::NEEDED {
                let name = feature_name(tag, cap);
                assert!(!name.is_empty() && name != *cap, "{tag}: {cap} -> {name}");
            }
        }
    }

    #[test]
    fn the_lost_features_read_as_a_list() {
        let list = |caps: &[&str]| features_list("en", caps);
        assert_eq!(list(&["lsp"]), "language features");
        assert_eq!(
            list(&["lsp", "agents"]),
            "language features and agent status"
        );
        assert_eq!(
            list(&["lsp", "pty_attach_raw", "agents"]),
            "language features, fast terminal rendering and agent status"
        );
        // hostd lists `NEEDED` in its own order (`pty_attach_raw` first): the sentence keeps one.
        assert_eq!(
            list(&["pty_attach_raw", "lsp", "agents"]),
            "language features, fast terminal rendering and agent status"
        );
    }

    #[test]
    fn an_unknown_version_and_an_unknown_capability_show_as_they_are() {
        assert_eq!(feature_name("en", "teleport"), "teleport");
        let s = Snapshot {
            info: json!({"proto": 1, "caps": ["pty_attach_raw", "lsp", "agents"], "pid": 7}),
            terminals: vec![],
        };
        let r = check_report_in("en", &s, "1.2.1", "").unwrap();
        assert!(
            r.starts_with("Terminal host unknown (pid 7, up 0s, no terminals)"),
            "{r}"
        );
    }

    #[test]
    fn a_report_in_another_language_keeps_the_commands_and_numbers() {
        let s = snap("0.6.2", &[], 2);
        let en = check_report_in("en", &s, "1.2.1", " --dir /d").unwrap();
        for tag in atlas_i18n::locales() {
            let r = check_report_in(tag, &s, "1.2.1", " --dir /d").unwrap();
            // The values and commands are verbatim in every language, the pseudo-locale (debug builds) included; the
            // word "pid" is message text, which en-XA accents, so the test looks for the number alone.
            assert!(
                r.contains("0.6.2") && r.contains("1.2.1") && r.contains("42"),
                "{tag}: {r}"
            );
            assert!(r.contains("atlas hostd --dir /d --reconcile"), "{r}");
            assert!(r.contains("  /w/0  zsh"), "{r}");
            assert_eq!(r == en, tag == "en", "{tag}: {r}");
        }
    }

    #[test]
    fn the_prompt_takes_yes_in_both_languages() {
        for yes in ["y", "YES\n", " s ", "sim"] {
            assert!(is_yes(yes), "{yes:?}");
        }
        for no in ["", "n", "no", "não", "maybe"] {
            assert!(!is_yes(no), "{no:?}");
        }
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
