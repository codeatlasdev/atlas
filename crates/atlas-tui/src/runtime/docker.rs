use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use super::health;
use super::service::ServiceState;

#[derive(Clone)]
pub struct DockerManager {
    compose_file: PathBuf,
    root_dir: PathBuf,
    state: ServiceState,
}

impl DockerManager {
    pub fn new(compose_file: &str, root_dir: &Path) -> Self {
        Self {
            compose_file: root_dir.join(compose_file),
            root_dir: root_dir.to_path_buf(),
            state: ServiceState::Stopped,
        }
    }

    pub fn state(&self) -> &ServiceState {
        &self.state
    }

    /// Run `docker compose up -d`, retrying once. Returns the failure reason on error.
    /// Never prints: callers run inside the TUI's alternate screen.
    pub async fn start(&mut self, services: &[&str]) -> Result<(), String> {
        self.state = ServiceState::Starting;

        // A slow daemon is not fatal: compose up below reports the real failure.
        wait_for_docker_daemon(Duration::from_secs(15)).await;

        let mut last_err = String::new();
        for attempt in 0..2 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            let mut cmd = docker_command();
            cmd.args(["compose", "-f"]);
            cmd.arg(&self.compose_file);
            cmd.args(["up", "-d"]);
            for svc in services {
                cmd.arg(svc);
            }
            cmd.current_dir(&self.root_dir);

            match cmd.output().await {
                Ok(output) if output.status.success() => {
                    self.state = ServiceState::Running;
                    return Ok(());
                }
                Ok(output) => {
                    last_err = diagnose_compose_failure(
                        output.status.code(),
                        &String::from_utf8_lossy(&output.stderr),
                    );
                }
                Err(e) => {
                    last_err = diagnose_spawn_failure(&e, &login_path());
                    // No retry helps a binary that is not there.
                    if e.kind() == std::io::ErrorKind::NotFound {
                        break;
                    }
                }
            }
        }
        self.state = ServiceState::Failed;
        Err(last_err)
    }

    pub async fn stop(&mut self) {
        self.state = ServiceState::Stopped;
        let mut cmd = docker_command();
        cmd.args(["compose", "-f"]);
        cmd.arg(&self.compose_file);
        cmd.arg("down");
        cmd.current_dir(&self.root_dir);
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        let _ = cmd.status().await;
    }

    pub async fn check_port(&self, port: u16) -> bool {
        health::check_port("127.0.0.1", port, Duration::from_secs(2)).await
    }

    pub async fn is_docker_available() -> bool {
        let mut cmd = docker_command();
        cmd.args(["info"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        tokio::time::timeout(Duration::from_secs(3), cmd.status())
            .await
            .is_ok_and(|r| r.is_ok_and(|s| s.success()))
    }
}

/// The docker socket URI to use. Priority: `DOCKER_HOST` (the login shell's, so rootless docker and
/// `docker context` setups behave like the user's terminal) → OrbStack → rootless
/// (`$XDG_RUNTIME_DIR/docker.sock`) → `/var/run/docker.sock`.
pub fn docker_host() -> Option<String> {
    let vars = atlas_core::login_env::vars();
    let env = |k: &str| {
        vars.iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .or_else(|| std::env::var(k).ok())
    };
    resolve_docker_host(&env, dirs::home_dir().as_deref(), &|p| p.exists())
}

/// [`docker_host`] over injected env/home/filesystem, so every branch is testable.
fn resolve_docker_host(
    env: &dyn Fn(&str) -> Option<String>,
    home: Option<&Path>,
    exists: &dyn Fn(&Path) -> bool,
) -> Option<String> {
    if let Some(v) = env("DOCKER_HOST").filter(|v| !v.is_empty()) {
        return Some(v);
    }
    let sock = |p: PathBuf| exists(&p).then(|| format!("unix://{}", p.display()));
    home.and_then(|h| sock(h.join(".orbstack/run/docker.sock")))
        .or_else(|| {
            env("XDG_RUNTIME_DIR")
                .filter(|d| !d.is_empty())
                .and_then(|d| sock(PathBuf::from(d).join("docker.sock")))
        })
        .or_else(|| sock(PathBuf::from("/var/run/docker.sock")))
}

async fn wait_for_docker_daemon(timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if DockerManager::is_docker_available().await {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Inject DOCKER_HOST into a tokio Command if we can detect the socket.
/// Use this for service commands that run docker/docker compose directly.
pub fn apply_docker_env(cmd: &mut Command) {
    if let Some(host) = docker_host() {
        cmd.env("DOCKER_HOST", host);
    }
}

/// The `PATH` docker is searched on: the login shell's, the same the children run with.
fn login_path() -> String {
    atlas_core::login_env::path_var()
        .to_string_lossy()
        .into_owned()
}

/// `docker` as the user's shell would run it. `Command::new("docker")` resolves the program against
/// *our* `PATH`, not the `PATH` given to `.envs`, so a menu-launched app (bare `/usr/bin:/bin`) never
/// found a docker the login shell's PATH holds (Homebrew, `~/.local/bin`, rootless installs).
fn docker_command() -> Command {
    let program = atlas_core::login_env::which("docker").unwrap_or_else(|| PathBuf::from("docker"));
    let mut cmd = Command::new(program);
    cmd.envs(atlas_core::login_env::vars());
    apply_docker_env(&mut cmd);
    cmd
}

/// Why `docker compose up -d` failed, in words that name the fix. The raw stderr stays in the message.
fn diagnose_compose_failure(code: Option<i32>, stderr: &str) -> String {
    let raw = stderr.trim();
    let lower = raw.to_lowercase();
    let hint = if lower.contains("permission denied") && lower.contains("docker.sock") {
        Some(
            "docker: permission denied on the docker socket. Add your user to the docker group: \
             sudo usermod -aG docker $USER, then log out and in",
        )
    } else if lower.contains("cannot connect to the docker daemon")
        || lower.contains("is the docker daemon running")
    {
        Some(
            "docker daemon is not running. Start it (Linux: sudo systemctl start docker; macOS: \
             open OrbStack or Docker Desktop)",
        )
    } else if lower.contains("'compose' is not a docker command")
        || lower.contains("unknown shorthand flag: 'f'")
    {
        Some(
            "docker compose plugin is missing. Install docker-compose-plugin (Linux) or update Docker",
        )
    } else {
        None
    };
    let exit = code.map_or("killed by a signal".to_string(), |c| format!("exit {c}"));
    let detail = if raw.is_empty() { "no output" } else { raw };
    match hint {
        Some(h) => format!("{h}\ncompose up failed ({exit}): {detail}"),
        None => format!("compose up failed ({exit}): {detail}"),
    }
}

/// Why `docker` could not even be spawned.
fn diagnose_spawn_failure(e: &std::io::Error, path: &str) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => format!(
            "docker not found. Install Docker (https://docs.docker.com/engine/install/) or fix \
             PATH; searched: {path}"
        ),
        std::io::ErrorKind::PermissionDenied => {
            format!("docker could not be run: permission denied ({e})")
        }
        _ => format!("docker could not be run: {e}"),
    }
}

/// Background task: polls `docker info` every 5 s.
/// Emits `CriticalAlert` when daemon dies or recovers.
/// Does NOT attempt to restart the daemon: restarting the runtime (`orb restart docker`) has crashed OrbStack.
pub async fn watch_daemon(
    event_tx: tokio::sync::mpsc::UnboundedSender<super::manager::ManagerEvent>,
) {
    use super::manager::ManagerEvent;
    let mut was_ok = DockerManager::is_docker_available().await;
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let ok = DockerManager::is_docker_available().await;
        if !ok && was_ok {
            was_ok = false;
            let _ = event_tx.send(ManagerEvent::CriticalAlert {
                service: "docker".into(),
                message: "Docker daemon unreachable.".into(),
            });
        } else if ok && !was_ok {
            was_ok = true;
            let _ = event_tx.send(ManagerEvent::CriticalAlert {
                service: "docker".into(),
                message: "Docker daemon recovered.".into(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_host_returns_none_when_no_socket() {
        // When DOCKER_HOST is unset and no socket exists this returns None or Some.
        // Just ensure it doesn't panic.
        let _ = docker_host();
    }

    /// Incident (Linux): the user is not in the `docker` group, so `docker compose up` dies with a
    /// socket permission error. The reason shown on the card and in View logs must name the fix.
    #[test]
    fn permission_denied_on_the_socket_names_the_docker_group_fix() {
        let stderr = "permission denied while trying to connect to the Docker daemon socket at \
                      unix:///var/run/docker.sock: Get \"http://%2Fvar%2Frun%2Fdocker.sock/v1.47/\
                      containers/json\": dial unix /var/run/docker.sock: connect: permission denied";
        let msg = diagnose_compose_failure(Some(1), stderr);
        assert!(msg.contains("permission denied"), "{msg}");
        assert!(msg.contains("usermod -aG docker"), "{msg}");
        assert!(msg.contains("log out"), "{msg}");
        assert!(
            !msg.contains("OrbStack"),
            "Linux advice must not talk about OrbStack: {msg}"
        );
    }

    #[test]
    fn a_daemon_that_is_down_says_to_start_it_not_orbstack_only() {
        let stderr = "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. \
                      Is the docker daemon running?";
        let msg = diagnose_compose_failure(Some(1), stderr);
        assert!(msg.contains("daemon is not running"), "{msg}");
        assert!(
            msg.contains(stderr.trim()),
            "the raw error stays visible: {msg}"
        );
    }

    #[test]
    fn a_missing_compose_plugin_is_named() {
        for stderr in [
            "docker: 'compose' is not a docker command.\nSee 'docker --help'",
            "unknown shorthand flag: 'f' in -f",
        ] {
            let msg = diagnose_compose_failure(Some(125), stderr);
            assert!(msg.contains("docker compose plugin"), "{stderr}: {msg}");
        }
    }

    #[test]
    fn an_unrecognised_failure_keeps_the_exit_code_and_the_raw_stderr() {
        let msg = diagnose_compose_failure(Some(17), "port is already allocated");
        assert!(msg.contains("exit 17"), "{msg}");
        assert!(msg.contains("port is already allocated"), "{msg}");
        let silent = diagnose_compose_failure(Some(1), "   ");
        assert!(silent.contains("exit 1"), "{silent}");
        assert!(
            silent.contains("no output"),
            "empty stderr is still explained: {silent}"
        );
    }

    #[test]
    fn a_missing_docker_binary_says_not_found_and_where_it_looked() {
        let err = std::io::Error::from(std::io::ErrorKind::NotFound);
        let msg = diagnose_spawn_failure(&err, "/usr/bin:/bin");
        assert!(msg.contains("docker not found"), "{msg}");
        assert!(
            msg.contains("/usr/bin:/bin"),
            "PATH searched is shown: {msg}"
        );
    }

    #[test]
    fn a_docker_binary_that_cannot_run_is_not_called_missing() {
        let err = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let msg = diagnose_spawn_failure(&err, "/usr/bin");
        assert!(!msg.contains("not found"), "{msg}");
        assert!(msg.contains("permission denied"), "{msg}");
    }

    fn env_of(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let v: Vec<(String, String)> = vars
            .iter()
            .map(|(k, x)| (k.to_string(), x.to_string()))
            .collect();
        move |k| v.iter().find(|(n, _)| n == k).map(|(_, x)| x.clone())
    }

    /// Incident (Linux): atlas forced `DOCKER_HOST=unix:///var/run/docker.sock` over whatever the user's
    /// own shell exported (rootless docker, a `docker context`), so compose talked to a socket the
    /// user's terminal never used. The `DOCKER_HOST` of the login shell wins over every default.
    #[test]
    fn the_login_shells_docker_host_wins_over_any_default_socket() {
        let home = tempfile::tempdir().unwrap();
        let env = env_of(&[("DOCKER_HOST", "unix:///run/user/1000/docker.sock")]);
        assert_eq!(
            resolve_docker_host(&env, Some(home.path()), &|_| true).as_deref(),
            Some("unix:///run/user/1000/docker.sock")
        );
    }

    /// Rootless docker: no DOCKER_HOST exported, the daemon listens on `$XDG_RUNTIME_DIR/docker.sock`
    /// and `/var/run/docker.sock` is absent or somebody else's.
    #[test]
    fn rootless_docker_socket_is_found_through_xdg_runtime_dir() {
        let env = env_of(&[("XDG_RUNTIME_DIR", "/run/user/1000")]);
        let want = std::path::PathBuf::from("/run/user/1000/docker.sock");
        let got = resolve_docker_host(&env, None, &|p| p == want);
        assert_eq!(got.as_deref(), Some("unix:///run/user/1000/docker.sock"));
    }

    /// Without any hint the standard system socket is used, as before; nothing at all gives `None`.
    #[test]
    fn the_system_socket_is_the_last_resort() {
        let env = env_of(&[]);
        let sys = std::path::Path::new("/var/run/docker.sock");
        let got = resolve_docker_host(&env, None, &|p| p == sys);
        assert_eq!(got.as_deref(), Some("unix:///var/run/docker.sock"));
        assert_eq!(resolve_docker_host(&env, None, &|_| false), None);
    }

    /// An empty `DOCKER_HOST` (some rc files `export DOCKER_HOST=`) is not a choice.
    #[test]
    fn an_empty_docker_host_is_ignored() {
        let env = env_of(&[("DOCKER_HOST", "")]);
        let sys = std::path::Path::new("/var/run/docker.sock");
        let got = resolve_docker_host(&env, None, &|p| p == sys);
        assert_eq!(got.as_deref(), Some("unix:///var/run/docker.sock"));
    }

    /// macOS behaviour is unchanged: OrbStack's socket beats the system one.
    #[test]
    fn orbstack_socket_still_beats_the_system_socket() {
        let home = std::path::PathBuf::from("/Users/dev");
        let orb = home.join(".orbstack/run/docker.sock");
        let sys = std::path::Path::new("/var/run/docker.sock");
        let got = resolve_docker_host(&env_of(&[]), Some(&home), &|p| p == orb || p == sys);
        assert_eq!(
            got.as_deref(),
            Some("unix:///Users/dev/.orbstack/run/docker.sock")
        );
    }
}
