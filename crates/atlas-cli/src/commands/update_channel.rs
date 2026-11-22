//! The channel half of `atlas update`: what an offered release means for the install that runs
//! (upgrade, current, or ahead), and the text and JSON of `atlas update channel`. Everything
//! here is pure (no network, no `~/.atlas`), so it is tested directly; `self_update` does the I/O.

use std::path::Path;

use atlas_core::channel::{self, Action, Channel, ChannelView, Decision};
use atlas_core::yanked::Yank;
use atlas_i18n::t;
use semver::Version;
use serde_json::{Value, json};

const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

/// A command inside a sentence; the sentence is the catalog's.
fn bold(text: &str) -> String {
    format!("{BOLD}{text}{RESET}")
}

/// One piece of a channel's copy in the display language. The English is `data/channels.json` (shared with the
/// website and the desktop) and also the catalog's `cmd_update_channel_<id>_<field>`; a test keeps the two equal,
/// so a change of the copy cannot leave the CLI saying another thing.
// i18n-keys: cmd_update_channel_*
fn channel_copy(channel: Channel, field: &str) -> String {
    channel_copy_in(atlas_i18n::lang(), channel, field)
}

fn channel_copy_in(tag: &str, channel: Channel, field: &str) -> String {
    atlas_i18n::translate(tag, &format!("cmd_update_channel_{channel}_{field}"), None)
}

/// The offered tag as a version plus what to do with it, or `None` when the tag is not SemVer.
pub(crate) fn offer(installed: &Version, tag: &str) -> Option<(Version, Decision)> {
    let offered = channel::parse_tag(tag)?;
    let decision = channel::decide(installed, &offered);
    Some((offered, decision))
}

/// The one-paragraph answer to `atlas update` for an offered tag: the three outcomes the policy
/// distinguishes. `downgrade` is whether the user already asked for `--downgrade` (the hint to
/// pass the flag is then redundant).
pub(crate) fn describe(installed: &Version, tag: &str, on: Channel, downgrade: bool) -> String {
    let Some((offered, decision)) = offer(installed, tag) else {
        return t!(
            "cmd_update_describe_unrecognized",
            channel = on.to_string(),
            tag = tag
        );
    };
    match decision {
        Decision::Upgrade => format!(
            "{}\n  {}",
            t!(
                "cmd_update_describe_upgrade",
                installed = installed.to_string(),
                offered = offered.to_string()
            ),
            t!("cmd_update_describe_run", command = bold("atlas update"))
        ),
        Decision::Current => format!(
            "\x1b[1;32m✓{RESET} {}",
            t!(
                "cmd_update_describe_current",
                installed = installed.to_string()
            )
        ),
        Decision::Ahead => {
            let how = if downgrade {
                String::new()
            } else {
                format!(
                    "\n  {}",
                    t!(
                        "cmd_update_describe_ahead_how",
                        offered = offered.to_string(),
                        command = bold("atlas update --downgrade")
                    )
                )
            };
            format!(
                "{}\n  {}{how}",
                t!(
                    "cmd_update_describe_ahead",
                    installed = installed.to_string(),
                    channel = on.to_string(),
                    offered = offered.to_string()
                ),
                t!(
                    "cmd_update_describe_ahead_nothing",
                    channel = on.to_string()
                )
            )
        }
    }
}

/// What a user who runs a withdrawn version (`yank`, from `data/yanked.json`) is told: why, what
/// replaces it, and what `atlas update` does about it. `offered` is the newest release the channel
/// still offers; when it is older than what runs, installing it is a rollback and says so.
pub(crate) fn yank_notice(
    installed: &Version,
    yank: &Yank,
    offered: Option<&Version>,
    on: Channel,
) -> String {
    let mut out = format!(
        "\x1b[1;33m!{RESET} {}",
        t!(
            "cmd_update_yank_withdrawn",
            installed = installed.to_string(),
            reason = yank.reason.trim()
        )
    );
    if let Some(r) = &yank.replacement {
        out.push_str(&format!(
            "\n  {}",
            t!(
                "cmd_update_yank_replacement",
                version = r.trim_start_matches('v')
            )
        ));
    }
    out.push_str(&match offered.map(|o| (o, channel::decide(installed, o))) {
        Some((o, Decision::Upgrade)) => format!(
            "\n  {}",
            t!(
                "cmd_update_yank_installs",
                command = bold("atlas update"),
                version = o.to_string()
            )
        ),
        Some((o, _)) => format!(
            "\n  {}",
            t!(
                "cmd_update_yank_rollback",
                channel = on.to_string(),
                offered = o.to_string(),
                command = bold("atlas update"),
                installed = installed.to_string()
            )
        ),
        None => format!(
            "\n  {}",
            t!("cmd_update_yank_none", channel = on.to_string())
        ),
    });
    out
}

/// `report` (an `atlas update --check --json` report) plus `yanked`: the withdrawal entry of the
/// running version (`version`, `reason`, `replacement`), or `null` when it is not withdrawn.
pub(crate) fn with_yank(mut report: Value, yank: Option<&Yank>) -> Value {
    report["yanked"] = json!(yank);
    report
}

/// `atlas update --check --json`.
pub(crate) fn check_json(installed: &Version, on: Channel, tag: Option<&str>) -> Value {
    let offered = tag.and_then(channel::parse_tag);
    let action = match offered.as_ref().map(|o| channel::decide(installed, o)) {
        Some(Decision::Upgrade) => Action::Update,
        Some(Decision::Ahead) => Action::Downgrade,
        Some(Decision::Current) | None => Action::None,
    };
    json!({
        "installed": installed.to_string(),
        "channel": on,
        "offered": offered.map(|v| v.to_string()),
        "tag": tag,
        "action": action,
    })
}

/// `check_json` for an install another tool owns (`how` = `homebrew` | `package`): the same
/// report plus who updates it. `action` becomes `package` / `managed` only when an upgrade is on
/// offer (the one case where something must be installed by that tool); `none` and `downgrade`
/// stay as they are. `command` is the exact line to run, only present for that upgrade.
pub(crate) fn managed_json(
    mut report: Value,
    how: &str,
    manager: Option<&str>,
    command: &str,
) -> Value {
    let upgrade = report["action"] == "update";
    report["managed"] = json!(true);
    report["manager"] = json!(manager);
    report["command"] = if upgrade { json!(command) } else { Value::Null };
    if upgrade {
        report["action"] = json!(if how == "package" {
            "package"
        } else {
            "managed"
        });
    }
    report
}

/// `atlas update channel --json`: the real serialized views plus the selection.
pub(crate) fn channels_json(
    views: &[ChannelView],
    selected: Channel,
    installed: &Version,
) -> Value {
    json!({
        "selected": selected,
        "installed": installed.to_string(),
        "channels": views,
    })
}

/// What a channel offers right now, for one line.
fn offer_line(v: &ChannelView, installed: &Version) -> String {
    match (&v.tag, v.action) {
        (None, _) => t!("cmd_update_offer_none"),
        (Some(tag), Action::Update) => t!(
            "cmd_update_offer_update",
            tag = tag.as_str(),
            installed = installed.to_string()
        ),
        (Some(tag), Action::Downgrade) => t!(
            "cmd_update_offer_downgrade",
            tag = tag.as_str(),
            installed = installed.to_string()
        ),
        (Some(tag), Action::None) => t!("cmd_update_offer_same", tag = tag.as_str()),
    }
}

/// `atlas update channel` without a name: every channel with its promise, risk and live offer.
pub(crate) fn render_channels(views: &[ChannelView], installed: &Version) -> String {
    let mut out = String::new();
    if channel::stage() == "alpha" {
        out.push_str(&t!("cmd_update_stage_note"));
        out.push_str("\n\n");
    }
    for (i, v) in views.iter().enumerate() {
        let mark = if v.selected {
            format!("  {}", t!("cmd_update_channels_selected"))
        } else {
            String::new()
        };
        out.push_str(&format!(
            "{}. {BOLD}{}{RESET}  \x1b[2m[{}]{RESET}{mark}\n",
            i + 1,
            v.label,
            channel_copy(v.channel, "risk")
        ));
        out.push_str(&format!("   {}\n", channel_copy(v.channel, "promise")));
        out.push_str(&format!(
            "   {}\n",
            t!(
                "cmd_update_channels_cadence",
                text = channel_copy(v.channel, "cadence")
            )
        ));
        out.push_str(&format!(
            "   {}\n",
            t!(
                "cmd_update_channels_for",
                text = channel_copy(v.channel, "audience")
            )
        ));
        out.push_str(&format!(
            "   {}\n",
            t!(
                "cmd_update_channels_offers",
                text = offer_line(v, installed)
            )
        ));
        if v.alpha_era_note.is_some() {
            out.push_str(&format!(
                "   {}\n",
                t!(
                    "cmd_update_channels_note",
                    text = channel_copy(v.channel, "note")
                )
            ));
        }
        out.push('\n');
    }
    out
}

/// After a channel was set: what switching means, and what it offers now.
pub(crate) fn render_switched(v: &ChannelView) -> String {
    let mut out = format!(
        "{}\n  {}\n",
        t!("cmd_update_switched_title", channel = bold(&v.label)),
        channel_copy(v.channel, "switch")
    );
    match (&v.tag, v.action) {
        (None, _) => out.push_str(&format!("  {}\n", t!("cmd_update_switched_none"))),
        (Some(tag), Action::Update) => out.push_str(&format!(
            "  {}\n",
            t!(
                "cmd_update_switched_update",
                tag = tag.as_str(),
                command = bold("atlas update")
            )
        )),
        (Some(tag), Action::Downgrade) => out.push_str(&format!(
            "  {}\n",
            t!(
                "cmd_update_switched_downgrade",
                tag = tag.as_str(),
                command = bold("atlas update --downgrade")
            )
        )),
        (Some(tag), Action::None) => out.push_str(&format!(
            "  {}\n",
            t!("cmd_update_switched_same", tag = tag.as_str())
        )),
    }
    out
}

/// An answer to the numbered picker.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Pick {
    /// Empty answer: leave the channel as it is.
    Keep,
    Set(Channel),
    Invalid,
}

/// `1`-`3` (the order of [`Channel::ALL`]) or a channel name; empty keeps the current one.
pub(crate) fn parse_pick(line: &str) -> Pick {
    let line = line.trim();
    if line.is_empty() {
        return Pick::Keep;
    }
    if let Ok(n) = line.parse::<usize>() {
        return n
            .checked_sub(1)
            .and_then(|i| Channel::ALL.get(i))
            .map_or(Pick::Invalid, |c| Pick::Set(*c));
    }
    line.parse().map_or(Pick::Invalid, Pick::Set)
}

/// `[y/N]` (`[s/N]` in pt-BR): only an explicit yes. A downgrade can leave data an older build cannot read.
pub(crate) fn parse_confirm(line: &str) -> bool {
    matches!(
        line.trim().to_lowercase().as_str(),
        "y" | "yes" | "s" | "sim"
    )
}

/// Write the channel to `file` (`atlas_core::channel::file()` outside tests).
pub(crate) fn persist_channel(file: &Path, channel: Channel) -> std::io::Result<()> {
    channel::write_to(file, channel)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ver(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn offer_decisions_follow_precedence() {
        let installed = ver("1.4.0-alpha.2");
        assert_eq!(
            offer(&installed, "v1.4.0-alpha.10").unwrap().1,
            Decision::Upgrade
        );
        assert_eq!(
            offer(&installed, "v1.4.0-alpha.2+build.7").unwrap().1,
            Decision::Current,
            "build metadata is not precedence"
        );
        assert_eq!(
            offer(&ver("1.5.0-alpha.1"), "v1.3.0").unwrap().1,
            Decision::Ahead,
            "stable 1.3.0 is older than an alpha of 1.5.0"
        );
        assert_eq!(
            offer(&ver("1.3.0"), "v1.4.0-beta.1").unwrap().1,
            Decision::Upgrade
        );
        assert!(offer(&installed, "nightly").is_none(), "non-semver tag");
    }

    #[test]
    fn decision_messages() {
        let up = describe(&ver("1.3.0"), "v1.4.0", Channel::Stable, false);
        assert!(up.contains("v1.3.0") && up.contains("v1.4.0"), "{up}");
        assert!(up.contains("atlas update"), "{up}");

        let cur = describe(&ver("1.3.0"), "v1.3.0", Channel::Stable, false);
        assert!(cur.contains("Already up to date"), "{cur}");

        let ahead = describe(&ver("1.5.0-alpha.1"), "v1.3.0", Channel::Stable, false);
        assert!(ahead.contains("v1.5.0-alpha.1"), "names what runs: {ahead}");
        assert!(
            ahead.contains("v1.3.0"),
            "names what stable offers: {ahead}"
        );
        assert!(ahead.contains("stable"), "names the channel: {ahead}");
        assert!(ahead.contains("atlas update --downgrade"), "{ahead}");
        assert!(ahead.contains("updates again"), "{ahead}");
        assert!(ahead.contains("Nothing is installed"), "{ahead}");

        let asked = describe(&ver("1.5.0-alpha.1"), "v1.3.0", Channel::Stable, true);
        assert!(
            !asked.contains("atlas update --downgrade"),
            "no hint when already asked"
        );

        assert!(describe(&ver("1.3.0"), "nightly", Channel::Beta, false).contains("nightly"));
    }

    #[test]
    fn check_json_has_the_documented_shape() {
        let j = check_json(&ver("1.3.0"), Channel::Beta, Some("v1.4.0-beta.1"));
        assert_eq!(
            j,
            json!({
                "installed": "1.3.0",
                "channel": "beta",
                "offered": "1.4.0-beta.1",
                "tag": "v1.4.0-beta.1",
                "action": "update",
            })
        );
        assert_eq!(
            check_json(&ver("1.5.0-alpha.1"), Channel::Stable, Some("v1.3.0"))["action"],
            "downgrade"
        );
        assert_eq!(
            check_json(&ver("1.3.0"), Channel::Stable, Some("v1.3.0"))["action"],
            "none"
        );
        let none = check_json(&ver("1.3.0"), Channel::Stable, None);
        assert_eq!(none["offered"], Value::Null);
        assert_eq!(none["tag"], Value::Null);
        assert_eq!(none["action"], "none");
    }

    fn yank(replacement: Option<&str>) -> Yank {
        Yank {
            version: "1.5.0-alpha.3".into(),
            reason: "corrupts the workspace".into(),
            replacement: replacement.map(Into::into),
        }
    }

    #[test]
    fn a_yanked_running_version_says_why_and_what_to_do() {
        let me = ver("1.5.0-alpha.3");
        let up = yank_notice(
            &me,
            &yank(Some("1.5.0-alpha.4")),
            Some(&ver("1.5.0-alpha.4")),
            Channel::Alpha,
        );
        for part in [
            "v1.5.0-alpha.3 was withdrawn",
            "corrupts the workspace",
            "Use v1.5.0-alpha.4",
            "installs v1.5.0-alpha.4",
        ] {
            assert!(up.contains(part), "{part}: {up}");
        }
        // The safe release is older than what runs: a rollback, said as one.
        let back = yank_notice(
            &me,
            &yank(None),
            Some(&ver("1.5.0-alpha.2")),
            Channel::Alpha,
        );
        assert!(
            back.contains("older than what you run") && back.contains("v1.5.0-alpha.2"),
            "{back}"
        );
        assert!(!back.contains("Use v"), "{back}");
        let none = yank_notice(&me, &yank(None), None, Channel::Beta);
        assert!(
            none.contains("No beta release that is not withdrawn"),
            "{none}"
        );
    }

    #[test]
    fn the_report_carries_the_withdrawal_or_null() {
        let base = check_json(
            &ver("1.5.0-alpha.3"),
            Channel::Alpha,
            Some("v1.5.0-alpha.4"),
        );
        assert_eq!(with_yank(base.clone(), None)["yanked"], Value::Null);
        assert_eq!(
            with_yank(base, Some(&yank(Some("1.5.0-alpha.4"))))["yanked"],
            json!({"version": "1.5.0-alpha.3", "reason": "corrupts the workspace", "replacement": "1.5.0-alpha.4"})
        );
    }

    #[test]
    fn managed_json_keeps_the_report_and_says_who_installs() {
        let base = check_json(&ver("1.3.0"), Channel::Beta, Some("v1.4.0-beta.1"));
        let pkg = managed_json(base.clone(), "package", Some("pacman"), "sudo pacman -U x");
        assert_eq!(
            pkg["action"], "package",
            "an update a package manager must install"
        );
        assert_eq!(pkg["managed"], true);
        assert_eq!(pkg["manager"], "pacman");
        assert_eq!(pkg["command"], "sudo pacman -U x");
        assert_eq!(pkg["installed"], "1.3.0");
        assert_eq!(pkg["offered"], "1.4.0-beta.1");
        let brew = managed_json(base, "homebrew", None, "brew upgrade atlas");
        assert_eq!(brew["action"], "managed");
        assert_eq!(brew["manager"], Value::Null);
    }

    #[test]
    fn managed_json_has_no_command_when_nothing_is_to_install() {
        let cur = check_json(&ver("1.3.0"), Channel::Stable, Some("v1.3.0"));
        let j = managed_json(cur, "package", Some("apt"), "sudo apt install ./x.deb");
        assert_eq!(
            j["action"], "none",
            "already current: nothing for anyone to install"
        );
        assert_eq!(j["managed"], true);
        assert_eq!(j["command"], Value::Null);
        let down = check_json(&ver("1.5.0-alpha.1"), Channel::Stable, Some("v1.3.0"));
        assert_eq!(
            managed_json(down, "package", Some("apt"), "c")["action"],
            "downgrade",
            "a downgrade is reported as such, never as a package install"
        );
    }

    fn views(selected: Channel, installed: &Version) -> Vec<ChannelView> {
        Channel::ALL
            .into_iter()
            .map(|c| {
                let tag = match c {
                    Channel::Stable => Some("v1.3.0".to_string()),
                    Channel::Beta => Some("v1.4.0-beta.1".to_string()),
                    Channel::Alpha => None,
                };
                channel::view(c, selected, installed, tag)
            })
            .collect()
    }

    #[test]
    fn channels_json_is_the_real_serialized_view_list() {
        let installed = ver("1.3.0");
        let vs = views(Channel::Beta, &installed);
        let j = channels_json(&vs, Channel::Beta, &installed);
        assert_eq!(j["selected"], "beta");
        assert_eq!(j["installed"], "1.3.0");
        let list = j["channels"].as_array().unwrap();
        assert_eq!(list.len(), 3);
        // Exactly what serde produces for the Rust value, not a hand-written copy.
        assert_eq!(list[1], serde_json::to_value(&vs[1]).unwrap());
        assert_eq!(list[1]["channel"], "beta");
        assert_eq!(list[1]["selected"], true);
        assert_eq!(list[1]["action"], "update");
        assert_eq!(list[0]["selected"], false);
        assert_eq!(list[2]["tag"], Value::Null);
    }

    #[test]
    fn channel_listing_shows_copy_offers_and_the_stage_note() {
        let installed = ver("1.3.0");
        let vs = views(Channel::Stable, &installed);
        let out = render_channels(&vs, &installed);
        for v in &vs {
            for part in [&v.label, &v.risk_label, &v.promise, &v.cadence, &v.audience] {
                assert!(out.contains(part.as_str()), "missing {part:?} in:\n{out}");
            }
        }
        assert!(out.contains("v1.4.0-beta.1"), "{out}");
        assert!(out.contains("(selected)"), "{out}");
        assert_eq!(
            out.matches("(selected)").count(),
            1,
            "only one channel is selected"
        );
        assert!(
            out.contains("nothing published"),
            "alpha has no tag in this fixture: {out}"
        );
        if channel::stage() == "alpha" {
            assert!(out.contains(channel::stage_note()), "{out}");
        }
    }

    #[test]
    fn switching_prints_the_switch_consequence_and_the_offer() {
        let installed = ver("1.3.0");
        let v = channel::view(
            Channel::Beta,
            Channel::Beta,
            &installed,
            Some("v1.4.0-beta.1".into()),
        );
        let out = render_switched(&v);
        assert!(out.contains(&v.switch_to), "{out}");
        assert!(out.contains("v1.4.0-beta.1"), "{out}");
        let none = channel::view(Channel::Alpha, Channel::Alpha, &installed, None);
        assert!(render_switched(&none).contains(&none.switch_to));
        let older = channel::view(
            Channel::Stable,
            Channel::Stable,
            &ver("1.5.0-alpha.1"),
            Some("v1.3.0".into()),
        );
        assert!(render_switched(&older).contains("atlas update --downgrade"));
    }

    #[test]
    fn picker_input_parsing() {
        assert_eq!(parse_pick("1"), Pick::Set(Channel::Stable));
        assert_eq!(parse_pick("2\n"), Pick::Set(Channel::Beta));
        assert_eq!(parse_pick(" 3 "), Pick::Set(Channel::Alpha));
        assert_eq!(parse_pick("beta"), Pick::Set(Channel::Beta));
        assert_eq!(parse_pick("ALPHA"), Pick::Set(Channel::Alpha));
        assert_eq!(parse_pick(""), Pick::Keep);
        assert_eq!(parse_pick("\n"), Pick::Keep);
        assert_eq!(parse_pick("   "), Pick::Keep);
        for bad in [
            "0",
            "4",
            "nightly",
            "1 2",
            "-1",
            "stable!",
            "99999999999999999999",
        ] {
            assert_eq!(parse_pick(bad), Pick::Invalid, "{bad:?}");
        }
    }

    #[test]
    fn downgrade_confirmation_defaults_to_no() {
        assert!(parse_confirm("y"));
        assert!(parse_confirm("YES\n"));
        assert!(
            !parse_confirm(""),
            "default is No: an older build may not read newer data"
        );
        assert!(!parse_confirm("n"));
        assert!(!parse_confirm("maybe"));
        // `[s/N]` in pt-BR
        assert!(parse_confirm("s"));
        assert!(parse_confirm("Sim\n"));
    }

    /// The copy of every channel exists in the catalog, word for word the English of `data/channels.json` (the file
    /// the website and the desktop read), and in pt-BR as another text. Editing the copy without the catalog fails.
    #[test]
    fn the_channel_copy_in_the_catalog_is_the_copy_of_channels_json() {
        for c in Channel::ALL {
            let info = c.info();
            for (field, json) in [
                ("promise", &info.promise),
                ("risk", &info.risk_label),
                ("cadence", &info.cadence),
                ("audience", &info.audience),
                ("switch", &info.switch_to),
            ] {
                assert_eq!(channel_copy_in("en", c, field), *json, "{c} {field}");
                assert_ne!(channel_copy_in("pt-BR", c, field), *json, "{c} {field}");
            }
            if let Some(note) = &info.alpha_era_note {
                assert_eq!(channel_copy_in("en", c, "note"), *note, "{c} note");
                assert_ne!(channel_copy_in("pt-BR", c, "note"), *note, "{c} note");
            }
        }
        assert_eq!(
            atlas_i18n::t_in!("en", "cmd_update_stage_note"),
            channel::stage_note()
        );
    }

    #[test]
    fn channel_is_persisted_in_the_given_file_only() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sub").join("update-channel");
        assert_eq!(
            channel::read_from(&file),
            Channel::Alpha,
            "no file = default"
        );
        persist_channel(&file, Channel::Stable).unwrap();
        assert_eq!(
            channel::read_from(&file),
            Channel::Stable,
            "an explicit choice wins"
        );
        persist_channel(&file, Channel::Alpha).unwrap();
        assert_eq!(channel::read_from(&file), Channel::Alpha);
        assert_eq!(std::fs::read_to_string(&file).unwrap().trim(), "alpha");
    }
}
