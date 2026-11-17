//! System-ssh transport. Every script runs as `bash -c <script>` on the host
//! (or locally for `host: local`), so `~/.ssh/config` aliases and pinned
//! known_hosts apply. Every interpolated value MUST go through [`shq`].

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub use atlas_core::ssh::shq;
use atlas_core::ssh::{SshTarget, Use};

pub struct Remote {
    pub host: String,
    /// Non-default SSH port; `None` means standard port 22.
    pub port: Option<u16>,
    /// A crew server: every script starts with the agents' `PATH` (`atlas_core::ssh::with_path`).
    agent_path: bool,
    /// A private key picked with `--identity` (`-i`), instead of the ones ssh finds by itself.
    identity: Option<std::path::PathBuf>,
}

pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Remote {
    pub fn new(host: &str) -> Self {
        Self {
            host: host.into(),
            port: None,
            agent_path: false,
            identity: None,
        }
    }

    /// Create with an explicit port; 22 is the default and is left off the command line.
    pub fn with_port(host: &str, port: u16) -> Self {
        Self {
            host: host.into(),
            port: SshTarget::new(host, Some(port)).port,
            agent_path: false,
            identity: None,
        }
    }

    /// A crew server (`port` 22 = default): the one way every `atlas crew` command reaches it.
    pub fn crew(host: &str, port: u16) -> Self {
        Self {
            agent_path: true,
            ..Self::with_port(host, port)
        }
    }

    /// The same remote reached with `identity` (a private key file) when there is one.
    pub fn with_identity(self, identity: Option<std::path::PathBuf>) -> Self {
        Self { identity, ..self }
    }

    fn is_local(&self) -> bool {
        self.host == "local"
    }

    fn command(&self, script: &str) -> Command {
        if self.is_local() {
            let mut c = Command::new("bash");
            c.arg("-c").arg(script);
            return c;
        }
        let target =
            SshTarget::new(self.host.clone(), self.port).with_identity(self.identity.clone());
        let mut c = Command::new("ssh");
        if self.agent_path {
            c.args(target.exec_argv(script));
        } else {
            c.args(target.argv(Use::Exec))
                .arg("--")
                .arg(atlas_core::ssh::remote_command(script));
        }
        c
    }

    async fn run(&self, script: &str, stdin: Option<&[u8]>, file: Option<&Path>) -> Result<Output> {
        let mut cmd = self.command(script);
        cmd.stdin(if stdin.is_some() || file.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
        let mut child = cmd.spawn().context("spawning ssh")?;
        if let Some(mut pipe) = child.stdin.take() {
            if let Some(data) = stdin {
                pipe.write_all(data).await?;
            }
            if let Some(f) = file {
                let mut src = tokio::fs::File::open(f).await?;
                tokio::io::copy(&mut src, &mut pipe).await?;
            }
            pipe.shutdown().await.ok();
        }
        let out = child.wait_with_output().await?;
        Ok(Output {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }

    /// Run a read-only or idempotent script; retried (1s, 3s) only when ssh
    /// itself fails (exit 255).
    pub async fn exec(&self, script: &str) -> Result<Output> {
        self.exec_stdin(script, None).await
    }

    /// Like [`exec`](Self::exec) with `data` piped to the script's stdin.
    pub async fn exec_stdin(&self, script: &str, data: Option<&[u8]>) -> Result<Output> {
        let mut last = None;
        for delay in [0u64, 1, 3] {
            if delay > 0 {
                tokio::time::sleep(Duration::from_secs(delay)).await;
            }
            let out = self.run(script, data, None).await?;
            if out.code != 255 || self.is_local() {
                return Ok(out);
            }
            last = Some(out);
        }
        Ok(last.expect("at least one attempt"))
    }

    /// Never retried: for the one non-idempotent call (launching the driver).
    pub async fn exec_once(&self, script: &str) -> Result<Output> {
        self.run(script, None, None).await
    }

    /// Stream `file` into `<dir>/artifact.tar.gz`; a retry wipes `dir` first.
    pub async fn upload(&self, file: &Path, dir: &str) -> Result<()> {
        let script = format!(
            "rm -rf {d} && mkdir -p {d} && cat > {d}/artifact.tar.gz",
            d = shq(dir)
        );
        let mut last = String::new();
        for delay in [0u64, 1, 3] {
            if delay > 0 {
                tokio::time::sleep(Duration::from_secs(delay)).await;
            }
            let out = self.run(&script, None, Some(file)).await?;
            if out.code == 0 {
                return Ok(());
            }
            last = out.stderr;
            if out.code != 255 {
                break;
            }
        }
        anyhow::bail!("upload to {} failed: {}", self.host, last.trim())
    }

    /// Run `script` with its stdout streamed into `dest` (binary-safe, never held in memory). `dest`
    /// is created new (0600 on unix): an existing file is an error, and a failed run removes it, so a
    /// partial archive never passes for a whole one. Never retried. Returns `(exit code, stderr)`.
    pub async fn exec_to_file(&self, script: &str, dest: &Path) -> Result<(i32, String)> {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        let file = atlas_core::sys::owner_only(&mut opts)
            .open(dest)
            .with_context(|| format!("creating {}", dest.display()))?;
        let mut cmd = self.command(script);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::from(file))
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let out = cmd
            .spawn()
            .context("spawning ssh")?
            .wait_with_output()
            .await?;
        let code = out.status.code().unwrap_or(-1);
        if code != 0 {
            let _ = std::fs::remove_file(dest);
        }
        Ok((code, String::from_utf8_lossy(&out.stderr).into_owned()))
    }

    /// Run `script` with `file` piped to its stdin. Never retried (an import is not idempotent).
    pub async fn exec_with_file(&self, script: &str, file: &Path) -> Result<Output> {
        self.run(script, None, Some(file)).await
    }

    /// Run with inherited stdout/stderr (for `logs -f`).
    pub async fn stream(&self, script: &str) -> Result<i32> {
        let status = self.command(script).stdin(Stdio::null()).status().await?;
        Ok(status.code().unwrap_or(-1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shq_escapes_single_quote() {
        assert_eq!(shq("a'b"), "'a'\\''b'");
        assert_eq!(shq("$(rm -rf /)"), "'$(rm -rf /)'");
    }

    fn args_of(r: &Remote, script: &str) -> Vec<String> {
        r.command(script)
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    // #517: a key picked with `--identity` is on every ssh call of the remote (crew or deploy), and only then.
    #[test]
    fn an_identity_reaches_every_ssh_call_of_the_remote() {
        for r in [
            Remote::crew("atlas-crew@srv", 22),
            Remote::with_port("prod", 22),
        ] {
            let with = args_of(&r.with_identity(Some("/k/id_work".into())), "echo hi");
            let i = with.iter().position(|a| a == "-i").unwrap();
            assert_eq!(with[i + 1], "/k/id_work");
            assert!(with.iter().any(|a| a == "IdentitiesOnly=yes"), "{with:?}");
        }
        let without = args_of(&Remote::crew("atlas-crew@srv", 22), "echo hi");
        assert!(!without.contains(&"-i".to_string()), "{without:?}");
    }

    // Issue #355: a crew server gets its port before the target and the agents' PATH in the remote command.
    // A deploy host (`Remote::new`) keeps its script untouched: deploy scripts must not run under a PATH made for
    // the crew's agents.
    #[test]
    fn only_a_crew_remote_gets_the_agents_path_and_both_get_the_port() {
        let crew = args_of(&Remote::crew("atlas-crew@srv", 2222), "echo hi");
        let p = crew.iter().position(|a| a == "-p").unwrap();
        assert_eq!(crew[p + 1], "2222");
        assert!(p < crew.iter().position(|a| a == "atlas-crew@srv").unwrap());
        assert!(crew.last().unwrap().contains(".local/bin"), "{crew:?}");

        let deploy = args_of(&Remote::with_port("prod", 2200), "echo hi");
        assert!(deploy.windows(2).any(|w| w[0] == "-p" && w[1] == "2200"));
        assert!(!deploy.last().unwrap().contains(".local/bin"), "{deploy:?}");
        assert!(deploy.last().unwrap().contains("echo hi"));

        let plain = args_of(&Remote::crew("atlas-crew@srv", 22), "echo hi");
        assert!(
            !plain.contains(&"-p".to_string()),
            "22 is the default: {plain:?}"
        );
    }

    // The `local` deploy host is this machine's bash.
    #[cfg(unix)]
    #[tokio::test]
    async fn local_exec_round_trips_quotes() {
        let r = Remote::new("local");
        let out = r
            .exec(&format!("printf %s {}", shq("it's $HOME")))
            .await
            .unwrap();
        assert_eq!(out.stdout, "it's $HOME");
    }

    // Issue #202: an export is binary (gzip), so it goes from the remote stdout to a file untouched, a
    // failed run leaves no partial file, and an existing file is never overwritten.
    #[cfg(unix)]
    #[tokio::test]
    async fn exec_to_file_keeps_binary_bytes_and_removes_a_failed_partial() {
        let t = tempfile::tempdir().unwrap();
        let r = Remote::new("local");
        let ok = t.path().join("ok.bin");
        let (code, _) = r
            .exec_to_file(r"printf '\000\377\200abc'", &ok)
            .await
            .unwrap();
        assert_eq!(code, 0);
        assert_eq!(
            std::fs::read(&ok).unwrap(),
            [0u8, 0xff, 0x80, b'a', b'b', b'c']
        );
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&ok).unwrap().permissions().mode() & 0o077,
                0
            );
        }

        let bad = t.path().join("bad.bin");
        let (code, err) = r
            .exec_to_file("printf partial; echo boom >&2; exit 3", &bad)
            .await
            .unwrap();
        assert_eq!((code, err.trim()), (3, "boom"));
        assert!(!bad.exists(), "a partial export must not stay behind");

        assert!(
            r.exec_to_file("true", &ok).await.is_err(),
            "never overwrites"
        );
        assert_eq!(std::fs::read(&ok).unwrap().len(), 6);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn exec_with_file_pipes_the_file_to_stdin() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("in.bin");
        std::fs::write(&f, [0u8, 1, 2, 255]).unwrap();
        let out = Remote::new("local")
            .exec_with_file("od -An -tu1 | tr -s ' '", &f)
            .await
            .unwrap();
        assert_eq!(out.stdout.trim(), "0 1 2 255");
    }
}
