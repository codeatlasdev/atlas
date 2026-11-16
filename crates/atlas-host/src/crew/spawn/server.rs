//! The helper's loop (`atlas crew-spawn`): accept hostd's connections, check who connected, check
//! every request against [`super::validate`], and act. It is the only root process that touches a
//! crewmate; everything it can do is one of the five [`Request`]s.
//!
//! State is the table of sessions it started. A reaper thread collects exits as they happen, so
//! `Wait` can answer after the fact and no child lingers as a zombie. The table is lost when the
//! helper restarts (`KillMode=process` keeps the agents alive): hostd then gets "unknown session"
//! and falls back to watching the pid (see `pty::Isolation`).

use std::collections::HashMap;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::Child;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use super::cgroup;
use super::exec::{self, ENGINE_DIRS, Ids, Where};
use super::peer::{self, Peer};
use super::protocol::{Request, Response};
use super::validate::{self, CM_ROOT, Refused};
use super::wire;

/// How long an exited session stays answerable.
const KEEP_EXITED: Duration = Duration::from_secs(10 * 60);
/// How often the reaper looks at running children.
const REAP_EVERY: Duration = Duration::from_millis(100);

/// What the helper needs to know about its surroundings. Production is [`Config::production`];
/// tests point every directory at a temp dir.
#[derive(Debug, Clone)]
pub struct Config {
    /// Uid of the only user (besides root) that may connect.
    pub crew_uid: u32,
    /// Parent of the crewmates' directories.
    pub cm_root: String,
    /// Directories engines are looked up in, in order.
    pub engines: Vec<String>,
    /// How a user name becomes uid/gid. Production asks the passwd database; tests, where the
    /// `atlas-cm-*` users do not exist, answer with the current user. The only seam: validation,
    /// framing, spawning and signalling are all the real code.
    pub resolve: fn(&str) -> io::Result<Ids>,
    /// How agents are capped: production detects its delegated cgroup at start ([`cgroup::Pool::detect`]);
    /// tests give a pool under a scratch cgroup, or `None`.
    pub memory: Arc<Memory>,
}

/// Where the memory cap stands.
pub enum Memory {
    /// Capped: leaves live in this pool.
    #[cfg(target_os = "linux")]
    On(cgroup::Pool),
    /// Not capped, and why (also what hostd reports).
    Off(String),
    /// Test-only seam: no cgroup, but every session that was killed by SIGKILL is reported as an OOM kill
    /// under this cap. A kernel OOM kill cannot be made on a developer's machine; everything on hostd's
    /// side of the verdict (wire, client, runner, task text) stays real, and the kernel half has its own
    /// root test (`cgroup::tests`).
    #[cfg(test)]
    Verdict(u64),
}

impl std::fmt::Debug for Memory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Memory({:?})", self.status())
    }
}

impl Memory {
    /// The helper's own cgroup, with the default cap for this machine (`agent_memory_max` unset,
    /// 4 agents until hostd says otherwise). One log line when there is none.
    pub fn detect() -> Self {
        #[cfg(target_os = "linux")]
        {
            let total = std::fs::read_to_string("/proc/meminfo")
                .ok()
                .and_then(|t| cgroup::mem_total_of(&t))
                .unwrap_or(0);
            let cap = cgroup::compute_cap(None, total, crate::crew::store::DEFAULT_MAX_AGENTS);
            match cgroup::Pool::detect(cap) {
                Ok(p) => Self::On(p),
                Err(why) => {
                    eprintln!("atlas crew-spawn: no per-agent memory cap: {why}");
                    Self::Off(why)
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            Self::Off("per-agent memory caps need Linux with cgroup v2".into())
        }
    }

    fn status(&self) -> cgroup::Status {
        match self {
            #[cfg(target_os = "linux")]
            Self::On(p) => p.status(),
            Self::Off(why) => cgroup::Status::off(why.clone()),
            #[cfg(test)]
            Self::Verdict(cap) => cgroup::Status::on(*cap),
        }
    }
}

impl Config {
    pub fn production(crew_uid: u32) -> Self {
        Self {
            crew_uid,
            cm_root: CM_ROOT.to_string(),
            engines: ENGINE_DIRS.iter().map(|s| s.to_string()).collect(),
            resolve: exec::lookup,
            memory: Arc::new(Memory::detect()),
        }
    }
}

enum State {
    Running(Child),
    /// `oom_cap` is decided once, when the exit is collected: a later session of the same crewmate
    /// shares the leaf and its counter, and must not be able to turn this one into an OOM kill.
    Exited {
        code: Option<i32>,
        at: Instant,
        oom_cap: Option<u64>,
    },
}

struct Entry {
    pid: u32,
    state: State,
    /// What the memory cap needs to tell, at the exit, whether the kernel killed this session.
    #[cfg(target_os = "linux")]
    watch: Option<OomWatch>,
    /// The cap a test's [`Memory::Verdict`] reports for this session if it dies by SIGKILL.
    #[cfg(test)]
    verdict: Option<u64>,
}

/// The leaf a session ran in, with its `oom_kill` count and the cap when it started.
#[cfg(target_os = "linux")]
struct OomWatch {
    leaf: std::sync::Arc<cgroup::Leaf>,
    before: u64,
    cap: u64,
}

/// The helper: configuration plus the sessions it started.
pub struct Server {
    cfg: Config,
    sessions: Mutex<HashMap<u64, Entry>>,
    next: AtomicU64,
}

impl Server {
    pub fn new(cfg: Config) -> Arc<Self> {
        Arc::new(Self {
            cfg,
            sessions: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
        })
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// Serve `listener` until it fails. One thread per connection (hostd opens a handful), one
    /// reaper for all of them.
    pub fn serve(self: &Arc<Self>, listener: UnixListener) -> io::Result<()> {
        let me = self.clone();
        std::thread::Builder::new()
            .name("crew-spawn-reaper".into())
            .spawn(move || me.reap_loop())?;
        for conn in listener.incoming() {
            let conn = conn?;
            let me = self.clone();
            std::thread::Builder::new()
                .name("crew-spawn-conn".into())
                .spawn(move || {
                    if let Err(e) = me.connection(conn) {
                        eprintln!("atlas crew-spawn: connection ended: {e}");
                    }
                })?;
        }
        Ok(())
    }

    fn reap_loop(&self) {
        loop {
            std::thread::sleep(REAP_EVERY);
            self.reap_once(Instant::now());
        }
    }

    /// Collect the exit of every finished child and forget sessions that have been over for
    /// [`KEEP_EXITED`].
    fn reap_once(&self, now: Instant) {
        let mut s = self.sessions.lock();
        for e in s.values_mut() {
            e.collect(now);
        }
        s.retain(|_, e| match &e.state {
            State::Exited { at, .. } => now.duration_since(*at) < KEEP_EXITED,
            State::Running(_) => true,
        });
    }

    fn connection(&self, conn: UnixStream) -> io::Result<()> {
        let who = peer::peer_of(&conn)?;
        if !peer::allowed(who, self.cfg.crew_uid) {
            // Not even an error reply: a stranger gets a closed socket.
            return Ok(());
        }
        loop {
            let Some(frame) = wire::recv::<Request>(&conn)? else {
                return Ok(());
            };
            let (resp, fd) = self.handle(who, frame.len, frame.msg);
            let fds: Vec<_> = fd.iter().map(|f| f.as_raw_fd()).collect();
            wire::send(&conn, &resp, &fds)?;
        }
    }

    /// One request. A spawn's reply carries the fd to hand over.
    pub fn handle(
        &self,
        _who: Peer,
        len: usize,
        req: Request,
    ) -> (Response, Option<std::os::fd::OwnedFd>) {
        if let Err(r) = validate::check_in(&self.cfg.cm_root, &req, len) {
            return (err(r), None);
        }
        match req {
            Request::Spawn(s) => match self.spawn(&s) {
                Ok((resp, fd)) => (resp, Some(fd)),
                Err(e) => (
                    Response::Error {
                        reason: e.to_string(),
                    },
                    None,
                ),
            },
            Request::Signal { session, sig } => (self.signal(session, sig), None),
            Request::Wait { session } => (self.wait(session), None),
            Request::Cwd { session } => (self.cwd(session), None),
            Request::Reap { user } => (self.reap_user(&user), None),
            Request::SetMemoryCap { bytes } => (self.set_memory_cap(bytes), None),
            Request::MemoryCap => (Response::MemoryCap(self.cfg.memory.status()), None),
        }
    }

    fn spawn(&self, s: &super::protocol::SpawnReq) -> io::Result<(Response, std::os::fd::OwnedFd)> {
        let ids = (self.cfg.resolve)(&s.user)?;
        let dir = validate::dir_in(&self.cfg.cm_root, &s.user)
            .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e))?;
        let engines: Vec<&str> = self.cfg.engines.iter().map(String::as_str).collect();
        let w = Where {
            engines: &engines,
            cm_root: &self.cfg.cm_root,
        };
        #[cfg(target_os = "linux")]
        let leaf = match &*self.cfg.memory {
            Memory::On(pool) => Some(pool.leaf(&s.user)?),
            Memory::Off(_) => None,
            #[cfg(test)]
            Memory::Verdict(_) => None,
        };
        #[cfg(target_os = "linux")]
        let (started, watch) = {
            // Counted before the child exists: a kill that happened during an earlier session of the
            // same crewmate is not this one's.
            let watch = leaf.as_ref().map(|l| OomWatch {
                leaf: l.clone(),
                before: l.oom_kills(),
                cap: match &*self.cfg.memory {
                    Memory::On(p) => p.cap(),
                    _ => 0,
                },
            });
            let started = exec::spawn_capped(
                &w,
                s,
                ids,
                &format!("{dir}/home"),
                leaf.as_ref().map(|l| l.procs_fd()),
            )?;
            (started, watch)
        };
        #[cfg(not(target_os = "linux"))]
        let started = exec::spawn_in(&w, s, ids, &format!("{dir}/home"))?;
        let pid = started.child.id();
        let session = self.next.fetch_add(1, Ordering::Relaxed);
        self.sessions.lock().insert(
            session,
            Entry {
                pid,
                state: State::Running(started.child),
                #[cfg(target_os = "linux")]
                watch,
                #[cfg(test)]
                verdict: match &*self.cfg.memory {
                    Memory::Verdict(c) => Some(*c),
                    _ => None,
                },
            },
        );
        Ok((Response::Spawned { session, pid }, started.fd))
    }

    fn signal(&self, session: u64, sig: i32) -> Response {
        let mut s = self.sessions.lock();
        let Some(e) = s.get_mut(&session) else {
            return unknown(session);
        };
        // Reap first: signalling the pid of a child that already exited and was collected could hit
        // whoever got that pid next.
        e.collect(Instant::now());
        match &e.state {
            State::Exited { .. } => Response::Done,
            State::Running(_) => {
                // The child called `setsid` before exec, so its pid is its process group id: `-pid` is its
                // whole tree (what an engine started), like `TurnProc` for the crew user. Not `Reap`: that
                // ends every process of the crewmate, including its other sessions.
                // SAFETY: kill(2) on the process group of a child we own and have not reaped; its pid is
                // positive and in i32 range (a `pid_t`).
                if unsafe { libc::kill(-(e.pid as libc::pid_t), sig) } == 0 {
                    Response::Done
                } else {
                    Response::Error {
                        reason: io::Error::last_os_error().to_string(),
                    }
                }
            }
        }
    }

    fn wait(&self, session: u64) -> Response {
        self.reap_once(Instant::now());
        match self.sessions.lock().get(&session) {
            None => unknown(session),
            Some(Entry {
                state: State::Running(_),
                ..
            }) => Response::Exit {
                exited: false,
                code: None,
                oom_cap: None,
            },
            Some(Entry {
                state: State::Exited { code, oom_cap, .. },
                ..
            }) => Response::Exit {
                exited: true,
                code: *code,
                oom_cap: *oom_cap,
            },
        }
    }

    /// A new cap for every agent: the leaves that exist are rewritten, new ones start with it.
    fn set_memory_cap(&self, bytes: u64) -> Response {
        match &*self.cfg.memory {
            #[cfg(target_os = "linux")]
            Memory::On(pool) => match pool.set_cap(bytes) {
                Ok(()) => Response::MemoryCap(pool.status()),
                Err(e) => Response::Error {
                    reason: format!("cannot change memory.max: {e}"),
                },
            },
            Memory::Off(_) => {
                let _ = bytes;
                Response::MemoryCap(self.cfg.memory.status())
            }
            #[cfg(test)]
            Memory::Verdict(_) => Response::MemoryCap(self.cfg.memory.status()),
        }
    }

    fn cwd(&self, session: u64) -> Response {
        let pid = match self.sessions.lock().get(&session) {
            Some(e) => e.pid,
            None => return unknown(session),
        };
        match proc_cwd(pid) {
            Ok(path) => Response::Cwd { path },
            Err(e) => Response::Error {
                reason: e.to_string(),
            },
        }
    }

    fn reap_user(&self, user: &str) -> Response {
        let ids = match (self.cfg.resolve)(user) {
            Ok(i) => i,
            Err(e) => {
                return Response::Error {
                    reason: e.to_string(),
                };
            }
        };
        if ids.uid == 0 {
            return Response::Error {
                reason: "refusing to reap uid 0".into(),
            };
        }
        match kill_uid(ids.uid) {
            // What was ours is now dead; the reaper collects the exits.
            Ok(_) => Response::Done,
            Err(e) => Response::Error {
                reason: e.to_string(),
            },
        }
    }
}

impl Entry {
    /// If the child has exited, record how, and whether the kernel killed it for memory (the `oom_kill`
    /// count of its leaf grew since it started; a crewmate runs one session at a time, so the leaf's
    /// count is this session's).
    fn collect(&mut self, now: Instant) {
        if let State::Running(child) = &mut self.state
            && let Ok(Some(st)) = child.try_wait()
        {
            let code = exit_code(st);
            #[cfg(test)]
            let fake = self.verdict.filter(|_| code == Some(137));
            #[cfg(not(test))]
            let fake = None;
            self.state = State::Exited {
                code,
                at: now,
                oom_cap: fake.or_else(|| self.oom_cap()),
            };
        }
    }

    fn oom_cap(&self) -> Option<u64> {
        #[cfg(target_os = "linux")]
        {
            let w = self.watch.as_ref()?;
            (w.leaf.oom_kills() > w.before).then_some(w.cap)
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }
}

fn err(r: Refused) -> Response {
    Response::Error {
        reason: r.to_string(),
    }
}

fn unknown(session: u64) -> Response {
    Response::Error {
        reason: format!("unknown session {session}"),
    }
}

/// `Some(code)`; a signal death is `128 + signal`, like a shell reports it.
fn exit_code(st: std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    st.code().or_else(|| st.signal().map(|s| 128 + s))
}

/// Working directory of `pid` as the kernel reports it.
fn proc_cwd(pid: u32) -> io::Result<String> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/{pid}/cwd")).map(|p| p.to_string_lossy().into_owned())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "reading another process's directory needs /proc (Linux)",
        ))
    }
}

/// Every pid whose real uid is `uid`, from `/proc`. Exact pids only: a recycled pid between the scan
/// and the kill is the same tiny window any `kill(pid)` has, and nothing is addressed by group.
#[cfg(target_os = "linux")]
pub fn pids_of_uid(uid: u32) -> io::Result<Vec<u32>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir("/proc")? {
        let Ok(entry) = entry else { continue };
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else {
            continue;
        };
        if status_uid(&status) == Some(uid) {
            out.push(pid);
        }
    }
    Ok(out)
}

/// Real uid from the text of `/proc/<pid>/status` (`Uid:\treal\teffective\tsaved\tfs`).
pub fn status_uid(status: &str) -> Option<u32> {
    status
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|v| v.parse().ok())
}

#[cfg(not(target_os = "linux"))]
pub fn pids_of_uid(_uid: u32) -> io::Result<Vec<u32>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "scanning processes by user needs /proc (Linux)",
    ))
}

/// SIGKILL every process of `uid`; returns how many were signalled. Never uid 0, never the caller's own
/// user (a crewmate is never the helper's user; killing it would take down everything that user runs,
/// which on a CI runner is the runner itself), never our own pid.
pub fn kill_uid(uid: u32) -> io::Result<usize> {
    if uid == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to kill uid 0",
        ));
    }
    // SAFETY: plain getter.
    if uid == unsafe { libc::geteuid() } {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "refusing to kill every process of the caller's own user",
        ));
    }
    let me = std::process::id();
    let mut n = 0;
    for pid in pids_of_uid(uid)? {
        if pid == me || pid == 0 || pid > i32::MAX as u32 {
            continue;
        }
        // SAFETY: kill(2) with an exact positive pid.
        if unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) } == 0 {
            n += 1;
        }
    }
    Ok(n)
}

/// Name of the group with this gid (`getgrgid_r`).
pub fn group_name(gid: u32) -> io::Result<String> {
    let mut buf = vec![0u8; 4096];
    // SAFETY: zeroed group is a valid out-parameter for getgrgid_r.
    let mut gr: libc::group = unsafe { std::mem::zeroed() };
    let mut out: *mut libc::group = std::ptr::null_mut();
    // SAFETY: all pointers are live for the call; the buffer length is passed.
    let rc =
        unsafe { libc::getgrgid_r(gid, &mut gr, buf.as_mut_ptr().cast(), buf.len(), &mut out) };
    if rc != 0 {
        return Err(io::Error::from_raw_os_error(rc));
    }
    if out.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no group with gid {gid}"),
        ));
    }
    // SAFETY: gr_name points into `buf`, NUL terminated, alive here.
    Ok(unsafe { std::ffi::CStr::from_ptr(gr.gr_name) }
        .to_string_lossy()
        .into_owned())
}

/// The crew user the helper serves, from the group of the socket it was given: the socket unit sets
/// `SocketGroup=<crew user>` (the group of `atlas-crew`, or of a migrated `atlas-bot`), so the group's
/// name is the user's. Returns `(user, uid)`.
pub fn crew_user_of_socket(fd: std::os::fd::RawFd) -> io::Result<(String, u32)> {
    // SAFETY: zeroed stat is a valid out-parameter; fd is a live socket.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: fstat on a live fd with a valid out pointer.
    if unsafe { libc::fstat(fd, &mut st) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let name = group_name(st.st_gid)?;
    let ids = exec::lookup(&name)?;
    if ids.uid == 0 {
        // A socket group of root would serve root only; the helper refuses to start rather than
        // run with "the crew user" being nobody in particular.
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("the socket's group {name} is not a crew user"),
        ));
    }
    Ok((name, ids.uid))
}

/// Bind the helper's socket at `path` (mode 0660, replacing a stale file), for running a helper by
/// hand and for tests. Under systemd the socket comes in as fd 3 instead ([`listener_from_systemd`]).
pub fn bind(path: &std::path::Path) -> io::Result<UnixListener> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _ = std::fs::remove_file(path);
    let l = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o660))?;
    Ok(l)
}

/// The socket systemd passed (`LISTEN_FDS=1`, fd 3, for this pid), if any.
pub fn listener_from_systemd() -> Option<UnixListener> {
    use std::os::fd::FromRawFd;
    let pid: u32 = std::env::var("LISTEN_PID").ok()?.parse().ok()?;
    let fds: i32 = std::env::var("LISTEN_FDS").ok()?.parse().ok()?;
    if pid != std::process::id() || fds < 1 {
        return None;
    }
    // SAFETY: systemd hands over fd 3 for this process (LISTEN_PID matched); nothing else owns it.
    Some(unsafe { UnixListener::from_raw_fd(3) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_uid_reads_the_real_uid_not_the_effective_one() {
        let st = "Name:\tsleep\nUmask:\t0022\nUid:\t1001\t0\t0\t0\nGid:\t1001\t1001\t1001\t1001\n";
        assert_eq!(status_uid(st), Some(1001));
        assert_eq!(status_uid("Name:\tx\n"), None);
        assert_eq!(status_uid("Uid:\tnope\n"), None);
    }

    #[test]
    fn exit_code_maps_a_signal_death_like_a_shell() {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            exit_code(std::process::ExitStatus::from_raw(3 << 8)),
            Some(3)
        );
        assert_eq!(exit_code(std::process::ExitStatus::from_raw(9)), Some(137));
    }

    // Incident class: kill_uid runs as root. uid 0 and our own pid are never killed, whatever the
    // caller computed.
    #[test]
    fn kill_uid_never_touches_root_or_the_caller() {
        assert_eq!(kill_uid(0).unwrap_err().kind(), io::ErrorKind::InvalidInput);
    }

    // Incident (#223 CI, every plain Linux run): the test rig resolves a crewmate to the test's own uid,
    // so ending an isolated terminal (`Reap`) ran `kill_uid(runner)` and SIGKILLed every process of the
    // CI user: the Actions agent, cargo, nextest ("the hosted runner lost communication"). The caller's
    // own user is never a crewmate (the helper is root), so it is refused like uid 0, and nothing of
    // ours is signalled.
    #[test]
    fn kill_uid_refuses_the_callers_own_user_and_leaves_its_processes_alone() {
        let mut sibling = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        // SAFETY: plain getter.
        let me = unsafe { libc::geteuid() };
        let r = kill_uid(me);
        let alive = sibling.try_wait().unwrap().is_none();
        let _ = sibling.kill();
        let _ = sibling.wait();
        assert_eq!(
            r.map_err(|e| e.kind()),
            Err(io::ErrorKind::InvalidInput),
            "kill_uid of our own user must be refused"
        );
        assert!(
            alive,
            "kill_uid of our own user killed one of our processes"
        );
    }

    // ---- the whole path: a real server on a socket, a real client, real fds and processes ----

    use super::super::client::is_unknown_session;
    use super::super::protocol::{Mode, SpawnReq};
    use super::super::testing::{Rig, USER, rig as new_rig};
    use std::io::{Read, Write};

    /// The shared rig with one engine, `claude`: it reports on itself and then runs `$FAKE` (a shell
    /// snippet from the request's env) so each test chooses how the child behaves.
    fn rig() -> Rig {
        let r = new_rig();
        r.engine("claude", "echo started pid=$$\neval \"$FAKE\"");
        r
    }

    fn req(r: &Rig, mode: Mode, fake: &str) -> SpawnReq {
        SpawnReq {
            user: USER.into(),
            argv: vec!["claude".into()],
            env: [("FAKE".to_string(), fake.to_string())].into(),
            cwd: r.work().to_string_lossy().into_owned(),
            mode,
            cols: 80,
            rows: 24,
        }
    }

    fn read_until(f: &mut std::fs::File, needle: &str) -> String {
        let mut out = String::new();
        let mut buf = [0u8; 4096];
        while !out.contains(needle) {
            match f.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => out.push_str(&String::from_utf8_lossy(&buf[..n])),
            }
        }
        out
    }

    // The contract hostd relies on: the master it gets back is the child's terminal, the pid it is
    // told is the child's, and the exit is reported with the code.
    #[test]
    fn a_pty_spawn_over_the_socket_returns_a_live_master_the_pid_and_the_exit_code() {
        let r = rig();
        let (sp, fd) = r
            .client
            .spawn(&req(&r, Mode::Pty, "read l; echo got=$l; exit 7"))
            .unwrap();
        let mut io = std::fs::File::from(fd);
        let out = read_until(&mut io, &format!("started pid={}", sp.pid));
        assert!(out.contains(&format!("started pid={}", sp.pid)), "{out}");
        io.write_all(b"hi\n").unwrap();
        let out = read_until(&mut io, "got=hi");
        assert!(out.contains("got=hi"), "{out}");
        assert_eq!(r.client.wait_exit(sp.session), Some(7));
        // An exit stays answerable.
        assert_eq!(r.client.wait(sp.session).unwrap(), (true, Some(7)));
    }

    #[test]
    fn a_running_session_reports_not_exited_until_it_ends() {
        let r = rig();
        let (sp, _fd) = r
            .client
            .spawn(&req(&r, Mode::Pipe, "/bin/sleep 30"))
            .unwrap();
        assert_eq!(r.client.wait(sp.session).unwrap(), (false, None));
        r.client.signal(sp.session, libc::SIGKILL).unwrap();
        // 137 = 128 + SIGKILL: the same code a shell shows for a killed child.
        assert_eq!(r.client.wait_exit(sp.session), Some(137));
    }

    #[test]
    fn signals_reach_the_child_and_a_signal_to_an_ended_session_is_not_an_error() {
        let r = rig();
        let (sp, _fd) = r
            .client
            .spawn(&req(
                &r,
                Mode::Pipe,
                "trap 'exit 42' TERM; while :; do /bin/sleep 0.05; done",
            ))
            .unwrap();
        // The trap is installed once the script printed its banner; give the shell a moment.
        std::thread::sleep(Duration::from_millis(300));
        r.client.signal(sp.session, libc::SIGTERM).unwrap();
        assert_eq!(r.client.wait_exit(sp.session), Some(42));
        // Signalling what already ended must not hit a recycled pid, and must not fail.
        r.client.signal(sp.session, libc::SIGKILL).unwrap();
    }

    // Merging #200 with the engine launcher of #359: a cancelled turn must end what the engine started
    // (a tool, a dev server), as `TurnProc` does for the crew user with `kill(-pgid)`. Every spawn is
    // its own session (`setsid`), so the group is the tree; signalling one pid left the grandchild
    // running, and `Reap` is no substitute: it ends every process of the user, a concurrent terminal
    // agent of the same crewmate included.
    #[test]
    fn a_signal_reaches_the_whole_process_group_of_the_session_not_only_its_leader() {
        let r = rig();
        let pidfile = r.cm.join("tool.pid");
        let (sp, _fd) = r
            .client
            .spawn(&req(
                &r,
                Mode::Pipe,
                &format!(
                    "(/bin/sleep 60 & echo $! > {}; wait) &\nwait",
                    pidfile.display()
                ),
            ))
            .unwrap();
        let tool = loop {
            if let Ok(t) = std::fs::read_to_string(&pidfile)
                && let Ok(pid) = t.trim().parse::<i32>()
            {
                break pid;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        // SAFETY: signal 0 only tests that the pid exists.
        assert_eq!(unsafe { libc::kill(tool, 0) }, 0, "the tool is running");
        r.client.signal(sp.session, libc::SIGKILL).unwrap();
        r.client.wait_exit(sp.session);
        let start = Instant::now();
        // A killed process whose parent is gone is reaped by init at once; a zombie only waits for that.
        while start.elapsed() < Duration::from_secs(5) {
            let dead = std::fs::read_to_string(format!("/proc/{tool}/stat"))
                .map(|s| {
                    s.rsplit(')')
                        .next()
                        .is_some_and(|r| r.trim_start().starts_with('Z'))
                })
                // No /proc (macOS): kill(pid, 0) says whether the pid is still there.
                .unwrap_or_else(|_| unsafe { libc::kill(tool, 0) } != 0);
            if dead {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("the tool process {tool} outlived the session's SIGKILL");
    }

    // #296 "degrade, never break": a helper with no cgroup to cap in (a Mac, no delegation, cgroup v1) still
    // spawns, says there is no cap and why, and an exit carries no OOM verdict.
    #[test]
    fn without_a_cgroup_the_helper_spawns_uncapped_and_says_why() {
        let r = rig();
        let st = r.client.memory_cap().unwrap();
        assert!(!st.available, "{st:?}");
        assert_eq!(st.reason.as_deref(), Some("tests run without a cgroup"));
        // Setting a cap changes nothing and answers the same, not an error.
        assert_eq!(r.client.set_memory_cap(1 << 30).unwrap(), st);
        let (sp, _fd) = r.client.spawn(&req(&r, Mode::Pipe, "exit 3")).unwrap();
        assert_eq!(r.client.wait_exit(sp.session), Some(3));
        assert_eq!(
            r.client.wait_oom(sp.session).unwrap(),
            (true, Some(3), None)
        );
    }

    #[test]
    fn a_signal_outside_the_list_is_refused_by_the_helper() {
        let r = rig();
        let (sp, _fd) = r
            .client
            .spawn(&req(&r, Mode::Pipe, "/bin/sleep 30"))
            .unwrap();
        let e = r.client.signal(sp.session, libc::SIGSTOP).unwrap_err();
        assert!(e.to_string().contains("signal not allowed"), "{e}");
        assert_eq!(r.client.wait(sp.session).unwrap(), (false, None));
        r.client.signal(sp.session, libc::SIGKILL).unwrap();
    }

    #[test]
    fn an_unknown_session_is_reported_as_such_for_every_session_request() {
        let r = rig();
        for e in [
            r.client.signal(999, libc::SIGTERM).unwrap_err(),
            r.client.wait(999).unwrap_err(),
            r.client.cwd(999).unwrap_err(),
        ] {
            assert!(is_unknown_session(&e), "{e}");
        }
        // wait_exit turns it into "stop waiting" (the caller watches the pid instead).
        assert_eq!(r.client.wait_exit(999), None);
    }

    #[test]
    fn a_pipe_spawn_is_a_socketpair_for_a_line_protocol() {
        let r = rig();
        let (sp, fd) = r
            .client
            .spawn(&req(&r, Mode::Pipe, "read l; echo echo=$l"))
            .unwrap();
        let mut io = std::fs::File::from(fd);
        io.write_all(b"ping\n").unwrap();
        let out = read_until(&mut io, "echo=ping");
        assert!(out.contains("echo=ping"), "{out}");
        assert_eq!(r.client.wait_exit(sp.session), Some(0));
    }

    // On macOS a socket whose last descriptor is the one in flight in an SCM_RIGHTS message loses its
    // data when the sender closes its copy before the receiver's recvmsg, and the peer (the child) is
    // already gone: the receiver reads EOF, whatever the child printed (a fast `claude` fake in CI
    // gave "no result JSON in output:" with exit 0). The helper must keep its end until hostd has it.
    #[test]
    fn what_a_fast_child_printed_survives_a_reply_that_is_read_late() {
        let r = rig();
        let conn = UnixStream::connect(r.client.path()).unwrap();
        let spawn = super::super::protocol::Request::Spawn(req(&r, Mode::Pipe, "echo printed"));
        wire::send(&conn, &spawn, &[]).unwrap();
        // The child is done (and its end closed) long before hostd reads the reply.
        std::thread::sleep(std::time::Duration::from_millis(700));
        let mut f = wire::recv::<super::super::protocol::Response>(&conn)
            .unwrap()
            .unwrap();
        let mut io = std::fs::File::from(f.fds.remove(0));
        let out = read_until(&mut io, "printed");
        assert!(
            out.contains("printed"),
            "the child's output was lost: {out:?}"
        );
    }

    // Refusals come back as errors and nothing is started: the validators run in the helper, not
    // only in the client.
    #[test]
    fn a_refused_spawn_is_an_error_reply_and_starts_nothing() {
        let r = rig();
        for tweak in [
            (|q: &mut SpawnReq| q.argv[0] = "sh".into()) as fn(&mut SpawnReq),
            |q| q.cwd = "/tmp".into(),
            |q| q.user = "root".into(),
            |q| {
                q.env.insert("LD_PRELOAD".into(), "/x.so".into());
            },
        ] {
            let mut q = req(&r, Mode::Pipe, "true");
            tweak(&mut q);
            let e = r.client.spawn(&q).unwrap_err();
            assert!(!e.to_string().is_empty());
        }
        assert!(r.server.sessions.lock().is_empty());
    }

    #[test]
    fn a_missing_engine_is_an_error_not_a_hang() {
        let r = rig();
        let mut q = req(&r, Mode::Pipe, "true");
        q.argv = vec!["codex".into()]; // the rig installs only `claude`
        let e = r.client.spawn(&q).unwrap_err();
        assert!(e.to_string().contains("codex is not installed"), "{e}");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn cwd_is_the_directory_the_child_is_in_now() {
        let r = rig();
        let (sp, mut_fd) = r
            .client
            .spawn(&req(&r, Mode::Pipe, "cd /tmp; echo cd; /bin/sleep 30"))
            .unwrap();
        let mut io = std::fs::File::from(mut_fd);
        read_until(&mut io, "cd\n");
        let now = r.client.cwd(sp.session).unwrap();
        assert_eq!(
            std::fs::canonicalize(now).unwrap(),
            std::fs::canonicalize("/tmp").unwrap()
        );
        r.client.signal(sp.session, libc::SIGKILL).unwrap();
    }

    // The second lock: a peer that is not the crew user (or root) is dropped without a reply.
    #[test]
    fn a_user_that_is_not_the_crew_user_gets_nothing() {
        let r = rig();
        let me_uid = unsafe { libc::geteuid() };
        let s = Server::new(Config {
            crew_uid: me_uid.wrapping_add(1).max(1),
            ..r.server.config().clone()
        });
        // Root (uid 0) is always allowed, so this proves the refusal only when not running as root.
        if me_uid == 0 {
            return;
        }
        let (a, b) = UnixStream::pair().unwrap();
        let t = std::thread::spawn(move || s.connection(b));
        // The server may already have closed (it checks who we are before reading), so the send
        // can fail with EPIPE; what must never happen is a reply.
        let _ = wire::send(&a, &Request::Wait { session: 1 }, &[]);
        assert!(
            !matches!(wire::recv::<Response>(&a), Ok(Some(_))),
            "a stranger got a reply"
        );
        t.join().unwrap().unwrap();
    }

    #[test]
    fn exited_sessions_are_forgotten_after_a_while_and_running_ones_never() {
        let r = rig();
        let (done, _f1) = r.client.spawn(&req(&r, Mode::Pipe, "exit 0")).unwrap();
        let (live, _f2) = r
            .client
            .spawn(&req(&r, Mode::Pipe, "/bin/sleep 30"))
            .unwrap();
        assert_eq!(r.client.wait_exit(done.session), Some(0));
        r.server
            .reap_once(Instant::now() + KEEP_EXITED + Duration::from_secs(1));
        assert!(is_unknown_session(
            &r.client.wait(done.session).unwrap_err()
        ));
        assert_eq!(r.client.wait(live.session).unwrap(), (false, None));
        r.client.signal(live.session, libc::SIGKILL).unwrap();
    }

    #[test]
    fn group_names_are_looked_up_and_an_unknown_gid_is_not_found() {
        assert!(!group_name(0).unwrap().is_empty());
        // Some gid with no group: macOS maps (gid_t)-2 to `nobody`, so look for one that is free.
        let free = (60_000u32..70_000)
            .find(|g| matches!(group_name(*g), Err(e) if e.kind() == io::ErrorKind::NotFound))
            .expect("no free gid in 60000..70000");
        assert_eq!(
            group_name(free).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    // The helper serves "the user named like the socket's group". A socket in root's group has no
    // crew user, and the helper must refuse to start instead of serving nobody in particular.
    #[test]
    fn a_socket_in_a_group_with_no_crew_user_is_refused() {
        use std::os::fd::AsRawFd;
        let t = tempfile::tempdir().unwrap();
        let l = bind(&t.path().join("s.sock")).unwrap();
        // The socket belongs to our primary group, whose name is not a user of this machine (or is
        // a real user: either way the answer is a clean Ok or an error, never a panic or uid 0).
        match crew_user_of_socket(l.as_raw_fd()) {
            Ok((name, uid)) => assert!(uid != 0 && !name.is_empty()),
            Err(e) => assert!(
                matches!(
                    e.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
                ),
                "{e}"
            ),
        }
        assert!(crew_user_of_socket(-1).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn pids_of_uid_finds_our_own_process_and_not_a_uid_nobody_has() {
        let me = unsafe { libc::getuid() };
        assert!(pids_of_uid(me).unwrap().contains(&std::process::id()));
        // 4294967294 is `(uid_t)-2`: never a real account.
        assert!(pids_of_uid(4_294_967_294).unwrap().is_empty());
    }
}
