//! Locating and installing language-server executables under `<base>/lsp/<server>/<version>/`.
//!
//! Resolution order: a binary already on `PATH` (or in `~/.cargo/bin`, `~/go/bin`), then a
//! previously installed copy, then a download. Downloads are verified (GitHub's published
//! sha256 digest, Node's `SHASUMS256.txt`, npm's lockfile integrity), staged next to the
//! final dir and published with one `rename`, so a crash never leaves a half-installed
//! version that looks complete. Concurrent callers for one target share a single install.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use tokio::process::Command;
use tokio::sync::OnceCell;

use crate::registry::{Install, Server};

/// Node runtime pinned for npm-based servers when no suitable `node` is on `PATH`.
pub const NODE_VERSION: &str = "v24.21.0";
/// Oldest system Node accepted (typescript-language-server's engine floor is the strictest).
const NODE_MIN: (u32, u32, u32) = (22, 22, 2);

/// Where a resolved server executable comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Already on `PATH` (or `~/.cargo/bin`, `~/go/bin`…): never touched by Atlas.
    Path,
    /// Installed by Atlas under `~/.atlas/lsp/<server>/<version>/`.
    Managed,
    /// The user's own command from settings (`lsp.<language>.command`).
    Custom,
}

/// How to launch a resolved server.
#[derive(Debug, Clone, PartialEq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Extra `PATH` entries (managed node's bin dir) so servers can spawn `node`/`tsserver`.
    pub path_prepend: Vec<PathBuf>,
    pub source: Source,
    /// Version of the main package when Atlas manages it (`None`: a PATH binary, version unknown).
    pub version: Option<String>,
}

impl Launch {
    /// A launch of an executable the user provides (tests, custom commands).
    pub fn custom(program: impl Into<PathBuf>, args: Vec<String>) -> Launch {
        Launch {
            program: program.into(),
            args,
            path_prepend: vec![],
            source: Source::Custom,
            version: None,
        }
    }
}

/// Why a server could not be resolved: machine-readable `code` for the UI, a one-line
/// `message`, and what to do about it. Travels as the error of [`Installer::resolve`]
/// (`anyhow::Error::downcast_ref::<Failure>()`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Failure {
    /// `no_go` | `no_curl` | `offline` | `unsupported_platform` | `install_failed` | `custom_not_found` | `crashed`
    pub code: &'static str,
    pub message: String,
    /// A sentence telling the user what to do.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// A shell command that fixes it, when there is one worth copying.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Failure {}

impl Failure {
    pub fn new(code: &'static str, message: impl Into<String>) -> Failure {
        Failure {
            code,
            message: message.into(),
            hint: None,
            command: None,
        }
    }
    pub fn hint(mut self, hint: impl Into<String>) -> Failure {
        self.hint = Some(hint.into());
        self
    }
    pub fn command(mut self, command: impl Into<String>) -> Failure {
        self.command = Some(command.into());
        self
    }

    /// Classify an install error: network trouble reads as `offline`, anything else as
    /// `install_failed` with the underlying message.
    pub fn from_install(server: &str, e: &anyhow::Error) -> Failure {
        if let Some(f) = e.downcast_ref::<Failure>() {
            return f.clone();
        }
        let text = format!("{e:#}");
        let lower = text.to_lowercase();
        if lower.contains("curl not found") {
            return Failure::new(
                "no_curl",
                format!("Could not download {server}: curl is missing"),
            )
            .hint("Atlas downloads language servers with curl. Install it and open the file again.")
            .command("brew install curl");
        }
        if lower.contains("no rust-analyzer build for") || lower.contains("no node.js build for") {
            return Failure::new("unsupported_platform", text)
                .hint("There is no prebuilt server for this OS/CPU. Install one yourself and set its command in Settings → Languages.");
        }
        let net = [
            "could not resolve",
            "connection refused",
            "timed out",
            "network is unreachable",
            "failed to connect",
            "ssl",
            "curl: (6)",
            "curl: (7)",
            "curl: (28)",
            "enotfound",
            "etimedout",
            "getaddrinfo",
        ]
        .iter()
        .any(|n| lower.contains(n));
        if net {
            Failure::new("offline", format!("Could not download {server}: {text}"))
                .hint("Check the internet connection and open the file again.")
        } else {
            Failure::new(
                "install_failed",
                format!("Installing {server} failed: {text}"),
            )
            .hint("The install log is in ~/.atlas/lsp/logs. Retry from Settings → Languages.")
        }
    }
}

/// Where a server is in resolution; mirrored into the status pushed to clients.
pub trait Progress: Send + Sync {
    fn set(&self, message: &str);
}

impl<F: Fn(&str) + Send + Sync> Progress for F {
    fn set(&self, message: &str) {
        self(message)
    }
}

pub fn lsp_dir(base: &Path) -> PathBuf {
    base.join("lsp")
}

pub fn log_dir(base: &Path) -> PathBuf {
    lsp_dir(base).join("logs")
}

fn version_dir(base: &Path, name: &str, version: &str) -> PathBuf {
    lsp_dir(base).join(name).join(version)
}

type Resolution = Arc<OnceCell<Result<Launch, Failure>>>;

/// Shares one resolution per (server, version) between concurrent first-opens.
#[derive(Default)]
pub struct Installer {
    cells: Mutex<HashMap<String, Resolution>>,
}

impl Installer {
    /// Resolve `server`, installing it if needed. A failure is not pinned: the next open
    /// (or `lsp_install`) tries again. The error downcasts to [`Failure`].
    pub async fn resolve(
        &self,
        base: &Path,
        server: &'static Server,
        progress: &dyn Progress,
    ) -> Result<Launch> {
        let key = format!("{}@{}", server.name, server.version);
        let cell = self.cells.lock().entry(key.clone()).or_default().clone();
        let res = cell
            .get_or_init(|| async {
                resolve_uncached(base, server, progress)
                    .await
                    .map_err(|e| Failure::from_install(server.name, &e))
            })
            .await
            .clone();
        if res.is_err() {
            self.cells.lock().remove(&key);
        }
        res.map_err(anyhow::Error::new)
    }

    /// Forget a cached resolution (after an uninstall, so the next resolve looks again).
    pub fn forget(&self, server: &Server) {
        self.cells
            .lock()
            .remove(&format!("{}@{}", server.name, server.version));
    }
}

fn path_launch(s: &Server, bin: PathBuf) -> Launch {
    Launch {
        program: bin,
        args: s.args.iter().map(|a| (*a).to_string()).collect(),
        path_prepend: vec![],
        source: Source::Path,
        version: None,
    }
}

fn managed(program: PathBuf, args: Vec<String>, prepend: Vec<PathBuf>, s: &Server) -> Launch {
    Launch {
        program,
        args,
        path_prepend: prepend,
        source: Source::Managed,
        version: Some(s.version.to_string()),
    }
}

fn no_go() -> Failure {
    Failure::new(
        "no_go",
        "gopls is not installed and `go` was not found to build it",
    )
    .hint("Install Go, then open the file again. Atlas builds gopls with it.")
    .command("brew install go")
}

async fn resolve_uncached(base: &Path, s: &'static Server, p: &dyn Progress) -> Result<Launch> {
    if let Some(bin) = which_in(s.bin, &extra_bin_dirs())
        && (s.install != Install::GithubGz || probe(&bin).await)
    {
        return Ok(path_launch(s, bin));
    }
    let dir = version_dir(base, s.name, s.version);
    match s.install {
        Install::GithubGz => {
            let bin = dir.join(exe(s.bin));
            if !bin.is_file() {
                p.set(&format!("Downloading {} {}", s.name, s.version));
                install_rust_analyzer(&dir, &exe(s.bin), s.version).await?;
            }
            Ok(managed(bin, vec![], vec![], s))
        }
        Install::GoInstall => {
            let bin = dir.join("bin").join(exe(s.bin));
            if !bin.is_file() {
                let go = which_in("go", &extra_bin_dirs()).ok_or_else(no_go)?;
                p.set(&format!("Installing {} {}", s.name, s.version));
                go_install(&go, &dir, s).await?;
            }
            Ok(managed(bin, vec![], vec![], s))
        }
        Install::Npm => {
            let (node, node_bin) = ensure_node(base, p).await?;
            let entry = dir.join("node_modules").join(s.entry);
            if !entry.is_file() {
                p.set(&format!("Installing {} {}", s.name, s.version));
                npm_install(base, &node, &dir, s).await?;
                if !entry.is_file() {
                    bail!("{} installed but {} is missing", s.name, entry.display());
                }
            }
            let mut args = vec![entry.to_string_lossy().into_owned()];
            args.extend(s.args.iter().map(|a| (*a).to_string()));
            Ok(managed(node, args, node_bin.into_iter().collect(), s))
        }
    }
}

/// What is installed for a server right now, without installing anything.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Detected {
    /// `path` | `managed`; absent when not installed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The toolchain needed to install it (`go`) is missing: install is impossible until it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<Failure>,
}

/// Look for `s` the way [`Installer::resolve`] would, but never download or run an installer.
/// A PATH rust-analyzer is probed (`--version`) because a rustup proxy may lack the component.
pub async fn detect(base: &Path, s: &'static Server) -> Detected {
    if let Some(bin) = which_in(s.bin, &extra_bin_dirs())
        && (s.install != Install::GithubGz || probe(&bin).await)
    {
        return Detected {
            source: Some(Source::Path),
            version: version_of(&bin).await,
            path: Some(bin),
            blocked: None,
        };
    }
    let dir = version_dir(base, s.name, s.version);
    let installed = match s.install {
        Install::GithubGz => dir.join(exe(s.bin)),
        Install::GoInstall => dir.join("bin").join(exe(s.bin)),
        Install::Npm => dir.join("node_modules").join(s.entry),
    };
    if installed.is_file() {
        return Detected {
            source: Some(Source::Managed),
            path: Some(installed),
            version: Some(s.version.to_string()),
            blocked: None,
        };
    }
    let blocked = (s.install == Install::GoInstall && which_in("go", &extra_bin_dirs()).is_none())
        .then(no_go);
    Detected {
        source: None,
        path: None,
        version: None,
        blocked,
    }
}

/// First line of `<bin> --version`, trimmed; `None` when the binary does not answer in 3 s.
async fn version_of(bin: &Path) -> Option<String> {
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        Command::new(bin)
            .arg("--version")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    let line = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()?
        .trim()
        .to_string();
    (out.status.success() && !line.is_empty()).then_some(line)
}

/// Remove Atlas's managed copy of `s` (every version). PATH binaries are never touched.
pub fn uninstall(base: &Path, s: &Server) -> Result<bool> {
    let dir = lsp_dir(base).join(s.name);
    if !dir.is_dir() {
        return Ok(false);
    }
    std::fs::remove_dir_all(&dir).with_context(|| format!("remove {}", dir.display()))?;
    Ok(true)
}

/// `~/.cargo/bin`, `~/go/bin` and friends: GUI apps often start without the shell's `PATH`.
pub fn extra_bin_dirs() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return vec![];
    };
    let mut dirs = vec![
        home.join(".cargo/bin"),
        home.join("go/bin"),
        home.join(".atlas/bin"),
    ];
    if cfg!(windows) {
        let env_dir =
            |var: &str, rest: &str| std::env::var_os(var).map(|d| PathBuf::from(d).join(rest));
        dirs.extend(
            [
                env_dir("ProgramFiles", "Go/bin"),
                env_dir("ProgramFiles", "nodejs"),
                env_dir("APPDATA", "npm"),
            ]
            .into_iter()
            .flatten(),
        );
    } else {
        dirs.extend([
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/local/go/bin"),
        ]);
    }
    dirs
}

/// First executable `name` on `PATH`, then in `extra` (`name.exe`, `name.cmd`… on Windows).
pub fn which_in(name: &str, extra: &[PathBuf]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let names = atlas_core::paths::exe_names(name);
    std::env::split_paths(&path)
        .chain(extra.iter().cloned())
        .flat_map(|d| names.iter().map(move |n| d.join(n)))
        .find(|p| atlas_core::paths::is_executable(p))
}

/// `name` as the executable file Atlas installs (`gopls`, `gopls.exe` on Windows).
fn exe(name: &str) -> String {
    atlas_core::paths::exe(name)
}

/// `rust-analyzer` on `PATH` may be the rustup proxy of a toolchain that lacks the component:
/// it exists but exits non-zero. Probe with `--version`.
pub async fn probe(bin: &Path) -> bool {
    Command::new(bin)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .status()
        .await
        .is_ok_and(|s| s.success())
}

// ---------------------------------------------------------------- download primitives

/// Fetch `url` to `dest` with `curl`: follows redirects, resumes a partial `dest`
/// (`-C -`) so an interrupted download continues, bounded by connect/stall timeouts.
async fn curl_to(url: &str, dest: &Path) -> Result<()> {
    let out = Command::new("curl")
        .args([
            "-fsSL",
            "--connect-timeout",
            "20",
            "--speed-limit",
            "1024",
            "--speed-time",
            "30",
            "--retry",
            "2",
            "-C",
            "-",
            "-H",
            "User-Agent: atlas-lsp",
            "-o",
        ])
        .arg(dest)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .context("curl not found — install it and try again")?;
    if !out.status.success() {
        bail!(
            "download {url} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

async fn curl_text(url: &str) -> Result<String> {
    let out = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "60",
            "-H",
            "User-Agent: atlas-lsp",
            url,
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .context("curl not found — install it and try again")?;
    if !out.status.success() {
        bail!(
            "fetch {url} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    String::from_utf8(out.stdout).context("non-utf8 response")
}

pub fn sha256_file(p: &Path) -> Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// Download to `<final>.part-<pid>` (resumable within this call), verify, return the path.
/// A corrupt partial (resume went wrong) is deleted and fetched once from scratch.
async fn fetch_verified(url: &str, part: &Path, want_sha256: &str) -> Result<()> {
    for attempt in 0..2 {
        curl_to(url, part).await?;
        let got = sha256_file(part)?;
        if got.eq_ignore_ascii_case(want_sha256) {
            return Ok(());
        }
        let _ = std::fs::remove_file(part);
        if attempt == 1 {
            bail!("sha256 mismatch for {url}: expected {want_sha256}, got {got}");
        }
    }
    unreachable!()
}

/// A fresh sibling staging dir of `final_dir`; removed on drop unless published.
struct Stage {
    dir: PathBuf,
    published: bool,
}

impl Stage {
    fn new(final_dir: &Path) -> Result<Stage> {
        let parent = final_dir.parent().context("no parent dir")?;
        std::fs::create_dir_all(parent)?;
        let name = final_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("x");
        let dir = parent.join(format!(".{name}.tmp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        Ok(Stage {
            dir,
            published: false,
        })
    }

    /// Atomically make the staged dir the final one. A concurrent winner (another hostd on
    /// the same home) keeps its copy; ours is discarded.
    fn publish(mut self, final_dir: &Path) -> Result<()> {
        match std::fs::rename(&self.dir, final_dir) {
            Ok(()) => {
                self.published = true;
                Ok(())
            }
            Err(_) if final_dir.is_dir() => Ok(()),
            Err(e) => Err(e).context("publish install"),
        }
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        if !self.published {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

// ---------------------------------------------------------------- rust-analyzer

/// GitHub target triple of this machine for rust-analyzer release assets.
fn ra_target() -> Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        (os, arch) => bail!("no rust-analyzer build for {os}/{arch}"),
    })
}

/// The asset's sha256 from the release metadata (`digest: "sha256:…"`), which GitHub computes
/// server-side; the download is rejected when it does not match.
async fn github_asset_digest(repo: &str, tag: &str, asset: &str) -> Result<String> {
    let json = curl_text(&format!(
        "https://api.github.com/repos/{repo}/releases/tags/{tag}"
    ))
    .await?;
    let v: serde_json::Value = serde_json::from_str(&json)?;
    v["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|a| a["name"] == asset)
        .and_then(|a| a["digest"].as_str())
        .and_then(|d| d.strip_prefix("sha256:"))
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("release {tag} has no verifiable digest for {asset}"))
}

async fn install_rust_analyzer(dir: &Path, bin_name: &str, tag: &str) -> Result<()> {
    // Windows builds ship as a zip holding `rust-analyzer.exe`; the others as a bare gzip.
    let ext = if cfg!(windows) { "zip" } else { "gz" };
    let asset = format!("rust-analyzer-{}.{ext}", ra_target()?);
    let digest = github_asset_digest("rust-lang/rust-analyzer", tag, &asset).await?;
    let stage = Stage::new(dir)?;
    let part = stage.dir.join("download.part");
    fetch_verified(
        &format!("https://github.com/rust-lang/rust-analyzer/releases/download/{tag}/{asset}"),
        &part,
        &digest,
    )
    .await?;
    let bin = stage.dir.join(bin_name);
    if cfg!(windows) {
        extract_zip(&part, &stage.dir, 0)?;
        if !bin.is_file() {
            bail!("rust-analyzer archive has no {bin_name}");
        }
    } else {
        gunzip_exec(&part, &bin)?;
    }
    let _ = std::fs::remove_file(&part);
    stage.publish(dir)
}

/// Extract the zip `archive` into `out` with the system `tar` (bsdtar ships with Windows 10+ and
/// reads zip), dropping `strip` leading path components. bsdtar refuses absolute and `..` paths.
fn extract_zip(archive: &Path, out: &Path, strip: usize) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let tar = std::env::var_os("SystemRoot")
        .map(|r| PathBuf::from(r).join("System32").join("tar.exe"))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("tar"));
    let st = std::process::Command::new(tar)
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(out)
        .arg(format!("--strip-components={strip}"))
        .stdin(Stdio::null())
        .output()
        .context("run tar")?;
    if !st.status.success() {
        bail!(
            "extract {}: {}",
            archive.display(),
            String::from_utf8_lossy(&st.stderr).trim()
        );
    }
    Ok(())
}

fn gunzip_exec(gz: &Path, out: &Path) -> Result<()> {
    let mut d = flate2::read::GzDecoder::new(std::fs::File::open(gz)?);
    let mut f = std::fs::File::create(out)?;
    std::io::copy(&mut d, &mut f).context("decompress")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(out, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

// ---------------------------------------------------------------- gopls

async fn go_install(go: &Path, dir: &Path, s: &Server) -> Result<()> {
    let stage = Stage::new(dir)?;
    let bin_dir = stage.dir.join("bin");
    let out = Command::new(go)
        .args(["install", &format!("{}@{}", s.packages[0], s.version)])
        .env("GOBIN", &bin_dir)
        .env("GOFLAGS", "-mod=mod")
        .env("GOTOOLCHAIN", "local")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output()
        .await
        .context("run go install")?;
    if !out.status.success() {
        bail!(
            "go install {} failed: {}",
            s.packages[0],
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    stage.publish(dir)
}

// ---------------------------------------------------------------- node + npm

fn parse_node_version(s: &str) -> Option<(u32, u32, u32)> {
    let mut it = s.trim().trim_start_matches('v').split('.');
    Some((
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
    ))
}

async fn node_version(node: &Path) -> Option<(u32, u32, u32)> {
    let out = Command::new(node)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .ok()?;
    out.status
        .success()
        .then(|| parse_node_version(&String::from_utf8_lossy(&out.stdout)))
        .flatten()
}

fn node_target() -> Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "darwin-arm64",
        ("macos", "x86_64") => "darwin-x64",
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("windows", "x86_64") => "win-x64",
        ("windows", "aarch64") => "win-arm64",
        (os, arch) => bail!("no Node.js build for {os}/{arch}"),
    })
}

/// Where the managed Node's executable lives under its install dir: `bin/node`, or `node.exe` at
/// the root on Windows (that is how nodejs.org lays out each archive).
fn node_bin_dir(dir: &Path) -> PathBuf {
    if cfg!(windows) {
        dir.to_path_buf()
    } else {
        dir.join("bin")
    }
}

/// The `node` to run npm servers with, and its bin dir to prepend to `PATH` (None: system).
/// A system Node is used only when new enough, so every npm server is guaranteed its engine.
async fn ensure_node(base: &Path, p: &dyn Progress) -> Result<(PathBuf, Option<PathBuf>)> {
    if std::env::var_os("ATLAS_LSP_MANAGED_NODE").is_none()
        && let Some(node) = which_in("node", &extra_bin_dirs())
        && node_version(&node).await.is_some_and(|v| v >= NODE_MIN)
    {
        return Ok((node, None));
    }
    let dir = version_dir(base, "node", NODE_VERSION);
    let bin_dir = node_bin_dir(&dir);
    if !bin_dir.join(exe("node")).is_file() {
        p.set(&format!("Downloading Node.js {NODE_VERSION}"));
        install_node(&dir, NODE_VERSION).await?;
    }
    Ok((bin_dir.join(exe("node")), Some(bin_dir)))
}

/// Parse one `SHASUMS256.txt` line set for `file`.
pub fn shasum_for(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (h, n) = l.split_once(char::is_whitespace)?;
        (n.trim() == file).then(|| h.to_string())
    })
}

async fn install_node(dir: &Path, version: &str) -> Result<()> {
    let ext = if cfg!(windows) { "zip" } else { "tar.gz" };
    let file = format!("node-{version}-{}.{ext}", node_target()?);
    let base = format!("https://nodejs.org/dist/{version}");
    let sums = curl_text(&format!("{base}/SHASUMS256.txt")).await?;
    let want = shasum_for(&sums, &file).ok_or_else(|| anyhow!("no checksum for {file}"))?;
    let stage = Stage::new(dir)?;
    let part = stage.dir.join(format!("node.{ext}.part"));
    fetch_verified(&format!("{base}/{file}"), &part, &want).await?;
    let unpack = stage.dir.join("unpack");
    if cfg!(windows) {
        extract_zip(&part, &unpack, 1)?;
    } else {
        extract_tar_gz(&part, &unpack, 1)?;
    }
    let _ = std::fs::remove_file(&part);
    // Hoist the unpacked tree into the stage root so the final layout is dir/bin/node.
    for e in std::fs::read_dir(&unpack)? {
        let e = e?;
        std::fs::rename(e.path(), stage.dir.join(e.file_name()))?;
    }
    std::fs::remove_dir(&unpack)?;
    if !node_bin_dir(&stage.dir).join(exe("node")).is_file() {
        bail!("Node archive has no node executable");
    }
    stage.publish(dir)
}

/// Extract `archive` into `out`, dropping `strip` leading path components. Entries that would
/// escape `out` (absolute paths, `..`) are rejected.
pub fn extract_tar_gz(archive: &Path, out: &Path, strip: usize) -> Result<()> {
    use std::path::Component;
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(std::fs::File::open(archive)?));
    ar.set_preserve_permissions(true);
    std::fs::create_dir_all(out)?;
    for entry in ar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let mut rel = PathBuf::new();
        for (i, c) in path.components().enumerate() {
            match c {
                Component::Normal(n) if i >= strip => rel.push(n),
                Component::Normal(_) | Component::CurDir => {}
                _ => bail!("unsafe path in archive: {}", path.display()),
            }
        }
        if rel.as_os_str().is_empty() {
            continue;
        }
        let dest = out.join(&rel);
        if let Some(p) = dest.parent() {
            std::fs::create_dir_all(p)?;
        }
        entry.unpack(&dest)?;
    }
    Ok(())
}

/// `npm install --prefix <stage> pkg@ver…` with the chosen node, then publish. No global
/// installs, no scripts (`--ignore-scripts`), npm's own lockfile integrity checks apply.
async fn npm_install(base: &Path, node: &Path, dir: &Path, s: &Server) -> Result<()> {
    let stage = Stage::new(dir)?;
    // A pinned package.json keeps the dir self-describing and stops npm from walking up.
    std::fs::write(
        stage.dir.join("package.json"),
        format!(
            "{{\"name\":\"atlas-lsp-{}\",\"private\":true,\"version\":\"0.0.0\"}}",
            s.name
        ),
    )?;
    let npm_cli = npm_cli_for(node).await?;
    let mut cmd = Command::new(node);
    cmd.arg(&npm_cli)
        .args([
            "install",
            "--prefix",
            &stage.dir.to_string_lossy(),
            "--no-audit",
            "--no-fund",
            "--no-update-notifier",
            "--ignore-scripts",
            "--save-exact",
            "--loglevel=error",
        ])
        .args(s.packages)
        .env("npm_config_cache", lsp_dir(base).join("npm-cache"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(bin) = node.parent() {
        cmd.env("PATH", prepend_path(&[bin.to_path_buf()]));
    }
    let out = cmd.output().await.context("run npm")?;
    if !out.status.success() {
        bail!(
            "npm install {} failed: {}",
            s.packages.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    stage.publish(dir)
}

/// `npm-cli.js` next to `node`: managed Node ships `lib/node_modules/npm`; a system Node's
/// layout is `<prefix>/bin/node` + `<prefix>/lib/node_modules/npm` (fnm/nvm/Homebrew follow it).
/// On Windows npm sits beside `node.exe`: `<dir>/node_modules/npm`.
async fn npm_cli_for(node: &Path) -> Result<PathBuf> {
    let real = std::fs::canonicalize(node).unwrap_or_else(|_| node.to_path_buf());
    for cand in [node, real.as_path()] {
        if cfg!(windows)
            && let Some(dir) = cand.parent()
        {
            let beside = dir.join("node_modules/npm/bin/npm-cli.js");
            if beside.is_file() {
                return Ok(beside);
            }
        }
        if let Some(prefix) = cand.parent().and_then(Path::parent) {
            let cli = prefix.join("lib/node_modules/npm/bin/npm-cli.js");
            if cli.is_file() {
                return Ok(cli);
            }
        }
    }
    bail!(
        "npm not found next to {} (expected lib/node_modules/npm)",
        node.display()
    )
}

/// `PATH` with `dirs` first.
pub fn prepend_path(dirs: &[PathBuf]) -> std::ffi::OsString {
    let cur = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths(dirs.iter().cloned().chain(std::env::split_paths(&cur))).unwrap_or(cur)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tgz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut b = tar::Builder::new(Vec::new());
        for (name, data) in entries {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o755);
            h.set_cksum();
            b.append_data(&mut h, name, *data).unwrap();
        }
        let raw = b.into_inner().unwrap();
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        e.write_all(&raw).unwrap();
        e.finish().unwrap()
    }

    #[test]
    fn install_errors_classify_into_actionable_failures() {
        let f = |m: &str| Failure::from_install("rust-analyzer", &anyhow!("{m}"));
        assert_eq!(
            f("curl: (6) Could not resolve host: github.com").code,
            "offline"
        );
        assert_eq!(f("download x failed: Operation timed out").code, "offline");
        assert_eq!(
            f("curl not found — install it and try again").code,
            "no_curl"
        );
        assert_eq!(
            f("no rust-analyzer build for freebsd/x86_64").code,
            "unsupported_platform"
        );
        let other = f("sha256 mismatch for u");
        assert_eq!(other.code, "install_failed");
        assert!(other.message.contains("sha256 mismatch"));
        // A typed Failure inside the chain is returned as is.
        let typed = anyhow::Error::new(no_go()).context("while installing");
        let t = Failure::from_install("gopls", &typed);
        assert_eq!(
            (t.code, t.command.as_deref()),
            ("no_go", Some("brew install go"))
        );
    }

    #[tokio::test]
    async fn detect_reports_managed_installs_and_blocked_toolchains_without_installing() {
        let tmp = tempfile::tempdir().unwrap();
        // An npm server nobody has on PATH: only a managed copy can satisfy detection.
        let s = crate::registry::by_name("bash-language-server").unwrap();
        if which_in(s.bin, &extra_bin_dirs()).is_some() {
            return; // the machine has its own copy; `Source::Path` is covered by `resolve` tests
        }
        let none = detect(tmp.path(), s).await;
        assert_eq!((none.source, none.path, none.version), (None, None, None));
        assert!(
            none.blocked.is_none(),
            "an npm server is never blocked: Node is downloaded"
        );

        let entry = version_dir(tmp.path(), s.name, s.version)
            .join("node_modules")
            .join(s.entry);
        std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
        std::fs::write(&entry, "x").unwrap();
        let managed = detect(tmp.path(), s).await;
        assert_eq!(managed.source, Some(Source::Managed));
        assert_eq!(managed.version.as_deref(), Some(s.version));
        assert_eq!(managed.path.as_deref(), Some(entry.as_path()));

        // gopls without Go: blocked with the install command, nothing is created.
        let gopls = crate::registry::by_name("gopls").unwrap();
        if which_in(gopls.bin, &extra_bin_dirs()).is_none()
            && which_in("go", &extra_bin_dirs()).is_none()
        {
            let go = detect(tmp.path(), gopls).await;
            let b = go.blocked.expect("gopls needs go");
            assert_eq!(
                (b.code, b.command.as_deref()),
                ("no_go", Some("brew install go"))
            );
        }
        assert!(!tmp.path().join("lsp/gopls").exists());
    }

    #[test]
    fn uninstall_removes_every_managed_version_and_reports_absence() {
        let tmp = tempfile::tempdir().unwrap();
        let s = crate::registry::by_name("gopls").unwrap();
        assert!(!uninstall(tmp.path(), s).unwrap());
        for v in ["v0.1.0", "v0.2.0"] {
            let d = version_dir(tmp.path(), s.name, v).join("bin");
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("gopls"), "x").unwrap();
        }
        assert!(uninstall(tmp.path(), s).unwrap());
        assert!(!lsp_dir(tmp.path()).join("gopls").exists());
        // Other servers are untouched.
        let other = version_dir(tmp.path(), "rust-analyzer", "x");
        std::fs::create_dir_all(&other).unwrap();
        assert!(!uninstall(tmp.path(), s).unwrap());
        assert!(other.exists());
    }

    #[test]
    fn shasums_are_matched_by_exact_filename() {
        let s = "aa  node-v1-linux-x64.tar.gz\nbb  node-v1-linux-x64.tar.gz.sig\ncc *node-v1-darwin-arm64.tar.gz\n";
        assert_eq!(
            shasum_for(s, "node-v1-linux-x64.tar.gz").as_deref(),
            Some("aa")
        );
        assert_eq!(
            shasum_for(s, "node-v1-darwin-arm64.tar.gz").as_deref(),
            None
        );
        assert_eq!(shasum_for(s, "missing"), None);
    }

    #[test]
    fn node_versions_parse_and_compare() {
        assert_eq!(parse_node_version("v24.21.0\n"), Some((24, 21, 0)));
        assert!(parse_node_version("v22.22.2").unwrap() >= NODE_MIN);
        assert!(parse_node_version("v22.9.0").unwrap() < NODE_MIN);
        assert!(parse_node_version("v20.0.0").unwrap() < NODE_MIN);
        assert_eq!(parse_node_version("nonsense"), None);
    }

    #[test]
    fn tar_strip_extracts_and_rejects_escapes() {
        let t = tempfile::tempdir().unwrap();
        let a = t.path().join("a.tgz");
        std::fs::write(
            &a,
            tgz(&[("node-v1/bin/node", b"#!x"), ("node-v1/lib/x.js", b"1")]),
        )
        .unwrap();
        let out = t.path().join("out");
        extract_tar_gz(&a, &out, 1).unwrap();
        assert_eq!(std::fs::read(out.join("bin/node")).unwrap(), b"#!x");
        assert!(out.join("lib/x.js").is_file());

        // A path with `..` is refused (built by hand: the tar builder refuses to write one).
        let mut raw = vec![0u8; 512];
        raw[..9].copy_from_slice(b"../escape");
        raw[100..107].copy_from_slice(b"0000644");
        raw[124..135].copy_from_slice(b"00000000001");
        raw[156] = b'0';
        raw[257..262].copy_from_slice(b"ustar");
        raw[148..156].copy_from_slice(b"        ");
        let sum: u32 = raw.iter().map(|b| *b as u32).sum();
        raw[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
        raw.extend_from_slice(&[b'x'; 1]);
        raw.resize(1024, 0);
        raw.extend_from_slice(&[0u8; 1024]);
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        e.write_all(&raw).unwrap();
        let bad = t.path().join("bad.tgz");
        std::fs::write(&bad, e.finish().unwrap()).unwrap();
        assert!(extract_tar_gz(&bad, &t.path().join("o2"), 0).is_err());
        assert!(!t.path().join("escape").exists());
    }

    #[test]
    fn gunzip_roundtrip_is_executable_and_sha_matches() {
        let t = tempfile::tempdir().unwrap();
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        e.write_all(b"binary-bytes").unwrap();
        let gz = t.path().join("ra.gz");
        std::fs::write(&gz, e.finish().unwrap()).unwrap();
        let out = t.path().join("ra");
        gunzip_exec(&gz, &out).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"binary-bytes");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert!(out.metadata().unwrap().permissions().mode() & 0o111 != 0);
        }
        // sha256("abc")
        std::fs::write(t.path().join("abc"), "abc").unwrap();
        assert_eq!(
            sha256_file(&t.path().join("abc")).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn stage_publishes_atomically_and_cleans_up() {
        let t = tempfile::tempdir().unwrap();
        let fin = t.path().join("srv/1.0");
        let s = Stage::new(&fin).unwrap();
        std::fs::write(s.dir.join("f"), "x").unwrap();
        let staged = s.dir.clone();
        assert!(!fin.exists());
        s.publish(&fin).unwrap();
        assert!(fin.join("f").is_file());
        assert!(!staged.exists());

        // dropped without publish: no leftovers, final dir untouched
        let s2 = Stage::new(&fin).unwrap();
        let staged2 = s2.dir.clone();
        drop(s2);
        assert!(!staged2.exists());
        assert!(fin.join("f").is_file());

        // losing a publish race keeps the winner
        let s3 = Stage::new(&fin).unwrap();
        std::fs::write(s3.dir.join("g"), "y").unwrap();
        s3.publish(&fin).unwrap();
        assert!(fin.join("f").is_file() && !fin.join("g").exists());
    }

    #[cfg(unix)]
    #[test]
    fn which_finds_executables_only() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().to_path_buf();
        std::fs::write(d.join("plain"), "").unwrap();
        gunzip_exec_for_test(&d.join("tool"));
        assert_eq!(
            which_in("tool", std::slice::from_ref(&d)),
            Some(d.join("tool"))
        );
        assert_eq!(which_in("plain", std::slice::from_ref(&d)), None);
    }

    #[cfg(unix)]
    fn gunzip_exec_for_test(p: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(p, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
