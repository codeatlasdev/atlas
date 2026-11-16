//! A real spawn helper for tests of the code that uses it (sessions, engines, the crew runner): a
//! [`Server`] on a temp socket, running as the current user, with fake engine binaries. Everything
//! between hostd's client and the child is the real code; only the user lookup is replaced (the
//! `atlas-cm-*` accounts do not exist on a developer's machine), see [`Config::resolve`].

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;

use super::client::Client;
use super::exec::Ids;
use super::layout::Setup;
use super::provision::{Host, PlanCfg, Service, Who};
use super::server::{self, Config, Server};

/// The user of the fixed crewmate the rig provisions (`3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b`).
pub const USER: &str = "atlas-cm-3f2a9c1e5b7d";

fn me(_: &str) -> io::Result<Ids> {
    // SAFETY: plain getters.
    Ok(Ids {
        uid: unsafe { libc::geteuid() },
        gid: unsafe { libc::getegid() },
    })
}

/// A provisioning host that touches nothing but a directory tree: it creates the crewmate's folders
/// under the rig's root (as the current user) and records the account operations a real host would
/// run. The only fake is the process boundary (`useradd`, `userdel`, `nft`).
pub struct TreeHost {
    pub log: Arc<parking_lot::Mutex<Vec<String>>>,
    /// Users that "exist" (created by `useradd`, removed by `userdel`).
    users: parking_lot::Mutex<std::collections::BTreeSet<String>>,
    /// Make the next `useradd` fail with this text (a full uid range, say).
    pub fail_useradd: Arc<parking_lot::Mutex<Option<String>>>,
}

impl Host for TreeHost {
    fn user_exists(&self, user: &str) -> bool {
        self.users.lock().contains(user)
    }
    fn run(&self, argv: &[&str]) -> io::Result<()> {
        self.log.lock().push(argv.join(" "));
        match argv[0] {
            "useradd" => {
                if let Some(why) = self.fail_useradd.lock().clone() {
                    return Err(io::Error::other(why));
                }
                self.users.lock().insert(argv[argv.len() - 1].to_string());
            }
            "userdel" => {
                self.users.lock().remove(argv[1]);
            }
            _ => {}
        }
        Ok(())
    }
    fn dir(&self, path: &std::path::Path, mode: u32, _: &Who, _: &str) -> io::Result<()> {
        std::fs::create_dir_all(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & 0o777))
    }
    fn file(&self, path: &std::path::Path, mode: u32, _: &Who, _: &str) -> io::Result<()> {
        if !path.exists() {
            std::fs::write(path, b"")?;
        }
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & 0o777))
    }
    fn kill_user(&self, user: &str) -> io::Result<()> {
        self.log.lock().push(format!("kill {user}"));
        Ok(())
    }
    fn remove_tree(&self, path: &std::path::Path) -> io::Result<()> {
        self.log.lock().push(format!("rm {}", path.display()));
        match std::fs::remove_dir_all(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            r => r,
        }
    }
    fn egress(&self) -> io::Result<()> {
        self.log.lock().push("egress".into());
        Ok(())
    }
}

/// A shell snippet that prints what the process running it looks like from the inside: `ls -l` of its open
/// descriptors (no pipe or substitution: the shell would hold one of its own while it lists), its `NoNewPrivs:` line and
/// its `/proc/self/cgroup`. Run as `exec sh -c '<PROBE>'`: the script's own descriptor for itself (dash keeps
/// one, not close-on-exec) is not the process's, and `$$` is that shell. Scripts that run it start with `#!/bin/bash`,
/// whose descriptor for the script is close-on-exec (dash's is fd 10 and is inherited; dash cannot close it either:
/// `10<&-` closes fd 0).
pub const PROBE: &str =
    r#"ls -l /proc/$$/fd; grep NoNewPrivs /proc/$$/status; cat /proc/$$/cgroup"#;

/// The descriptor numbers a [`PROBE`] saw open, in order (`... 3 -> socket:[123]` lines of `ls -l`).
pub fn probe_fds(probe: &str) -> Vec<u32> {
    probe
        .lines()
        .filter_map(|l| {
            let t: Vec<&str> = l.split_whitespace().collect();
            let arrow = t.iter().position(|w| *w == "->")?;
            t.get(arrow.checked_sub(1)?)?.parse().ok()
        })
        .collect()
}

/// A listening socket the way systemd's socket activation leaves it (close-on-exec cleared), adopted through
/// the code the helper runs at start ([`server::listener_from_fd`]), and the `socket:[inode]` text a [`PROBE`]
/// shows if a child holds it. Every other descriptor of the test process is first made close-on-exec, so a
/// descriptor a test runner leaked into the process cannot appear in a probe: what a probe shows is the helper's.
pub fn systemd_listener(dir: &std::path::Path) -> (std::os::unix::net::UnixListener, String) {
    use std::os::fd::{AsRawFd, OwnedFd};
    let fd =
        OwnedFd::from(std::os::unix::net::UnixListener::bind(dir.join("systemd.sock")).unwrap());
    if let Ok(fds) = std::fs::read_dir("/proc/self/fd") {
        for n in fds
            .flatten()
            .filter_map(|e| e.file_name().to_str()?.parse::<i32>().ok())
        {
            // SAFETY: plain fcntl; an fd that is not open (the directory's own, already closed) just fails.
            unsafe { libc::fcntl(n, libc::F_SETFD, libc::FD_CLOEXEC) };
        }
    }
    // SAFETY: plain fcntl on an fd we own: what systemd's socket activation leaves.
    unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, 0) };
    let socket = std::fs::read_link(format!("/proc/self/fd/{}", fd.as_raw_fd()))
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(socket.starts_with("socket:["), "{socket}");
    (server::listener_from_fd(fd), socket)
}

pub struct Rig {
    // Kept alive for the sockets and directories of the rig.
    _dir: tempfile::TempDir,
    pub client: Client,
    pub server: Arc<Server>,
    /// `<root>/<slug>`.
    pub cm: PathBuf,
    pub engines: PathBuf,
    /// What the provisioning service ran.
    pub prov_log: Arc<parking_lot::Mutex<Vec<String>>>,
    pub fail_useradd: Arc<parking_lot::Mutex<Option<String>>>,
    root: String,
    spawn_sock: PathBuf,
    prov_sock: PathBuf,
}

impl Rig {
    /// The checkout of the fixed crewmate [`ID`] (exists after [`Rig::provision`]).
    pub fn work(&self) -> PathBuf {
        self.cm.join("work")
    }

    /// Provision the fixed crewmate [`ID`] through the real provisioning service, so its directories exist.
    pub fn provision(&self) {
        super::provision::ProvClient::at(&self.prov_sock)
            .provision(USER)
            .unwrap();
    }

    /// What hostd is given to run its crewmates through this rig.
    pub fn setup(&self) -> Setup {
        Setup {
            socket: self.spawn_sock.clone(),
            provision_socket: self.prov_sock.clone(),
            root: self.root.clone(),
            hook_port: 0,
        }
    }

    /// Install a fake engine `name`: a shell script run as the child.
    pub fn engine(&self, name: &str, script: &str) {
        let bin = self.engines.join(name);
        std::fs::write(&bin, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

pub fn rig() -> Rig {
    rig_with(server::Memory::Off("tests run without a cgroup".into()))
}

/// [`rig`] whose helper reports `memory` (see [`server::Memory::Verdict`]).
pub fn rig_with(memory: server::Memory) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let engines = dir.path().join("engines");
    let cm = dir.path().join("cm/3f2a9c1e5b7d");
    std::fs::create_dir_all(&engines).unwrap();
    let server = Server::new(Config {
        crew_uid: unsafe { libc::geteuid() },
        cm_root: dir.path().join("cm").to_string_lossy().into(),
        engines: vec![engines.to_string_lossy().into()],
        resolve: me,
        memory: Arc::new(memory),
    });
    let sock = dir.path().join("spawn.sock");
    let listener = server::bind(&sock).unwrap();
    let s2 = server.clone();
    std::thread::spawn(move || {
        let _ = s2.serve(listener);
    });
    let root = dir.path().join("cm").to_string_lossy().into_owned();
    // The provisioning service: the real service loop over a tree host.
    let log = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let fail_useradd = Arc::new(parking_lot::Mutex::new(None));
    let host = TreeHost {
        log: log.clone(),
        users: Default::default(),
        fail_useradd: fail_useradd.clone(),
    };
    let prov = Service::new(
        PlanCfg {
            crew: "atlas-crew".into(),
            cm_root: root.clone(),
        },
        unsafe { libc::geteuid() },
        Box::new(host),
    );
    let prov_sock = dir.path().join("provision.sock");
    let pl = server::bind(&prov_sock).unwrap();
    std::thread::spawn(move || {
        let _ = prov.serve(pl);
    });
    let rig = Rig {
        client: Client::at(&sock),
        server,
        cm,
        engines,
        prov_log: log,
        fail_useradd,
        root,
        spawn_sock: sock,
        prov_sock,
        _dir: dir,
    };
    // The fixed crewmate exists like any other: through the provisioning service.
    rig.provision();
    rig.prov_log.lock().clear();
    rig
}

impl Rig {
    /// A git repository in the checkout whose **own config** runs a command: `core.fsmonitor` appends
    /// `$USER` to the returned marker file. That is what a crewmate can do to the git that hostd runs in its
    /// tree (security review of #448, B2). The helper starts git as the crewmate with `USER` set to the
    /// crewmate's user; hostd's own git would leave the test process's `USER` (or none) there. A marker line
    /// that is not [`USER`] means git ran as someone else; no marker at all means nothing triggered it.
    pub fn booby_trapped_repo(&self) -> PathBuf {
        let marker = self.cm.join("marker");
        self.planted_repo(&format!(
            "printf '%s\\n' \"$USER\" >> {}\nexit 1\n",
            marker.display()
        ));
        marker
    }

    /// A repository in the checkout whose `core.fsmonitor` is `#!/bin/sh` + `hook_body`, which git runs on
    /// every `status` as whoever runs git: the way a test sees what a crewmate's git process is like from the
    /// inside (its descriptors, `no_new_privs`, cgroup).
    pub fn planted_repo(&self, hook_body: &str) {
        let work = self.work();
        let hook = self.cm.join("fsmonitor.sh");
        std::fs::write(&hook, format!("#!/bin/bash\n{hook_body}")).unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        let git = |args: &[&str]| {
            let o = std::process::Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(args)
                .current_dir(&work)
                .env_remove("GIT_DIR")
                .output()
                .unwrap();
            assert!(
                o.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&o.stderr)
            );
        };
        git(&["init", "-q"]);
        std::fs::write(work.join("a.txt"), "one\n").unwrap();
        git(&["add", "a.txt"]);
        git(&["commit", "-q", "-m", "one"]);
        // Set only now, so the setup above does not trigger it.
        git(&["config", "core.fsmonitor", &hook.to_string_lossy()]);
        std::fs::write(work.join("a.txt"), "two\n").unwrap();
        std::fs::write(work.join("new.txt"), "new\n").unwrap();
    }

    /// A planted repo whose command writes, to the returned file, what its own process looks like: [`PROBE`].
    #[cfg(target_os = "linux")]
    pub fn probe_repo(&self) -> PathBuf {
        let out = self.cm.join("probe");
        self.planted_repo(&format!("exec bash -c '{PROBE}' > {}\n", out.display()));
        out
    }

    /// Hooks, in the crewmate's own `~/.gitconfig`, that record the environment of every git process that runs
    /// them (clone's `post-checkout`, a fetch's `reference-transaction`, a pull's `post-merge`): what a crewmate's
    /// code sees of the git hostd asks for. Returns the file they append to.
    pub fn env_recording_hooks(&self) -> PathBuf {
        let log = self.cm.join("env.log");
        let hooks = self.cm.join("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        for name in ["post-checkout", "reference-transaction", "post-merge"] {
            let h = hooks.join(name);
            std::fs::write(&h, format!("#!/bin/sh\nenv >> {}\n", log.display())).unwrap();
            std::fs::set_permissions(&h, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::write(
            self.cm.join("home/.gitconfig"),
            format!("[core]\n\thooksPath = {}\n", hooks.display()),
        )
        .unwrap();
        log
    }

    /// What the marker holds: one `$USER` per run of the planted command.
    pub fn marker_users(marker: &std::path::Path) -> Vec<String> {
        std::fs::read_to_string(marker)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}
