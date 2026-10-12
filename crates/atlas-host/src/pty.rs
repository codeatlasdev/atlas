//! One PTY session: child process, ring buffer, scrollback file, OSC title
//! tracking and the set of attached connections receiving `pty_output`.
//!
//! Output path: reader thread → channel → coalescer task (8 ms / 64 KiB) →
//! [`Session::output`] which appends to the ring + file and broadcasts under one
//! lock, so an attach-with-replay never loses or duplicates bytes.

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::protocol::{Mode, PtyCreate, Scope};

const RING_MAX: usize = 2 * 1024 * 1024;
const RING_DROP: usize = 64 * 1024;
const FILE_MAX: u64 = 4 * 1024 * 1024;
const COALESCE: Duration = Duration::from_millis(8);
const COALESCE_BYTES: usize = 64 * 1024;

pub type Out = mpsc::UnboundedSender<String>;
/// Bounded: a raw reader that falls behind blocks the pty reader (backpressure to
/// the program, as in a native terminal) instead of hostd buffering without limit.
pub type RawOut = mpsc::Sender<Vec<u8>>;
pub const RAW_QUEUE: usize = 16;
/// A raw reader that accepts nothing for this long is dropped (hung client): the
/// program must not freeze behind it.
const RAW_STALL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    pub cwd: String,
    pub argv: Vec<String>,
    #[serde(default)]
    pub scope: Scope,
    pub created_at: u64,
}

/// Blocks until the child exits; returns its code.
type Waiter = Box<dyn FnOnce() -> Option<i32> + Send>;

/// The pty master: owned by `portable_pty` for sessions this daemon spawned, a bare fd
/// for sessions adopted from a previous daemon (see `handoff`).
enum Master {
    Pty(Box<dyn MasterPty + Send>),
    Fd(std::os::fd::OwnedFd),
}

impl Master {
    fn dup_file(&self) -> std::io::Result<std::fs::File> {
        let fd = self
            .raw_fd()
            .ok_or_else(|| std::io::Error::other("no pty fd"))?;
        // SAFETY: `fd` is owned by `self`, alive for this call; the clone is independent.
        let b = unsafe { std::os::fd::BorrowedFd::borrow_raw(fd) };
        b.try_clone_to_owned().map(std::fs::File::from)
    }

    fn raw_fd(&self) -> Option<std::os::fd::RawFd> {
        match self {
            Master::Pty(m) => m.as_raw_fd(),
            Master::Fd(f) => Some(std::os::fd::AsRawFd::as_raw_fd(f)),
        }
    }

    fn resize(&self, size: PtySize) -> Result<()> {
        match self {
            Master::Pty(m) => Ok(m.resize(size)?),
            Master::Fd(f) => {
                let ws = libc::winsize {
                    ws_row: size.rows,
                    ws_col: size.cols,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                };
                // SAFETY: valid pty master fd and winsize pointer.
                if unsafe { libc::ioctl(std::os::fd::AsRawFd::as_raw_fd(f), libc::TIOCSWINSZ, &ws) }
                    != 0
                {
                    return Err(std::io::Error::last_os_error().into());
                }
                Ok(())
            }
        }
    }

    fn process_group_leader(&self) -> Option<i32> {
        match self {
            Master::Pty(m) => m.process_group_leader(),
            // SAFETY: tcgetpgrp on a valid fd.
            Master::Fd(f) => match unsafe { libc::tcgetpgrp(std::os::fd::AsRawFd::as_raw_fd(f)) } {
                n if n > 0 => Some(n),
                _ => None,
            },
        }
    }
}

struct Io {
    /// `None` in pipe mode (no terminal to resize).
    master: Option<Master>,
    writer: Box<dyn Write + Send>,
}

struct Buf {
    ring: VecDeque<u8>,
    attachers: HashMap<u64, Out>,
    /// Attached without output: title/exit events only.
    listeners: HashMap<u64, Out>,
    /// `pty_attach_raw` connections: raw output chunks.
    raw: HashMap<u64, RawOut>,
    file: Option<std::fs::File>,
    file_len: u64,
}

pub struct Session {
    pub id: String,
    pub meta: Meta,
    pub pid: Option<u32>,
    title: Mutex<String>,
    /// `None` while alive; `Some(code)` once exited (`Some(None)` = unknown/cold).
    exited: Mutex<Option<Option<i32>>>,
    size: Mutex<(u16, u16)>,
    io: Mutex<Option<Io>>,
    buf: Mutex<Buf>,
    dir: PathBuf,
    /// Set by `freeze`: the reader thread stops after its current read and the
    /// coalescer flushes and signals `frozen` instead of finishing the session.
    stop: Arc<std::sync::atomic::AtomicBool>,
    frozen: Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn bin_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.bin"))
}
fn meta_path(dir: &Path, id: &str) -> PathBuf {
    dir.join(format!("{id}.json"))
}

fn open_append(p: &Path) -> Option<std::fs::File> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)
        .ok()
}

/// The account's login shell (what Terminal uses), else `$SHELL`, else zsh.
pub fn login_shell() -> String {
    // SAFETY: getpwuid returns a pointer into static storage or null; we copy out at once.
    let account = unsafe {
        let pw = libc::getpwuid(libc::getuid());
        (!pw.is_null() && !(*pw).pw_shell.is_null()).then(|| {
            std::ffi::CStr::from_ptr((*pw).pw_shell)
                .to_string_lossy()
                .into_owned()
        })
    };
    account
        .or_else(|| std::env::var("SHELL").ok())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/bin/zsh".into())
}

impl Session {
    /// Spawn `req` in a new PTY. `scroll_dir` holds `<id>.bin` + `<id>.json`.
    pub fn spawn(req: PtyCreate, scroll_dir: &Path) -> Result<Arc<Session>> {
        let id = crate::paths::new_id();
        let argv = if req.argv.is_empty() {
            vec![login_shell(), "-l".into()]
        } else {
            req.argv.clone()
        };
        let mut env: Vec<(String, String)> = vec![
            ("TERM".into(), "xterm-256color".into()),
            ("COLORTERM".into(), "truecolor".into()),
            ("ATLAS_HOSTD".into(), "1".into()),
            ("ATLAS_PTY_ID".into(), id.clone()),
        ];
        env.extend(req.env.iter().map(|(k, v)| (k.clone(), v.clone())));
        let (reader, writer, master, pid, wait): (Box<dyn Read + Send>, _, _, _, Waiter) =
            if req.mode == Mode::Pipe {
                // Line protocols (omp --mode rpc-ui): stdin/stdout pipes, stderr dropped
                // so it can't corrupt the JSONL stream.
                use std::os::unix::process::CommandExt;
                let mut cmd = std::process::Command::new(&argv[0]);
                cmd.args(&argv[1..])
                    .current_dir(&req.cwd)
                    .envs(env)
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null());
                // SAFETY: setsid is async-signal-safe; own session like a PTY child.
                unsafe {
                    cmd.pre_exec(|| {
                        libc::setsid();
                        Ok(())
                    });
                }
                let mut child = cmd.spawn().with_context(|| format!("spawn {}", argv[0]))?;
                let stdout = child.stdout.take().context("stdout")?;
                let stdin = child.stdin.take().context("stdin")?;
                let pid = Some(child.id());
                let wait: Waiter = Box::new(move || child.wait().ok().and_then(|s| s.code()));
                (
                    Box::new(stdout),
                    Box::new(stdin) as Box<dyn Write + Send>,
                    None,
                    pid,
                    wait,
                )
            } else {
                let pair = native_pty_system()
                    .openpty(PtySize {
                        rows: req.rows.max(1),
                        cols: req.cols.max(1),
                        pixel_width: 0,
                        pixel_height: 0,
                    })
                    .context("openpty")?;
                let mut cmd = CommandBuilder::new(&argv[0]);
                cmd.args(&argv[1..]);
                cmd.cwd(&req.cwd);
                for (k, v) in env {
                    cmd.env(k, v);
                }
                let mut child = pair
                    .slave
                    .spawn_command(cmd)
                    .with_context(|| format!("spawn {}", argv[0]))?;
                drop(pair.slave);
                let pid = child.process_id();
                let reader = pair.master.try_clone_reader().context("pty reader")?;
                let writer = pair.master.take_writer().context("pty writer")?;
                let wait: Waiter =
                    Box::new(move || child.wait().ok().map(|st| st.exit_code() as i32));
                (reader, writer, Some(Master::Pty(pair.master)), pid, wait)
            };

        std::fs::create_dir_all(scroll_dir)?;
        let meta = Meta {
            cwd: req.cwd.clone(),
            argv: argv.clone(),
            scope: req.scope.clone(),
            created_at: now_s(),
        };
        let _ = std::fs::write(
            meta_path(scroll_dir, &id),
            serde_json::to_vec(&meta).unwrap_or_default(),
        );
        let file_path = bin_path(scroll_dir, &id);
        let _ = std::fs::remove_file(&file_path);
        let s = Arc::new(Session {
            id: id.clone(),
            meta,
            pid,
            title: Mutex::new(req.title.clone().unwrap_or_default()),
            exited: Mutex::new(None),
            size: Mutex::new((req.cols, req.rows)),
            io: Mutex::new(Some(Io { master, writer })),
            buf: Mutex::new(Buf {
                ring: VecDeque::new(),
                attachers: HashMap::new(),
                listeners: HashMap::new(),
                raw: HashMap::new(),
                file: open_append(&file_path),
                file_len: 0,
            }),
            dir: scroll_dir.to_path_buf(),
            stop: Arc::default(),
            frozen: Mutex::new(None),
        });
        s.run(reader, wait);
        Ok(s)
    }

    /// Reader thread → bounded channel → coalescer task. `wait` blocks until the child
    /// is gone (after the reader hits EOF) and yields its code.
    fn run(self: &Arc<Self>, mut reader: Box<dyn Read + Send>, wait: Waiter) {
        enum Msg {
            Data(Vec<u8>),
            Exit(Option<i32>),
            /// `freeze` asked the reader to stop; everything before this is flushed.
            Frozen,
        }
        // Bounded (64 × 16 KiB): with a slow raw reader the thread blocks, the kernel
        // pty buffer fills, and the program waits.
        let (tx, mut rx) = mpsc::channel::<Msg>(64);
        let stop = self.stop.clone();
        // Pollable pty master (not pipe mode): lets `freeze` interrupt a blocked read.
        let poll_fd = self
            .io
            .lock()
            .as_ref()
            .and_then(|io| io.master.as_ref()?.raw_fd());
        std::thread::spawn(move || {
            let mut chunk = vec![0u8; 16 * 1024];
            loop {
                if stop.load(std::sync::atomic::Ordering::Acquire) {
                    let _ = tx.blocking_send(Msg::Frozen);
                    return;
                }
                if let Some(fd) = poll_fd {
                    let mut p = libc::pollfd {
                        fd,
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    // SAFETY: one valid pollfd; 100 ms bound so `stop` is noticed.
                    let r = unsafe { libc::poll(&mut p, 1, 100) };
                    if r == 0
                        || (r < 0
                            && std::io::Error::last_os_error().kind()
                                == std::io::ErrorKind::Interrupted)
                    {
                        continue;
                    }
                }
                match reader.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.blocking_send(Msg::Data(chunk[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                }
            }
            let code = wait();
            let _ = tx.blocking_send(Msg::Exit(code));
        });
        let sess = self.clone();
        tokio::spawn(async move {
            let mut title = TitleParser::default();
            while let Some(msg) = rx.recv().await {
                let mut pending = match msg {
                    Msg::Data(d) => d,
                    Msg::Exit(code) => {
                        sess.finish(code);
                        return;
                    }
                    Msg::Frozen => {
                        sess.ack_frozen();
                        return;
                    }
                };
                let deadline = tokio::time::Instant::now() + COALESCE;
                let mut exit = None;
                let mut frozen = false;
                while pending.len() < COALESCE_BYTES {
                    match tokio::time::timeout_at(deadline, rx.recv()).await {
                        Ok(Some(Msg::Data(d))) => pending.extend_from_slice(&d),
                        Ok(Some(Msg::Exit(code))) => {
                            exit = Some(code);
                            break;
                        }
                        Ok(Some(Msg::Frozen)) => {
                            frozen = true;
                            break;
                        }
                        Ok(None) | Err(_) => break,
                    }
                }
                if let Some(t) = title.feed(&pending) {
                    sess.set_title(t);
                }
                for (conn, out) in sess.output(&pending) {
                    let sent = tokio::time::timeout(RAW_STALL, out.send(pending.clone())).await;
                    if !matches!(sent, Ok(Ok(()))) {
                        sess.buf.lock().raw.remove(&conn);
                    }
                }
                if let Some(code) = exit {
                    sess.finish(code);
                    return;
                }
                if frozen {
                    sess.ack_frozen();
                    return;
                }
            }
        });
    }

    fn ack_frozen(&self) {
        if let Some(tx) = self.frozen.lock().take() {
            let _ = tx.send(());
        }
    }

    /// Restore an exited session from a scrollback file left by a previous daemon.
    pub fn cold(id: &str, scroll_dir: &Path) -> Option<Arc<Session>> {
        let data = std::fs::read(bin_path(scroll_dir, id)).ok()?;
        let meta = std::fs::read(meta_path(scroll_dir, id))
            .ok()
            .and_then(|b| serde_json::from_slice::<Meta>(&b).ok())
            .map(|m| Meta {
                argv: Vec::new(),
                ..m
            })
            .unwrap_or(Meta {
                cwd: String::new(),
                argv: Vec::new(),
                scope: Scope::default(),
                created_at: 0,
            });
        let start = data.len().saturating_sub(RING_MAX);
        Some(Arc::new(Session {
            id: id.to_string(),
            meta,
            pid: None,
            title: Mutex::new(String::new()),
            exited: Mutex::new(Some(None)),
            size: Mutex::new((80, 24)),
            io: Mutex::new(None),
            buf: Mutex::new(Buf {
                ring: data[start..].iter().copied().collect(),
                attachers: HashMap::new(),
                listeners: HashMap::new(),
                raw: HashMap::new(),
                file: None,
                file_len: data.len() as u64,
            }),
            dir: scroll_dir.to_path_buf(),
            stop: Arc::default(),
            frozen: Mutex::new(None),
        }))
    }

    /// Ring = tail of the shared scrollback file.
    fn ring_from_file(scroll_dir: &Path, id: &str) -> (VecDeque<u8>, u64) {
        let data = std::fs::read(bin_path(scroll_dir, id)).unwrap_or_default();
        let start = data.len().saturating_sub(RING_MAX);
        (data[start..].iter().copied().collect(), data.len() as u64)
    }

    /// Take over a session frozen by the previous daemon (see `handoff`). A live one
    /// arrives with its pty master `fd`; an exited one is restored like a cold session
    /// but keeps its title, size and exit code.
    pub fn adopt(
        st: crate::handoff::SessionState,
        fd: Option<std::os::fd::OwnedFd>,
        scroll_dir: &Path,
    ) -> Result<Arc<Session>> {
        let (ring, file_len) = Self::ring_from_file(scroll_dir, &st.id);
        let meta = Meta {
            cwd: st.cwd,
            argv: st.argv,
            scope: st.scope,
            created_at: st.created_at,
        };
        let mut buf = Buf {
            ring,
            attachers: HashMap::new(),
            listeners: HashMap::new(),
            raw: HashMap::new(),
            file: None,
            file_len,
        };
        let mut io = None;
        let mut run = None;
        if let (Some(fd), false) = (fd, st.exited) {
            let reader = fd
                .try_clone()
                .context("dup pty master")
                .map(std::fs::File::from)?;
            let writer = fd
                .try_clone()
                .context("dup pty master")
                .map(std::fs::File::from)?;
            buf.file = open_append(&bin_path(scroll_dir, &st.id));
            io = Some(Io {
                master: Some(Master::Fd(fd)),
                writer: Box::new(writer),
            });
            run = Some(reader);
        }
        let s = Arc::new(Session {
            id: st.id,
            meta,
            pid: st.pid,
            title: Mutex::new(st.title),
            exited: Mutex::new(st.exited.then_some(st.exit_code)),
            size: Mutex::new((st.cols, st.rows)),
            io: Mutex::new(io),
            buf: Mutex::new(buf),
            dir: scroll_dir.to_path_buf(),
            stop: Arc::default(),
            frozen: Mutex::new(None),
        });
        if let Some(reader) = run {
            // The child is not ours: no waitpid. Pty EOF, then the kqueue watch opened
            // now (while it is alive); the exit code is only known when the OS reports it.
            s.run(Box::new(reader), s.exit_waiter());
        }
        Ok(s)
    }

    /// Waiter for a child this daemon did not spawn (adopted, or resumed after a failed
    /// handoff). The watch is registered here, not when the waiter runs.
    fn exit_waiter(&self) -> Waiter {
        let watch = self.pid.map(crate::handoff::ExitWatch::new);
        Box::new(move || watch.and_then(crate::handoff::ExitWatch::wait))
    }

    /// Stop reading, flush what was read into the ring/file, and describe the session
    /// for the successor. Returns `None` for pipe-mode sessions (no pty master, so
    /// nothing to pass; they end with this daemon). The master fd stays owned by
    /// `self` until the caller has sent it.
    pub async fn freeze(
        &self,
    ) -> Option<(crate::handoff::SessionState, Option<std::os::fd::RawFd>)> {
        let exited = self.exited();
        let fd = if exited.is_none() {
            let fd = self.io.lock().as_ref()?.master.as_ref()?.raw_fd()?;
            let (tx, rx) = tokio::sync::oneshot::channel();
            *self.frozen.lock() = Some(tx);
            self.stop.store(true, std::sync::atomic::Ordering::Release);
            // Reader exits within one poll tick (100 ms); a session that exits while
            // freezing finishes instead (then it is reported as exited).
            let _ = tokio::time::timeout(Duration::from_secs(3), rx).await;
            Some(fd)
        } else {
            None
        };
        let exited = self.exited();
        let (cols, rows) = *self.size.lock();
        let file_len = self.buf.lock().file_len;
        Some((
            crate::handoff::SessionState {
                id: self.id.clone(),
                cwd: self.meta.cwd.clone(),
                argv: self.meta.argv.clone(),
                scope: self.meta.scope.clone(),
                created_at: self.meta.created_at,
                title: self.title.lock().clone(),
                cols,
                rows,
                pid: self.pid,
                file_len,
                exited: exited.is_some(),
                exit_code: exited.flatten(),
                fd: None,
            },
            if exited.is_none() { fd } else { None },
        ))
    }

    /// Resume reading after a failed handoff (the successor never took the session).
    pub fn thaw(self: &Arc<Self>) {
        if self.exited().is_some() {
            return;
        }
        self.stop.store(false, std::sync::atomic::Ordering::Release);
        let reader = self
            .io
            .lock()
            .as_ref()
            .and_then(|io| io.master.as_ref()?.dup_file().ok());
        if let Some(reader) = reader {
            self.run(Box::new(reader), self.exit_waiter());
        }
    }

    /// Title/exit: every attached connection.
    fn notify(b: &mut Buf, line: &str) {
        Self::broadcast(&mut b.attachers, line);
        Self::broadcast(&mut b.listeners, line);
    }

    fn broadcast(attachers: &mut HashMap<u64, Out>, line: &str) {
        attachers.retain(|_, out| out.send(line.to_string()).is_ok());
    }

    /// Ring, scrollback file and JSON attachers; returns the raw readers, which the
    /// caller feeds outside the lock (awaiting them is the backpressure).
    fn output(&self, bytes: &[u8]) -> Vec<(u64, RawOut)> {
        let mut b = self.buf.lock();
        b.ring.extend(bytes);
        if b.ring.len() > RING_MAX {
            let over = b.ring.len() - RING_MAX;
            let n = (over.div_ceil(RING_DROP) * RING_DROP).min(b.ring.len());
            b.ring.drain(..n);
        }
        b.file_len += bytes.len() as u64;
        if b.file_len > FILE_MAX {
            let ring: Vec<u8> = b.ring.iter().copied().collect();
            let p = bin_path(&self.dir, &self.id);
            if std::fs::write(&p, &ring).is_ok() {
                b.file = open_append(&p);
                b.file_len = ring.len() as u64;
            }
        } else if let Some(f) = b.file.as_mut() {
            let _ = f.write_all(bytes);
        }
        let raw = b.raw.iter().map(|(k, v)| (*k, v.clone())).collect();
        // Only raw readers (GPU terminal): skip the JSON + base64 encode.
        if !b.attachers.is_empty() {
            let line =
                json!({"ev": "pty_output", "pty": self.id, "data": B64.encode(bytes)}).to_string();
            Self::broadcast(&mut b.attachers, &line);
        }
        raw
    }

    fn set_title(&self, t: String) {
        *self.title.lock() = t.clone();
        let line = json!({"ev": "pty_title", "pty": self.id, "title": t}).to_string();
        Self::notify(&mut self.buf.lock(), &line);
    }

    fn finish(&self, code: Option<i32>) {
        *self.exited.lock() = Some(code);
        self.io.lock().take();
        // Raw streams end with the pty: dropping the senders closes those connections.
        self.buf.lock().raw.clear();
        let line = json!({"ev": "pty_exit", "pty": self.id, "code": code}).to_string();
        Self::notify(&mut self.buf.lock(), &line);
    }

    pub fn exited(&self) -> Option<Option<i32>> {
        *self.exited.lock()
    }

    fn ring_bytes(b: &Buf) -> Vec<u8> {
        let (a, c) = b.ring.as_slices();
        let mut all = Vec::with_capacity(a.len() + c.len());
        all.extend_from_slice(a);
        all.extend_from_slice(c);
        all
    }

    fn info(&self) -> Value {
        let (cols, rows) = *self.size.lock();
        json!({
            "cols": cols,
            "rows": rows,
            "title": *self.title.lock(),
            "exited": self.exited().map(|c| json!({"code": c})),
        })
    }

    pub fn attach(&self, conn: u64, out: Out, replay: bool, output: bool) -> Value {
        let mut b = self.buf.lock();
        if !output {
            b.listeners.insert(conn, out);
            drop(b);
            return self.info();
        }
        if replay {
            let all = Self::ring_bytes(&b);
            let _ = out.send(
                json!({"ev": "pty_replay", "pty": self.id, "data": B64.encode(&all)}).to_string(),
            );
        }
        b.attachers.insert(conn, out);
        drop(b);
        self.info()
    }

    /// Raw byte subscription. An exited pty gets its replay and then a closed stream.
    pub fn attach_raw(&self, conn: u64, out: RawOut, replay: bool) -> Value {
        let mut b = self.buf.lock();
        if replay {
            let _ = out.try_send(Self::ring_bytes(&b));
        }
        // Under the buf lock: `finish` clears `raw` under the same lock, so no sender outlives the pty.
        if self.exited().is_none() {
            b.raw.insert(conn, out);
        }
        drop(b);
        self.info()
    }

    pub fn detach(&self, conn: u64) {
        let mut b = self.buf.lock();
        b.attachers.remove(&conn);
        b.listeners.remove(&conn);
        b.raw.remove(&conn);
    }

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

    /// TERM, wait up to 3 s, then KILL.
    pub async fn kill(&self) {
        if self.signal("TERM").is_err() {
            return;
        }
        for _ in 0..30 {
            if self.exited().is_some() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let _ = self.signal("KILL");
        for _ in 0..10 {
            if self.exited().is_some() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub fn remove_files(&self) {
        let _ = std::fs::remove_file(bin_path(&self.dir, &self.id));
        let _ = std::fs::remove_file(meta_path(&self.dir, &self.id));
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
        let b = self.buf.lock();
        let raw: Vec<u8> = b.ring.iter().copied().collect();
        drop(b);
        let text = crate::strip_ansi(&String::from_utf8_lossy(&raw));
        let all: Vec<&str> = text.trim_end_matches('\n').lines().collect();
        let from = all.len().saturating_sub(lines as usize);
        all[from..].join("\n")
    }

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

    /// Current working directory of the child (macOS/Linux via `lsof`).
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

/// Incremental OSC 0/2 title parser (`ESC ] 0;title BEL` or `ESC ] 2;title ESC \`).
#[derive(Default)]
pub struct TitleParser {
    st: u8,
    buf: Vec<u8>,
}

impl TitleParser {
    pub fn feed(&mut self, bytes: &[u8]) -> Option<String> {
        let mut last = None;
        for &c in bytes {
            self.st = match (self.st, c) {
                (0, 0x1b) => 1,
                (0, _) => 0,
                (1, b']') => 2,
                (1, 0x1b) => 1,
                (1, _) => 0,
                (2, b'0' | b'2') => 3,
                (2, _) => 0,
                (3, b';') => {
                    self.buf.clear();
                    4
                }
                (3, _) => 0,
                (4, 0x07) => {
                    last = Some(String::from_utf8_lossy(&self.buf).into_owned());
                    0
                }
                (4, 0x1b) => 5,
                (4, _) => {
                    if self.buf.len() < 4096 {
                        self.buf.push(c);
                    }
                    4
                }
                (5, b'\\') => {
                    last = Some(String::from_utf8_lossy(&self.buf).into_owned());
                    0
                }
                (5, _) => 0,
                _ => 0,
            };
        }
        last
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_split_across_chunks() {
        let mut p = TitleParser::default();
        assert_eq!(p.feed("x\x1b]0;π ".as_bytes()), None);
        assert_eq!(p.feed(b": work\x07y").as_deref(), Some("π : work"));
        assert_eq!(p.feed(b"\x1b]2;two\x1b\\").as_deref(), Some("two"));
        assert_eq!(p.feed(b"\x1b]1;icon\x07"), None);
    }

    /// A raw reader that never reads must not freeze the program: it is dropped
    /// after `RAW_STALL` and the output (far more than the queues hold) completes.
    #[tokio::test(flavor = "multi_thread")]
    async fn stuck_raw_reader_does_not_block_the_program() {
        let dir = std::env::temp_dir().join(format!("atlas-host-raw-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let req = PtyCreate {
            cwd: "/".into(),
            argv: vec![
                "/bin/sh".into(),
                "-c".into(),
                "head -c 20000000 /dev/zero".into(),
            ],
            env: Default::default(),
            cols: 80,
            rows: 24,
            scope: Default::default(),
            title: None,
            mode: Default::default(),
        };
        let s = Session::spawn(req, &dir).unwrap();
        let (tx, _rx_never_read) = mpsc::channel(RAW_QUEUE);
        s.attach_raw(1, tx, false);
        let t0 = std::time::Instant::now();
        while s.exited().is_none() {
            assert!(
                t0.elapsed() < Duration::from_secs(20),
                "program froze behind the reader"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(s.buf.lock().raw.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
