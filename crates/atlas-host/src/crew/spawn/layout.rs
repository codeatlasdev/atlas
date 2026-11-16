//! Where a crewmate lives once it has its own Linux user, and whether that is the case at all.
//!
//! Isolation is on exactly when the spawn helper's socket exists (`atlas crew install` creates it):
//! no helper and no trace of an install means every crewmate keeps running as the crew user under
//! `<hostd dir>/crew/<id>/`, which is what a developer's Mac and every server installed before #200 do.
//! Nothing else decides it, so a half-installed server can never end up with some crewmates isolated
//! and a store that does not know it. A server that **has** the install (the crewmates' parent
//! directory, or an `atlas-cm-*` user) but no helper socket is not "isolation off": it is broken, and
//! [`detect`] says so, so tasks are refused instead of quietly running as the crew user.
//!
//! ```text
//! /var/lib/atlas-cm/<slug>/        root 0711: everyone only traverses it, nobody can replace what is in it
//!   home/                          crewmate user 0700: $HOME, the engines' sign-ins
//!   work/                          crewmate user, crew group, 2770 (setgid): the checkout; hostd writes it
//!                                  through the group (attachments), and never runs git in it
//!   crew-runtime/                  crew user, group crewmate, 0750: what the agent reads, never writes
//!   memory.md                      crewmate user, crew group, 0660: the agent appends, hostd edits
//! ```
//!
//! `<slug>` is the first 12 hex digits of the crewmate id (`validate::user_for`).

use std::path::{Path, PathBuf};

use super::validate::{CM_ROOT, dir_in, slug_of, user_for};

/// Where the helper listens (the socket unit's `ListenStream`).
pub const SOCKET_PATH: &str = "/run/atlas-crew/spawn.sock";
/// Where the provisioning service listens: the second root service, which alone may create users.
pub const PROVISION_SOCKET_PATH: &str = "/run/atlas-crew/provision.sock";
/// Overrides [`SOCKET_PATH`] (tests, and an operator running a helper by hand).
pub const SOCKET_ENV: &str = "ATLAS_CREW_SPAWN_SOCK";
/// Overrides [`PROVISION_SOCKET_PATH`].
pub const PROVISION_SOCKET_ENV: &str = "ATLAS_CREW_PROVISION_SOCK";
/// Overrides [`CM_ROOT`]: tests keep crewmate directories in a temp dir.
pub const ROOT_ENV: &str = "ATLAS_CREW_CM_ROOT";

/// The files of one isolated crewmate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Isolated {
    /// `atlas-cm-<slug>`.
    pub user: String,
    root: String,
}

impl Isolated {
    /// The isolated layout of crewmate `id` under `root`; `None` when `id` is not a crewmate id.
    pub fn under(root: &str, id: &str) -> Option<Self> {
        Some(Self {
            user: user_for(id)?,
            root: root.to_string(),
        })
    }

    /// `<root>/<slug>`: everything of this crewmate.
    pub fn dir(&self) -> PathBuf {
        // `user` was built by `user_for`, so the slug is always valid.
        PathBuf::from(dir_in(&self.root, &self.user).unwrap_or_default())
    }

    pub fn home(&self) -> PathBuf {
        self.dir().join("home")
    }

    /// The checkout (or the folder of a crewmate without a project).
    pub fn work(&self) -> PathBuf {
        self.dir().join("work")
    }

    /// What the agent may read and not change: MCP config, the omp approval extension and overlay,
    /// the hook endpoint.
    pub fn runtime(&self) -> PathBuf {
        self.dir().join("crew-runtime")
    }

    /// The memory file the prompt names: the agent appends to it, so it cannot live in the crew
    /// home the agent cannot enter.
    pub fn memory(&self) -> PathBuf {
        self.dir().join("memory.md")
    }

    /// The hook endpoint this crewmate's agent reads (`ATLAS_HOOK_ENDPOINT`): same file format as
    /// the desktop's, with a token that only opens this crewmate's routes.
    pub fn hook_endpoint(&self) -> PathBuf {
        self.runtime().join("hook-endpoint.env")
    }
}

/// Parent of every crewmate directory.
pub fn cm_root() -> String {
    std::env::var(ROOT_ENV).unwrap_or_else(|_| CM_ROOT.to_string())
}

/// The helper's socket path.
pub fn socket_path() -> PathBuf {
    std::env::var_os(SOCKET_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(SOCKET_PATH))
}

/// The provisioning service's socket path.
pub fn provision_socket_path() -> PathBuf {
    std::env::var_os(PROVISION_SOCKET_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(PROVISION_SOCKET_PATH))
}

/// Is there a helper to ask? Its socket exists and is a socket (not a stale file, not a directory).
#[cfg(unix)]
pub fn active_at(sock: &Path) -> bool {
    use std::os::unix::fs::FileTypeExt;
    std::fs::symlink_metadata(sock).is_ok_and(|m| m.file_type().is_socket())
}

/// Windows has no helper.
#[cfg(not(unix))]
pub fn active_at(_sock: &Path) -> bool {
    false
}

/// The loopback port hostd's hook server listens on when isolation is on: crewmates reach hostd
/// there and nowhere else on the machine. Fixed (not random) because the firewall names it. Lives
/// here, not in the Unix-only `provision`, so every platform's build can name it.
pub const HOOK_PORT: u16 = 47631;

/// Where the egress rules live so a reboot restores them (`atlas-crew-egress.service` loads this
/// file). Here for the same reason as [`HOOK_PORT`]: the CLI writes install scripts naming it on
/// every platform.
pub const EGRESS_FILE: &str = "/etc/atlas-crew/egress.nft";

/// Where isolation lives on this machine, decided once when hostd starts: `Some` exactly when the
/// helper's socket exists. hostd keeps it (in the crew store) instead of looking at the filesystem on
/// every call, so a server never has some paths isolated and others not within one run, and a test
/// can run its own helper without touching process-wide state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Setup {
    /// The spawn helper's socket.
    pub socket: PathBuf,
    /// The provisioning service's socket.
    pub provision_socket: PathBuf,
    /// Parent of the crewmates' directories.
    pub root: String,
    /// The loopback port hostd's hook server listens on, which the firewall lets crewmates reach
    /// (`HOOK_PORT`). 0 asks for any free port (tests, where many daemons share a machine).
    pub hook_port: u16,
}

/// What this machine says about isolation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Detected {
    /// No helper and no trace of an install: crewmates run as the crew user, as they always did.
    Off,
    /// The helper's socket exists.
    On(Setup),
    /// The server has the install (the crewmates' directory or an `atlas-cm-*` user) and no helper
    /// socket: a stopped socket unit, a failed start. Running crewmates as the crew user here would
    /// silently undo the isolation the operator installed, so tasks are refused with this text.
    Missing(String),
}

impl Setup {
    /// What this machine has, at the default or configured paths.
    pub fn detect() -> Detected {
        let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
        Self::detect_in(&socket_path(), &cm_root(), &passwd)
    }

    /// [`Setup::detect`] with the helper's socket, the crewmates' parent directory and the text of the
    /// passwd file given.
    pub fn detect_in(socket: &Path, root: &str, passwd: &str) -> Detected {
        if active_at(socket) {
            return Detected::On(Self {
                socket: socket.to_path_buf(),
                provision_socket: provision_socket_path(),
                root: root.to_string(),
                hook_port: HOOK_PORT,
            });
        }
        let has_users = passwd
            .lines()
            .filter_map(|l| l.split(':').next())
            .any(|n| slug_of(n).is_ok());
        if has_users || Path::new(root).exists() {
            return Detected::Missing(format!(
                "this server has per-crewmate isolation installed ({root} or an atlas-cm-* user exists) but the spawn helper's socket {} is missing, so no crewmate can run: running one as the crew user would undo the isolation. Start it (`sudo systemctl start atlas-crew-spawn.socket`) or re-run `atlas crew install`",
                socket.display()
            ));
        }
        Detected::Off
    }

    /// The isolated layout of crewmate `id`; `None` when `id` is not a crewmate id.
    pub fn of(&self, id: &str) -> Option<Isolated> {
        Isolated::under(&self.root, id)
    }

    /// The crewmate whose directory `dir` is in (`<root>/<slug>/...`); `None` for any other path. What
    /// tells hostd that a tree it is about to run something in is one a crewmate controls.
    pub fn owner_of(&self, dir: &Path) -> Option<Isolated> {
        let slug = dir
            .strip_prefix(&self.root)
            .ok()?
            .components()
            .next()?
            .as_os_str()
            .to_str()?;
        // `slug_of` is the one place that knows what a slug looks like.
        let user = format!("{}{slug}", super::validate::USER_PREFIX);
        slug_of(&user).ok()?;
        Some(Isolated {
            user,
            root: self.root.clone(),
        })
    }
}

/// `path`'s text, read without following a link at its last component and at most 1 MiB of it. For files
/// inside a crewmate's directory that hostd (the crew user) opens: the crewmate could have replaced one with
/// a link to a file only hostd can read, `hostd.token` or another crewmate's secrets.
#[cfg(unix)]
pub fn read_nofollow(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    open_nofollow(path, std::fs::OpenOptions::new().read(true))?
        .take(1 << 20)
        .read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// `data` written over `path`, in place, without following a link at its last component. In place on
/// purpose: replacing the file would make hostd its owner, and the agent could no longer append to it.
#[cfg(unix)]
pub fn write_nofollow(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    open_nofollow(
        path,
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true),
    )?
    .write_all(data)
}

/// `O_NONBLOCK` too: a FIFO planted in place of the file must fail or return at once, never hang hostd.
#[cfg(unix)]
fn open_nofollow(path: &Path, o: &mut std::fs::OpenOptions) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    o.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
}

#[cfg(not(unix))]
pub fn read_nofollow(path: &Path) -> std::io::Result<String> {
    Ok(String::from_utf8_lossy(&std::fs::read(path)?).into_owned())
}

#[cfg(not(unix))]
pub fn write_nofollow(path: &Path, data: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, data)
}

/// `data` written to `root/rel`, and `root/rel` read back: for files hostd (the crew user) touches inside a
/// crewmate's checkout, which the crewmate controls. `rel` is `/`-separated with no empty, `.` or `..` segment;
/// every component of it is opened relative to its parent without following a link (`openat` with
/// `O_NOFOLLOW`), directories are made on the way when writing, and the last one must be a regular file. A
/// link the crewmate planted in place of any component is an error, never a file made or read somewhere else
/// that the crew user can reach (`hostd.token`, another crewmate's `work/`). `root` itself is the path hostd
/// computed for the checkout and is opened as given. Security review of the crew isolation, N5.
#[cfg(unix)]
pub fn write_beneath(root: &Path, rel: &str, data: &mut dyn std::io::Read) -> std::io::Result<()> {
    let mut file = open_beneath(root, rel, true)?;
    std::io::copy(data, &mut file).map(|_| ())
}

/// [`write_beneath`]'s reading half: the file's text, an error when it holds more than `max` bytes.
pub fn read_beneath(root: &Path, rel: &str, max: u64) -> std::io::Result<String> {
    read_bytes_beneath(root, rel, max).map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// [`read_beneath`] for bytes (an image): never reads past `max`, so a sparse file of the crewmate's cannot fill
/// hostd's memory.
#[cfg(unix)]
pub fn read_bytes_beneath(root: &Path, rel: &str, max: u64) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let file = open_beneath(root, rel, false)?;
    if file.metadata()?.len() > max {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file too large",
        ));
    }
    let mut bytes = Vec::new();
    file.take(max).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// `root/rel` opened for reading the way [`read_bytes_beneath`] does (no link followed anywhere on the way, a
/// regular file), for a caller that streams it.
#[cfg(unix)]
pub fn open_regular_beneath(root: &Path, rel: &str) -> std::io::Result<std::fs::File> {
    open_beneath(root, rel, false)
}

/// Whether `work` already holds a checkout: its `.git` exists and is not a link. hostd decides with this whether to
/// clone into a crewmate's tree, and a `.git` the crewmate made a link (to another crewmate's repository, say)
/// is not one, and is never asked about through the link.
pub fn has_checkout(work: &Path) -> bool {
    std::fs::symlink_metadata(work.join(".git")).is_ok_and(|m| !m.file_type().is_symlink())
}

/// What a directory entry is, without following a link: a link is [`Kind::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
    Other,
}

/// The entries of the directory `root/rel` (`rel` empty: `root` itself), sorted by name, each with its [`Kind`]:
/// the directory is opened one component at a time without following a link, like [`read_bytes_beneath`], so a
/// directory the crewmate swapped for a link is an error and never a listing of what it points at.
#[cfg(unix)]
pub fn read_dir_beneath(root: &Path, rel: &str) -> std::io::Result<Vec<(String, Kind)>> {
    use std::ffi::{CStr, CString};
    use std::io::{Error, ErrorKind};
    use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;

    let invalid = || Error::new(ErrorKind::InvalidInput, format!("bad path {rel:?}"));
    let open = |at: Option<&OwnedFd>, name: &CString| -> std::io::Result<OwnedFd> {
        let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        // SAFETY: a NUL-terminated name, and a descriptor that is open (or AT_FDCWD for the absolute root).
        let fd = unsafe {
            libc::openat(
                at.map_or(libc::AT_FDCWD, |d| d.as_raw_fd()),
                name.as_ptr(),
                flags,
            )
        };
        // SAFETY: a fresh descriptor from openat that nothing else owns.
        (fd >= 0)
            .then(|| unsafe { OwnedFd::from_raw_fd(fd) })
            .ok_or_else(Error::last_os_error)
    };
    // `root` is the path hostd computed and is opened as given (a link in it is hostd's own doing, not the crewmate's).
    let mut dir = {
        let c = CString::new(root.as_os_str().as_bytes()).map_err(|_| invalid())?;
        // SAFETY: as above.
        let fd = unsafe {
            libc::open(
                c.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };
        // SAFETY: a fresh descriptor.
        (fd >= 0)
            .then(|| unsafe { OwnedFd::from_raw_fd(fd) })
            .ok_or_else(Error::last_os_error)?
    };
    if !rel.is_empty() {
        for part in rel.split('/') {
            if part.is_empty() || part == "." || part == ".." || part.contains('\0') {
                return Err(invalid());
            }
            dir = open(Some(&dir), &CString::new(part).map_err(|_| invalid())?)?;
        }
    }
    let dup = open(Some(&dir), &CString::new(".").map_err(|_| invalid())?)?;
    // SAFETY: `fdopendir` takes ownership of `dup` on success; on failure we still own it and close it by dropping.
    let stream = unsafe { libc::fdopendir(dup.as_raw_fd()) };
    if stream.is_null() {
        return Err(Error::last_os_error());
    }
    let _ = dup.into_raw_fd();
    let mut out = vec![];
    loop {
        // SAFETY: `stream` is a live DIR*; readdir returns null at the end (errno is not checked: a short
        // listing is only a shorter list of candidates).
        let e = unsafe { libc::readdir(stream) };
        if e.is_null() {
            break;
        }
        // SAFETY: `d_name` is NUL-terminated inside the dirent readdir returned.
        let name = unsafe { CStr::from_ptr((*e).d_name.as_ptr()) };
        let Ok(name) = name.to_str() else { continue };
        if name == "." || name == ".." {
            continue;
        }
        // SAFETY: reading a plain field of the dirent.
        let kind = match unsafe { (*e).d_type } {
            libc::DT_DIR => Kind::Dir,
            libc::DT_REG => Kind::File,
            libc::DT_UNKNOWN => {
                let c = CString::new(name).map_err(|_| invalid())?;
                // SAFETY: a zeroed stat is a valid out-parameter; `dir` is open.
                let mut st: libc::stat = unsafe { std::mem::zeroed() };
                let r = unsafe {
                    libc::fstatat(
                        dir.as_raw_fd(),
                        c.as_ptr(),
                        &mut st,
                        libc::AT_SYMLINK_NOFOLLOW,
                    )
                };
                match (r, st.st_mode & libc::S_IFMT) {
                    (0, libc::S_IFDIR) => Kind::Dir,
                    (0, libc::S_IFREG) => Kind::File,
                    _ => Kind::Other,
                }
            }
            _ => Kind::Other,
        };
        out.push((name.to_string(), kind));
    }
    // SAFETY: closes the stream (and the descriptor it owns) exactly once.
    unsafe { libc::closedir(stream) };
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

#[cfg(not(unix))]
pub fn read_dir_beneath(root: &Path, rel: &str) -> std::io::Result<Vec<(String, Kind)>> {
    let mut out = vec![];
    for e in std::fs::read_dir(root.join(rel))?.flatten() {
        let Ok(t) = e.file_type() else { continue };
        let kind = if t.is_dir() {
            Kind::Dir
        } else if t.is_file() {
            Kind::File
        } else {
            Kind::Other
        };
        out.push((e.file_name().to_string_lossy().into_owned(), kind));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Whether `e` is what the `*_beneath` opens answer for a link on the way (`O_NOFOLLOW`: ELOOP, or ENOTDIR when
/// the link stands for a directory).
pub fn is_link_refusal(e: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        matches!(e.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR))
    }
    #[cfg(not(unix))]
    {
        let _ = e;
        false
    }
}

#[cfg(not(unix))]
pub fn open_regular_beneath(root: &Path, rel: &str) -> std::io::Result<std::fs::File> {
    std::fs::File::open(root.join(rel))
}

#[cfg(unix)]
fn open_beneath(root: &Path, rel: &str, write: bool) -> std::io::Result<std::fs::File> {
    use std::ffi::CString;
    use std::io::{Error, ErrorKind};
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;

    let invalid = || Error::new(ErrorKind::InvalidInput, format!("bad path {rel:?}"));
    let parts: Vec<&str> = rel.split('/').collect();
    if parts
        .iter()
        .any(|p| p.is_empty() || *p == "." || *p == ".." || p.contains('\0'))
    {
        return Err(invalid());
    }
    let own = |fd: libc::c_int| -> std::io::Result<OwnedFd> {
        // SAFETY: `fd` is a fresh descriptor from open/openat that nothing else owns.
        (fd >= 0)
            .then(|| unsafe { OwnedFd::from_raw_fd(fd) })
            .ok_or_else(Error::last_os_error)
    };
    let root = CString::new(root.as_os_str().as_bytes()).map_err(|_| invalid())?;
    // SAFETY: a NUL-terminated path.
    let mut dir = own(unsafe {
        libc::open(
            root.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    })?;
    let (last, parents) = parts.split_last().ok_or_else(invalid)?;
    for part in parents {
        use std::os::fd::AsRawFd;
        let c = CString::new(*part).map_err(|_| invalid())?;
        if write {
            // SAFETY: `dir` is open, `c` is NUL-terminated.
            if unsafe { libc::mkdirat(dir.as_raw_fd(), c.as_ptr(), 0o755) } == 0 {
                // What hostd makes is for the crewmate to walk through, whatever hostd's umask is.
                let made = own(unsafe {
                    libc::openat(
                        dir.as_raw_fd(),
                        c.as_ptr(),
                        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                    )
                })?;
                // SAFETY: plain fchmod on a directory we just made and hold open.
                unsafe { libc::fchmod(made.as_raw_fd(), 0o755) };
            } else if Error::last_os_error().kind() != ErrorKind::AlreadyExists {
                return Err(Error::last_os_error());
            }
        }
        // SAFETY: as above. A link here is ELOOP (or ENOTDIR), never followed.
        dir = own(unsafe {
            libc::openat(
                dir.as_raw_fd(),
                c.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })?;
    }
    use std::os::fd::AsRawFd;
    let c = CString::new(*last).map_err(|_| invalid())?;
    // O_NONBLOCK: a FIFO planted in place of the file fails (ENXIO) or is refused below, never hangs hostd.
    let flags = libc::O_NOFOLLOW
        | libc::O_NONBLOCK
        | libc::O_CLOEXEC
        | if write {
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC
        } else {
            libc::O_RDONLY
        };
    // SAFETY: as above; the mode applies only when the file is created.
    let fd =
        own(unsafe { libc::openat(dir.as_raw_fd(), c.as_ptr(), flags, 0o644 as libc::c_uint) })?;
    let file = std::fs::File::from(fd);
    if !file.metadata()?.is_file() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("{rel:?} is not a regular file"),
        ));
    }
    if write {
        // SAFETY: plain fchmod on a regular file we hold open: readable by the crewmate, whatever hostd's umask.
        unsafe { libc::fchmod(file.as_raw_fd(), 0o644) };
    }
    Ok(file)
}

#[cfg(not(unix))]
pub fn write_beneath(root: &Path, rel: &str, data: &mut dyn std::io::Read) -> std::io::Result<()> {
    let path = root.join(rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::io::copy(data, &mut std::fs::File::create(path)?).map(|_| ())
}

#[cfg(not(unix))]
pub fn read_bytes_beneath(root: &Path, rel: &str, max: u64) -> std::io::Result<Vec<u8>> {
    let path = root.join(rel);
    if std::fs::metadata(&path)?.len() > max {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "file too large",
        ));
    }
    std::fs::read(path)
}

#[cfg(all(test, unix))]
mod beneath_tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, symlink};

    fn put(root: &Path, rel: &str, bytes: &[u8]) -> std::io::Result<()> {
        write_beneath(root, rel, &mut &bytes[..])
    }

    // Security review of the crew isolation, N5: hostd (the crew user) staged attachments into the crewmate's
    // `work/` with `create_dir_all` and `fs::copy`, which follow every link on the way, so a crewmate that made
    // `.atlas` (or `.atlas/attachments`, or the file) a link made hostd create or overwrite files in any
    // directory the crew user can write: `hostd.token`, another crewmate's `work/`.
    #[test]
    fn files_land_under_the_root_with_their_directories_made() {
        let t = tempfile::tempdir().unwrap();
        put(t.path(), ".atlas/attachments/t1/a.png", b"png").unwrap();
        assert_eq!(
            std::fs::read(t.path().join(".atlas/attachments/t1/a.png")).unwrap(),
            b"png"
        );
        // Readable by the crewmate (a different user, "others" of the file), and rewritten in place.
        let mode = std::fs::metadata(t.path().join(".atlas/attachments/t1/a.png"))
            .unwrap()
            .mode();
        assert_eq!(mode & 0o044, 0o044, "{mode:o}");
        put(t.path(), ".atlas/attachments/t1/a.png", b"new").unwrap();
        assert_eq!(
            std::fs::read(t.path().join(".atlas/attachments/t1/a.png")).unwrap(),
            b"new"
        );
    }

    #[test]
    fn a_link_in_place_of_a_directory_on_the_way_is_never_followed_or_written_through() {
        for planted in [".atlas", ".atlas/attachments", ".atlas/attachments/t1"] {
            let t = tempfile::tempdir().unwrap();
            let victim = tempfile::tempdir().unwrap();
            // Everything above the link is real, the link points at somebody else's directory.
            if let Some(parent) = Path::new(planted).parent() {
                std::fs::create_dir_all(t.path().join(parent)).unwrap();
            }
            symlink(victim.path(), t.path().join(planted)).unwrap();
            let err = put(t.path(), ".atlas/attachments/t1/a.png", b"png").unwrap_err();
            assert_ne!(err.kind(), std::io::ErrorKind::NotFound, "{planted}: {err}");
            let left: Vec<_> = walk(victim.path());
            assert!(
                left.is_empty(),
                "{planted}: wrote through the link: {left:?}"
            );
        }
    }

    #[test]
    fn a_link_in_place_of_the_file_is_not_written_through() {
        let t = tempfile::tempdir().unwrap();
        let victim = tempfile::tempdir().unwrap();
        let secret = victim.path().join("hostd.token");
        std::fs::write(&secret, "the token").unwrap();
        std::fs::create_dir_all(t.path().join(".atlas/attachments/t1")).unwrap();
        symlink(&secret, t.path().join(".atlas/attachments/t1/a.png")).unwrap();
        assert!(put(t.path(), ".atlas/attachments/t1/a.png", b"png").is_err());
        assert_eq!(std::fs::read_to_string(&secret).unwrap(), "the token");
    }

    // A FIFO in place of the file would block a blocking open for good: the crew server's task would hang.
    #[test]
    fn a_fifo_in_place_of_the_file_fails_at_once() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("d")).unwrap();
        let fifo = std::ffi::CString::new(t.path().join("d/a.png").to_str().unwrap()).unwrap();
        // SAFETY: a NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let start = std::time::Instant::now();
        assert!(put(t.path(), "d/a.png", b"png").is_err());
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
    }

    // Security review of the crew isolation, S3: `locate` read a screenshot with `fs::read`, with no size limit
    // and following links; a sparse 50 GB `x.png` in the crewmate's tree filled hostd's memory.
    #[test]
    fn bytes_are_read_up_to_a_limit_and_never_through_a_link() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("ok.png"), vec![1u8; 100]).unwrap();
        std::fs::File::create(t.path().join("huge.png"))
            .unwrap()
            .set_len(1 << 40)
            .unwrap();
        symlink(t.path().join("ok.png"), t.path().join("link.png")).unwrap();
        assert_eq!(
            read_bytes_beneath(t.path(), "ok.png", 1000).unwrap().len(),
            100
        );
        assert!(read_bytes_beneath(t.path(), "ok.png", 99).is_err());
        assert!(read_bytes_beneath(t.path(), "huge.png", 32 << 20).is_err());
        assert!(read_bytes_beneath(t.path(), "link.png", 1000).is_err());
    }

    #[test]
    fn a_path_that_climbs_or_has_empty_segments_is_refused() {
        let t = tempfile::tempdir().unwrap();
        for bad in ["../x", "a/../../x", "a//b", "/abs", "", "a/./b"] {
            assert!(put(t.path(), bad, b"x").is_err(), "{bad:?}");
        }
        assert!(walk(t.path()).is_empty());
    }

    fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
        let mut out = vec![];
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            out.push(e.path());
            if e.file_type().unwrap().is_dir() {
                out.extend(walk(&e.path()));
            }
        }
        out
    }

    #[test]
    fn a_listing_never_enters_a_link_and_marks_what_it_finds() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("work");
        let other = t.path().join("other");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(root.join("a.txt"), "a").unwrap();
        std::fs::write(other.join("secret"), "s").unwrap();
        symlink(&other, root.join("dirlink")).unwrap();
        symlink(other.join("secret"), root.join("filelink")).unwrap();
        assert_eq!(
            read_dir_beneath(&root, "").unwrap(),
            [
                ("a.txt".into(), Kind::File),
                ("dirlink".into(), Kind::Other),
                ("filelink".into(), Kind::Other),
                ("src".into(), Kind::Dir),
            ]
        );
        assert!(
            read_dir_beneath(&root, "dirlink").is_err(),
            "listed what a link points at"
        );
        std::fs::write(root.join("src/b.txt"), "b").unwrap();
        assert_eq!(
            read_dir_beneath(&root, "src").unwrap(),
            [("b.txt".into(), Kind::File)]
        );
        assert!(read_dir_beneath(&root, "../other").is_err());
        assert!(read_dir_beneath(&root, "a.txt").is_err());
    }

    #[test]
    fn a_dot_git_that_is_a_link_is_not_a_checkout() {
        let t = tempfile::tempdir().unwrap();
        let work = t.path().join("work");
        let other = t.path().join("other/.git");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        assert!(!has_checkout(&work), "no .git");
        symlink(&other, work.join(".git")).unwrap();
        assert!(!has_checkout(&work), "a link to another repository");
        std::fs::remove_file(work.join(".git")).unwrap();
        std::fs::create_dir(work.join(".git")).unwrap();
        assert!(has_checkout(&work));
        std::fs::remove_dir(work.join(".git")).unwrap();
        std::fs::write(work.join(".git"), "gitdir: x").unwrap();
        assert!(has_checkout(&work), "a worktree's .git file");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";

    #[test]
    fn every_path_of_a_crewmate_is_under_its_own_slug() {
        let i = Isolated::under("/var/lib/atlas-cm", ID).unwrap();
        assert_eq!(i.user, "atlas-cm-3f2a9c1e5b7d");
        assert_eq!(i.dir(), PathBuf::from("/var/lib/atlas-cm/3f2a9c1e5b7d"));
        assert_eq!(i.home(), i.dir().join("home"));
        assert_eq!(i.work(), i.dir().join("work"));
        assert_eq!(i.runtime(), i.dir().join("crew-runtime"));
        assert_eq!(i.memory(), i.dir().join("memory.md"));
        assert_eq!(
            i.hook_endpoint(),
            PathBuf::from("/var/lib/atlas-cm/3f2a9c1e5b7d/crew-runtime/hook-endpoint.env")
        );
        for p in [i.home(), i.work(), i.runtime(), i.memory()] {
            assert!(p.starts_with(i.dir()), "{p:?}");
        }
    }

    // Two crewmates never share a directory: the slug is 12 hex digits of the id, and ids that
    // differ in them (every real one does) land in different directories.
    #[test]
    fn different_crewmates_get_different_directories() {
        let other = "4f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";
        let a = Isolated::under("/r", ID).unwrap();
        let b = Isolated::under("/r", other).unwrap();
        assert_ne!(a.dir(), b.dir());
        assert_ne!(a.user, b.user);
    }

    // Anything that is not a lowercase UUID has no layout: a path-shaped "id" can never reach the
    // directory arithmetic, which is what keeps `..` out of root-run code.
    #[test]
    fn a_non_uuid_id_has_no_isolated_layout() {
        for bad in [
            "",
            "../etc",
            "3F2A9C1E-5B7D-4E8F-9A0B-1C2D3E4F5A6B",
            "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6",
            "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b/..",
            "../../../../../tmp/x/3f2a9c1e-5b7d-4e8f-9a0b-1c",
        ] {
            assert!(Isolated::under("/r", bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn setup_gives_each_crewmate_its_layout_under_its_root_and_none_for_a_bad_id() {
        let s = Setup {
            socket: "/s".into(),
            provision_socket: "/p".into(),
            root: "/r".into(),
            hook_port: 0,
        };
        assert_eq!(s.of(ID).unwrap().dir(), PathBuf::from("/r/3f2a9c1e5b7d"));
        assert!(s.of("../x").is_none());
    }

    // The point of "no helper = today's behaviour": a missing socket, a regular file at the path
    // and a directory at the path are all "not active".
    #[cfg(unix)]
    #[test]
    fn isolation_is_active_only_when_the_helpers_socket_exists() {
        use std::os::unix::net::UnixListener;
        let t = tempfile::tempdir().unwrap();
        let sock = t.path().join("spawn.sock");
        let check = active_at;
        assert!(!check(&sock), "nothing there");
        std::fs::write(&sock, b"").unwrap();
        assert!(!check(&sock), "a regular file is not a helper");
        std::fs::remove_file(&sock).unwrap();
        std::fs::create_dir(&sock).unwrap();
        assert!(!check(&sock), "a directory is not a helper");
        std::fs::remove_dir(&sock).unwrap();
        let _l = UnixListener::bind(&sock).unwrap();
        assert!(check(&sock), "a listening socket is");
    }

    // Regression (security review of #448, should-fix 1): isolation used to be on only if the helper's
    // socket existed when hostd started. A server whose socket unit was stopped (or whose helper never
    // started, #430) then ran every crewmate as the crew user again, with no error. An installed server with
    // no socket is `Missing`; only a machine with no trace of an install is `Off`.
    #[cfg(unix)]
    #[test]
    fn an_installed_server_without_its_helper_socket_is_missing_never_off() {
        use std::os::unix::net::UnixListener;
        let t = tempfile::tempdir().unwrap();
        let sock = t.path().join("spawn.sock");
        let root = t.path().join("atlas-cm");
        let root_s = root.to_string_lossy().into_owned();
        let user = "atlas-cm-3f2a9c1e5b7d:x:991:991::/var/lib/atlas-cm/3f2a9c1e5b7d/home:/usr/sbin/nologin\n";
        let other = "root:x:0:0::/root:/bin/bash\natlas-crew:x:990:990::/x:/bin/bash\natlas-cm-short:x:1:1::/x:/bin/false\n";

        // A developer's machine: no root, no crewmate user (look-alikes do not count), no socket.
        assert_eq!(Setup::detect_in(&sock, &root_s, other), Detected::Off);
        assert_eq!(Setup::detect_in(&sock, &root_s, ""), Detected::Off);

        // The crewmates' directory alone is a trace; so is a crewmate user alone.
        std::fs::create_dir(&root).unwrap();
        for passwd in [other, user] {
            match Setup::detect_in(&sock, &root_s, passwd) {
                Detected::Missing(why) => {
                    assert!(why.contains(&sock.display().to_string()), "{why}");
                    assert!(why.contains("atlas-crew-spawn.socket"), "{why}");
                }
                d => panic!("{d:?}"),
            }
        }
        std::fs::remove_dir(&root).unwrap();
        assert!(matches!(
            Setup::detect_in(&sock, &root_s, user),
            Detected::Missing(_)
        ));

        // With the socket there, it is on, at the paths asked for.
        let _l = UnixListener::bind(&sock).unwrap();
        match Setup::detect_in(&sock, &root_s, user) {
            Detected::On(s) => assert_eq!((s.socket, s.root), (sock.clone(), root_s)),
            d => panic!("{d:?}"),
        }
    }

    // Regression (merging #200 with the guards of #214 and the attachments of #209): hostd runs git in the
    // agent's tree, and which crewmate's it is comes from the path alone. Only a path inside a crewmate's
    // directory under the configured root is one.
    #[test]
    fn owner_of_names_whichever_crewmate_owns_the_directory_and_nobody_for_other_paths() {
        let s = Setup {
            socket: "/s".into(),
            provision_socket: "/p".into(),
            root: "/r".into(),
            hook_port: 0,
        };
        let want = s.of(ID).unwrap();
        for inside in [
            "/r/3f2a9c1e5b7d/work",
            "/r/3f2a9c1e5b7d/work/crates/x/src",
            "/r/3f2a9c1e5b7d",
        ] {
            assert_eq!(
                s.owner_of(Path::new(inside)),
                Some(want.clone()),
                "{inside}"
            );
        }
        for outside in [
            "/r",
            "/r/not-a-slug/work",
            "/r/3f2a9c1e5b7/work", // 11 digits
            "/r/3f2a9c1e5b7dx/work",
            "/other/3f2a9c1e5b7d/work",
            "/r2/3f2a9c1e5b7d/work",
            "/home/me/project",
        ] {
            assert!(s.owner_of(Path::new(outside)).is_none(), "{outside}");
        }
    }
}
