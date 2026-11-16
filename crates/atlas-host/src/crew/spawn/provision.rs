//! The second root service (`atlas crew-spawn --provision`): the one thing that creates and removes
//! a crewmate's Linux user, and keeps the egress rules in step with the users that exist.
//!
//! It is a separate process from the spawn helper on purpose. Starting a process as a crewmate needs
//! five capabilities and a read-only filesystem; creating an account rewrites `/etc/passwd`. Giving
//! the spawn helper that would turn a bug in its (hostile-input-facing) spawn path into "write any
//! system file", so the account work lives here, with a protocol that has two operations and no
//! free-form text: a user of the exact `atlas-cm-<12 hex>` shape, and a flag.
//!
//! A request becomes a [`Step`] list ([`provision_plan`], [`deprovision_plan`]: pure, tested without
//! root), and a [`Host`] runs the steps (the real one calls `useradd`, `userdel`, `chown`, `nft`).

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use super::peer;
use super::validate::{CM_ROOT, Refused, USER_PREFIX, slug_of};
use super::wire;

/// What hostd may ask the provisioning service for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ProvRequest {
    /// Create the user and its directories (idempotent: running it again changes nothing).
    Provision { user: String },
    /// Remove the user, its processes and its group. `purge` also deletes its directory.
    Deprovision { user: String, purge: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProvResponse {
    Done,
    Error { reason: String },
}

/// Who owns a path: a crewmate user, or the crew user that runs hostd.
///
/// The group is what lets hostd (the crew user, whose groups are fixed when its process starts) reach a
/// crewmate's files: a group made for a crewmate after hostd started can never be one of hostd's, so what hostd
/// must reach is in the crew user's own primary group instead (found by the real-systemd end-to-end test).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Who {
    /// Owned by root, in root's group: a directory the crewmate must be able to enter but never change.
    Root,
    /// Owned by the crewmate, in the crew user's group: the crewmate uses it and hostd reaches it.
    Crewmate,
    /// Owned and grouped by the crewmate alone: nobody else, hostd included, needs it.
    CrewmatePrivate,
    /// Owned by the crew user (hostd), in the crewmate's group so the agent can read it.
    Crew,
}

/// One action of a plan, in the order it must run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// `useradd` unless the user exists.
    CreateUser { user: String, dir: PathBuf },
    /// A directory with this mode and owner (and the group [`Who`] says).
    Dir {
        path: PathBuf,
        mode: u32,
        owner: Who,
    },
    /// An empty file with this mode if it does not exist yet.
    File {
        path: PathBuf,
        mode: u32,
        owner: Who,
    },
    /// SIGKILL every process of the user (`userdel` refuses a user with processes).
    KillProcesses { user: String },
    /// `userdel` (and its group) when the user exists.
    DeleteUser { user: String },
    /// Delete the crewmate's directory.
    RemoveTree { path: PathBuf },
    /// Rewrite the egress rules from the users that exist now.
    Egress,
}

/// What a plan needs to know about the server.
#[derive(Debug, Clone)]
pub struct PlanCfg {
    /// The crew user (hostd's account).
    pub crew: String,
    pub cm_root: String,
}

/// Create a crewmate's account and directories. The layout is the one `layout::Isolated` names:
///
/// ```text
/// <dir>/              root:root           0711   the crewmate (and hostd) only traverse it; nobody can replace what is in it
/// <dir>/home          crewmate:crewmate   0700   $HOME: the engines' sign-ins
/// <dir>/work          crewmate:crew-group 2770   setgid: hostd (group) writes, files keep the group
/// <dir>/crew-runtime  crew:crewmate       0750   hostd writes what the agent reads; the agent cannot change it
/// <dir>/memory.md     crewmate:crew-group 0660   the agent appends, hostd edits
/// ```
pub fn provision_plan(cfg: &PlanCfg, user: &str) -> Result<Vec<Step>, Refused> {
    let slug = slug_of(user)?;
    let dir = Path::new(&cfg.cm_root).join(slug);
    Ok(vec![
        Step::CreateUser {
            user: user.to_string(),
            dir: dir.clone(),
        },
        Step::Dir {
            path: dir.clone(),
            mode: 0o711,
            owner: Who::Root,
        },
        Step::Dir {
            path: dir.join("home"),
            mode: 0o700,
            owner: Who::CrewmatePrivate,
        },
        Step::Dir {
            path: dir.join("work"),
            mode: 0o2770,
            owner: Who::Crewmate,
        },
        Step::Dir {
            path: dir.join("crew-runtime"),
            mode: 0o750,
            owner: Who::Crew,
        },
        Step::File {
            path: dir.join("memory.md"),
            mode: 0o660,
            owner: Who::Crewmate,
        },
        Step::Egress,
    ])
}

/// Remove a crewmate's account. Processes first, the account second, the files only on `purge`
/// (without it they stay, owned by a uid that is about to be reusable: the caller says so).
pub fn deprovision_plan(cfg: &PlanCfg, user: &str, purge: bool) -> Result<Vec<Step>, Refused> {
    let slug = slug_of(user)?;
    let mut steps = vec![
        Step::KillProcesses {
            user: user.to_string(),
        },
        Step::DeleteUser {
            user: user.to_string(),
        },
    ];
    if purge {
        steps.push(Step::RemoveTree {
            path: Path::new(&cfg.cm_root).join(slug),
        });
    }
    steps.push(Step::Egress);
    Ok(steps)
}

/// The process and filesystem boundary: the real [`System`] touches the machine, a test's recorder
/// does not.
pub trait Host {
    fn user_exists(&self, user: &str) -> bool;
    fn run(&self, argv: &[&str]) -> io::Result<()>;
    fn dir(&self, path: &Path, mode: u32, owner: &Who, crewmate: &str) -> io::Result<()>;
    fn file(&self, path: &Path, mode: u32, owner: &Who, crewmate: &str) -> io::Result<()>;
    fn kill_user(&self, user: &str) -> io::Result<()>;
    fn remove_tree(&self, path: &Path) -> io::Result<()>;
    fn egress(&self) -> io::Result<()>;
}

/// Run `steps` in order; the first failure stops the plan and is returned with the step named.
pub fn execute(host: &dyn Host, cfg: &PlanCfg, crewmate: &str, steps: &[Step]) -> io::Result<()> {
    for step in steps {
        run_step(host, cfg, crewmate, step)
            .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", describe(step))))?;
    }
    Ok(())
}

fn run_step(host: &dyn Host, cfg: &PlanCfg, crewmate: &str, step: &Step) -> io::Result<()> {
    match step {
        Step::CreateUser { user, dir } => {
            if host.user_exists(user) {
                return Ok(());
            }
            // The parent of the crewmates' directories exists before any of them (`atlas crew
            // install`); `--home-dir` is only recorded, never created.
            host.run(&[
                "useradd",
                "--system",
                "--user-group",
                "--no-create-home",
                "--home-dir",
                &dir.join("home").to_string_lossy(),
                "--shell",
                "/usr/sbin/nologin",
                user,
            ])
        }
        Step::Dir { path, mode, owner } => host.dir(path, *mode, owner, crewmate),
        Step::File { path, mode, owner } => host.file(path, *mode, owner, crewmate),
        Step::KillProcesses { user } => {
            if host.user_exists(user) {
                host.kill_user(user)?;
            }
            Ok(())
        }
        Step::DeleteUser { user } => {
            if host.user_exists(user) {
                host.run(&["userdel", user])?;
            }
            Ok(())
        }
        Step::RemoveTree { path } => {
            // Only ever a crewmate directory directly under the root.
            if path.parent() != Some(Path::new(&cfg.cm_root)) {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "not a crewmate directory",
                ));
            }
            host.remove_tree(path)
        }
        Step::Egress => host.egress(),
    }
}

fn describe(step: &Step) -> String {
    match step {
        Step::CreateUser { user, .. } => format!("creating user {user}"),
        Step::Dir { path, .. } => format!("preparing {}", path.display()),
        Step::File { path, .. } => format!("preparing {}", path.display()),
        Step::KillProcesses { user } => format!("ending the processes of {user}"),
        Step::DeleteUser { user } => format!("removing user {user}"),
        Step::RemoveTree { path } => format!("deleting {}", path.display()),
        Step::Egress => "updating the egress rules".to_string(),
    }
}

// ---- egress -----------------------------------------------------------------------------------

pub use super::layout::{EGRESS_FILE, HOOK_PORT};

/// What one crewmate's processes may reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// TCP ports open to the internet (git over ssh and https, package registries, model APIs).
    pub tcp_ports: Vec<u16>,
    /// Extra destinations (`10.1.2.3`, `10.0.5.0/24`) allowed on top, e.g. a private model gateway.
    pub allow: Vec<String>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            tcp_ports: vec![22, 80, 443],
            allow: vec![],
        }
    }
}

/// Destinations a crewmate never reaches: the machine's own network, the cloud metadata service and
/// every private range (another server in the VPC is exactly what a prompt-injected agent would try).
pub(crate) const NEVER: [&str; 5] = [
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "169.254.0.0/16",
    "100.64.0.0/10",
];
/// `64:ff9b::/96` is NAT64: on an IPv6-only network (DNS64) `64:ff9b::a00:1` reaches `10.0.0.1`, so it is as private as
/// the IPv4 ranges it carries (security review 5). `64:ff9b:1::/48` is the local-use NAT64 prefix (RFC 8215): a
/// network may translate any IPv4 address through it too (review 6). A crewmate on such a network cannot reach
/// IPv4-only hosts.
pub(crate) const NEVER6: [&str; 4] = ["fc00::/7", "fe80::/10", "64:ff9b::/96", "64:ff9b:1::/48"];

/// The nftables ruleset: a table that is replaced as a whole (atomic: `nft -f` applies the file or
/// none of it) with one chain per crewmate user, selected by `meta skuid`.
///
/// Per user, in order: replies to what it started, DNS (resolvers may be private), the hook port on
/// loopback, its own `allow` list, a drop for loopback and every private range, then its open ports,
/// then drop. Users the rule names must exist when it loads, so [`System::egress`] builds it from
/// the passwd database, never from a stale list.
pub fn egress_ruleset(users: &[(String, Policy)]) -> String {
    let mut out = String::from(
        "table inet atlas_crew\ndelete table inet atlas_crew\ntable inet atlas_crew {\n",
    );
    for (user, policy) in users {
        let chain = chain_name(user);
        out.push_str(&format!("  chain {chain} {{\n"));
        out.push_str("    ct state established,related accept\n");
        out.push_str("    udp dport 53 accept\n    tcp dport 53 accept\n");
        out.push_str(&format!(
            "    ip daddr 127.0.0.1 tcp dport {HOOK_PORT} accept\n"
        ));
        for dest in &policy.allow {
            let fam = if dest.contains(':') { "ip6" } else { "ip" };
            out.push_str(&format!("    {fam} daddr {dest} accept\n"));
        }
        out.push_str("    ip daddr 127.0.0.0/8 drop\n    ip6 daddr ::1 drop\n");
        out.push_str(&format!("    ip daddr {{ {} }} drop\n", NEVER.join(", ")));
        out.push_str(&format!("    ip6 daddr {{ {} }} drop\n", NEVER6.join(", ")));
        let ports: Vec<String> = policy.tcp_ports.iter().map(u16::to_string).collect();
        if !ports.is_empty() {
            out.push_str(&format!(
                "    tcp dport {{ {} }} accept\n",
                ports.join(", ")
            ));
        }
        out.push_str("    drop\n  }\n");
    }
    out.push_str("  chain output {\n    type filter hook output priority 0; policy accept;\n");
    if !users.is_empty() {
        let map: Vec<String> = users
            .iter()
            .map(|(u, _)| format!("\"{u}\" : jump {}", chain_name(u)))
            .collect();
        out.push_str(&format!("    meta skuid vmap {{ {} }}\n", map.join(", ")));
    }
    out.push_str("  }\n}\n");
    out
}

fn chain_name(user: &str) -> String {
    format!("cm_{}", user.trim_start_matches(USER_PREFIX))
}

// ---- the real host ----------------------------------------------------------------------------

/// The machine: `useradd`, `userdel`, `chown`, `nft`. Root only.
pub struct System {
    pub cfg: PlanCfg,
    /// Where `useradd` and friends are looked up (root's `PATH` is not trusted).
    pub sbin: Vec<String>,
}

impl System {
    pub fn new(cfg: PlanCfg) -> Self {
        Self {
            cfg,
            sbin: vec![
                "/usr/sbin".into(),
                "/sbin".into(),
                "/usr/bin".into(),
                "/bin".into(),
            ],
        }
    }

    fn bin(&self, name: &str) -> io::Result<String> {
        self.sbin
            .iter()
            .map(|d| format!("{d}/{name}"))
            .find(|p| Path::new(p).is_file())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{name} not found")))
    }
}

/// uid and gid of `name`, a crewmate user or the crew user.
fn ids_of(name: &str) -> io::Result<super::exec::Ids> {
    super::exec::lookup(name)
}

impl Host for System {
    fn user_exists(&self, user: &str) -> bool {
        ids_of(user).is_ok()
    }

    fn run(&self, argv: &[&str]) -> io::Result<()> {
        let st = std::process::Command::new(self.bin(argv[0])?)
            .args(&argv[1..])
            .env_clear()
            .stdin(std::process::Stdio::null())
            .output()?;
        if st.status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "{} failed: {}",
                argv[0],
                String::from_utf8_lossy(&st.stderr).trim()
            )))
        }
    }

    fn dir(&self, path: &Path, mode: u32, owner: &Who, crewmate: &str) -> io::Result<()> {
        ensure(path, Kind::Dir, mode, &self.owner_ids(owner, crewmate)?)
    }

    fn file(&self, path: &Path, mode: u32, owner: &Who, crewmate: &str) -> io::Result<()> {
        ensure(path, Kind::File, mode, &self.owner_ids(owner, crewmate)?)
    }

    fn kill_user(&self, user: &str) -> io::Result<()> {
        super::server::kill_uid(ids_of(user)?.uid).map(|_| ())
    }

    fn remove_tree(&self, path: &Path) -> io::Result<()> {
        match std::fs::remove_dir_all(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            r => r,
        }
    }

    fn egress(&self) -> io::Result<()> {
        let users = crewmate_users(&self.cfg)?;
        let rules = egress_ruleset(
            &users
                .into_iter()
                .map(|u| (u, Policy::default()))
                .collect::<Vec<_>>(),
        );
        atlas_core::fs::write_atomic(
            Path::new(EGRESS_FILE),
            rules.as_bytes(),
            atlas_core::fs::Mode::Default,
        )?;
        // `nft -c` first: a broken file must not replace working rules.
        let nft = self.bin("nft")?;
        let check: &[&str] = &["-c", "-f", EGRESS_FILE];
        let apply: &[&str] = &["-f", EGRESS_FILE];
        for args in [check, apply] {
            let out = std::process::Command::new(&nft)
                .args(args)
                .stdin(std::process::Stdio::null())
                .output()?;
            if !out.status.success() {
                return Err(io::Error::other(format!(
                    "nft {}: {}",
                    args.join(" "),
                    String::from_utf8_lossy(&out.stderr).trim()
                )));
            }
        }
        Ok(())
    }
}

impl System {
    fn owner_ids(&self, owner: &Who, crewmate: &str) -> io::Result<(u32, u32)> {
        let cm = ids_of(crewmate)?;
        let crew = ids_of(&self.cfg.crew)?;
        Ok(match owner {
            Who::Root => (0, 0),
            Who::Crewmate => (cm.uid, crew.gid),
            Who::CrewmatePrivate => (cm.uid, cm.gid),
            // The crew user owns it; the group stays the crewmate's, so the agent can read it.
            Who::Crew => (crew.uid, cm.gid),
        })
    }
}

/// What an entry of a crewmate's directory must be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Dir,
    File,
}

/// Create `path` if it is missing, then [`apply`] owner and mode to it.
fn ensure(path: &Path, kind: Kind, mode: u32, ids: &(u32, u32)) -> io::Result<()> {
    let (parent, name) = split(path)?;
    let parent = open_dir(parent)?;
    // SAFETY: valid fd and NUL-terminated name; 0700/0600 until `apply` sets the real mode.
    let made = unsafe {
        match kind {
            Kind::Dir => libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700),
            Kind::File => {
                let fd = libc::openat(
                    parent.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_CREAT
                        | libc::O_EXCL
                        | libc::O_WRONLY
                        | libc::O_NOFOLLOW
                        | libc::O_CLOEXEC,
                    0o600,
                );
                if fd >= 0 {
                    libc::close(fd);
                    0
                } else {
                    -1
                }
            }
        }
    };
    if made != 0 && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists {
        return Err(io::Error::last_os_error());
    }
    apply_at(&parent, &name, path, kind, mode, ids)
}

/// Give `path` this owner and mode, **without following a link**. The crewmate controls what is in
/// its directory (and, before [`Who::Root`], the directory itself), and this runs as root: a plain
/// `chown(path)` or `chmod(path)` on `memory.md -> /etc/passwd` would change `/etc/passwd`. The entry
/// is opened relative to its parent with `O_NOFOLLOW`, its type is checked on the open fd, and
/// `fchown`/`fchmod` act on that fd, so nothing can be swapped between the check and the change.
fn apply_at(
    parent: &OwnedFd,
    name: &std::ffi::CStr,
    path: &Path,
    kind: Kind,
    mode: u32,
    (uid, gid): &(u32, u32),
) -> io::Result<()> {
    // `O_NONBLOCK`: a FIFO planted here must not hang the (serial) service on open.
    // SAFETY: valid fd and NUL-terminated name.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        let e = io::Error::last_os_error();
        return Err(if e.raw_os_error() == Some(libc::ELOOP) {
            refuse(path, "is a symbolic link")
        } else {
            e
        });
    }
    // SAFETY: openat returned a fresh fd we own.
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    // SAFETY: zeroed stat is a valid out-parameter.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: valid fd and out-pointer.
    if unsafe { libc::fstat(fd.as_raw_fd(), &mut st) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let (want, label) = match kind {
        Kind::Dir => (libc::S_IFDIR, "is not a directory"),
        Kind::File => (libc::S_IFREG, "is not a regular file"),
    };
    if st.st_mode & libc::S_IFMT != want {
        return Err(refuse(path, label));
    }
    // A second name for a file is a way to reach something else with the same change.
    if kind == Kind::File && st.st_nlink > 1 {
        return Err(refuse(path, "has more than one hard link"));
    }
    // `chown` first: it clears the setuid and setgid bits that the `chmod` after it sets.
    // SAFETY: valid fd.
    if unsafe { libc::fchown(fd.as_raw_fd(), *uid, *gid) } != 0
        || unsafe { libc::fchmod(fd.as_raw_fd(), mode as libc::mode_t) } != 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn refuse(path: &Path, why: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("{} {why}: refusing to change it", path.display()),
    )
}

fn split(path: &Path) -> io::Result<(&Path, std::ffi::CString)> {
    let bad = || io::Error::new(io::ErrorKind::InvalidInput, "not a path of an entry");
    let name = path.file_name().ok_or_else(bad)?;
    let name = std::ffi::CString::new(name.as_encoded_bytes()).map_err(|_| bad())?;
    Ok((path.parent().ok_or_else(bad)?, name))
}

/// A directory by path, never through a link at its last component.
fn open_dir(path: &Path) -> io::Result<OwnedFd> {
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in path"))?;
    // SAFETY: valid NUL-terminated path.
    let fd = unsafe {
        libc::open(
            c.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: open returned a fresh fd we own.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

/// The crewmate users that exist, from the passwd database.
fn crewmate_users(_cfg: &PlanCfg) -> io::Result<Vec<String>> {
    let passwd = std::fs::read_to_string("/etc/passwd")?;
    Ok(crewmate_users_in(&passwd))
}

/// `atlas-cm-<12 hex>` account names in the text of a passwd file, sorted.
pub fn crewmate_users_in(passwd: &str) -> Vec<String> {
    let mut v: Vec<String> = passwd
        .lines()
        .filter_map(|l| l.split(':').next())
        .filter(|n| slug_of(n).is_ok())
        .map(str::to_string)
        .collect();
    v.sort();
    v
}

// ---- the service ------------------------------------------------------------------------------

/// The provisioning service's loop. Sequential: account changes are rare, and two `useradd`s at once
/// would fight over `/etc/passwd`'s lock anyway.
pub struct Service {
    pub cfg: PlanCfg,
    pub crew_uid: u32,
    host: Box<dyn Host + Send + Sync>,
    serial: Mutex<()>,
}

impl Service {
    pub fn new(cfg: PlanCfg, crew_uid: u32, host: Box<dyn Host + Send + Sync>) -> Arc<Self> {
        Arc::new(Self {
            cfg,
            crew_uid,
            host,
            serial: Mutex::new(()),
        })
    }

    /// The production service for crew user `crew` (uid `crew_uid`).
    pub fn production(crew: &str, crew_uid: u32) -> Arc<Self> {
        let cfg = PlanCfg {
            crew: crew.to_string(),
            cm_root: CM_ROOT.to_string(),
        };
        Self::new(cfg.clone(), crew_uid, Box::new(System::new(cfg)))
    }

    pub fn serve(self: &Arc<Self>, listener: UnixListener) -> io::Result<()> {
        for conn in listener.incoming() {
            let conn = conn?;
            let me = self.clone();
            std::thread::spawn(move || {
                if let Err(e) = me.connection(conn) {
                    eprintln!("atlas crew-spawn --provision: connection ended: {e}");
                }
            });
        }
        Ok(())
    }

    fn connection(&self, conn: UnixStream) -> io::Result<()> {
        let who = peer::peer_of(&conn)?;
        if !peer::allowed(who, self.crew_uid) {
            return Ok(());
        }
        while let Some(f) = wire::recv::<ProvRequest>(&conn)? {
            let resp = self.handle(f.msg);
            wire::send(&conn, &resp, &[])?;
        }
        Ok(())
    }

    pub fn handle(&self, req: ProvRequest) -> ProvResponse {
        let _one_at_a_time = self.serial.lock();
        let (user, plan) = match &req {
            ProvRequest::Provision { user } => (user, provision_plan(&self.cfg, user)),
            ProvRequest::Deprovision { user, purge } => {
                (user, deprovision_plan(&self.cfg, user, *purge))
            }
        };
        let steps = match plan {
            Ok(s) => s,
            Err(r) => {
                return ProvResponse::Error {
                    reason: r.to_string(),
                };
            }
        };
        match execute(self.host.as_ref(), &self.cfg, user, &steps) {
            Ok(()) => ProvResponse::Done,
            Err(e) => ProvResponse::Error {
                reason: e.to_string(),
            },
        }
    }
}

/// hostd's side: ask the provisioning service. One connection per call.
#[derive(Debug, Clone)]
pub struct ProvClient {
    path: PathBuf,
}

impl ProvClient {
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn call(&self, req: &ProvRequest) -> io::Result<()> {
        let conn = UnixStream::connect(&self.path)?;
        // `useradd` and `nft` are quick; a minute is a hung service.
        let t = Some(std::time::Duration::from_secs(60));
        conn.set_read_timeout(t)?;
        wire::send(&conn, req, &[])?;
        match wire::recv::<ProvResponse>(&conn)? {
            Some(f) => match f.msg {
                ProvResponse::Done => Ok(()),
                ProvResponse::Error { reason } => Err(io::Error::other(reason)),
            },
            None => Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "the provisioning service closed the connection (is this user allowed to use it?)",
            )),
        }
    }

    pub fn provision(&self, user: &str) -> io::Result<()> {
        self.call(&ProvRequest::Provision {
            user: user.to_string(),
        })
    }

    pub fn deprovision(&self, user: &str, purge: bool) -> io::Result<()> {
        self.call(&ProvRequest::Deprovision {
            user: user.to_string(),
            purge,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const USER: &str = "atlas-cm-3f2a9c1e5b7d";

    fn cfg() -> PlanCfg {
        PlanCfg {
            crew: "atlas-crew".into(),
            cm_root: "/var/lib/atlas-cm".into(),
        }
    }

    // Records what a plan would do; `exists` says which users are already there.
    #[derive(Default)]
    struct Rec {
        log: Mutex<Vec<String>>,
        exists: bool,
        fail_on: Option<&'static str>,
    }
    impl Host for Rec {
        fn user_exists(&self, _: &str) -> bool {
            self.exists
        }
        fn run(&self, argv: &[&str]) -> io::Result<()> {
            self.log.lock().push(argv.join(" "));
            if self.fail_on == Some(argv[0]) {
                return Err(io::Error::other("boom"));
            }
            Ok(())
        }
        fn dir(&self, p: &Path, m: u32, o: &Who, _: &str) -> io::Result<()> {
            self.log
                .lock()
                .push(format!("dir {} {m:o} {o:?}", p.display()));
            Ok(())
        }
        fn file(&self, p: &Path, m: u32, o: &Who, _: &str) -> io::Result<()> {
            self.log
                .lock()
                .push(format!("file {} {m:o} {o:?}", p.display()));
            Ok(())
        }
        fn kill_user(&self, u: &str) -> io::Result<()> {
            self.log.lock().push(format!("kill {u}"));
            Ok(())
        }
        fn remove_tree(&self, p: &Path) -> io::Result<()> {
            self.log.lock().push(format!("rm {}", p.display()));
            Ok(())
        }
        fn egress(&self) -> io::Result<()> {
            self.log.lock().push("egress".into());
            Ok(())
        }
    }

    fn run_plan(rec: &Rec, steps: &[Step]) -> io::Result<()> {
        execute(rec, &cfg(), USER, steps)
    }

    #[test]
    fn provisioning_creates_the_user_then_its_directories_with_the_documented_modes() {
        let rec = Rec::default();
        run_plan(&rec, &provision_plan(&cfg(), USER).unwrap()).unwrap();
        let log = rec.log.lock().clone();
        assert_eq!(
            log,
            [
                "useradd --system --user-group --no-create-home --home-dir /var/lib/atlas-cm/3f2a9c1e5b7d/home --shell /usr/sbin/nologin atlas-cm-3f2a9c1e5b7d",
                "dir /var/lib/atlas-cm/3f2a9c1e5b7d 711 Root",
                "dir /var/lib/atlas-cm/3f2a9c1e5b7d/home 700 CrewmatePrivate",
                "dir /var/lib/atlas-cm/3f2a9c1e5b7d/work 2770 Crewmate",
                "dir /var/lib/atlas-cm/3f2a9c1e5b7d/crew-runtime 750 Crew",
                "file /var/lib/atlas-cm/3f2a9c1e5b7d/memory.md 660 Crewmate",
                "egress",
            ]
        );
    }

    // Idempotent: provisioning a user that exists must not run useradd again (it would fail),
    // and still repairs directories and modes.
    #[test]
    fn provisioning_an_existing_user_skips_useradd_and_still_repairs_the_directories() {
        let rec = Rec {
            exists: true,
            ..Rec::default()
        };
        run_plan(&rec, &provision_plan(&cfg(), USER).unwrap()).unwrap();
        let log = rec.log.lock();
        assert!(!log.iter().any(|l| l.starts_with("useradd")), "{log:?}");
        assert!(log.iter().any(|l| l.starts_with("dir ")));
        assert_eq!(log.last().unwrap(), "egress");
    }

    #[test]
    fn deprovisioning_kills_then_deletes_the_user_and_keeps_the_files_unless_purged() {
        let rec = Rec {
            exists: true,
            ..Rec::default()
        };
        run_plan(&rec, &deprovision_plan(&cfg(), USER, false).unwrap()).unwrap();
        assert_eq!(
            *rec.log.lock(),
            [
                "kill atlas-cm-3f2a9c1e5b7d",
                "userdel atlas-cm-3f2a9c1e5b7d",
                "egress"
            ]
        );
        let rec = Rec {
            exists: true,
            ..Rec::default()
        };
        run_plan(&rec, &deprovision_plan(&cfg(), USER, true).unwrap()).unwrap();
        assert_eq!(
            *rec.log.lock(),
            [
                "kill atlas-cm-3f2a9c1e5b7d",
                "userdel atlas-cm-3f2a9c1e5b7d",
                "rm /var/lib/atlas-cm/3f2a9c1e5b7d",
                "egress"
            ]
        );
    }

    // Deleting a crewmate whose user is already gone (a retry, a half-finished delete) must finish,
    // not fail on `userdel`.
    #[test]
    fn deprovisioning_a_user_that_is_already_gone_still_cleans_up() {
        let rec = Rec::default(); // exists: false
        run_plan(&rec, &deprovision_plan(&cfg(), USER, true).unwrap()).unwrap();
        assert_eq!(
            *rec.log.lock(),
            ["rm /var/lib/atlas-cm/3f2a9c1e5b7d", "egress"]
        );
    }

    #[test]
    fn a_failing_step_stops_the_plan_and_names_what_failed() {
        let rec = Rec {
            fail_on: Some("useradd"),
            ..Rec::default()
        };
        let e = run_plan(&rec, &provision_plan(&cfg(), USER).unwrap()).unwrap_err();
        assert!(
            e.to_string()
                .contains("creating user atlas-cm-3f2a9c1e5b7d"),
            "{e}"
        );
        assert_eq!(rec.log.lock().len(), 1, "nothing ran after the failure");
    }

    // The service interpolates nothing into a shell, but the names still become paths and argv:
    // every plan refuses what is not exactly a crewmate user.
    #[test]
    fn plans_refuse_anything_that_is_not_a_crewmate_user() {
        for bad in [
            "root",
            "atlas-crew",
            "",
            "atlas-cm-../../etc",
            "atlas-cm-3f2a9c1e5b7d/x",
            "-r",
        ] {
            assert!(provision_plan(&cfg(), bad).is_err(), "{bad:?}");
            assert!(deprovision_plan(&cfg(), bad, true).is_err(), "{bad:?}");
        }
    }

    // RemoveTree is the one step that deletes recursively as root: a plan edited to point
    // anywhere but a direct child of the crewmates' root is refused at execution too.
    #[test]
    fn remove_tree_only_deletes_a_direct_child_of_the_crewmates_root() {
        let rec = Rec::default();
        for bad in [
            "/",
            "/var/lib",
            "/var/lib/atlas-cm",
            "/var/lib/atlas-cm/x/y",
            "/etc",
        ] {
            let e = run_plan(&rec, &[Step::RemoveTree { path: bad.into() }]).unwrap_err();
            assert_eq!(e.kind(), io::ErrorKind::PermissionDenied, "{bad}");
        }
        assert!(rec.log.lock().is_empty());
    }

    // Service level: a request is validated, planned and executed, and the answer is Done or a reason.
    #[test]
    fn the_service_answers_done_or_a_reason_and_refuses_bad_users_without_running_anything() {
        let svc = Service::new(cfg(), 990, Box::new(Rec::default()));
        assert_eq!(
            svc.handle(ProvRequest::Provision { user: USER.into() }),
            ProvResponse::Done
        );
        match svc.handle(ProvRequest::Deprovision {
            user: "root".into(),
            purge: true,
        }) {
            ProvResponse::Error { reason } => {
                assert!(reason.contains("not a provisioned"), "{reason}")
            }
            other => panic!("{other:?}"),
        }
    }

    // ---- egress ---------------------------------------------------------------------------

    fn pol() -> Policy {
        Policy::default()
    }

    #[test]
    fn the_ruleset_replaces_its_own_table_and_gives_each_user_its_own_chain() {
        let r = egress_ruleset(&[
            ("atlas-cm-aaaaaaaaaaaa".into(), pol()),
            ("atlas-cm-bbbbbbbbbbbb".into(), pol()),
        ]);
        // Replace-whole idiom: create (no error if missing), delete, define.
        assert!(r.starts_with(
            "table inet atlas_crew\ndelete table inet atlas_crew\ntable inet atlas_crew {\n"
        ));
        assert!(r.contains("chain cm_aaaaaaaaaaaa {"));
        assert!(r.contains("chain cm_bbbbbbbbbbbb {"));
        assert!(r.contains("\"atlas-cm-aaaaaaaaaaaa\" : jump cm_aaaaaaaaaaaa"));
        assert!(r.contains("\"atlas-cm-bbbbbbbbbbbb\" : jump cm_bbbbbbbbbbbb"));
        assert!(r.contains("type filter hook output priority 0; policy accept;"));
    }

    // The order is the policy: an `accept` after a `drop` never fires.
    #[test]
    fn within_a_chain_replies_dns_and_the_hook_port_come_before_the_private_range_drops() {
        let r = egress_ruleset(&[("atlas-cm-aaaaaaaaaaaa".into(), pol())]);
        let at = |needle: &str| {
            r.find(needle)
                .unwrap_or_else(|| panic!("missing {needle}\n{r}"))
        };
        let established = at("ct state established,related accept");
        let dns = at("udp dport 53 accept");
        let hook = at(&format!("ip daddr 127.0.0.1 tcp dport {HOOK_PORT} accept"));
        let loopback_drop = at("ip daddr 127.0.0.0/8 drop");
        let private_drop = at("10.0.0.0/8");
        let ports = at("tcp dport { 22, 80, 443 } accept");
        let last_drop = r.rfind("    drop\n").unwrap();
        assert!(established < dns && dns < hook && hook < loopback_drop);
        assert!(loopback_drop < private_drop && private_drop < ports && ports < last_drop);
        // The cloud metadata service and the private ranges are named.
        for must in [
            "169.254.0.0/16",
            "192.168.0.0/16",
            "172.16.0.0/12",
            "100.64.0.0/10",
            "fc00::/7",
            // NAT64 carries any IPv4 address, private ones included.
            "64:ff9b::/96",
            "64:ff9b:1::/48",
        ] {
            assert!(r.contains(must), "{must}");
        }
    }

    #[test]
    fn a_users_extra_allowed_destinations_come_before_the_drops_and_use_the_right_family() {
        let r = egress_ruleset(&[(
            "atlas-cm-aaaaaaaaaaaa".into(),
            Policy {
                tcp_ports: vec![443],
                allow: vec!["10.1.2.3".into(), "10.9.0.0/24".into(), "fd00::5".into()],
            },
        )]);
        let allow = r.find("ip daddr 10.1.2.3 accept").unwrap();
        assert!(r.contains("ip daddr 10.9.0.0/24 accept"));
        assert!(r.contains("ip6 daddr fd00::5 accept"));
        assert!(allow < r.find("ip daddr { 10.0.0.0/8").unwrap());
        assert!(r.contains("tcp dport { 443 } accept"));
    }

    #[test]
    fn no_users_means_a_table_with_an_output_chain_and_no_verdict_map() {
        let r = egress_ruleset(&[]);
        assert!(r.contains("chain output"));
        assert!(!r.contains("vmap"), "nothing to select: {r}");
        assert!(!r.contains("chain cm_"));
    }

    #[test]
    fn users_come_from_the_passwd_text_and_only_crewmate_shaped_names_count() {
        let passwd = "root:x:0:0::/root:/bin/bash\n\
atlas-crew:x:990:990::/var/lib/atlas-crew:/bin/bash\n\
atlas-cm-bbbbbbbbbbbb:x:992:992::/var/lib/atlas-cm/bbbbbbbbbbbb/home:/usr/sbin/nologin\n\
atlas-cm-aaaaaaaaaaaa:x:991:991::/var/lib/atlas-cm/aaaaaaaaaaaa/home:/usr/sbin/nologin\n\
atlas-cm-tooshort:x:993:993::/x:/bin/false\n\
atlas-cm-AAAAAAAAAAAA:x:994:994::/x:/bin/false\n";
        assert_eq!(
            crewmate_users_in(passwd),
            ["atlas-cm-aaaaaaaaaaaa", "atlas-cm-bbbbbbbbbbbb"]
        );
    }

    // ---- a crewmate that owns its directory must not steer root's chown/chmod --------------------

    fn my_ids() -> (u32, u32) {
        // SAFETY: plain getters.
        unsafe { (libc::geteuid(), libc::getegid()) }
    }

    fn mode_of(p: &Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o7777
    }

    // Regression (security review of #448, B1): the provisioning service runs as root with CAP_CHOWN and
    // CAP_FOWNER on every task, and the crewmate owned `<dir>`, so it could `ln -s /etc/passwd memory.md`
    // and have root chown/chmod the target. A link at any entry is refused and its target is untouched.
    #[test]
    fn apply_refuses_a_symlink_planted_at_an_entry_and_leaves_its_target_alone() {
        let t = tempfile::tempdir().unwrap();
        let secret_file = t.path().join("secret");
        std::fs::write(&secret_file, b"x").unwrap();
        std::fs::set_permissions(&secret_file, std::fs::Permissions::from_mode(0o600)).unwrap();
        let secret_dir = t.path().join("secretdir");
        std::fs::create_dir(&secret_dir).unwrap();
        std::fs::set_permissions(&secret_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        for (name, kind, target, mode, before) in [
            ("memory.md", Kind::File, &secret_file, 0o660, 0o600),
            ("work", Kind::Dir, &secret_dir, 0o2770, 0o700),
            ("home", Kind::Dir, &secret_dir, 0o700, 0o700),
            ("crew-runtime", Kind::Dir, &secret_dir, 0o750, 0o700),
        ] {
            let link = t.path().join(name);
            std::os::unix::fs::symlink(target, &link).unwrap();
            let e = ensure(&link, kind, mode, &my_ids()).unwrap_err();
            assert!(e.to_string().contains("symbolic link"), "{name}: {e}");
            assert_eq!(mode_of(target), before, "{name}: the target's mode changed");
        }
        // A dangling link is no better: creating the file must not write through it.
        let gone = t.path().join("gone");
        let dangling = t.path().join("dangling");
        std::os::unix::fs::symlink(&gone, &dangling).unwrap();
        assert!(ensure(&dangling, Kind::File, 0o660, &my_ids()).is_err());
        assert!(!gone.exists(), "created the link's target");
    }

    // The directory itself may be the link (a crewmate of an install made before `<dir>` was root's).
    #[test]
    fn ensure_refuses_when_the_parent_of_an_entry_is_a_link() {
        let t = tempfile::tempdir().unwrap();
        let real = t.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = t.path().join("dir");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(ensure(&link.join("work"), Kind::Dir, 0o2770, &my_ids()).is_err());
        assert!(!real.join("work").exists());
    }

    #[test]
    fn ensure_refuses_the_wrong_type_and_a_hard_linked_file_and_never_blocks_on_a_fifo() {
        let t = tempfile::tempdir().unwrap();
        let file = t.path().join("memory.md");
        std::fs::write(&file, b"x").unwrap();
        let e = ensure(&file, Kind::Dir, 0o700, &my_ids()).unwrap_err();
        assert!(e.to_string().contains("not a directory"), "{e}");
        let dir = t.path().join("work");
        std::fs::create_dir(&dir).unwrap();
        let e = ensure(&dir, Kind::File, 0o660, &my_ids()).unwrap_err();
        assert!(e.to_string().contains("not a regular file"), "{e}");
        let other = t.path().join("other-name");
        std::fs::hard_link(&file, &other).unwrap();
        let e = ensure(&file, Kind::File, 0o660, &my_ids()).unwrap_err();
        assert!(e.to_string().contains("hard link"), "{e}");
        let fifo = t.path().join("fifo");
        let c = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: valid path.
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        assert!(ensure(&fifo, Kind::File, 0o660, &my_ids()).is_err());
    }

    #[test]
    fn ensure_creates_what_is_missing_with_the_asked_mode_and_repairs_what_is_there() {
        let t = tempfile::tempdir().unwrap();
        let (d, f) = (t.path().join("work"), t.path().join("memory.md"));
        ensure(&d, Kind::Dir, 0o2770, &my_ids()).unwrap();
        ensure(&f, Kind::File, 0o660, &my_ids()).unwrap();
        assert_eq!((mode_of(&d), mode_of(&f)), (0o2770, 0o660));
        std::fs::write(&f, b"kept").unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600)).unwrap();
        ensure(&f, Kind::File, 0o660, &my_ids()).unwrap();
        assert_eq!(mode_of(&f), 0o660);
        assert_eq!(
            std::fs::read(&f).unwrap(),
            b"kept",
            "contents are never touched"
        );
        // The parent is never created: a missing root of the crewmates is an install problem.
        assert!(ensure(&t.path().join("no/such/dir"), Kind::Dir, 0o700, &my_ids()).is_err());
    }

    // ---- real filesystem: modes survive chown (setgid is dropped by a chown after chmod) -------

    #[test]
    fn ensure_sets_the_setgid_bit_that_a_later_chown_would_have_cleared() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().join("work");
        std::fs::create_dir(&d).unwrap();
        ensure(&d, Kind::Dir, 0o2770, &my_ids()).unwrap();
        let mode = std::fs::metadata(&d).unwrap().permissions().mode();
        assert_eq!(mode & 0o7777, 0o2770, "{mode:o}");
    }

    // Needs root on Linux: the real `useradd`/`userdel`/`chown` path of the provisioning service. Run by
    // the root CI job (`sudo -E cargo test -p atlas-host crew::spawn -- --ignored`). Creates a throwaway
    // crewmate user and directory tree and removes both.
    #[test]
    #[ignore = "needs root on Linux: creates and deletes a system user (see docs/contributing.md)"]
    fn a_real_provision_creates_the_user_and_modes_and_a_real_deprovision_removes_them() {
        // SAFETY: plain getter.
        assert_eq!(unsafe { libc::geteuid() }, 0, "this test must run as root");
        let t = tempfile::tempdir().unwrap();
        std::fs::set_permissions(t.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let user = "atlas-cm-eeeeeeeeeeee";
        let cfg = PlanCfg {
            crew: "root".into(),
            cm_root: t.path().to_string_lossy().into_owned(),
        };
        // No nft in the sandbox: the real host minus the firewall step.
        struct NoNft(System);
        impl Host for NoNft {
            fn user_exists(&self, u: &str) -> bool {
                self.0.user_exists(u)
            }
            fn run(&self, a: &[&str]) -> io::Result<()> {
                self.0.run(a)
            }
            fn dir(&self, p: &Path, m: u32, o: &Who, c: &str) -> io::Result<()> {
                self.0.dir(p, m, o, c)
            }
            fn file(&self, p: &Path, m: u32, o: &Who, c: &str) -> io::Result<()> {
                self.0.file(p, m, o, c)
            }
            fn kill_user(&self, u: &str) -> io::Result<()> {
                self.0.kill_user(u)
            }
            fn remove_tree(&self, p: &Path) -> io::Result<()> {
                self.0.remove_tree(p)
            }
            fn egress(&self) -> io::Result<()> {
                Ok(())
            }
        }
        let host = NoNft(System::new(cfg.clone()));
        struct Cleanup<'a>(&'a NoNft, &'a PlanCfg);
        impl Drop for Cleanup<'_> {
            fn drop(&mut self) {
                let _ = execute(
                    self.0,
                    self.1,
                    "atlas-cm-eeeeeeeeeeee",
                    &deprovision_plan(self.1, "atlas-cm-eeeeeeeeeeee", true).unwrap(),
                );
            }
        }
        let _guard = Cleanup(&host, &cfg);
        execute(&host, &cfg, user, &provision_plan(&cfg, user).unwrap()).unwrap();
        let ids = super::super::exec::lookup(user).unwrap();
        assert_ne!(ids.uid, 0);
        let dir = t.path().join("eeeeeeeeeeee");
        let meta = |p: &str| std::fs::metadata(dir.join(p)).unwrap();
        use std::os::unix::fs::MetadataExt;
        let work = meta("work");
        assert_eq!(work.permissions().mode() & 0o7777, 0o2770);
        // hostd reaches it through the crew user's group (here root's: the crew user stands in as root).
        assert_eq!(work.uid(), ids.uid);
        assert_eq!(work.gid(), 0);
        assert_eq!(meta("home").permissions().mode() & 0o7777, 0o700);
        assert_eq!(meta("home").gid(), ids.gid, "$HOME is the crewmate's alone");
        // crew-runtime: owned by the crew user (root here), group the crewmate's.
        let rt = meta("crew-runtime");
        assert_eq!((rt.uid(), rt.gid()), (0, ids.gid));
        assert_eq!(meta("memory.md").permissions().mode() & 0o7777, 0o660);
        // The crewmate's directory is root's: it can enter it and never replace what is in it.
        let top = std::fs::metadata(&dir).unwrap();
        assert_eq!(
            (top.uid(), top.gid(), top.permissions().mode() & 0o7777),
            (0, 0, 0o711)
        );
        // A link planted where `memory.md` is (as the old crewmate-owned directory allowed) is refused by the
        // real service, and root's file behind it keeps its owner and mode.
        let secret = t.path().join("secret");
        std::fs::write(&secret, b"x").unwrap();
        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::remove_file(dir.join("memory.md")).unwrap();
        std::os::unix::fs::symlink(&secret, dir.join("memory.md")).unwrap();
        let e = execute(&host, &cfg, user, &provision_plan(&cfg, user).unwrap()).unwrap_err();
        assert!(e.to_string().contains("symbolic link"), "{e}");
        let s = std::fs::metadata(&secret).unwrap();
        assert_eq!((s.uid(), s.permissions().mode() & 0o7777), (0, 0o600));
        std::fs::remove_file(dir.join("memory.md")).unwrap();
        // Idempotent.
        execute(&host, &cfg, user, &provision_plan(&cfg, user).unwrap()).unwrap();
        // Removal takes the user and the tree.
        execute(
            &host,
            &cfg,
            user,
            &deprovision_plan(&cfg, user, true).unwrap(),
        )
        .unwrap();
        assert!(super::super::exec::lookup(user).is_err());
        assert!(!dir.exists());
    }
}
