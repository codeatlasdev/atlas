use atlas_core::sys::CommandExt as _;
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
            let mut cmd = Command::new("docker");
            cmd.no_window();
            cmd.envs(atlas_core::login_env::vars());
            cmd.args(["compose", "-f"]);
            cmd.arg(&self.compose_file);
            cmd.args(["up", "-d"]);
            for svc in services {
                cmd.arg(svc);
            }
            cmd.current_dir(&self.root_dir);
            apply_docker_env(&mut cmd);

            match cmd.output().await {
                Ok(output) if output.status.success() => {
                    self.state = ServiceState::Running;
                    return Ok(());
                }
                Ok(output) => {
                    last_err = format!(
                        "compose up failed (exit {:?}): {}",
                        output.status.code(),
                        String::from_utf8_lossy(&output.stderr).trim()
                    );
                }
                Err(e) => last_err = format!("compose spawn error: {e}"),
            }
        }
        self.state = ServiceState::Failed;
        Err(last_err)
    }

    pub async fn stop(&mut self) {
        self.state = ServiceState::Stopped;
        let mut cmd = Command::new("docker");
        cmd.no_window();
        cmd.envs(atlas_core::login_env::vars());
        cmd.args(["compose", "-f"]);
        cmd.arg(&self.compose_file);
        cmd.arg("down");
        cmd.current_dir(&self.root_dir);
        if let Some(host) = docker_host() {
            cmd.env("DOCKER_HOST", host);
        }
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        let _ = cmd.status().await;
    }

    pub async fn check_port(&self, port: u16) -> bool {
        health::check_port("127.0.0.1", port, Duration::from_secs(2)).await
    }

    pub async fn is_docker_available() -> bool {
        let mut cmd = Command::new("docker");
        cmd.no_window();
        cmd.envs(atlas_core::login_env::vars())
            .args(["info"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        tokio::time::timeout(Duration::from_secs(3), cmd.status())
            .await
            .is_ok_and(|r| r.is_ok_and(|s| s.success()))
    }
}

/// Returns the docker socket URI to use, preferring OrbStack when available.
/// Priority: DOCKER_HOST env var → OrbStack socket → /var/run/docker.sock
pub fn docker_host() -> Option<String> {
    if let Ok(v) = std::env::var("DOCKER_HOST")
        && !v.is_empty()
    {
        return Some(v);
    }
    // OrbStack socket
    let orb = dirs::home_dir()
        .map(|h| h.join(".orbstack/run/docker.sock"))
        .filter(|p| p.exists());
    if let Some(p) = orb {
        return Some(format!("unix://{}", p.display()));
    }
    // Default docker socket
    let default = std::path::Path::new("/var/run/docker.sock");
    if default.exists() {
        return Some("unix:///var/run/docker.sock".into());
    }
    None
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
}
