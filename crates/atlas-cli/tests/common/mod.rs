//! Helpers shared by the end-to-end test binaries (`mod common;`). Each test binary uses
//! a subset, so unused items are expected.
#![allow(dead_code)]

use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn python_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Skip (loudly) when python3 is missing: the fixture services need it.
#[macro_export]
macro_rules! require_python {
    () => {
        if !$crate::common::python_available() {
            eprintln!("SKIP {}: python3 not available", module_path!());
            return;
        }
    };
}

/// Bind :0, read the port, release it.
pub fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// A local port of one test, claimed so that no other test can hold it and no connection of the machine can
/// collide with it. Dropping the lease frees the claim.
pub struct PortLease {
    port: u16,
    claim: std::path::PathBuf,
}

impl PortLease {
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for PortLease {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.claim);
    }
}

/// A port for a fixture that a CHILD process binds later (a fake ssh, a service atlas starts), so the test cannot
/// keep a listener on it. [`free_port`] cannot give that: it binds `:0` and lets go, which leaves a number from the
/// kernel's ephemeral range that any other test (4000 run side by side under nextest) may be handed in the same
/// instant, and that every outgoing connection on the machine picks its source port from. A probe that finds a
/// listener there is probing someone else's (#496: `enabled_tunnel_starts_at_boot` saw the status say `running` and
/// the port closed). This one comes from a range no OS hands out (20000-29999: Linux starts at 32768, macOS and
/// Windows at 49152), and one file per port in the temp dir says which process holds it, so two tests never get the
/// same one. A claim whose process is gone is taken over.
pub fn reserve_port() -> PortLease {
    const FIRST: u16 = 20_000;
    const COUNT: u16 = 10_000;
    let dir = std::env::temp_dir().join("atlas-test-ports");
    std::fs::create_dir_all(&dir).unwrap();
    let start = (std::process::id() as u64)
        .wrapping_mul(2_654_435_761)
        .wrapping_add(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos() as u64,
        );
    for i in 0..u64::from(COUNT) {
        let port = FIRST + (start.wrapping_add(i) % u64::from(COUNT)) as u16;
        let claim = dir.join(port.to_string());
        let claimed = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&claim)
            .map(|mut f| {
                use std::io::Write;
                let _ = write!(f, "{}", std::process::id());
            })
            .is_ok();
        if !claimed {
            // Held by a process that is gone (a test killed before it could drop its lease): take it over. A claim
            // without a pid yet is a creator between `create_new` and its write: held.
            let holder = std::fs::read_to_string(&claim)
                .ok()
                .and_then(|t| t.trim().parse::<u32>().ok());
            if !holder.is_some_and(|p| !atlas_core::sys::pid_alive(p)) {
                continue;
            }
            let _ = std::fs::remove_file(&claim);
            if std::fs::write(&claim, std::process::id().to_string()).is_err() {
                continue;
            }
        }
        // Nothing may listen on it now (software outside the tests can use this range too).
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return PortLease { port, claim };
        }
        let _ = std::fs::remove_file(&claim);
    }
    panic!("no free port in {FIRST}..{}", FIRST + COUNT);
}

/// Poll `f` until it returns Some, or panic with `what` after `timeout`.
pub fn wait_for<T>(what: &str, timeout: Duration, f: impl FnMut() -> Option<T>) -> T {
    wait_for_with(what, timeout, f, String::new)
}

/// Like [`wait_for`], but the timeout panic also carries `diagnose()`: why the condition never
/// held (instance stdout, status, service logs). A CI timeout is otherwise a one-line mystery.
pub fn wait_for_with<T>(
    what: &str,
    timeout: Duration,
    mut f: impl FnMut() -> Option<T>,
    diagnose: impl FnOnce() -> String,
) -> T {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(v) = f() {
            return v;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for {what}\n{}", diagnose());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

unsafe extern "C" {
    #[link_name = "kill"]
    pub fn libc_kill(pid: i32, sig: i32) -> i32;
}
