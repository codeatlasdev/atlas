//! `atlas crew-spawn`: the two root services behind one-Linux-user-per-crewmate (issue #200). Run by
//! systemd (`atlas-crew-spawn.service`, `atlas-crew-provision.service`), never by hand on a normal day.
//!
//! - default: the spawn helper, which starts each crewmate's agent as its own user
//!   (`atlas_host::crew::spawn::server`).
//! - `--provision`: the provisioning service, which creates and removes a crewmate's user and keeps the
//!   egress rules in step (`atlas_host::crew::spawn::provision`).
//!
//! Both take their socket from systemd (socket activation, fd 3) and serve only the crew user, whose
//! name they read from the socket's group. `--socket` and `--crew-user` run one by hand, for tests.

use anyhow::{Result, bail};
use clap::Args;

use super::AtlasCommand;

#[derive(Debug, Args)]
pub struct CrewSpawnArgs {
    /// Run the provisioning service (users, directories, egress rules) instead of the spawn helper
    #[arg(long)]
    pub provision: bool,
    /// Listen on this socket path instead of the one systemd passes (running by hand)
    #[arg(long, value_name = "PATH", requires = "crew_user")]
    pub socket: Option<std::path::PathBuf>,
    /// The only user (besides root) that may connect, when `--socket` is given
    #[arg(long, value_name = "USER", requires = "socket")]
    pub crew_user: Option<String>,
    /// Provision one crewmate's user and directories now, with the plan the provisioning service runs,
    /// then exit: `--provision-user <CREWMATE USER> <CREW USER>`. What `atlas crew install` runs for the
    /// crewmates that exist, so the installer and the service share one code path
    #[arg(
        long,
        num_args = 2,
        value_names = ["CREWMATE_USER", "CREW_USER"],
        conflicts_with_all = ["provision", "socket", "crew_user"]
    )]
    pub provision_user: Option<Vec<String>>,
}

impl AtlasCommand for CrewSpawnArgs {
    #[cfg(unix)]
    async fn run(self) -> Result<()> {
        use atlas_host::crew::spawn::{exec, provision, server};

        // SAFETY: plain getter.
        if unsafe { libc::geteuid() } != 0 {
            bail!(
                "atlas crew-spawn runs as root (it is started by systemd: atlas-crew-spawn.service)"
            );
        }
        if let Some(names) = &self.provision_user {
            let (user, crew) = (&names[0], &names[1]);
            return match provision::Service::production(crew, exec::lookup(crew)?.uid)
                .handle(provision::ProvRequest::Provision { user: user.clone() })
            {
                provision::ProvResponse::Done => Ok(()),
                provision::ProvResponse::Error { reason } => bail!("{reason}"),
            };
        }
        let (listener, user, uid) = match (&self.socket, &self.crew_user) {
            (Some(path), Some(user)) => {
                let uid = exec::lookup(user)?.uid;
                (server::bind(path)?, user.clone(), uid)
            }
            _ => {
                let l = server::listener_from_systemd().ok_or_else(|| {
                    anyhow::anyhow!(
                        "no socket from systemd (LISTEN_FDS): start atlas-crew-spawn.socket, or pass --socket and --crew-user"
                    )
                })?;
                let (user, uid) = server::crew_user_of_socket(&l)?;
                (l, user, uid)
            }
        };
        eprintln!(
            "atlas crew-spawn{}: serving {user} (uid {uid})",
            if self.provision { " --provision" } else { "" }
        );
        // The services are plain blocking loops on their own threads: off the async runtime.
        let provisioning = self.provision;
        tokio::task::spawn_blocking(move || {
            if provisioning {
                provision::Service::production(&user, uid).serve(listener)
            } else {
                server::Server::new(server::Config::production(uid)).serve(listener)
            }
        })
        .await??;
        Ok(())
    }

    #[cfg(not(unix))]
    async fn run(self) -> Result<()> {
        let _ = self;
        bail!("atlas crew-spawn needs Linux")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct Cli {
        #[command(flatten)]
        a: CrewSpawnArgs,
    }

    #[test]
    fn socket_and_crew_user_go_together_and_provision_is_a_flag() {
        assert!(Cli::try_parse_from(["x"]).is_ok());
        assert!(
            Cli::try_parse_from(["x", "--provision"])
                .unwrap()
                .a
                .provision
        );
        let a = Cli::try_parse_from(["x", "--socket", "/s", "--crew-user", "u"])
            .unwrap()
            .a;
        assert_eq!(a.crew_user.as_deref(), Some("u"));
        assert!(Cli::try_parse_from(["x", "--socket", "/s"]).is_err());
        assert!(Cli::try_parse_from(["x", "--crew-user", "u"]).is_err());
    }

    // The installer's one-shot: a crewmate user and the crew user, and nothing else with it.
    #[test]
    fn provision_user_takes_the_crewmate_and_the_crew_user_and_excludes_the_service_flags() {
        let a = Cli::try_parse_from([
            "x",
            "--provision-user",
            "atlas-cm-3f2a9c1e5b7d",
            "atlas-crew",
        ])
        .unwrap()
        .a;
        assert_eq!(
            a.provision_user.as_deref(),
            Some(
                &[
                    "atlas-cm-3f2a9c1e5b7d".to_string(),
                    "atlas-crew".to_string()
                ][..]
            )
        );
        assert!(Cli::try_parse_from(["x", "--provision-user", "only-one"]).is_err());
        assert!(Cli::try_parse_from(["x", "--provision-user", "a", "b", "--provision"]).is_err());
    }

    // Not root, not under systemd: it refuses with the reason instead of serving a socket nobody gave it.
    #[cfg(unix)]
    #[tokio::test]
    async fn it_refuses_to_run_as_anyone_but_root() {
        // SAFETY: plain getter.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let e = CrewSpawnArgs {
            provision: false,
            socket: None,
            crew_user: None,
            provision_user: None,
        }
        .run()
        .await
        .unwrap_err();
        assert!(e.to_string().contains("runs as root"), "{e}");
    }
}
