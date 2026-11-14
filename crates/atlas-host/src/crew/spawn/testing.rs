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
use super::server::{self, Config, Server};

/// The crewmate id the rig provisions, and its user.
pub const ID: &str = "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";
pub const USER: &str = "atlas-cm-3f2a9c1e5b7d";

fn me(_: &str) -> io::Result<Ids> {
    // SAFETY: plain getters.
    Ok(Ids {
        uid: unsafe { libc::geteuid() },
        gid: unsafe { libc::getegid() },
    })
}

pub struct Rig {
    pub dir: tempfile::TempDir,
    pub client: Client,
    pub server: Arc<Server>,
    /// `<root>/<slug>`.
    pub cm: PathBuf,
    pub engines: PathBuf,
}

impl Rig {
    /// The crewmate's checkout, which exists.
    pub fn work(&self) -> PathBuf {
        self.cm.join("work")
    }

    /// Install a fake engine `name`: a shell script run as the child.
    pub fn engine(&self, name: &str, script: &str) {
        let bin = self.engines.join(name);
        std::fs::write(&bin, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// The root the helper was configured with (`ATLAS_CREW_CM_ROOT` for code that reads it).
    pub fn root(&self) -> String {
        self.dir.path().join("cm").to_string_lossy().into_owned()
    }
}

pub fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let engines = dir.path().join("engines");
    let cm = dir.path().join("cm/3f2a9c1e5b7d");
    for d in ["work", "home", "crew-runtime"] {
        std::fs::create_dir_all(cm.join(d)).unwrap();
    }
    std::fs::create_dir_all(&engines).unwrap();
    let server = Server::new(Config {
        crew_uid: unsafe { libc::geteuid() },
        cm_root: dir.path().join("cm").to_string_lossy().into(),
        engines: vec![engines.to_string_lossy().into()],
        resolve: me,
    });
    let sock = dir.path().join("spawn.sock");
    let listener = server::bind(&sock).unwrap();
    let s2 = server.clone();
    std::thread::spawn(move || {
        let _ = s2.serve(listener);
    });
    Rig {
        client: Client::at(sock),
        server,
        cm,
        engines,
        dir,
    }
}
