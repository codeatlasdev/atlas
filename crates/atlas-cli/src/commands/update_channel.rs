//! The channel half of `atlas update`: what an offered release means for the install that runs
//! (upgrade, current, or ahead), and the text and JSON of `atlas update channel`. Everything
//! here is pure (no network, no `~/.atlas`), so it is tested directly; `self_update` does the I/O.

use std::path::Path;

use atlas_core::channel::{self, Action, Channel, ChannelView, Decision};
use semver::Version;
use serde_json::{Value, json};

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
        return format!("Nothing installable on the {on} channel (unrecognised tag '{tag}').");
    };
    match decision {
        Decision::Upgrade => format!(
            "Update available: v{installed} → v{offered}\n  Run \x1b[1matlas update\x1b[0m to install."
        ),
        Decision::Current => format!("\x1b[1;32m✓\x1b[0m Already up to date (v{installed})"),
        Decision::Ahead => {
            let how = if downgrade {
                String::new()
            } else {
                format!(
                    "\n  To install v{offered} anyway (a downgrade): \x1b[1matlas update --downgrade\x1b[0m"
                )
            };
            format!(
                "You run v{installed}, which is newer than what {on} offers (v{offered}).\n  \
                 Nothing is installed: you will get updates again when the {on} channel passes your build.{how}"
            )
        }
    }
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
        (None, _) => "nothing published (or offline)".to_string(),
        (Some(tag), Action::Update) => format!("{tag} (update from v{installed})"),
        (Some(tag), Action::Downgrade) => {
            format!("{tag} (older than v{installed}: nothing is installed unless you ask)")
        }
        (Some(tag), Action::None) => format!("{tag} (what you run)"),
    }
}

/// `atlas update channel` without a name: every channel with its promise, risk and live offer.
pub(crate) fn render_channels(views: &[ChannelView], installed: &Version) -> String {
    let mut out = String::new();
    if channel::stage() == "alpha" {
        out.push_str(channel::stage_note());
        out.push_str("\n\n");
    }
    for (i, v) in views.iter().enumerate() {
        let mark = if v.selected { "  (selected)" } else { "" };
        out.push_str(&format!(
            "{}. \x1b[1m{}\x1b[0m  \x1b[2m[{}]\x1b[0m{mark}\n",
            i + 1,
            v.label,
            v.risk_label
        ));
        out.push_str(&format!("   {}\n", v.promise));
        out.push_str(&format!("   Cadence:  {}\n", v.cadence));
        out.push_str(&format!("   For:      {}\n", v.audience));
        out.push_str(&format!("   Offers:   {}\n", offer_line(v, installed)));
        if let Some(note) = &v.alpha_era_note {
            out.push_str(&format!("   Note:     {note}\n"));
        }
        out.push('\n');
    }
    out
}

/// After a channel was set: what switching means, and what it offers now.
pub(crate) fn render_switched(v: &ChannelView) -> String {
    let mut out = format!(
        "Update channel: \x1b[1m{}\x1b[0m\n  {}\n",
        v.label, v.switch_to
    );
    match (&v.tag, v.action) {
        (None, _) => out.push_str("  It offers nothing right now (or Atlas is offline).\n"),
        (Some(tag), Action::Update) => {
            out.push_str(&format!("  Offers {tag}: run \x1b[1matlas update\x1b[0m to install it.\n"));
        }
        (Some(tag), Action::Downgrade) => out.push_str(&format!(
            "  Offers {tag}, which is older than what you run; nothing changes unless you run \x1b[1matlas update --downgrade\x1b[0m.\n"
        )),
        (Some(tag), Action::None) => out.push_str(&format!("  Offers {tag}: that is what you run.\n")),
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

/// `[y/N]`: only an explicit yes. A downgrade can leave data an older build cannot read.
pub(crate) fn parse_confirm(line: &str) -> bool {
    matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
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
