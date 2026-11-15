//! `atlas crew update <ssh-target> [--version X] [--rollback]` — bring a crew server's `atlas` to a release
//! (the CLI's own by default), keeping the previous binary as `atlas.prev` and rolling back by itself when
//! hostd does not come back. Only the binary dir changes: the crew home (crewmates, memory, secrets, the paused
//! flag) is never touched, so a paused crew stays paused. The one other thing it writes is the capture runner
//! (`runner.mjs` and `detectors.mjs`), and only on a server that already has the browsers: the new hostd asks
//! that runner for things an old one does not do.

use anyhow::{Result, bail};
use clap::Args;

use crate::commands::AtlasCommand;
use crate::commands::deploy::remote::Remote;

use super::binary;
use super::browsers;
use super::install::{self, host_from_target};
use super::layout::SERVICE;
use super::privileged::Priv;

#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// SSH target for the crew server (e.g. user@hostname or hostname)
    pub ssh_target: String,
    /// Release to install (default: this CLI's version)
    #[arg(long, value_name = "X.Y.Z", conflicts_with_all = ["rollback", "binary"])]
    pub version: Option<String>,
    /// Restore the binary the last update kept (`atlas.prev`) instead of installing a release
    #[arg(long, conflicts_with = "binary")]
    pub rollback: bool,
    /// SSH port (default 22)
    #[arg(long, default_value_t = 22)]
    pub port: u16,
    /// Upload this Linux `atlas` instead of downloading a release (same as `atlas crew install --binary`)
    #[arg(long)]
    pub binary: Option<std::path::PathBuf>,
}

/// What an update run does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// Restore `atlas.prev`.
    Rollback,
    /// Install this release (`Some`) or the uploaded binary (`None`: its version is not known).
    Install { version: Option<String> },
}

/// Decide the run from the flags. `cli` is this CLI's version.
pub fn plan(args: &UpdateArgs, cli: &str) -> Result<Plan> {
    if args.rollback {
        return Ok(Plan::Rollback);
    }
    if args.binary.is_some() {
        return Ok(Plan::Install { version: None });
    }
    // The version goes into a URL run on the server: only a real semver gets there.
    let version = args
        .version
        .as_deref()
        .map_or(cli, |v| v.strip_prefix('v').unwrap_or(v));
    if semver::Version::parse(version).is_err() {
        bail!("`{version}` is not a version (use X.Y.Z, e.g. {cli})");
    }
    Ok(Plan::Install {
        version: Some(version.to_owned()),
    })
}

/// A script's exit code → what to tell the person (`swap_script` / `rollback_script` codes).
fn outcome(code: i32, stderr: &str, target: &str) -> Result<()> {
    match code {
        0 => Ok(()),
        3 => bail!(
            "the new binary did not bring hostd up within {}s; rolled back to the previous one and it is running. \
Look at: atlas crew logs {target}",
            binary::HEALTH_TIMEOUT_S
        ),
        4 => bail!(
            "hostd is down on {target}: the update failed and the rollback did not bring it back either. \
Look at: atlas crew logs {target}"
        ),
        _ => bail!("update failed on {target}: {}", stderr.trim()),
    }
}

impl AtlasCommand for UpdateArgs {
    async fn run(self) -> Result<()> {
        let plan = plan(&self, env!("CARGO_PKG_VERSION"))?;
        let r = if self.port == 22 {
            Remote::new(&self.ssh_target)
        } else {
            Remote::with_port(&self.ssh_target, self.port)
        };
        let host = host_from_target(&self.ssh_target);
        let target = install::preflight(&r, &self.ssh_target).await?;
        let p = Priv::prepare(
            &r,
            &self.ssh_target,
            false,
            &format!("atlas crew update {}", self.ssh_target),
        )
        .await?;
        let unit = format!("test -f /etc/systemd/system/{SERVICE}.service");
        if r.exec(&unit).await?.code != 0 {
            bail!(
                "{host} is not a crew server (no {SERVICE} service): run atlas crew install {} first",
                self.ssh_target
            );
        }

        let script = match &plan {
            Plan::Rollback => {
                eprintln!("\x1b[36m▸\x1b[0m restoring the previous binary on {host}");
                binary::rollback_script(binary::HEALTH_TIMEOUT_S)
            }
            Plan::Install { version } => {
                match (version, &self.binary) {
                    (Some(v), _) => {
                        eprintln!("\x1b[36m▸\x1b[0m staging atlas {v} ({target}) on {host}");
                        install::stage_release(&p, v, target).await?;
                    }
                    (None, Some(path)) => {
                        eprintln!("\x1b[36m▸\x1b[0m uploading {} to {host}", path.display());
                        install::stage_upload(&p, path).await?;
                    }
                    (None, None) => unreachable!("plan() gives no version only for --binary"),
                }
                binary::swap_script(binary::HEALTH_TIMEOUT_S)
            }
        };
        eprintln!("\x1b[36m▸\x1b[0m restarting {SERVICE} and waiting for hostd");
        // `--rollback` flips atlas and atlas.prev: running it twice undoes it, so a dropped ssh link must not
        // retry it (`exec_once`). The swap is guarded against a second run and may be retried.
        let o = match plan {
            Plan::Rollback => r.exec_once(&script).await?,
            Plan::Install { .. } => p.exec(&script).await?,
        };
        outcome(o.code, &o.stderr, &self.ssh_target)?;
        print!("{}", o.stdout);
        let expected = match &plan {
            Plan::Install { version } => version.as_deref(),
            Plan::Rollback => None,
        };
        if let Some(warning) = binary::report(&o.stdout, expected) {
            eprintln!("\x1b[33m◆\x1b[0m {warning}");
        }
        // A new hostd asks the runner for the UI detectors; an old runner writes no findings and
        // would fail every scenario of the UI gate. A rollback keeps the runner it has.
        if matches!(plan, Plan::Install { .. }) && browsers::refresh(&p, &self.ssh_target).await? {
            eprintln!("\x1b[36m▸\x1b[0m refreshed the capture runner on {host}");
        }
        println!(
            "\x1b[32m✓\x1b[0m atlas on {host} updated; the previous binary is kept (atlas crew update {} --rollback).",
            self.ssh_target
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(version: Option<&str>, rollback: bool, binary: bool) -> UpdateArgs {
        UpdateArgs {
            ssh_target: "me@server".into(),
            version: version.map(Into::into),
            rollback,
            port: 22,
            binary: binary.then(|| "/tmp/atlas".into()),
        }
    }

    // Issue #203: an update is pinned to the CLI's version unless told otherwise.
    #[test]
    fn the_default_is_the_clis_own_version() {
        assert_eq!(
            plan(&args(None, false, false), "1.6.0").unwrap(),
            Plan::Install {
                version: Some("1.6.0".into())
            }
        );
    }

    #[test]
    fn a_version_flag_wins_and_a_leading_v_is_accepted() {
        for v in ["1.5.2", "v1.5.2"] {
            assert_eq!(
                plan(&args(Some(v), false, false), "1.6.0").unwrap(),
                Plan::Install {
                    version: Some("1.5.2".into())
                }
            );
        }
        assert_eq!(
            plan(&args(Some("1.7.0-alpha.3"), false, false), "1.6.0").unwrap(),
            Plan::Install {
                version: Some("1.7.0-alpha.3".into())
            }
        );
    }

    // The version ends up in a URL on the server: anything but a semver is refused before any ssh.
    #[test]
    fn a_version_that_is_not_semver_is_refused() {
        for bad in ["latest", "1.6", "1.6.0; rm -rf /", "../x", "", "v"] {
            let e = plan(&args(Some(bad), false, false), "1.6.0").unwrap_err();
            assert!(e.to_string().contains("not a version"), "{bad}: {e}");
        }
    }

    #[test]
    fn rollback_installs_nothing() {
        assert_eq!(
            plan(&args(None, true, false), "1.6.0").unwrap(),
            Plan::Rollback
        );
    }

    // An uploaded binary has no known version: the health check accepts any answering hostd.
    #[test]
    fn an_uploaded_binary_has_no_expected_version() {
        assert_eq!(
            plan(&args(None, false, true), "1.6.0").unwrap(),
            Plan::Install { version: None }
        );
    }
}
