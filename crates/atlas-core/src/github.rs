//! Public GitHub Releases client shared by `atlas update`, `atlas desktop` and the desktop app.
//!
//! Talks to the unauthenticated REST API and the public asset URLs through `curl`
//! (present on every supported OS), so installs and updates need no GitHub login.

use std::path::Path;
use std::process::Command;

use serde::Deserialize;
use thiserror::Error;

pub const MISSING: &str = "curl not found — install it and try again";

#[derive(Debug, Error)]
pub enum GithubError {
    #[error("{MISSING}")]
    Missing,
    #[error("GitHub request failed: {0}")]
    Failed(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseInfo {
    pub tag_name: String,
    #[serde(rename = "html_url")]
    pub url: String,
    pub published_at: String,
    #[serde(default)]
    pub body: Option<String>,
}

impl ReleaseInfo {
    /// The tag without its leading `v`.
    pub fn version(&self) -> &str {
        self.tag_name.trim_start_matches('v')
    }

    /// The release notes ("" when the release has none).
    pub fn notes(&self) -> &str {
        self.body.as_deref().unwrap_or_default()
    }
}

fn api(repo: &str, path: &str) -> String {
    format!("https://api.github.com/repos/{repo}/{path}")
}

/// A GitHub token when one is at hand: `GITHUB_TOKEN` / `GH_TOKEN`, else `gh auth token`.
/// Public releases need none; a private repo (or a rate-limited network) does.
fn token() -> Option<String> {
    static TOKEN: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    TOKEN
        .get_or_init(|| {
            ["GITHUB_TOKEN", "GH_TOKEN"]
                .iter()
                .find_map(|k| std::env::var(k).ok().filter(|v| !v.trim().is_empty()))
                .or_else(|| {
                    let out = Command::new("gh").args(["auth", "token"]).output().ok()?;
                    let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    (out.status.success() && !t.is_empty()).then_some(t)
                })
        })
        .clone()
}

/// Base `curl` invocation: silent, fail on HTTP errors, follow redirects, bounded time,
/// authenticated when a token is available. The token goes through curl's stdin config
/// (`-K -`) so it never shows up in the process list.
pub fn curl() -> Command {
    let mut c = Command::new("curl");
    c.args(["-fsSL", "--max-time", "120", "-H", "User-Agent: atlas-cli"]);
    if token().is_some() {
        c.args(["-K", "-"]);
    }
    c
}

/// Run `cmd` and return its stdout; a spawn failure means curl is missing.
fn run(mut cmd: Command) -> Result<Vec<u8>, GithubError> {
    use std::io::Write;
    let auth = token().map(|t| format!("header = \"Authorization: Bearer {t}\"\n"));
    cmd.stdin(if auth.is_some() {
        std::process::Stdio::piped()
    } else {
        std::process::Stdio::null()
    })
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().map_err(|_| GithubError::Missing)?;
    if let (Some(a), Some(mut stdin)) = (auth, child.stdin.take()) {
        let _ = stdin.write_all(a.as_bytes());
    }
    let out = child.wait_with_output().map_err(|_| GithubError::Missing)?;
    if out.status.success() {
        return Ok(out.stdout);
    }
    Err(GithubError::Failed(
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    ))
}

fn parse<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, GithubError> {
    serde_json::from_slice(bytes).map_err(|e| GithubError::Failed(e.to_string()))
}

/// Latest published (non-draft, non-prerelease) release of `repo`.
pub fn latest_release(repo: &str) -> Result<ReleaseInfo, GithubError> {
    let mut c = curl();
    c.arg(api(repo, "releases/latest"));
    parse(&run(c)?)
}

/// One release by tag (`v1.2.3`).
pub fn release_by_tag(repo: &str, tag: &str) -> Result<ReleaseInfo, GithubError> {
    let mut c = curl();
    c.arg(api(repo, &format!("releases/tags/{tag}")));
    parse(&run(c)?)
}

/// Tags of the newest published releases, newest first.
pub fn release_tags(repo: &str) -> Result<Vec<String>, GithubError> {
    #[derive(Deserialize)]
    struct Tag {
        tag_name: String,
    }
    let mut c = curl();
    c.arg(api(repo, "releases?per_page=30"));
    let tags: Vec<Tag> = parse(&run(c)?)?;
    Ok(tags.into_iter().map(|t| t.tag_name).collect())
}

/// Download the release asset `name` of `tag` into `dest`. With a token it goes through the
/// API (works for private repos too); without one, the public download URL.
pub fn download_asset(repo: &str, tag: &str, name: &str, dest: &Path) -> Result<(), GithubError> {
    let mut c = curl();
    c.arg("-o").arg(dest);
    if token().is_some() {
        #[derive(Deserialize)]
        struct Asset {
            name: String,
            url: String,
        }
        #[derive(Deserialize)]
        struct Rel {
            assets: Vec<Asset>,
        }
        let mut q = curl();
        q.arg(api(repo, &format!("releases/tags/{tag}")));
        let rel: Rel = parse(&run(q)?)?;
        let a = rel
            .assets
            .into_iter()
            .find(|a| a.name == name)
            .ok_or_else(|| GithubError::Failed(format!("{name} is not in release {tag}")))?;
        c.args(["-H", "Accept: application/octet-stream"])
            .arg(a.url);
    } else {
        c.arg(format!(
            "https://github.com/{repo}/releases/download/{tag}/{name}"
        ));
    }
    run(c).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_release_json() {
        let json = r#"{"tag_name":"v1.2.3","html_url":"u","published_at":"t","body":"b","id":1}"#;
        let r: ReleaseInfo = parse(json.as_bytes()).unwrap();
        assert_eq!(
            (r.version(), r.url.as_str(), r.notes()),
            ("1.2.3", "u", "b")
        );
    }

    #[test]
    fn null_body_is_empty_notes() {
        let json = r#"{"tag_name":"v1.0.0","html_url":"u","published_at":"t","body":null}"#;
        let r: ReleaseInfo = parse(json.as_bytes()).unwrap();
        assert_eq!(r.notes(), "");
    }

    #[test]
    fn bad_json_is_a_failure() {
        assert!(matches!(
            parse::<ReleaseInfo>(b"<html>"),
            Err(GithubError::Failed(_))
        ));
    }

    #[test]
    fn api_urls_are_public_rest_endpoints() {
        assert_eq!(
            api("o/r", "releases/latest"),
            "https://api.github.com/repos/o/r/releases/latest"
        );
    }
}
