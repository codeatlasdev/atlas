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
