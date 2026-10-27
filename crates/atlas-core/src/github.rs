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
//!
//! Failure policy: no lookup ever answers with a different release than the one asked for.
//! An unreadable feed is [`GithubError::Feed`]; a newest tag whose `manifest.json` could not
//! be checked (network error, 5xx, rate limit; only 404/410 mean "not published") is
//! [`GithubError::Unknown`]; callers keep what they knew. Metadata requests have a connect
//! timeout and `--retry`; downloads have a stall guard instead of a total time cap and resume.

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
    /// The Atom feed could not be read, so the beta/alpha offer is unknown (not "nothing newer").
    #[error("could not read the GitHub release feed: {0}")]
    Feed(String),
    /// Whether a tag has a published release could not be determined (network error, not a 404).
    #[error("could not check whether {tag} is published: {reason}")]
    Unknown { tag: String, reason: String },
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

/// Metadata `curl` against the real program.
fn curl() -> Command {
    Net::system().curl()
}

/// What every request shares: quiet (errors still print), connect timeout, retry policy.
/// `-f` is left to the caller: a status probe wants the HTTP code, not an exit code. `program`
/// is `curl` outside the tests, which point it at a stub.
fn base_curl(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut c = Command::new(program);
    crate::sys::CommandExt::no_window(&mut c)
        .args([
            "-sS",
            "--connect-timeout",
            "10",
            "--retry",
            "3",
            "--retry-all-errors",
            "--retry-delay",
            "1",
            "-H",
            "User-Agent: atlas-cli",
        ])
        .stdin(Stdio::null());
    c
}

/// Seconds a download may stay below [`STALL_BYTES_PER_SEC`] before curl gives up.
const STALL_SECS: &str = "30";
/// A transfer slower than this for [`STALL_SECS`] is stalled (slow but moving is fine).
const STALL_BYTES_PER_SEC: &str = "1024";

/// `curl` for a large asset: no total time cap (the Desktop app is big and links are slow), a
/// stall guard instead, and resume (`-C -`) so a retry continues the partial file.
fn curl_download(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut c = base_curl(program);
    c.arg("-f");
    c.args([
        "--speed-limit",
        STALL_BYTES_PER_SEC,
        "--speed-time",
        STALL_SECS,
        "-C",
        "-",
        "-L",
    ]);
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

/// Where requests go: the `curl` program. [`Net::system`] is the real one; the tests aim it at
/// a stub so the network-facing logic runs without a network.
struct Net {
    curl: std::ffi::OsString,
}

impl Net {
    fn system() -> Self {
        Net {
            curl: "curl".into(),
        }
    }

    /// Metadata request: fails on HTTP errors, bounded transfer time.
    fn curl(&self) -> Command {
        let mut c = self.probe();
        c.arg("-f");
        c
    }

    /// Like [`Net::curl`] but HTTP errors are not failures: the caller reads `%{http_code}`.
    fn probe(&self) -> Command {
        let mut c = base_curl(&self.curl);
        c.args(["--max-time", "30"]);
        c
    }

    /// Tag the redirect `github.com/<repo>/releases/latest` → `.../releases/tag/<tag>` points
    /// at. Pre-GA that is the newest build of the default channel (an alpha), not necessarily
    /// a Stable: only [`Net::stable_tag`] turns it into one.
    fn latest_tag(&self, repo: &str) -> Result<String, GithubError> {
        let mut c = self.curl();
        c.args(["-o", NULL_DEVICE, "-w", "%{redirect_url}"])
            .arg(format!("https://github.com/{repo}/releases/latest"));
        let url = String::from_utf8_lossy(&run(c)?).trim().to_string();
        url.rsplit_once("/releases/tag/")
            .map(|(_, tag)| tag.to_string())
            .filter(|t| !t.is_empty())
            .ok_or_else(|| GithubError::Failed(format!("{repo} has no published release")))
    }
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

impl Net {
    fn atom(&self, repo: &str) -> Result<String, GithubError> {
        self.atom_page(repo, None)
    }

    /// A page of the feed: the 10 newest releases, or the 10 that follow `after`
    /// (`?after=<tag>`). Any failure is [`GithubError::Feed`]: an unreadable feed is not an
    /// empty one.
    fn atom_page(&self, repo: &str, after: Option<&str>) -> Result<String, GithubError> {
        let mut c = self.curl();
        c.arg(match after {
            Some(tag) => format!("https://github.com/{repo}/releases.atom?after={tag}"),
            None => format!("https://github.com/{repo}/releases.atom"),
        });
        match run(c) {
            Ok(body) => Ok(String::from_utf8_lossy(&body).into_owned()),
            Err(GithubError::Failed(e)) => Err(GithubError::Feed(e)),
            Err(e) => Err(e),
        }
    }
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

impl Net {
    /// The tag Stable follows: [`stable_from`] over the feed and the redirect, reading older
    /// feed pages (`?after=` the oldest tag seen) while a run of prereleases hides every
    /// Stable. The first page is `first` (already fetched by the caller), so the common case
    /// costs nothing. `Ok(None)` means there is no Stable; a feed page that cannot be read is
    /// an error, because a Stable may be behind it.
    fn stable_tag(
        &self,
        repo: &str,
        first: &[String],
        latest: Option<&str>,
    ) -> Result<Option<String>, GithubError> {
        let mut seen: Vec<String> = first.to_vec();
        let mut page = first.len();
        for _ in 0..STABLE_PAGES {
            let found = stable_from(seen.iter().map(String::as_str), latest);
            // A feed page is 10 long; a shorter one is the end of the history.
            if found.is_some() || page < 10 {
                return Ok(found);
            }
            let Some(oldest) = seen.last().cloned() else {
                return Ok(None);
            };
            let older = atom_tags(&self.atom_page(repo, Some(&oldest))?);
            page = older.len();
            if older.is_empty() {
                return Ok(None);
            }
            seen.extend(older);
        }
        Ok(stable_from(seen.iter().map(String::as_str), latest))
    }

    /// `HEAD` on the release's `manifest.json`: the tag is pushed before the release is built
    /// and published, so a tag alone does not mean there is something to install.
    ///
    /// Only an explicit 404/410 means "not published". Any other answer or a network failure
    /// is [`GithubError::Unknown`], so a blip never drops the newest tag.
    fn is_published(&self, repo: &str, tag: &str) -> Result<bool, GithubError> {
        let unknown = |reason: String| GithubError::Unknown {
            tag: tag.to_string(),
            reason,
        };
        // No `-f`: the status code is the answer, not an exit code.
        let mut c = self.probe();
        c.args(["-L", "-I", "-o", NULL_DEVICE, "-w", "%{http_code}"])
            .arg(format!(
                "https://github.com/{repo}/releases/download/{tag}/manifest.json"
            ));
        let out = run(c).map_err(|e| unknown(e.to_string()))?;
        match String::from_utf8_lossy(&out).trim() {
            "404" | "410" => Ok(false),
            code if code.starts_with('2') => Ok(true),
            code => Err(unknown(format!("HTTP {code}"))),
        }
    }

    /// The newest tag in `candidates` that `channel` accepts and that is actually published
    /// (the tag is pushed before the release exists, so walk down past unpublished ones);
    /// `stable` when none. A tag whose publication cannot be determined is an error rather
    /// than skipped.
    fn newest_published(
        &self,
        repo: &str,
        channel: Channel,
        mut candidates: Vec<&str>,
        stable: Option<&str>,
    ) -> Result<Option<String>, GithubError> {
        while let Some(tag) = channel::resolve(channel, candidates.iter().copied(), None) {
            if Some(tag.as_str()) == stable || self.is_published(repo, &tag)? {
                return Ok(Some(tag));
            }
            candidates.retain(|c| *c != tag);
        }
        Ok(stable.map(str::to_string))
    }

    /// The tag `channel` follows. Every channel reads the Atom feed and an unreadable feed is
    /// an error ([`GithubError::Feed`]): answering "Stable" for beta/alpha would report a
    /// downgrade as "nothing newer". Callers keep their previous answer on error.
    fn channel_tag(&self, repo: &str, channel: Channel) -> Result<String, GithubError> {
        let latest = self.latest_tag(repo);
        let feed = atom_tags(&self.atom(repo)?);
        let stable = self.stable_tag(repo, &feed, latest.as_deref().ok())?;
        let found = if channel == Channel::Stable {
            stable.clone()
        } else {
            let mut candidates: Vec<&str> = feed.iter().map(String::as_str).collect();
            candidates.extend(stable.as_deref());
            self.newest_published(repo, channel, candidates, stable.as_deref())?
        };
        found.ok_or_else(|| match latest {
            Err(e) => e,
            Ok(_) => GithubError::Failed(format!("{repo} has no Stable release")),
        })
    }
}

/// The tag `channel` follows, without fetching the changelog (what update checks need). An
/// unreadable Atom feed, or a newest tag whose publication cannot be determined, is an error:
/// the caller keeps what it knew instead of being told a different channel's release.
pub fn channel_tag(repo: &str, channel: Channel) -> Result<String, GithubError> {
    Net::system().channel_tag(repo, channel)
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
    Net::system().channel_views(repo, installed, selected)
}

impl Net {
    fn channel_views(
        &self,
        repo: &str,
        installed: &semver::Version,
        selected: Channel,
    ) -> Vec<channel::ChannelView> {
        // One feed fetch and one Stable lookup serve all three channels. A channel whose offer
        // cannot be determined (feed or HEAD failure) is shown with no offer, never with a
        // different channel's release.
        let latest = self.latest_tag(repo).ok();
        let feed = self.atom(repo).map(|x| atom_tags(&x));
        let stable = match &feed {
            Ok(f) => self.stable_tag(repo, f, latest.as_deref()).ok().flatten(),
            Err(_) => None,
        };
        let tags = |c: Channel| -> Option<String> {
            if c == Channel::Stable {
                return stable.clone();
            }
            let feed = feed.as_ref().ok()?;
            let mut cand: Vec<&str> = feed.iter().map(String::as_str).collect();
            cand.extend(stable.as_deref());
            self.newest_published(repo, c, cand, stable.as_deref())
                .ok()
                .flatten()
        };
        Channel::ALL
            .into_iter()
            .map(|c| channel::view(c, selected, installed, tags(c)))
            .collect()
    }
}

/// Download the release asset `name` of `tag` into `dest`.
pub fn download_asset(repo: &str, tag: &str, name: &str, dest: &Path) -> Result<(), GithubError> {
    download_asset_progress(repo, tag, name, dest, |_, _| {})
}

/// Like [`download_asset`], reporting `(bytes_done, bytes_total)` (total 0 while unknown)
/// as the file grows. curl writes straight to `dest`; its size is polled from this thread
/// until curl exits, so no output parsing and no extra copy are involved. There is no total
/// time limit: curl aborts a transfer that stalls (see [`curl_download`]) and retries resume
/// the partial file.
pub fn download_asset_progress(
    repo: &str,
    tag: &str,
    name: &str,
    dest: &Path,
    on_progress: impl FnMut(u64, u64),
) -> Result<(), GithubError> {
    Net::system().download(repo, tag, name, dest, on_progress)
}

impl Net {
    fn download(
        &self,
        repo: &str,
        tag: &str,
        name: &str,
        dest: &Path,
        on_progress: impl FnMut(u64, u64),
    ) -> Result<(), GithubError> {
        let url = format!("https://github.com/{repo}/releases/download/{tag}/{name}");
        let total = self.content_length(&url).unwrap_or(0);
        // `-C -` continues whatever is already in `dest`: a leftover from an earlier attempt
        // must not be appended to, so start from an empty file.
        let _ = std::fs::remove_file(dest);
        let mut c = curl_download(&self.curl);
        c.arg("-o").arg(dest).arg(&url);
        watch_download(
            c,
            dest,
            total,
            Watch {
                poll: std::time::Duration::from_millis(120),
                stall: STALL_LIMIT,
            },
            on_progress,
        )
    }
}

/// Longest the file may stay the same size before the transfer is declared stalled. curl's
/// own `--speed-time` ([`STALL_SECS`]) normally fires first; this is the backstop for a curl
/// that hangs without exiting, and makes the guarantee testable without a network.
const STALL_LIMIT: std::time::Duration = std::time::Duration::from_secs(60);

/// Polling cadence and stall limit of [`watch_download`].
#[derive(Clone, Copy)]
struct Watch {
    poll: std::time::Duration,
    stall: std::time::Duration,
}

/// Run `cmd` (which writes `dest`) to completion, calling `on_progress(size, total)` every
/// `poll` while the file grows and once more at the end with the final size. A failed exit
/// is an error carrying the command's stderr. A file that does not grow for `stall` is a
/// stalled transfer: the command is killed and the call fails; a slow download that keeps
/// growing is never cut off, however long it takes.
fn watch_download(
    mut cmd: Command,
    dest: &Path,
    total: u64,
    watch: Watch,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<(), GithubError> {
    cmd.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|_| GithubError::Missing)?;
    let size = || std::fs::metadata(dest).map_or(0, |m| m.len());
    let mut last = u64::MAX;
    let mut moved_at = std::time::Instant::now();
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
            moved_at = std::time::Instant::now();
            on_progress(done, total);
        } else if moved_at.elapsed() >= watch.stall {
            let _ = child.kill();
            let _ = child.wait();
            return Err(GithubError::Failed(format!(
                "download stalled at {done} bytes (no data for {}s)",
                watch.stall.as_secs_f32()
            )));
        }
        std::thread::sleep(watch.poll);
    }
    let done = size();
    on_progress(done, done.max(total));
    Ok(())
}

/// `Content-Length` of release asset `name` of `tag`; `None` when unknown or offline.
pub fn asset_size(repo: &str, tag: &str, name: &str) -> Option<u64> {
    Net::system().content_length(&format!(
        "https://github.com/{repo}/releases/download/{tag}/{name}"
    ))
}

impl Net {
    /// `Content-Length` of the final response of `url` (follows redirects); `None` when
    /// unknown.
    fn content_length(&self, url: &str) -> Option<u64> {
        let mut c = self.curl();
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

    /// Fast polling with a stall limit that only the stall tests reach.
    fn quick(ms: u64) -> Watch {
        Watch {
            poll: std::time::Duration::from_millis(ms),
            stall: std::time::Duration::from_secs(30),
        }
    }

    /// A `curl` stand-in: a shell script in `dir` that answers by URL from files in `dir`, so
    /// the network-facing logic runs for real without a network. `feed` is the Atom body (the
    /// feed request fails with curl's exit 7 when the file is absent), `latest` the redirect
    /// target, and `code-<tag>` the HTTP status of that tag's `manifest.json` (a missing file
    /// is a dropped connection: curl exit 6).
    #[cfg(unix)]
    fn stub_curl(dir: &Path) -> Net {
        use std::os::unix::fs::PermissionsExt;
        let script = format!(
            r#"#!/bin/sh
d='{d}'
for last; do :; done
case "$last" in
  *releases.atom*) [ -f "$d/feed" ] || {{ echo 'curl: (7) Failed to connect' >&2; exit 7; }}; cat "$d/feed" ;;
  */releases/latest) printf '%s' "$(cat "$d/latest")" ;;
  */manifest.json)
    tag=${{last%/manifest.json}}; tag=${{tag##*/}}
    [ -f "$d/code-$tag" ] || {{ echo 'curl: (6) Could not resolve host' >&2; exit 6; }}
    cat "$d/code-$tag" ;;
  */releases/download/*)
    # Asset: `payload` is the body. HEAD (-I) prints its length; GET writes `-o <file>` and, with
    # `-C -`, only the bytes the file does not have yet (curl's resume contract).
    head=0; out=; resume=0; prev=
    for a; do
      [ "$a" = -I ] && head=1
      [ "$prev" = -o ] && out=$a
      [ "$prev" = -C ] && [ "$a" = - ] && resume=1
      prev=$a
    done
    if [ $head = 1 ]; then wc -c < "$d/payload" | tr -d ' '; exit 0; fi
    [ -f "$d/fail-download" ] && {{ echo 'curl: (28) Operation too slow' >&2; exit 28; }}
    have=0; [ $resume = 1 ] && [ -f "$out" ] && have=$(wc -c < "$out" | tr -d ' ')
    if [ "$have" -gt 0 ]; then tail -c +$((have + 1)) "$d/payload" >> "$out"; else cp "$d/payload" "$out"; fi ;;
  *) echo "stub curl: unexpected $last" >&2; exit 2 ;;
esac
"#,
            d = dir.display()
        );
        let path = dir.join("curl");
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        Net {
            curl: path.into_os_string(),
        }
    }

    #[cfg(unix)]
    fn atom_of(tags: &[&str]) -> String {
        let entries: String = tags
            .iter()
            .map(|t| {
                format!(
                    r#"<entry><link rel="alternate" href="https://github.com/o/r/releases/tag/{t}"/></entry>"#
                )
            })
            .collect();
        format!("<feed>{entries}</feed>")
    }

    #[cfg(unix)]
    fn put(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    // G4: an unreadable feed used to degrade beta/alpha to "the Stable the redirect names".
    #[cfg(unix)]
    #[test]
    fn an_unreadable_feed_is_an_error_for_every_channel_not_a_stable_fallback() {
        let dir = tempfile::tempdir().unwrap();
        put(
            dir.path(),
            "latest",
            "https://github.com/o/r/releases/tag/v1.4.0",
        );
        let net = stub_curl(dir.path());
        for c in Channel::ALL {
            let e = net.channel_tag("o/r", c).unwrap_err();
            assert!(matches!(e, GithubError::Feed(_)), "{c}: {e:?}");
            assert!(e.to_string().contains("Failed to connect"), "{c}: {e}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_readable_feed_still_resolves_every_channel() {
        let dir = tempfile::tempdir().unwrap();
        put(
            dir.path(),
            "latest",
            "https://github.com/o/r/releases/tag/v1.5.0-alpha.2",
        );
        put(
            dir.path(),
            "feed",
            &atom_of(&["v1.5.0-alpha.2", "v1.4.0", "v1.4.0-beta.1"]),
        );
        put(dir.path(), "code-v1.5.0-alpha.2", "200");
        let net = stub_curl(dir.path());
        assert_eq!(net.channel_tag("o/r", Channel::Stable).unwrap(), "v1.4.0");
        assert_eq!(
            net.channel_tag("o/r", Channel::Alpha).unwrap(),
            "v1.5.0-alpha.2"
        );
        assert_eq!(net.channel_tag("o/r", Channel::Beta).unwrap(), "v1.4.0");
    }

    // G5: a network failure on the HEAD used to look like "unpublished" and drop the newest tag.
    #[cfg(unix)]
    #[test]
    fn a_head_network_failure_is_unknown_not_unpublished() {
        let dir = tempfile::tempdir().unwrap();
        let net = stub_curl(dir.path());
        // No code-v1.5.0-alpha.2 file: the stub drops the connection.
        let e = net.is_published("o/r", "v1.5.0-alpha.2").unwrap_err();
        assert!(
            matches!(&e, GithubError::Unknown { tag, .. } if tag == "v1.5.0-alpha.2"),
            "{e:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn only_an_explicit_404_means_unpublished() {
        let dir = tempfile::tempdir().unwrap();
        let net = stub_curl(dir.path());
        for (code, want) in [
            ("200", Some(true)),
            ("404", Some(false)),
            ("410", Some(false)),
        ] {
            put(dir.path(), "code-v1.0.0", code);
            assert_eq!(net.is_published("o/r", "v1.0.0").ok(), want, "HTTP {code}");
        }
        // A server error or a rate limit says nothing about the release.
        for code in ["500", "503", "403", "429"] {
            put(dir.path(), "code-v1.0.0", code);
            assert!(
                matches!(
                    net.is_published("o/r", "v1.0.0"),
                    Err(GithubError::Unknown { .. })
                ),
                "HTTP {code}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_walk_down_skips_a_404_tag_but_stops_on_an_unknown_one() {
        let dir = tempfile::tempdir().unwrap();
        let net = stub_curl(dir.path());
        let cands = || vec!["v1.5.0-alpha.3", "v1.5.0-alpha.2", "v1.4.0"];
        // The newest tag is pushed but not built yet: walk down to the published one.
        put(dir.path(), "code-v1.5.0-alpha.3", "404");
        put(dir.path(), "code-v1.5.0-alpha.2", "200");
        assert_eq!(
            net.newest_published("o/r", Channel::Alpha, cands(), Some("v1.4.0"))
                .unwrap()
                .as_deref(),
            Some("v1.5.0-alpha.2")
        );
        // The newest tag cannot be checked (no answer): not "older build", an error.
        std::fs::remove_file(dir.path().join("code-v1.5.0-alpha.3")).unwrap();
        let e = net
            .newest_published("o/r", Channel::Alpha, cands(), Some("v1.4.0"))
            .unwrap_err();
        assert!(
            matches!(&e, GithubError::Unknown { tag, .. } if tag == "v1.5.0-alpha.3"),
            "{e:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn channel_tag_fails_instead_of_dropping_the_newest_prerelease_on_a_head_error() {
        let dir = tempfile::tempdir().unwrap();
        put(
            dir.path(),
            "latest",
            "https://github.com/o/r/releases/tag/v1.4.0",
        );
        put(dir.path(), "feed", &atom_of(&["v1.5.0-alpha.2", "v1.4.0"]));
        let net = stub_curl(dir.path());
        let e = net.channel_tag("o/r", Channel::Alpha).unwrap_err();
        assert!(matches!(e, GithubError::Unknown { .. }), "{e:?}");
        // Stable needs no HEAD: it is unaffected.
        assert_eq!(net.channel_tag("o/r", Channel::Stable).unwrap(), "v1.4.0");
    }

    #[cfg(unix)]
    #[test]
    fn channel_views_never_offer_another_channels_release_when_the_feed_is_down() {
        let dir = tempfile::tempdir().unwrap();
        put(
            dir.path(),
            "latest",
            "https://github.com/o/r/releases/tag/v1.4.0",
        );
        let net = stub_curl(dir.path());
        let installed = semver::Version::parse("1.3.0").unwrap();
        let views = net.channel_views("o/r", &installed, Channel::Alpha);
        assert_eq!(views.len(), Channel::ALL.len());
        for v in &views {
            assert_eq!(
                (v.tag.as_deref(), v.offered.as_deref()),
                (None, None),
                "{:?}",
                v.channel
            );
        }
    }

    // G6: curl flags. Metadata retries and has a connect timeout; downloads have a stall guard
    // and resume instead of a total time cap.
    #[test]
    fn metadata_requests_retry_and_bound_connect_and_total_time() {
        let args = |c: &Command| -> Vec<String> {
            c.get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect()
        };
        let a = args(&Net::system().curl());
        for flag in [
            "--connect-timeout",
            "--retry",
            "--retry-all-errors",
            "--max-time",
            "-f",
        ] {
            assert!(a.iter().any(|x| x == flag), "{flag} in {a:?}");
        }
    }

    #[test]
    fn downloads_have_a_stall_guard_and_resume_but_no_total_time_cap() {
        let c = curl_download("curl");
        let a: Vec<String> = c
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        for flag in [
            "--speed-limit",
            "--speed-time",
            "-C",
            "--retry",
            "--connect-timeout",
        ] {
            assert!(a.iter().any(|x| x == flag), "{flag} in {a:?}");
        }
        assert!(!a.iter().any(|x| x == "--max-time"), "{a:?}");
    }

    // The watcher's stall guard: a download that stops growing is an error, one that keeps
    // growing slowly is fine.
    #[cfg(unix)]
    #[test]
    fn a_stalled_download_is_an_error_and_the_process_is_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.bin");
        let script = format!("printf aaaa > {}; sleep 30", dest.display());
        let started = std::time::Instant::now();
        let e = watch_download(
            sh(&script),
            &dest,
            100,
            Watch {
                poll: std::time::Duration::from_millis(10),
                stall: std::time::Duration::from_millis(400),
            },
            |_, _| {},
        )
        .unwrap_err();
        assert!(e.to_string().contains("stalled at 4 bytes"), "{e}");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "the hung process must be killed, not waited for"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_slow_download_that_keeps_moving_is_never_cut_off() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.bin");
        // 6 chunks 150 ms apart (~0.9 s total) against a 400 ms stall limit: slower than the
        // limit overall, but never idle for as long as it.
        let script = format!(
            "for i in 1 2 3 4 5 6; do printf x >> {d}; sleep 0.15; done",
            d = dest.display()
        );
        let mut last = (0, 0);
        watch_download(
            sh(&script),
            &dest,
            6,
            Watch {
                poll: std::time::Duration::from_millis(10),
                stall: std::time::Duration::from_millis(400),
            },
            |d, t| last = (d, t),
        )
        .unwrap();
        assert_eq!(last, (6, 6));
    }

    #[cfg(unix)]
    #[test]
    fn download_writes_the_asset_and_reports_the_known_total() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "payload", "0123456789");
        let net = stub_curl(dir.path());
        let dest = dir.path().join("asset.bin");
        let mut last = (0, 0);
        net.download("o/r", "v1.0.0", "asset.bin", &dest, |d, t| last = (d, t))
            .unwrap();
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "0123456789");
        assert_eq!(last, (10, 10));
    }

    // `-C -` continues whatever is in the file, so a leftover from an earlier, different
    // attempt must not be appended to: the sha256 check would fail on a corrupt mix.
    #[cfg(unix)]
    #[test]
    fn download_starts_from_an_empty_file_even_with_resume_on() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "payload", "0123456789");
        let net = stub_curl(dir.path());
        let dest = dir.path().join("asset.bin");
        std::fs::write(&dest, "STALE-LEFTOVER").unwrap();
        net.download("o/r", "v1.0.0", "asset.bin", &dest, |_, _| {})
            .unwrap();
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "0123456789");
    }

    // curl's exit 28 is `--speed-time` firing (a stalled transfer): it surfaces as an error
    // carrying curl's message, never as a silent success with a short file.
    #[cfg(unix)]
    #[test]
    fn a_download_curl_aborts_as_too_slow_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "payload", "0123456789");
        put(dir.path(), "fail-download", "");
        let net = stub_curl(dir.path());
        let dest = dir.path().join("asset.bin");
        let e = net
            .download("o/r", "v1.0.0", "asset.bin", &dest, |_, _| {})
            .unwrap_err();
        assert!(e.to_string().contains("too slow"), "{e}");
    }

    // The curl contract the download relies on: with `-C -` a retry appends only the missing
    // tail, so a partial file plus a resumed transfer is the whole asset.
    #[cfg(unix)]
    #[test]
    fn a_resumed_transfer_appends_only_the_missing_bytes() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "payload", "0123456789");
        let net = stub_curl(dir.path());
        let dest = dir.path().join("asset.bin");
        std::fs::write(&dest, "0123").unwrap();
        let mut c = curl_download(&net.curl);
        c.arg("-o")
            .arg(&dest)
            .arg("https://github.com/o/r/releases/download/v1.0.0/asset.bin");
        watch_download(c, &dest, 10, quick(10), |_, _| {}).unwrap();
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "0123456789");
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
        watch_download(sh(&script), &dest, 12, quick(20), |d, t| {
            seen.push((d, t));
        })
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
        watch_download(sh(&script), &dest, 0, quick(10), |d, t| {
            last = (d, t);
        })
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
            quick(10),
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
            quick(10),
            |_, _| {},
        )
        .unwrap_err();
        assert!(matches!(e, GithubError::Missing));
    }
}
