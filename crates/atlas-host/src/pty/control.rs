//! Driving and inspecting a live session: input, resize, signals, kill,
//! scrollback maintenance, text/summary/cwd queries.

use std::io::Write;
use std::time::Duration;

use anyhow::{Result, anyhow};
use portable_pty::PtySize;
use serde_json::{Value, json};

use super::{Session, bin_path, meta_path, open_append};

impl Session {
    pub fn write(&self, data: &[u8]) -> Result<()> {
        let mut io = self.io.lock();
        let io = io.as_mut().ok_or_else(|| anyhow!("exited"))?;
        io.writer.write_all(data)?;
        io.writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        *self.size.lock() = (cols, rows);
        if let Some(master) = self.io.lock().as_ref().and_then(|io| io.master.as_ref()) {
            master.resize(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })?;
        }
        Ok(())
    }

    /// Signal the child (session leader). Exact PID only, never a group.
    #[cfg(unix)]
    pub fn signal(&self, sig: &str) -> Result<()> {
        let s = match sig {
            "TERM" => libc::SIGTERM,
            "KILL" => libc::SIGKILL,
            "INT" => libc::SIGINT,
            "HUP" => libc::SIGHUP,
            other => return Err(anyhow!("unknown signal {other}")),
        };
        if self.exited().is_some() {
            return Ok(());
        }
        let pid = self.pid.ok_or_else(|| anyhow!("no pid"))?;
        // SAFETY: plain kill(2) on a positive pid we spawned.
        if unsafe { libc::kill(pid as libc::pid_t, s) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }

    /// Windows has no signals: `INT` types Ctrl-C into the console (ConPTY turns it into a
    /// `CTRL_C_EVENT` for the foreground program); `TERM`, `KILL` and `HUP` end the shell and
    /// everything it started.
    #[cfg(windows)]
    pub fn signal(&self, sig: &str) -> Result<()> {
        if !matches!(sig, "TERM" | "KILL" | "INT" | "HUP") {
            return Err(anyhow!("unknown signal {sig}"));
        }
        if self.exited().is_some() {
            return Ok(());
        }
        if sig == "INT" {
            return self.write(b"\x03");
        }
        let pid = self.pid.ok_or_else(|| anyhow!("no pid"))?;
        atlas_core::sys::kill_tree(pid);
        Ok(())
    }

    /// Poll up to `ticks` × 100 ms for the child to exit.
    async fn wait_exit(&self, ticks: u32) {
        for _ in 0..ticks {
            if self.exited().is_some() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// TERM, wait up to 3 s, then KILL.
    pub async fn kill(&self) {
        if self.signal("TERM").is_err() {
            return;
        }
        self.wait_exit(30).await;
        if self.exited().is_some() {
            return;
        }
        let _ = self.signal("KILL");
        self.wait_exit(10).await;
    }

    pub fn remove_files(&self) {
        let _ = std::fs::remove_file(bin_path(&self.dir, &self.id));
        let _ = std::fs::remove_file(meta_path(&self.dir, &self.id));
        let _ = std::fs::remove_file(super::batch_path(&self.dir, &self.id));
    }

    pub fn clear(&self) {
        let mut b = self.buf.lock();
        b.ring.clear();
        b.file_len = 0;
        let p = bin_path(&self.dir, &self.id);
        let _ = std::fs::write(&p, b"");
        if b.file.is_some() {
            b.file = open_append(&p);
        }
    }

    /// Last `lines` lines of the ring, ANSI removed.
    pub fn read_text(&self, lines: u32) -> String {
        let raw = self.buf.lock().ring_bytes();
        let text = crate::strip_ansi(&String::from_utf8_lossy(&raw));
        let all: Vec<&str> = text.trim_end_matches('\n').lines().collect();
        let from = all.len().saturating_sub(lines as usize);
        all[from..].join("\n")
    }

    /// The program in the foreground: the newest process under the shell, or the shell itself
    /// (Windows has no terminal process groups).
    #[cfg(windows)]
    fn foreground(&self) -> Option<(i32, String)> {
        let pid = self.pid?;
        if self.exited().is_some() {
            return None;
        }
        let rows = atlas_core::sys::process_rows();
        let fg = atlas_core::sys::descendants(pid, &rows)
            .last()
            .copied()
            .unwrap_or(pid);
        let name = rows.iter().find(|r| r.pid == fg)?.exe.clone();
        let name = name
            .strip_suffix(".exe")
            .or_else(|| name.strip_suffix(".EXE"))
            .unwrap_or(&name)
            .to_string();
        Some((fg as i32, name))
    }

    #[cfg(unix)]
    fn foreground(&self) -> Option<(i32, String)> {
        let io = self.io.lock();
        let pid = io.as_ref()?.master.as_ref()?.process_group_leader()?;
        drop(io);
        let out = std::process::Command::new("ps")
            .args(["-o", "comm=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let name = name.rsplit('/').next().unwrap_or(&name).to_string();
        Some((pid, name))
    }

    pub fn summary(&self) -> Value {
        json!({
            "pty": self.id,
            "cwd": self.meta.cwd,
            "argv": self.meta.argv,
            "scope": self.meta.scope,
            "title": *self.title.lock(),
            "created_at": self.meta.created_at,
            "exited": self.exited().map(|c| json!({"code": c})),
            "pid": self.pid,
            "fg": self.foreground().map(|(pid, name)| json!({"pid": pid, "name": name})),
        })
    }

    /// Current working directory of the child (macOS/Linux via `lsof`). Windows does not expose
    /// another process's directory; the terminal learns it from the shell's OSC 7/9;9 instead.
    #[cfg(windows)]
    pub async fn cwd_now(&self) -> Option<String> {
        None
    }

    #[cfg(unix)]
    pub async fn cwd_now(&self) -> Option<String> {
        let pid = self.pid?;
        let fut = tokio::process::Command::new("lsof")
            .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
            .output();
        let out = tokio::time::timeout(Duration::from_secs(1), fut)
            .await
            .ok()?
            .ok()?;
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .find_map(|l| l.strip_prefix('n').map(str::to_string))
    }
}
