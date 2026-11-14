//! Update channels: which releases a client follows, and what it does with them.
//!
//! A channel is a **promotion gate**, not a claim about product maturity (see
//! `docs/versioning.md`): `alpha` is cut from `main`, `beta` is a soaked alpha on a release
//! branch, `stable` is a soaked beta plus version metadata. Which channel a client follows is
//! persisted in one tiny file shared by the CLI and Atlas Desktop.

use std::fmt;
use std::str::FromStr;

use semver::Version;

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Where a client looks for updates. [`Channel::default`] is what a machine follows until its
/// user picks one (absent or unreadable `update-channel`): **Alpha** until the first Stable that
/// meets the general availability criteria (`docs/versioning.md`). Mirrored by `DEFAULT_CHANNEL`
/// in `site/functions/_lib/channels.ts` and `scripts/bootstrap.sh`, and recorded as `default` in
/// `data/channels.json` (what `release.yml` reads); tests fail if they differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Beta,
    #[default]
    Alpha,
}

/// Environment override, for scripts and CI (an invalid value is ignored).
pub const ENV: &str = "ATLAS_UPDATE_CHANNEL";

impl Channel {
    /// Most to least conservative: the order the UIs list them in.
    pub const ALL: [Channel; 3] = [Channel::Stable, Channel::Beta, Channel::Alpha];

    pub const fn as_str(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Beta => "beta",
            Channel::Alpha => "alpha",
        }
    }

    /// Whether this channel offers `v`: a release is offered everywhere; a prerelease only on
    /// the channels at least as adventurous as its label. Unknown labels are offered nowhere.
    pub fn accepts(self, v: &Version) -> bool {
        match Channel::of_version(v) {
            None => false,
            Some(Channel::Stable) => true,
            Some(Channel::Beta) => self != Channel::Stable,
            Some(Channel::Alpha) => self == Channel::Alpha,
        }
    }

    /// The channel a build of version `v` was cut for (`None` for a label Atlas does not ship).
    pub fn of_version(v: &Version) -> Option<Channel> {
        if v.pre.is_empty() {
            return Some(Channel::Stable);
        }
        match v.pre.as_str().split('.').next()? {
            "alpha" => Some(Channel::Alpha),
            "beta" | "rc" => Some(Channel::Beta),
            _ => None,
        }
    }

    pub fn info(self) -> &'static ChannelInfo {
        let copy = copy();
        copy.channels
            .iter()
            .find(|c| c.id == self.as_str())
            .expect("data/channels.json has every channel")
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Channel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "stable" => Ok(Channel::Stable),
            "beta" => Ok(Channel::Beta),
            "alpha" => Ok(Channel::Alpha),
            other => Err(format!(
                "unknown channel '{other}' (expected stable, beta or alpha)"
            )),
        }
    }
}

/// The user-facing copy of one channel (`data/channels.json`, shared with the website).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChannelInfo {
    pub id: String,
    pub label: String,
    pub promise: String,
    pub risk: String,
    pub risk_label: String,
    pub cadence: String,
    pub audience: String,
    pub switch_to: String,
    #[serde(default)]
    pub alpha_era_note: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Copy {
    default: Channel,
    stage: String,
    stage_note: String,
    channels: Vec<ChannelInfo>,
}

fn copy() -> &'static Copy {
    static COPY: std::sync::OnceLock<Copy> = std::sync::OnceLock::new();
    COPY.get_or_init(|| {
        serde_json::from_str(include_str!("../data/channels.json"))
            .expect("data/channels.json is valid")
    })
}

/// The default channel as `data/channels.json` records it, the file `release.yml` and the site read
/// (a Rust `#[default]` is not readable from there). A test pins it to [`Channel::default`].
pub fn default_in_data() -> Channel {
    copy().default
}

/// Product maturity (`alpha` until general availability), independent of the channel.
pub fn stage() -> &'static str {
    &copy().stage
}

/// The sentence every surface shows while [`stage`] is `alpha`.
pub fn stage_note() -> &'static str {
    &copy().stage_note
}

/// A release tag as a version: one optional leading `v`, then strict SemVer. Moving tags such as
/// `nightly` and anything that is not SemVer give `None`.
pub fn parse_tag(tag: &str) -> Option<Version> {
    Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()
}

/// SemVer precedence. `Version`'s own `Ord` also compares build metadata; the spec says not to.
pub fn precedence(a: &Version, b: &Version) -> Ordering {
    a.cmp_precedence(b)
}

/// What a client does with an offered version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// The offer is newer: update (automatic checks may say so).
    Upgrade,
    /// Same precedence as what runs.
    Current,
    /// The offer is *older* than what runs (for example after switching to a more conservative
    /// channel). Never installed on its own; only an explicit downgrade does that.
    Ahead,
}

pub fn decide(installed: &Version, offered: &Version) -> Decision {
    match precedence(offered, installed) {
        Ordering::Greater => Decision::Upgrade,
        Ordering::Equal => Decision::Current,
        Ordering::Less => Decision::Ahead,
    }
}

/// The tag `channel` should follow: the highest precedence among the tags it accepts. `tags`
/// are what `releases.atom` lists (any order, junk allowed); `stable_tag` is the target of the
/// `releases/latest` redirect and is always a candidate, so a channel is never worse than
/// Stable even when the 10-entry Atom window no longer reaches it.
pub fn resolve<'a>(
    channel: Channel,
    tags: impl IntoIterator<Item = &'a str>,
    stable_tag: Option<&'a str>,
) -> Option<String> {
    let all = tags.into_iter().chain(stable_tag);
    let best = all
        .filter_map(|t| parse_tag(t).map(|v| (v, t)))
        .filter(|(v, _)| channel.accepts(v))
        .max_by(|a, b| precedence(&a.0, &b.0));
    // Stable never offers a prerelease even if the redirect says so.
    best.map(|(_, t)| t.to_string())
}

/// What picking a channel would do for the install that runs `installed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// Nothing to do (same version, or nothing known for this channel).
    None,
    /// A newer build is offered.
    Update,
    /// Only an older build is offered: installed only on an explicit request.
    Downgrade,
}

/// One channel as the CLI and the desktop show it: the copy, whether it is the one followed, and
/// what it offers right now. Serialized as-is to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct ChannelView {
    pub channel: Channel,
    pub label: String,
    pub promise: String,
    pub risk: String,
    pub risk_label: String,
    pub cadence: String,
    pub audience: String,
    pub switch_to: String,
    pub alpha_era_note: Option<String>,
    pub selected: bool,
    /// The tag this channel resolves to (`None` when unknown or nothing is published).
    pub tag: Option<String>,
    /// That tag's version (no leading `v`).
    pub offered: Option<String>,
    pub action: Action,
}

pub fn view(
    channel: Channel,
    selected: Channel,
    installed: &Version,
    tag: Option<String>,
) -> ChannelView {
    let info = channel.info();
    let offered = tag.as_deref().and_then(parse_tag);
    let action = match offered.as_ref().map(|o| decide(installed, o)) {
        Some(Decision::Upgrade) => Action::Update,
        Some(Decision::Ahead) => Action::Downgrade,
        Some(Decision::Current) | None => Action::None,
    };
    ChannelView {
        channel,
        label: info.label.clone(),
        promise: info.promise.clone(),
        risk: info.risk.clone(),
        risk_label: info.risk_label.clone(),
        cadence: info.cadence.clone(),
        audience: info.audience.clone(),
        switch_to: info.switch_to.clone(),
        alpha_era_note: info.alpha_era_note.clone(),
        selected: channel == selected,
        offered: offered.map(|v| v.to_string()),
        tag,
        action,
    }
}

// ── Persistence (one word in ~/.atlas/update-channel, shared by the CLI and the app) ──────────

pub fn file() -> PathBuf {
    crate::paths::atlas_home().join("update-channel")
}

/// The channel stored in `path`; absent or unreadable content is the default ([`Channel::default`]).
pub fn read_from(path: &Path) -> Channel {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_default()
}

/// The temp file a save by process `pid` writes before the rename: beside `path`, named after the
/// whole file name (so an extension is never replaced) and the process (the CLI and the app save
/// the same file and must not share a temp).
fn tmp_path(path: &Path, pid: u32) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{pid}.tmp"));
    path.with_file_name(name)
}

pub fn write_to(path: &Path, channel: Channel) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Written beside the target then renamed: a reader never sees half a word.
    let tmp = tmp_path(path, std::process::id());
    let saved =
        std::fs::write(&tmp, format!("{channel}\n")).and_then(|()| std::fs::rename(&tmp, path));
    if saved.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    saved
}

/// `override_` (the [`ENV`] value) when valid, else `stored`.
pub fn effective(override_: Option<&str>, stored: Channel) -> Channel {
    override_.and_then(|s| s.parse().ok()).unwrap_or(stored)
}

/// The channel this machine follows: [`ENV`], else the shared file, else [`Channel::default`].
pub fn current() -> Channel {
    effective(std::env::var(ENV).ok().as_deref(), read_from(&file()))
}

pub fn set(channel: Channel) -> std::io::Result<()> {
    write_to(&file(), channel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn names_round_trip_and_unknown_names_are_rejected() {
        for c in Channel::ALL {
            assert_eq!(c.as_str().parse::<Channel>().unwrap(), c);
            assert_eq!(c.to_string(), c.as_str());
        }
        assert_eq!(" Beta\n".parse::<Channel>().unwrap(), Channel::Beta);
        for bad in ["", "nightly", "canary", "latest", "next", "stable2"] {
            assert!(bad.parse::<Channel>().is_err(), "{bad}");
        }
        // Pre-GA: the channel nobody chose is Alpha. Flipping it back to Stable is this one line
        // plus the `#[default]` on `Channel` (see `docs/versioning.md`).
        assert_eq!(Channel::default(), Channel::Alpha);
    }

    // Incident guard: `release.yml` decides GitHub's prerelease/Latest flags from channels.json, not
    // from this enum. If the two drifted, "Latest" on GitHub would point at the wrong channel.
    #[test]
    fn the_default_recorded_in_channels_json_is_the_default_attribute() {
        assert_eq!(default_in_data(), Channel::default());
    }

    #[test]
    fn each_channel_accepts_exactly_its_prerelease_labels() {
        let table = [
            ("1.4.0", [true, true, true]),
            ("1.4.0-rc.1", [false, true, true]),
            ("1.4.0-beta.2", [false, true, true]),
            ("1.4.0-alpha.3", [false, false, true]),
            ("1.4.0-alpha", [false, false, true]),
            ("1.4.0-nightly.20261012", [false, false, false]),
            ("1.4.0-foo", [false, false, false]),
            ("1.4.0-Alpha.1", [false, false, false]),
        ];
        for (ver, want) in table {
            let got = Channel::ALL.map(|c| c.accepts(&v(ver)));
            assert_eq!(got, want, "{ver}");
        }
    }

    #[test]
    fn semver_precedence_orders_the_spec_chain_and_numeric_counters() {
        let chain = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
        ];
        for w in chain.windows(2) {
            assert_eq!(
                precedence(&v(w[0]), &v(w[1])),
                std::cmp::Ordering::Less,
                "{w:?}"
            );
        }
        assert_eq!(
            precedence(&v("1.4.0-alpha.10"), &v("1.4.0-alpha.9")),
            std::cmp::Ordering::Greater
        );
        // The documented footgun: a counter glued to the label compares as text.
        assert_eq!(
            precedence(&v("1.4.0-alpha10"), &v("1.4.0-alpha9")),
            std::cmp::Ordering::Less
        );
        // A prerelease of the next minor is newer than the previous stable.
        assert_eq!(
            precedence(&v("1.4.0-alpha.1"), &v("1.3.9")),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn build_metadata_never_changes_precedence() {
        // `Version`'s own `Ord` breaks the tie on build metadata; the spec says equal.
        assert_ne!(v("1.0.0+a").cmp(&v("1.0.0+b")), std::cmp::Ordering::Equal);
        assert_eq!(
            precedence(&v("1.0.0+a"), &v("1.0.0+b")),
            std::cmp::Ordering::Equal
        );
        assert_eq!(decide(&v("1.0.0+a"), &v("1.0.0+b")), Decision::Current);
    }

    #[test]
    fn parse_tag_strips_one_v_and_rejects_everything_else() {
        assert_eq!(parse_tag("v1.4.0-beta.1"), Some(v("1.4.0-beta.1")));
        assert_eq!(parse_tag("1.4.0"), Some(v("1.4.0")));
        for bad in [
            "nightly",
            "collab-staging",
            "vv1.0.0",
            "",
            "v",
            "v1.4",
            "v1.4.0-alpha.01",
            "create-vite@9.2.1",
        ] {
            assert_eq!(parse_tag(bad), None, "{bad}");
        }
    }

    #[test]
    fn maturity_of_a_build_follows_its_version_label() {
        assert_eq!(Channel::of_version(&v("1.3.0")), Some(Channel::Stable));
        assert_eq!(Channel::of_version(&v("1.4.0-rc.1")), Some(Channel::Beta));
        assert_eq!(Channel::of_version(&v("1.4.0-beta.1")), Some(Channel::Beta));
        assert_eq!(
            Channel::of_version(&v("1.4.0-alpha.1")),
            Some(Channel::Alpha)
        );
        assert_eq!(Channel::of_version(&v("1.4.0-foo")), None);
    }

    #[test]
    fn decisions_never_downgrade_on_their_own() {
        assert_eq!(decide(&v("1.3.0"), &v("1.4.0-alpha.1")), Decision::Upgrade);
        assert_eq!(decide(&v("1.4.0-beta.1"), &v("1.4.0")), Decision::Upgrade);
        assert_eq!(
            decide(&v("1.4.0-alpha.9"), &v("1.4.0-alpha.10")),
            Decision::Upgrade
        );
        assert_eq!(decide(&v("1.3.0"), &v("1.3.0")), Decision::Current);
        // alpha user switching to stable: stable is older, so it is "ahead", not an update
        assert_eq!(decide(&v("1.5.0-alpha.3"), &v("1.4.0")), Decision::Ahead);
        assert_eq!(decide(&v("1.4.0"), &v("1.4.0-rc.1")), Decision::Ahead);
    }

    #[derive(Deserialize)]
    struct Vectors {
        cases: Vec<Case>,
    }
    #[derive(Deserialize)]
    struct Case {
        name: String,
        tags: Vec<String>,
        stable_tag: Option<String>,
        expect: std::collections::HashMap<String, Option<String>>,
    }

    /// The same vectors drive `site/functions/_lib/channels.test.ts`: both resolvers must agree.
    #[test]
    fn resolution_matches_the_shared_vectors() {
        let raw = include_str!("../../../testdata/channels/resolve.json");
        let vectors: Vectors = serde_json::from_str(raw).unwrap();
        assert!(vectors.cases.len() >= 10);
        for case in vectors.cases {
            for c in Channel::ALL {
                let got = resolve(
                    c,
                    case.tags.iter().map(String::as_str),
                    case.stable_tag.as_deref(),
                );
                assert_eq!(
                    got.as_deref(),
                    case.expect[c.as_str()].as_deref(),
                    "{} [{c}]",
                    case.name
                );
            }
        }
    }

    #[test]
    fn the_channel_file_is_shared_and_tolerates_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("update-channel");
        assert_eq!(read_from(&file), Channel::Alpha, "absent = default");
        // An explicit Stable is a choice, not "absent": it survives the default being Alpha.
        write_to(&file, Channel::Stable).unwrap();
        assert_eq!(read_from(&file), Channel::Stable, "explicit stable is kept");
        write_to(&file, Channel::Alpha).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "alpha\n");
        assert_eq!(read_from(&file), Channel::Alpha);
        std::fs::write(&file, "nightly\n").unwrap();
        assert_eq!(read_from(&file), Channel::Alpha, "garbage = default");
        std::fs::write(&file, "").unwrap();
        assert_eq!(read_from(&file), Channel::Alpha, "empty = default");
        write_to(&file, Channel::Beta).unwrap();
        assert_eq!(read_from(&file), Channel::Beta);
    }

    #[test]
    fn the_env_override_wins_over_the_file_and_an_invalid_one_is_ignored() {
        assert_eq!(effective(Some("beta"), Channel::Alpha), Channel::Beta);
        assert_eq!(effective(Some("bogus"), Channel::Alpha), Channel::Alpha);
        assert_eq!(effective(None, Channel::Beta), Channel::Beta);
    }

    /// Pre-GA the default is Alpha. A 1.4.0 Stable user who never picked a channel has no file:
    /// they must be offered the newest alpha as an upgrade (never a downgrade), and Stable stays
    /// selectable for whoever chose it.
    #[test]
    fn a_stable_install_without_a_channel_file_is_offered_the_newest_alpha() {
        let dir = tempfile::tempdir().unwrap();
        let followed = effective(None, read_from(&dir.path().join("update-channel")));
        assert_eq!(followed, Channel::Alpha);
        let tags = ["v1.4.0", "v1.5.0-alpha.2", "v1.5.0-alpha.1", "v1.4.0-rc.1"];
        let tag = resolve(followed, tags, Some("v1.4.0")).unwrap();
        assert_eq!(tag, "v1.5.0-alpha.2");
        assert_eq!(
            decide(&v("1.4.0"), &parse_tag(&tag).unwrap()),
            Decision::Upgrade
        );
        // Nothing newer than Stable exists yet: Alpha is never worse than Stable, so it offers Stable.
        assert_eq!(
            resolve(followed, ["v1.4.0"], Some("v1.4.0")).as_deref(),
            Some("v1.4.0")
        );
        // Someone who chose Stable still gets only releases.
        assert_eq!(
            resolve(Channel::Stable, tags, Some("v1.4.0")).as_deref(),
            Some("v1.4.0")
        );
    }

    #[test]
    fn a_channel_view_says_what_each_channel_would_do_for_this_install() {
        let installed = v("1.5.0-alpha.3");
        let offers = [
            (Channel::Stable, Some("v1.4.0")),
            (Channel::Beta, Some("v1.4.0-rc.1")),
            (Channel::Alpha, Some("v1.5.0-alpha.4")),
        ];
        let views: Vec<_> = offers
            .iter()
            .map(|(c, t)| view(*c, Channel::Alpha, &installed, t.map(str::to_string)))
            .collect();
        assert_eq!(
            views.iter().map(|x| x.action).collect::<Vec<_>>(),
            [Action::Downgrade, Action::Downgrade, Action::Update]
        );
        assert_eq!(
            views.iter().map(|x| x.selected).collect::<Vec<_>>(),
            [false, false, true]
        );
        assert_eq!(
            views[0].offered.as_deref(),
            Some("1.4.0"),
            "version without the v"
        );
        assert_eq!(views[0].tag.as_deref(), Some("v1.4.0"));
        // No release known for a channel (offline, or nothing published): no action.
        let none = view(Channel::Beta, Channel::Stable, &installed, None);
        assert_eq!((none.action, none.offered), (Action::None, None));
        // Same version: nothing to do.
        let same = view(
            Channel::Alpha,
            Channel::Alpha,
            &installed,
            Some("v1.5.0-alpha.3".into()),
        );
        assert_eq!(same.action, Action::None);
        // A build of this install's own maturity, for the "Installed" badge.
        assert_eq!(Channel::of_version(&installed), Some(Channel::Alpha));
    }

    #[test]
    fn a_view_serializes_with_the_keys_the_frontend_reads() {
        let x = view(
            Channel::Beta,
            Channel::Beta,
            &v("1.3.0"),
            Some("v1.4.0-beta.1".into()),
        );
        let j = serde_json::to_value(&x).unwrap();
        for key in [
            "channel",
            "label",
            "promise",
            "risk",
            "risk_label",
            "cadence",
            "audience",
            "switch_to",
            "alpha_era_note",
            "selected",
            "tag",
            "offered",
            "action",
        ] {
            assert!(j.get(key).is_some(), "missing {key}: {j}");
        }
        assert_eq!(j["action"], "update");
        assert_eq!(j["channel"], "beta");
        assert_eq!(j["risk"], "medium");
    }

    #[test]
    fn copy_exists_for_every_channel_and_matches_the_policy_doc() {
        let doc = include_str!("../../../docs/versioning.md");
        for c in Channel::ALL {
            let i = c.info();
            for s in [&i.promise, &i.risk_label, &i.cadence, &i.audience] {
                assert!(!s.is_empty(), "{c}");
            }
            assert!(
                doc.contains(&i.promise),
                "docs/versioning.md must contain the {c} promise verbatim: {}",
                i.promise
            );
        }
        assert_eq!(
            Channel::ALL.map(|c| c.info().id.as_str()),
            ["stable", "beta", "alpha"]
        );
    }

    /// Incident class: the CLI and the app both save the channel through one shared
    /// `update-channel.tmp`, so two saves at once could rename a half-written file into place.
    #[test]
    fn two_writers_never_share_a_temp_file() {
        let p = Path::new("/h/.atlas/update-channel");
        let (a, b) = (tmp_path(p, 1), tmp_path(p, 2));
        assert_ne!(a, b);
        assert_eq!(
            a.parent(),
            p.parent(),
            "beside the target, so the rename is atomic"
        );
        assert_ne!(a, p);
        // An extension is never replaced: `x.json` and `x.txt` get different temps.
        assert_ne!(
            tmp_path(Path::new("/h/x.json"), 1),
            tmp_path(Path::new("/h/x.txt"), 1)
        );
    }

    #[test]
    fn a_failed_save_leaves_no_temp_file_and_the_old_choice() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("update-channel");
        write_to(&file, Channel::Beta).unwrap();
        // The target cannot be replaced (a non-empty directory sits where a file would go).
        let blocked = dir.path().join("blocked");
        std::fs::create_dir_all(blocked.join("inside")).unwrap();
        assert!(write_to(&blocked, Channel::Stable).is_err());
        let mut names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            ["blocked", "update-channel"],
            "no temp left: {names:?}"
        );
        assert_eq!(
            read_from(&file),
            Channel::Beta,
            "an unrelated choice is untouched"
        );
    }

    /// What was saved is read back exactly, and a save over an existing choice replaces it whole.
    #[test]
    fn saving_replaces_the_previous_choice_whole() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("deep/er/update-channel");
        for c in Channel::ALL {
            write_to(&file, c).unwrap();
            assert_eq!(read_from(&file), c);
            assert_eq!(std::fs::read_to_string(&file).unwrap(), format!("{c}\n"));
        }
    }
}
