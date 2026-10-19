//! Public GitHub Releases client shared by `atlas update`, `atlas desktop` and the desktop app.
//!
//! Uses no REST API and no token: the unauthenticated API allows 60 requests an hour per IP,
//! which a shared network (or a few update checks) exhausts. Instead, through `curl`
//! (present on every supported OS):
//! - the `releases/latest` redirect on github.com is a hint only: while the default channel is
//!   Alpha, `release.yml` publishes alphas as ordinary releases, so the redirect can be an alpha.
//!   Stable is therefore the newest tag without a prerelease label, see [`stable_tag`];
//! - every channel reads `releases.atom` (public, 10 newest published releases, no
//!   prerelease flag, so the channel is inferred from the SemVer tag) and picks the newest
//!   version the channel accepts, see [`crate::channel::resolve`];
//! - release notes are the `## [X.Y.Z] - date` sections of `CHANGELOG.md` at that tag
//!   (the release body is the same section, extracted by `release.yml`);
//! - assets come from the public `releases/download` URLs.

use std::path::Path;
use std::process::{Command, Stdio};

use thiserror::Error;

use crate::channel::{self, Channel};

pub const MISSING: &str = "curl not found — install it and try again";

/// Where curl discards a body (`-o`): Windows has no `/dev/null`.
const NULL_DEVICE: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

#[derive(Debug, Error)]
pub enum GithubError {
    #[error("{MISSING}")]
    Missing,
    #[error("GitHub request failed: {0}")]
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub url: String,
    /// `YYYY-MM-DD` from the CHANGELOG heading ("" when it has none).
    pub published_at: String,
    pub body: String,
}

impl ReleaseInfo {
    /// The tag without its leading `v`.
    pub fn version(&self) -> &str {
        self.tag_name.trim_start_matches('v')
    }

    /// The release notes ("" when the release has none).
    pub fn notes(&self) -> &str {
        &self.body
    }
}

/// Base `curl` invocation: silent, fail on HTTP errors, bounded time.
fn curl() -> Command {
    let mut c = Command::new("curl");
    c.args(["-fsS", "--max-time", "120", "-H", "User-Agent: atlas-cli"])
        .stdin(Stdio::null());
    c
}

/// Run `cmd` and return its stdout; a spawn failure means curl is missing.
fn run(mut cmd: Command) -> Result<Vec<u8>, GithubError> {
    let out = cmd.output().map_err(|_| GithubError::Missing)?;
    if out.status.success() {
        return Ok(out.stdout);
    }
    Err(GithubError::Failed(
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    ))
}

/// Tag the redirect `github.com/<repo>/releases/latest` → `.../releases/tag/<tag>` points at.
/// Pre-GA that is the newest build of the default channel (an alpha), not necessarily a Stable:
/// only [`stable_tag`] turns it into one.
fn latest_tag(repo: &str) -> Result<String, GithubError> {
    let mut c = curl();
    c.args(["-o", NULL_DEVICE, "-w", "%{redirect_url}"])
        .arg(format!("https://github.com/{repo}/releases/latest"));
    let url = String::from_utf8_lossy(&run(c)?).trim().to_string();
    url.rsplit_once("/releases/tag/")
        .map(|(_, tag)| tag.to_string())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| GithubError::Failed(format!("{repo} has no published release")))
}

/// `CHANGELOG.md` of `repo` at `tag`.
fn changelog(repo: &str, tag: &str) -> Result<String, GithubError> {
    let mut c = curl();
    c.arg(format!(
        "https://raw.githubusercontent.com/{repo}/{tag}/CHANGELOG.md"
    ));
    Ok(String::from_utf8_lossy(&run(c)?).into_owned())
}

/// Released sections of a Keep a Changelog file, newest first: `## [X.Y.Z] - date` headings
/// (`[Unreleased]` and other non-version headings are skipped) with their bodies.
fn sections(repo: &str, md: &str) -> Vec<ReleaseInfo> {
    let mut out: Vec<ReleaseInfo> = Vec::new();
    let mut current: Option<ReleaseInfo> = None;
    for line in md.lines() {
        if let Some(head) = line.strip_prefix("## ") {
            out.extend(current.take());
            let Some((version, rest)) = head
                .strip_prefix('[')
                .and_then(|h| h.split_once(']'))
                .filter(|(v, _)| v.starts_with(|c: char| c.is_ascii_digit()))
            else {
                continue;
            };
            let tag = format!("v{version}");
            current = Some(ReleaseInfo {
                url: format!("https://github.com/{repo}/releases/tag/{tag}"),
                tag_name: tag,
                published_at: rest
                    .trim_start_matches([' ', '-', '—', '–'])
                    .trim()
                    .to_string(),
                body: String::new(),
            });
        } else if let Some(r) = current.as_mut() {
            r.body.push_str(line);
            r.body.push('\n');
        }
    }
    out.extend(current);
    for r in &mut out {
        r.body = r.body.trim().to_string();
    }
    out
}

/// Entry tags of a `releases.atom` feed, in feed order (newest published first), percent-decoded.
/// Moving tags (`nightly`) and non-release tags are kept: judging them is [`channel::resolve`]'s job.
pub fn atom_tags(xml: &str) -> Vec<String> {
    const MARK: &str = "/releases/tag/";
    xml.split("<entry>")
        .skip(1)
        .filter_map(|entry| {
            let entry = entry.split("</entry>").next()?;
            let link = entry
                .split("<link ")
                .find(|l| l.contains("rel=\"alternate\""))?;
            let href = link.split("href=\"").nth(1)?.split('"').next()?;
            let (_, tag) = href.split_once(MARK)?;
            Some(percent_decode(tag)).filter(|t| !t.is_empty())
        })
        .collect()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| (c as char).to_digit(16);
        if b[i] == b'%'
            && let (Some(h), Some(l)) = (
                b.get(i + 1).and_then(|c| hex(*c)),
                b.get(i + 2).and_then(|c| hex(*c)),
            )
        {
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn atom(repo: &str) -> Result<String, GithubError> {
    atom_page(repo, None)
}

/// A page of the feed: the 10 newest releases, or the 10 that follow `after` (`?after=<tag>`).
fn atom_page(repo: &str, after: Option<&str>) -> Result<String, GithubError> {
    let mut c = curl();
    c.arg(match after {
        Some(tag) => format!("https://github.com/{repo}/releases.atom?after={tag}"),
        None => format!("https://github.com/{repo}/releases.atom"),
    });
    Ok(String::from_utf8_lossy(&run(c)?).into_owned())
}

/// Feed pages read at most while looking for a Stable behind a run of prereleases (10 releases each).
const STABLE_PAGES: usize = 5;

/// The newest Stable (no prerelease label) among `tags` and the `latest` redirect. `latest` is
/// only a candidate: while Alpha is the default channel it is an alpha, which Stable never
/// follows. This is [`channel::resolve`] for Stable, kept as the one place that says so.
fn stable_from<'a>(
    tags: impl IntoIterator<Item = &'a str>,
    latest: Option<&'a str>,
) -> Option<String> {
    channel::resolve(Channel::Stable, tags, latest)
}

/// The tag Stable follows: [`stable_from`] over the feed and the redirect, reading older feed
/// pages (`?after=` the oldest tag seen) while a run of prereleases hides every Stable. The
/// first page is `first` (already fetched by the caller), so the common case costs nothing.
fn stable_tag(repo: &str, first: &[String], latest: Option<&str>) -> Option<String> {
    let mut seen: Vec<String> = first.to_vec();
    let mut page = first.len();
    for _ in 0..STABLE_PAGES {
        let found = stable_from(seen.iter().map(String::as_str), latest);
        // A feed page is 10 long; a shorter one is the end of the history.
        if found.is_some() || page < 10 {
            return found;
        }
        let oldest = seen.last()?.clone();
        let older = atom_tags(&atom_page(repo, Some(&oldest)).ok()?);
        page = older.len();
        if older.is_empty() {
            return None;
        }
        seen.extend(older);
    }
    stable_from(seen.iter().map(String::as_str), latest)
}

/// `HEAD` on the release's `manifest.json`: the tag is pushed before the release is built and
/// published, so a tag alone does not mean there is something to install.
fn is_published(repo: &str, tag: &str) -> bool {
    content_length(&format!(
        "https://github.com/{repo}/releases/download/{tag}/manifest.json"
    ))
    .is_some()
}

/// The newest tag in `candidates` that `channel` accepts and that is actually published (the tag
/// is pushed before the release exists, so walk down past unpublished ones); `stable` when none.
fn newest_published(
    repo: &str,
    channel: Channel,
    mut candidates: Vec<&str>,
    stable: Option<&str>,
) -> Option<String> {
    while let Some(tag) = channel::resolve(channel, candidates.iter().copied(), None) {
        if Some(tag.as_str()) == stable || is_published(repo, &tag) {
            return Some(tag);
        }
        candidates.retain(|c| *c != tag);
    }
    stable.map(str::to_string)
}

/// The tag `channel` follows, without fetching the changelog (what update checks need). Every
/// channel reads the Atom feed; if it cannot be read, the channel falls back to the Stable the
/// `releases/latest` redirect names (when it names one).
pub fn channel_tag(repo: &str, channel: Channel) -> Result<String, GithubError> {
    let latest = latest_tag(repo);
    let feed = atom(repo).map(|x| atom_tags(&x)).unwrap_or_default();
    let stable = stable_tag(repo, &feed, latest.as_deref().ok());
    let found = if channel == Channel::Stable {
        stable.clone()
    } else {
        let mut candidates: Vec<&str> = feed.iter().map(String::as_str).collect();
        candidates.extend(stable.as_deref());
        newest_published(repo, channel, candidates, stable.as_deref())
    };
    found.ok_or_else(|| match latest {
        Err(e) => e,
        Ok(_) => GithubError::Failed(format!("{repo} has no Stable release")),
    })
}

/// The section of `tag` in `sections` as a [`ReleaseInfo`].
fn release_from_sections(
    tag: &str,
    sections: Vec<ReleaseInfo>,
) -> Result<ReleaseInfo, GithubError> {
    let version = tag.trim_start_matches('v');
    sections
        .into_iter()
        .find(|r| r.version() == version)
        .ok_or_else(|| GithubError::Failed(format!("CHANGELOG.md at {tag} has no [{version}]")))
}

/// Every published release the channel's newest tag knows about, newest first: the sections of
/// `CHANGELOG.md` at that tag, which carries the whole history (older tags may not exist here).
pub fn releases(repo: &str, channel: Channel) -> Result<Vec<ReleaseInfo>, GithubError> {
    Ok(sections(
        repo,
        &changelog(repo, &channel_tag(repo, channel)?)?,
    ))
}

/// Newest release `channel` offers: the latest Stable for `Stable`, otherwise the newest
/// published version the channel accepts (never older than the latest Stable).
pub fn latest_release(repo: &str, channel: Channel) -> Result<ReleaseInfo, GithubError> {
    let tag = channel_tag(repo, channel)?;
    release_from_sections(&tag, sections(repo, &changelog(repo, &tag)?))
}

/// Releases in `(installed, latest]`, newest first (`installed == latest` keeps just that one:
/// "what changed in the version I run"). A prerelease section is dropped when its own release is
/// also in range: the stable section is the complete summary, its alphas and betas are history.
pub fn in_range(all: Vec<ReleaseInfo>, installed: &str, latest: &str) -> Vec<ReleaseInfo> {
    let (Some(from), Some(to)) = (channel::parse_tag(installed), channel::parse_tag(latest)) else {
        return Vec::new();
    };
    let ord = channel::precedence;
    let in_window: Vec<(semver::Version, ReleaseInfo)> = all
        .into_iter()
        .filter_map(|r| channel::parse_tag(&r.tag_name).map(|v| (v, r)))
        .filter(|(v, _)| {
            ord(v, &to).is_le()
                && (ord(v, &from).is_gt() || ord(&from, &to).is_eq() && ord(v, &to).is_eq())
        })
        .collect();
    let released: Vec<semver::Version> = in_window
        .iter()
        .filter(|(v, _)| v.pre.is_empty())
        .map(|(v, _)| v.clone())
        .collect();
    in_window
        .into_iter()
        .filter(|(v, _)| {
            v.pre.is_empty()
                || !released
                    .iter()
                    .any(|r| (r.major, r.minor, r.patch) == (v.major, v.minor, v.patch))
        })
        .map(|(_, r)| r)
        .collect()
}

/// Every channel as the CLI and the desktop show it for the install running `installed`, with
/// what each one offers now. Network failures leave that channel's offer empty (the copy and
/// the selection still render offline). Resolves the three channels concurrently.
pub fn channel_views(
    repo: &str,
    installed: &semver::Version,
    selected: Channel,
) -> Vec<channel::ChannelView> {
    // One feed fetch and one Stable lookup serve all three channels.
    let latest = latest_tag(repo).ok();
    let feed = atom(repo).map(|x| atom_tags(&x)).unwrap_or_default();
    let stable = stable_tag(repo, &feed, latest.as_deref());
    let tags = |c: Channel| -> Option<String> {
        if c == Channel::Stable {
            return stable.clone();
        }
        let mut cand: Vec<&str> = feed.iter().map(String::as_str).collect();
        cand.extend(stable.as_deref());
        newest_published(repo, c, cand, stable.as_deref())
    };
    Channel::ALL
        .into_iter()
        .map(|c| channel::view(c, selected, installed, tags(c)))
        .collect()
}

/// Download the release asset `name` of `tag` into `dest`.
pub fn download_asset(repo: &str, tag: &str, name: &str, dest: &Path) -> Result<(), GithubError> {
    download_asset_progress(repo, tag, name, dest, |_, _| {})
}

/// Like [`download_asset`], reporting `(bytes_done, bytes_total)` (total 0 while unknown)
/// as the file grows. curl writes straight to `dest`; its size is polled from this thread
/// until curl exits, so no output parsing and no extra copy are involved.
pub fn download_asset_progress(
    repo: &str,
    tag: &str,
    name: &str,
    dest: &Path,
    on_progress: impl FnMut(u64, u64),
) -> Result<(), GithubError> {
    let url = format!("https://github.com/{repo}/releases/download/{tag}/{name}");
    let total = content_length(&url).unwrap_or(0);
    let mut c = curl();
    c.arg("-L").arg("-o").arg(dest).arg(&url);
    watch_download(
        c,
        dest,
        total,
        std::time::Duration::from_millis(120),
        on_progress,
    )
}

/// Run `cmd` (which writes `dest`) to completion, calling `on_progress(size, total)` every
/// `poll` while the file grows and once more at the end with the final size. A failed exit
/// is an error carrying the command's stderr.
fn watch_download(
    mut cmd: Command,
    dest: &Path,
    total: u64,
    poll: std::time::Duration,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<(), GithubError> {
    cmd.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|_| GithubError::Missing)?;
    let size = || std::fs::metadata(dest).map_or(0, |m| m.len());
    let mut last = u64::MAX;
    loop {
        if let Some(st) = child
            .try_wait()
            .map_err(|e| GithubError::Failed(e.to_string()))?
        {
            if !st.success() {
                let mut err = String::new();
                if let Some(mut s) = child.stderr.take() {
                    let _ = std::io::Read::read_to_string(&mut s, &mut err);
                }
                return Err(GithubError::Failed(err.trim().to_string()));
            }
            break;
        }
        let done = size();
        if done != last {
            last = done;
            on_progress(done, total);
        }
        std::thread::sleep(poll);
    }
    let done = size();
    on_progress(done, done.max(total));
    Ok(())
}

/// `Content-Length` of release asset `name` of `tag`; `None` when unknown or offline.
pub fn asset_size(repo: &str, tag: &str, name: &str) -> Option<u64> {
    content_length(&format!(
        "https://github.com/{repo}/releases/download/{tag}/{name}"
    ))
}

/// `Content-Length` of the final response of `url` (follows redirects); `None` when unknown.
fn content_length(url: &str) -> Option<u64> {
    let mut c = curl();
    c.args([
        "-L",
        "-I",
        "-o",
        NULL_DEVICE,
        "-w",
        "%header{content-length}",
    ])
    .arg(url);
    let out = run(c).ok()?;
    String::from_utf8_lossy(&out).trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MD: &str = "# Changelog\n\nintro\n\n## [Unreleased]\n\n- wip\n\n\
        ## [1.1.0] — 2026-10-12\n\nBig release.\n\n### Added\n- a\n\n\
        ## [1.0.1] - 2026-10-11\n\n- fix\n";

    #[test]
    fn sections_skip_unreleased_and_keep_bodies_newest_first() {
        let s = sections("o/r", MD);
        assert_eq!(
            s.iter().map(|r| r.tag_name.as_str()).collect::<Vec<_>>(),
            ["v1.1.0", "v1.0.1"]
        );
        assert_eq!(s[0].notes(), "Big release.\n\n### Added\n- a");
        assert_eq!(s[0].published_at, "2026-10-12");
        assert_eq!(s[0].url, "https://github.com/o/r/releases/tag/v1.1.0");
        assert_eq!((s[1].version(), s[1].notes()), ("1.0.1", "- fix"));
    }

    #[test]
    fn heading_without_date_or_sections_still_parses() {
        let s = sections("o/r", "## [2.0.0]\n");
        assert_eq!(
            (s[0].version(), s[0].published_at.as_str(), s[0].notes()),
            ("2.0.0", "", "")
        );
        assert!(sections("o/r", "no headings").is_empty());
    }

    fn infos(vs: &[&str]) -> Vec<ReleaseInfo> {
        vs.iter()
            .map(|v| ReleaseInfo {
                tag_name: format!("v{v}"),
                url: String::new(),
                published_at: String::new(),
                body: String::new(),
            })
            .collect()
    }
    fn versions(r: &[ReleaseInfo]) -> Vec<&str> {
        r.iter().map(ReleaseInfo::version).collect()
    }

    #[test]
    fn whats_new_range_keeps_only_the_range_newest_first() {
        let all = || infos(&["1.0.2", "1.0.1", "1.0.0", "0.9.0"]);
        assert_eq!(
            versions(&in_range(all(), "1.0.0", "1.0.2")),
            ["1.0.2", "1.0.1"]
        );
        assert_eq!(versions(&in_range(all(), "1.0.0", "1.0.1")), ["1.0.1"]);
        assert_eq!(
            versions(&in_range(all(), "1.0.2", "1.0.2")),
            ["1.0.2"],
            "what changed in the version I run"
        );
        assert!(in_range(all(), "garbage", "1.0.2").is_empty());
    }

    #[test]
    fn whats_new_hides_prerelease_sections_whose_stable_is_in_range() {
        // Stable section = complete summary since the last stable; its alphas/betas are noise.
        let all = infos(&[
            "1.5.0-alpha.1",
            "1.4.0",
            "1.4.0-rc.1",
            "1.4.0-beta.2",
            "1.4.0-beta.1",
            "1.4.0-alpha.2",
            "1.4.0-alpha.1",
            "1.3.0",
        ]);
        assert_eq!(
            versions(&in_range(all.clone(), "1.3.0", "1.4.0")),
            ["1.4.0"]
        );
        // An alpha-channel user sees the stable summary plus the alpha that has no stable yet.
        assert_eq!(
            versions(&in_range(all.clone(), "1.3.0", "1.5.0-alpha.1")),
            ["1.5.0-alpha.1", "1.4.0"]
        );
        // A beta user going to the next beta sees incremental sections: no stable exists yet.
        assert_eq!(
            versions(&in_range(all.clone(), "1.4.0-beta.1", "1.4.0-rc.1")),
            ["1.4.0-rc.1", "1.4.0-beta.2"]
        );
        // ...and when the stable arrives, only the stable summary.
        assert_eq!(versions(&in_range(all, "1.4.0-beta.2", "1.4.0")), ["1.4.0"]);
    }

    #[test]
    fn whats_new_never_lists_a_downgrade_target_range() {
        let all = infos(&["1.4.0", "1.3.0"]);
        // installed 1.5.0-alpha.1 looking at stable 1.4.0: nothing is "new".
        assert!(in_range(all, "1.5.0-alpha.1", "1.4.0").is_empty());
    }

    /// Real `releases.atom` responses with the HTML bodies elided (testdata/channels).
    const ATLAS_ATOM: &str = include_str!("../../../testdata/channels/releases.atom");
    const ZED_ATOM: &str = include_str!("../../../testdata/channels/zed-releases.atom");

    #[test]
    fn atom_tags_come_from_entry_links_in_feed_order() {
        assert_eq!(
            atom_tags(ATLAS_ATOM),
            ["v1.3.0", "v1.2.1", "v1.1.2", "v1.1.1", "v1.1.0"]
        );
        // The feed-level <link rel="alternate" .../releases> is not an entry.
        assert!(!atom_tags(ATLAS_ATOM).iter().any(|t| t.is_empty()));
    }

    #[test]
    fn atom_keeps_prereleases_and_moving_tags_for_the_resolver_to_judge() {
        let tags = atom_tags(ZED_ATOM);
        assert_eq!(tags.len(), 10, "the feed's hard cap");
        assert!(tags.contains(&"v1.24.2-pre".to_string()));
        assert!(tags.contains(&"nightly".to_string()));
    }

    #[test]
    fn atom_tags_are_percent_decoded() {
        let xml = r#"<feed><entry><link rel="alternate" type="text/html" href="https://github.com/o/r/releases/tag/%40tauri-apps%2Fcli-v2.12.2"/></entry>
            <entry><link rel="alternate" href="https://github.com/o/r/releases/tag/v1.0.0-beta.1"/></entry></feed>"#;
        assert_eq!(atom_tags(xml), ["@tauri-apps/cli-v2.12.2", "v1.0.0-beta.1"]);
    }

    #[test]
    fn atom_garbage_and_empty_feeds_give_no_tags() {
        for xml in ["", "not xml", "<feed></feed>", "<entry>no link</entry>"] {
            assert!(atom_tags(xml).is_empty(), "{xml:?}");
        }
    }

    #[test]
    fn a_tag_with_a_non_semver_name_never_resolves_for_any_channel() {
        use crate::channel::{Channel, resolve};
        let tags = atom_tags(ZED_ATOM);
        // zed's `-pre` tags are real semver prereleases with an unknown label: offered nowhere.
        for c in Channel::ALL {
            assert_eq!(
                resolve(c, tags.iter().map(String::as_str), Some("v1.23.3")).as_deref(),
                Some("v1.23.3"),
                "{c}"
            );
        }
    }

    // Incident guard: while the default channel is alpha, `releases/latest` points at the newest alpha
    // (release.yml publishes the default channel's builds as non-prerelease). A client that chose Stable
    // must still land on the newest real Stable, so the redirect is only a hint, never trusted blindly.
    #[test]
    fn stable_ignores_a_latest_redirect_that_carries_a_prerelease_label() {
        let feed = [
            "v1.5.0-alpha.4",
            "v1.5.0-alpha.3",
            "v1.4.0",
            "v1.4.0-beta.1",
        ];
        assert_eq!(
            stable_from(feed, Some("v1.5.0-alpha.4")).as_deref(),
            Some("v1.4.0"),
            "redirect says alpha, feed has v1.4.0"
        );
        // Redirect is a real Stable (GA behaviour): it is still a candidate.
        assert_eq!(stable_from(feed, Some("v1.4.0")).as_deref(), Some("v1.4.0"));
        // The feed window (10 entries) no longer reaches the last Stable and the redirect is an alpha:
        // nothing trustworthy, so no Stable rather than an alpha.
        assert_eq!(
            stable_from(["v1.6.0-alpha.2", "v1.6.0-alpha.1"], Some("v1.6.0-alpha.2")),
            None
        );
        // Feed unreadable: a redirect that is a Stable still works.
        assert_eq!(
            stable_from(Vec::<&str>::new(), Some("v1.4.0")).as_deref(),
            Some("v1.4.0")
        );
        // A newer real Stable beats an older one in either source.
        assert_eq!(
            stable_from(["v1.4.1", "v1.4.0"], Some("v1.5.0-alpha.4")).as_deref(),
            Some("v1.4.1")
        );
    }

    #[test]
    fn a_stable_hotfix_published_after_an_alpha_is_still_the_stable() {
        // Feed order is publication order: the hotfix is first, but the rule is SemVer precedence.
        assert_eq!(
            stable_from(["v1.4.1", "v1.5.0-alpha.1", "v1.4.0"], Some("v1.4.1")).as_deref(),
            Some("v1.4.1")
        );
    }

    #[test]
    fn a_missing_changelog_section_is_an_error_naming_the_version() {
        let e = release_from_sections("v1.4.0-beta.1", sections("o/r", MD)).unwrap_err();
        assert!(e.to_string().contains("1.4.0-beta.1"), "{e}");
    }

    #[test]
    fn prerelease_sections_parse_with_their_dates() {
        let md =
            "## [1.4.0-alpha.2] — 2026-10-14\n\n- b\n\n## [1.4.0-alpha.1] - 2026-10-13\n\n- a\n";
        let r = release_from_sections("v1.4.0-alpha.2", sections("o/r", md)).unwrap();
        assert_eq!(
            (r.version(), r.published_at.as_str(), r.notes()),
            ("1.4.0-alpha.2", "2026-10-14", "- b")
        );
        assert_eq!(r.url, "https://github.com/o/r/releases/tag/v1.4.0-alpha.2");
    }

    #[cfg(unix)]
    fn sh(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.args(["-c", script]);
        c
    }

    // The fake download is a POSIX sh script.
    #[cfg(unix)]
    #[test]
    fn watch_reports_growth_then_the_final_size() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.bin");
        let script = format!(
            "printf aaaa > {d}; sleep 0.4; printf bbbb >> {d}; sleep 0.4; printf cccc >> {d}",
            d = dest.display()
        );
        let mut seen = Vec::new();
        watch_download(
            sh(&script),
            &dest,
            12,
            std::time::Duration::from_millis(20),
            |d, t| {
                seen.push((d, t));
            },
        )
        .unwrap();
        assert!(
            seen.windows(2).all(|w| w[0].0 <= w[1].0),
            "monotonic: {seen:?}"
        );
        assert!(
            seen.iter().any(|&(d, _)| d == 4 || d == 8),
            "intermediate: {seen:?}"
        );
        assert_eq!(seen.last(), Some(&(12, 12)));
    }

    #[cfg(unix)]
    #[test]
    fn watch_total_follows_the_file_when_the_size_was_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.bin");
        let script = format!("printf hello > {}", dest.display());
        let mut last = (0, 0);
        watch_download(
            sh(&script),
            &dest,
            0,
            std::time::Duration::from_millis(10),
            |d, t| {
                last = (d, t);
            },
        )
        .unwrap();
        assert_eq!(last, (5, 5));
    }

    #[cfg(unix)]
    #[test]
    fn watch_failure_carries_the_commands_stderr() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.bin");
        let e = watch_download(
            sh("echo 'curl: (22) 404' >&2; exit 22"),
            &dest,
            0,
            std::time::Duration::from_millis(10),
            |_, _| {},
        )
        .unwrap_err();
        assert!(e.to_string().contains("404"), "{e}");
    }

    #[test]
    fn watch_missing_program_is_reported_as_missing_curl() {
        let dir = tempfile::tempdir().unwrap();
        let e = watch_download(
            Command::new("definitely-not-a-program"),
            &dir.path().join("x"),
            0,
            std::time::Duration::from_millis(10),
            |_, _| {},
        )
        .unwrap_err();
        assert!(matches!(e, GithubError::Missing));
    }
}
