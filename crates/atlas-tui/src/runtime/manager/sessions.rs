use std::process::Stdio;

use tokio::process::Command;

use super::actions::drain_pumps;
use super::mobile::detect_mobile_phase;
use super::types::ManagedSession;
use super::{ManagerEvent, ServiceManager};
use crate::runtime::logs::LogStream;

impl ServiceManager {
    /// Spawn a `session: true` action with an optional pre-selected device ID.
    /// Injects APPLE_DEVELOPMENT_TEAM env var from atlas.local.yaml — the correct
    /// way for Tauri to pick up the team ID on project generation.
    pub async fn run_session_with_device(
        &mut self,
        key: char,
        device_id: Option<String>,
    ) -> Option<String> {
        let mut action = self.find_action(key)?;
        // Prefer explicit device_id arg; fall back to pinned device_id in atlas.yaml
        let resolved = device_id.clone().or_else(|| action.device_id.clone());
        if let Some(id) = &resolved {
            action.command = format!("{} --device {}", action.command, id);
        }
        self.run_session_action(action, resolved).await
    }

    /// Spawn a `session: true` action — opens a dedicated log tab with
    /// stdout/stderr streamed in real time and basic stdin forwarding.
    /// If a session with the same label is already running, it is killed first.
    pub async fn run_session(&mut self, key: char) -> Option<String> {
        let action = self.find_action(key)?;
        self.run_session_action(action, None).await
    }

    async fn run_session_action(
        &mut self,
        action: crate::config::ActionConfig,
        device_id_hint: Option<String>,
    ) -> Option<String> {
        let label = action.label.clone();

        // Kill any existing session with the same label
        self.stop_session(&label).await;

        // Suspend services as with regular actions; resume only those that were
        // live (a service the user stopped on purpose must stay stopped).
        let resume = self.pause_for_action(&action.pause_services, None).await;

        // Wait for suspended services' ports to be free before spawning.
        // This avoids the race where the child vite process is still holding
        // the port when Tauri's beforeDevCommand tries to bind it.
        for name in &resume {
            if let Some(port) = self.services.get(name.as_str()).and_then(|s| s.status.port) {
                crate::runtime::health::wait_port_free(
                    "127.0.0.1",
                    port,
                    std::time::Duration::from_secs(5),
                )
                .await;
            }
        }

        // iOS: ensure the generated project.yml has the correct PATH for the build phase.
        // Tauri's xcode-script needs bun and cargo, but Xcode runs build phases with a
        // minimal PATH that doesn't include ~/.bun/bin or ~/.cargo/bin.
        // We patch project.yml transparently — survives tauri ios init overwrites.
        if let Some(crate::config::SessionPlatform::Ios) = &action.platform {
            self.patch_ios_project_yml_path().await;
        }

        let mut cmd = Command::new("sh");
        cmd.args(["-c", &action.command])
            .current_dir(&self.root_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);

        // The user's login-shell env (a desktop launch has a bare PATH), then env_files on top.
        cmd.envs(atlas_core::login_env::vars());
        cmd.envs(&self.file_env);

        // Inject env from atlas.local.yaml frontend service (Android SDK, etc.)
        for svc in self.services.values() {
            for (k, v) in &svc.env {
                cmd.env(k, v);
            }
        }

        // Inject APPLE_DEVELOPMENT_TEAM from mobile.ios.team_id — this is the
        // env var Tauri reads to set DEVELOPMENT_TEAM in the generated .xcodeproj.
        // Each dev sets their own team_id in atlas.local.yaml (never committed).
        if let Some(crate::config::SessionPlatform::Ios) = &action.platform
            && let Some(ios) = &self.config_mobile.ios
            && let Some(team_id) = &ios.team_id
        {
            cmd.env("APPLE_DEVELOPMENT_TEAM", team_id);
        }

        // host_mode: tell the dev server which host to advertise to the device.
        // Only for mobile (platform) sessions. A value the user already provides
        // (env_files, a service `env`, or the inherited environment) is kept.
        if action.platform.is_some() {
            let user_set = |var: &str| {
                self.file_env.contains_key(var)
                    || self.services.values().any(|s| s.env.contains_key(var))
                    || std::env::var_os(var).is_some()
            };
            let missing: Vec<&str> = crate::runtime::dev_host::ENV_VARS
                .iter()
                .copied()
                .filter(|v| !user_set(v))
                .collect();
            if !missing.is_empty() {
                let (mode, platform, device) = (
                    action.host_mode,
                    action.platform.clone(),
                    device_id_hint.clone(),
                );
                // route/ifconfig/xcrun are blocking subprocesses: off the runtime.
                let host = tokio::task::spawn_blocking(move || {
                    crate::runtime::dev_host::resolve_blocking(
                        mode,
                        platform.as_ref(),
                        device.as_deref(),
                    )
                })
                .await
                .unwrap_or_else(|_| crate::runtime::dev_host::LOOPBACK.to_string());
                for var in missing {
                    cmd.env(var, &host);
                }
            }
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = self.event_tx.send(ManagerEvent::SessionEnded {
                    label: label.clone(),
                    ok: false,
                });
                tracing::error!("session spawn failed: {e}");
                return None;
            }
        };

        let stdin = child.stdin.take();

        // stdout and stderr both stream log lines, with mobile phase detection
        // when platform is set (Gradle/xcode errors come on stderr).
        let pumps: Vec<_> =
            [
                child.stdout.take().map(|s| {
                    self.session_pump(s, &label, action.platform.clone(), LogStream::Stdout)
                }),
                child.stderr.take().map(|s| {
                    self.session_pump(s, &label, action.platform.clone(), LogStream::Stderr)
                }),
            ]
            .into_iter()
            .flatten()
            .collect();

        let _ = self.event_tx.send(ManagerEvent::SessionStarted {
            label: label.clone(),
        });

        self.sessions.insert(
            label.clone(),
            ManagedSession {
                child,
                stdin,
                resume: resume.clone(),
                pumps,
            },
        );

        Some(label)
    }

    fn session_pump(
        &self,
        stream: impl tokio::io::AsyncRead + Unpin + Send + 'static,
        label: &str,
        platform: Option<crate::config::SessionPlatform>,
        kind: LogStream,
    ) -> tokio::task::JoinHandle<()> {
        let tx = self.event_tx.clone();
        let tab = label.to_string();
        crate::runtime::logs::spawn_line_pump(stream, move |line| {
            if let Some(p) = &platform
                && let Some(phase) = detect_mobile_phase(&line, p)
            {
                let _ = tx.send(ManagerEvent::MobileSessionPhaseChanged {
                    label: tab.clone(),
                    phase,
                });
            }
            let _ = tx.send(ManagerEvent::LogLine {
                name: tab.clone(),
                line,
                stream: kind,
            });
        })
    }

    /// Collect sessions whose child exited on its own. `try_wait` reaps the
    /// zombie, so (unlike `kill(pid, 0)`) it sees the exit. Emits `SessionEnded`
    /// and resumes the services the session had paused.
    pub(crate) async fn reap_sessions(&mut self) {
        let exited: Vec<(String, bool)> = self
            .sessions
            .iter_mut()
            .filter_map(|(label, s)| match s.child.try_wait() {
                Ok(Some(status)) => Some((label.clone(), status.success())),
                _ => None,
            })
            .collect();
        for (label, ok) in exited {
            let Some(mut session) = self.sessions.remove(&label) else {
                continue;
            };
            // The child is gone: let the pumps flush buffered output, but a
            // background grandchild holding the pipes must not keep them alive.
            drain_pumps(std::mem::take(&mut session.pumps)).await;
            let _ = self.event_tx.send(ManagerEvent::SessionEnded { label, ok });
            for name in session.resume {
                let _ = self.event_tx.send(ManagerEvent::ActionCompleted {
                    label: String::new(),
                    ok: true,
                    resume: vec![name],
                });
            }
        }
    }

    /// Kill a running session by label and clean up.
    pub async fn stop_session(&mut self, label: &str) {
        if let Some(mut session) = self.sessions.remove(label) {
            // Drop stdin first so the process sees EOF
            drop(session.stdin.take());
            crate::runtime::process::kill_gracefully(
                &mut session.child,
                std::time::Duration::from_secs(2),
            )
            .await;
            let _ = session.child.kill().await;
            for pump in session.pumps.drain(..) {
                pump.abort();
            }
        }
    }

    /// Send input bytes to a named session's stdin.
    pub async fn send_session_input(&mut self, label: &str, bytes: &[u8]) {
        if let Some(session) = self.sessions.get_mut(label) {
            session.send_input(bytes).await;
        }
    }

    /// True if a session with the given label is active.
    pub fn session_active(&self, label: &str) -> bool {
        self.sessions.contains_key(label)
    }
}
