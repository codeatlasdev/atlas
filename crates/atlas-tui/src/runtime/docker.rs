use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::process::Command;

use super::health;
use super::service::ServiceState;

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

    /// Start docker compose services
    pub async fn start(&mut self, services: &[&str]) -> bool {
        self.state = ServiceState::Starting;

        let mut cmd = Command::new("docker");
        cmd.args([
            "compose",
            "-f",
            self.compose_file.to_str().unwrap_or(""),
            "up",
            "-d",
        ]);
        for svc in services {
            cmd.arg(svc);
        }
        cmd.current_dir(&self.root_dir);

        match cmd.output().await {
            Ok(output) if output.status.success() => {
                self.state = ServiceState::Running;
                true
            }
            _ => {
                self.state = ServiceState::Failed;
                false
            }
        }
    }

    /// Stop docker compose services
    pub async fn stop(&mut self) {
        let _ = Command::new("docker")
            .args([
                "compose",
                "-f",
                self.compose_file.to_str().unwrap_or(""),
                "stop",
            ])
            .current_dir(&self.root_dir)
            .output()
            .await;
        self.state = ServiceState::Stopped;
    }

    /// Check if a specific port is healthy (e.g., redis on 6379)
    pub async fn check_port(&self, port: u16) -> bool {
        health::check_port("127.0.0.1", port, Duration::from_secs(2)).await
    }

    /// Check if docker is available
    pub async fn is_docker_available() -> bool {
        Command::new("docker")
            .args(["info"])
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_manager_new() {
        let dm = DockerManager::new("docker-compose.yml", Path::new("/tmp"));
        assert_eq!(*dm.state(), ServiceState::Stopped);
        assert_eq!(dm.compose_file, PathBuf::from("/tmp/docker-compose.yml"));
    }
}
