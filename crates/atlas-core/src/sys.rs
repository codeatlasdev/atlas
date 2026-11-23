//! OS primitives the CLI, runtime, hostd and app share, so their call sites stay platform-neutral:
//! pid probes and kills, advisory file locks, randomness, detached spawns and the command shell.
//! Unix keeps its signal and `flock` semantics; Windows maps them onto Win32.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

/// True while `pid` names a live process (EPERM counts as alive on Unix).
pub fn pid_alive(pid: u32) -> bool {
    imp::pid_alive(pid)
}

/// Ask `pid` to stop: `SIGTERM` on Unix. Windows has no polite stop for an arbitrary process, so
/// this ends it (`TerminateProcess`). False when it could not be signalled.
pub fn terminate(pid: u32) -> bool {
    imp::terminate(pid)
}

/// End `pid` now: `SIGKILL` on Unix, `TerminateProcess` on Windows.
pub fn force_kill(pid: u32) -> bool {
    imp::force_kill(pid)
}

/// Identity of the file behind an open handle (device + inode on Unix, volume serial + file index
/// on Windows): two handles name the same file exactly when their identities are equal, even
/// after a rename. `None` when the OS will not say.
pub fn file_identity(f: &File) -> Option<(u64, u64)> {
    imp::file_identity(f)
}

/// Files created through `o` are readable by this user only: mode 0600 on Unix. On Windows a new
/// file takes its folder's ACL, and `~/.atlas` lies in the user's private profile.
pub fn owner_only(o: &mut std::fs::OpenOptions) -> &mut std::fs::OpenOptions {
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(o, 0o600);
    o
}

/// Our parent's pid (0 when unknown).
pub fn parent_pid() -> u32 {
    imp::parent_pid()
}

/// Take an exclusive, non-blocking advisory lock on `file`, held until the file is closed.
/// `Ok(false)` when another process holds it. On Windows the lock covers a byte far past the end
/// of the file, so the file's own contents (a pid) stay readable by the processes that lost.
pub fn try_lock(file: &File) -> io::Result<bool> {
    imp::try_lock(file)
}

/// Replace `to` with `from` in one step, atomically for anyone opening the name `to` meanwhile: they get the old
/// file or the new one, never "not found". `rename(2)` on Unix; on Windows `std::fs::rename` does not promise that
/// (it can leave the name missing for an instant while a reader holds the old file), so the rename is done with POSIX
/// semantics there. Not for directories.
pub fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    imp::replace_file(from, to)
}

/// What [`replace_file`] does after the POSIX rename failed (Windows).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub(crate) enum RenameStep {
    /// A refusal that ends on its own (the antivirus or the indexer holds the new file without delete sharing for a
    /// moment): wait this long, then rename again.
    Retry(std::time::Duration),
    /// Anything else the POSIX rename failed with: the replace fails and the old file stays under its name.
    /// `std::fs::rename` is not an answer to a refusal that is not about the file system (the same open handle refuses
    /// it too) or to an error it would repeat; where it does succeed it is the legacy replace that can leave the name
    /// missing or the old file delete-pending for a reader (#743).
    GiveUp,
    /// Not supported (an old Windows, not NTFS, write-protected media): `std::fs::rename`, all that is left there.
    Fallback,
}

/// The step after the POSIX rename's attempt number `attempt` (0-based) failed with Windows error `code`. Waits double
/// from 1 ms and stop at 50 ms each, 28 retries (about 1.2 s in all): a scan of a new file by an antivirus under a
/// loaded machine (CI) outlasted 160 ms, and the write stalls only in that case.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub(crate) fn after_posix_rename_error(code: Option<i32>, attempt: u32) -> RenameStep {
    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const RETRIES: u32 = 28;
    match code {
        Some(ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION) if attempt < RETRIES => {
            RenameStep::Retry(std::time::Duration::from_millis(
                (1u64 << attempt.min(6)).min(50),
            ))
        }
        // Invalid function, write protect, not supported, invalid parameter, not implemented: no POSIX rename here.
        Some(1 | 19 | 50 | 87 | 120) | None => RenameStep::Fallback,
        Some(_) => RenameStep::GiveUp,
    }
}

/// The Windows replace loop with the POSIX rename given, so tests refuse it on every OS. A refusal that passes is
/// retried, one that outlasts the retries fails the replace with the old file untouched (the name never leaves it),
/// and a rename that cannot be done at all, or a readonly target (which refuses like a scan does, and waiting does
/// not help), goes to `std::fs::rename`, which has its own readonly handling.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub(crate) fn replace_file_via(
    from: &Path,
    to: &Path,
    mut posix_rename: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let mut attempt = 0;
    loop {
        let Err(e) = posix_rename(from, to) else {
            return Ok(());
        };
        let readonly = std::fs::metadata(to).is_ok_and(|m| m.permissions().readonly());
        match after_posix_rename_error(e.raw_os_error(), attempt) {
            _ if readonly => return imp::fallback_replace(from, to),
            RenameStep::Retry(wait) => {
                std::thread::sleep(wait);
                attempt += 1;
            }
            RenameStep::GiveUp => return Err(e),
            RenameStep::Fallback => return imp::fallback_replace(from, to),
        }
    }
}

/// Fill `buf` from the OS CSPRNG.
pub fn random_bytes(buf: &mut [u8]) -> io::Result<()> {
    imp::random_bytes(buf)
}

/// This machine's host name, `None` when unreadable.
pub fn hostname() -> Option<String> {
    imp::hostname()
}

/// The 1, 5 and 15 minute load averages; `None` where the OS has none (Windows).
pub fn load_avg() -> Option<(f64, f64, f64)> {
    imp::load_avg()
}

/// Percent of physical memory the OS can hand out without hurting (0-100); `None` when unreadable.
/// macOS: `kern.memorystatus_level` (the figure the kernel's own pressure logic uses, free plus
/// reclaimable); Linux: `MemAvailable / MemTotal`; Windows: `GlobalMemoryStatusEx`.
pub fn mem_available_pct() -> Option<u8> {
    imp::mem_available_pct()
}

/// Installed physical memory in bytes, `None` when unreadable.
pub fn mem_total_bytes() -> Option<u64> {
    imp::mem_total_bytes()
}

/// `(MemAvailable, MemTotal)` of a `/proc/meminfo` text, in kB. Pure so it is testable anywhere.
#[cfg_attr(all(target_os = "macos", not(test)), allow(dead_code))]
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn parse_meminfo(text: &str) -> Option<(u64, u64)> {
    let field = |name: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix(':'))
            .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
    };
    Some((field("MemAvailable")?, field("MemTotal")?)).filter(|(_, total)| *total > 0)
}

/// The executable file of `pid`, `None` when it is gone or unreadable.
pub fn exe_path(pid: u32) -> Option<PathBuf> {
    imp::exe_path(pid)
}

/// The full command line of `pid` (program and arguments, space separated as the OS shows it).
pub fn command_line(pid: u32) -> Option<String> {
    imp::command_line(pid)
}

pub use crate::port_owner::{PortOwner, port_owner};

/// Does the file name `name` (a path's last component, any case) name the program `program`?
/// `atlas` matches `atlas`, and on Windows also `atlas.exe`.
pub fn is_program(name: &str, program: &str) -> bool {
    name == program
        || (cfg!(windows)
            && name
                .to_ascii_lowercase()
                .strip_suffix(".exe")
                .is_some_and(|n| n == program))
}

/// Run the child in its own session (Unix `setsid`) or detached from our console and Ctrl-C
/// group (Windows), so it outlives us and our terminal's signals.
pub fn detach(cmd: &mut std::process::Command) {
    imp::detach(cmd)
}

/// [`detach`] for a tokio command.
pub fn detach_tokio(cmd: &mut tokio::process::Command) {
    imp::detach_tokio(cmd)
}

/// Spawn `cmd` [`detach`]ed so it outlives us. Windows also tries to leave our job object
/// (terminals, IDEs and installers kill their job's processes with them), and spawns plainly
/// detached when the job forbids that.
pub fn spawn_detached(cmd: &mut std::process::Command) -> io::Result<std::process::Child> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(imp::DETACHED | imp::CREATE_BREAKAWAY_FROM_JOB);
        if let Ok(c) = cmd.spawn() {
            return Ok(c);
        }
        cmd.creation_flags(imp::DETACHED);
        cmd.spawn()
    }
    #[cfg(unix)]
    {
        detach(cmd);
        cmd.spawn()
    }
}

/// Spawn options that only mean something on one OS, for std and tokio commands alike.
pub trait CommandExt {
    /// Windows: start the child without a console window (`CREATE_NO_WINDOW`). Atlas Desktop and
    /// hostd have no console, so every console program they run (git, a service, a shell) would
    /// otherwise flash one up. No-op elsewhere.
    fn no_window(&mut self) -> &mut Self;
    /// [`detach`] as a builder method.
    fn detached(&mut self) -> &mut Self;
}

impl CommandExt for std::process::Command {
    fn no_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(self, imp::CREATE_NO_WINDOW);
        self
    }
    fn detached(&mut self) -> &mut Self {
        detach(self);
        self
    }
}

impl CommandExt for tokio::process::Command {
    fn no_window(&mut self) -> &mut Self {
        #[cfg(windows)]
        self.creation_flags(imp::CREATE_NO_WINDOW);
        self
    }
    fn detached(&mut self) -> &mut Self {
        detach_tokio(self);
        self
    }
}

/// Linux: start the child with `PR_SET_NO_NEW_PRIVS`, so it never gains the file capabilities of its binary. Why it
/// matters: `/usr/bin/sway` carries `cap_sys_nice=ep`, and a process that gained capabilities at exec has an unreadable
/// `/proc/<pid>/environ` even for its owner, so the `ATLAS_SESSION` tag the hygiene sweep finds orphans by is
/// invisible and a sway left behind by a crash is never reaped. The child only loses what the capability gave
/// (sway: realtime priority). No-op elsewhere.
pub fn no_new_privs(cmd: &mut std::process::Command) {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: the closure runs between fork and exec and only calls prctl(2), which is async-signal-safe.
        unsafe {
            std::os::unix::process::CommandExt::pre_exec(cmd, || {
                if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = cmd;
}

#[cfg(unix)]
static INTERRUPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Unix: from now on SIGINT and SIGTERM only set a flag ([`interrupted`]) instead of ending the process, so a
/// long start can stop what it already launched and exit by itself. The caller must poll [`interrupted`].
#[cfg(unix)]
pub fn watch_interrupts() {
    extern "C" fn on_signal(_: libc::c_int) {
        INTERRUPTED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    for sig in [libc::SIGINT, libc::SIGTERM] {
        // SAFETY: the handler only stores to an atomic, which is async-signal-safe.
        unsafe { libc::signal(sig, on_signal as *const () as libc::sighandler_t) };
    }
}

/// Whether SIGINT or SIGTERM arrived since [`watch_interrupts`].
#[cfg(unix)]
pub fn interrupted() -> bool {
    INTERRUPTED.load(std::sync::atomic::Ordering::Relaxed)
}

/// A command that runs `script` through the platform shell: `sh -c` on Unix, `cmd.exe /d /s /c`
/// on Windows (what npm uses for scripts).
pub fn shell(script: &str) -> std::process::Command {
    imp::shell(script)
}

/// The argv that runs `script` through the platform shell, for requests that carry an argv (a
/// hostd `exec`, a terminal): `sh -c <script>`, or `cmd.exe /d /s /c <script>` on Windows, which
/// hostd recognises ([`shell_script`]) and hands to cmd verbatim.
pub fn shell_argv(script: &str) -> Vec<String> {
    let shell: &[&str] = if cfg!(windows) {
        &["cmd.exe", "/d", "/s", "/c"]
    } else {
        &["sh", "-c"]
    };
    shell
        .iter()
        .copied()
        .chain([script])
        .map(String::from)
        .collect()
}

/// The script of a Windows [`shell_argv`]; `None` for any other argv (and always on Unix, where
/// `sh -c` takes its script as an ordinary argument).
pub fn shell_script(argv: &[String]) -> Option<&str> {
    match argv {
        [exe, d, s, c, script]
            if cfg!(windows)
                && is_program(
                    &std::path::Path::new(exe)
                        .file_name()?
                        .to_string_lossy()
                        .to_ascii_lowercase(),
                    "cmd",
                )
                && d.eq_ignore_ascii_case("/d")
                && s.eq_ignore_ascii_case("/s")
                && c.eq_ignore_ascii_case("/c") =>
        {
            Some(script)
        }
        _ => None,
    }
}

/// Copy-on-write clone of a directory tree or of one file; `Unsupported` where the file system
/// cannot (never a full copy). See `sys_clone`.
pub use crate::sys_clone::{clone_dir, clone_file};

/// [`shell`] for a tokio command.
pub fn shell_tokio(script: &str) -> tokio::process::Command {
    imp::shell_tokio(script)
}

#[cfg(unix)]
mod imp {
    use super::*;
    use std::os::unix::io::AsRawFd;
    use std::os::unix::process::CommandExt;

    pub fn pid_alive(pid: u32) -> bool {
        if pid == 0 || pid > i32::MAX as u32 {
            return false;
        }
        // SAFETY: signal 0 only checks existence.
        let sent = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
        sent || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    fn signal(pid: u32, sig: libc::c_int) -> bool {
        // Never 0 or a negative number: those address process groups.
        if pid == 0 || pid > i32::MAX as u32 {
            return false;
        }
        // SAFETY: kill(2) with an exact positive pid has no memory preconditions.
        unsafe { libc::kill(pid as libc::pid_t, sig) == 0 }
    }

    pub fn terminate(pid: u32) -> bool {
        signal(pid, libc::SIGTERM)
    }

    pub fn force_kill(pid: u32) -> bool {
        signal(pid, libc::SIGKILL)
    }

    pub fn parent_pid() -> u32 {
        // SAFETY: getppid has no preconditions.
        unsafe { libc::getppid() as u32 }
    }

    pub fn file_identity(f: &File) -> Option<(u64, u64)> {
        use std::os::unix::fs::MetadataExt;
        let m = f.metadata().ok()?;
        Some((m.dev(), m.ino()))
    }

    pub fn try_lock(file: &File) -> io::Result<bool> {
        // SAFETY: the fd is open for the lifetime of `file`.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(true);
        }
        let e = io::Error::last_os_error();
        if e.raw_os_error() == Some(libc::EWOULDBLOCK) {
            Ok(false)
        } else {
            Err(e)
        }
    }

    pub fn random_bytes(buf: &mut [u8]) -> io::Result<()> {
        use std::io::Read;
        File::open("/dev/urandom")?.read_exact(buf)
    }

    pub fn hostname() -> Option<String> {
        let mut buf = [0u8; 256];
        // SAFETY: buf is valid for buf.len() bytes; gethostname NUL-terminates on success.
        let rc = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
        if rc != 0 {
            return None;
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
        Some(String::from_utf8_lossy(&buf[..end]).into_owned())
    }

    pub fn load_avg() -> Option<(f64, f64, f64)> {
        let mut l = [0f64; 3];
        // SAFETY: `l` holds the 3 doubles getloadavg writes.
        (unsafe { libc::getloadavg(l.as_mut_ptr(), 3) } == 3).then_some((l[0], l[1], l[2]))
    }

    /// `sysctlbyname` of a fixed-size integer (`u32` or `u64`), `None` when the name is unknown.
    #[cfg(target_os = "macos")]
    fn sysctl_int<T: Default>(name: &std::ffi::CStr) -> Option<T> {
        let mut v = T::default();
        let mut len = std::mem::size_of::<T>();
        // SAFETY: `v` is `len` writable bytes; `name` is NUL-terminated; no new value is set.
        let rc = unsafe {
            libc::sysctlbyname(
                name.as_ptr(),
                (&mut v as *mut T).cast(),
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        (rc == 0 && len == std::mem::size_of::<T>()).then_some(v)
    }

    pub fn mem_available_pct() -> Option<u8> {
        #[cfg(target_os = "macos")]
        {
            sysctl_int::<u32>(c"kern.memorystatus_level").map(|p| p.min(100) as u8)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let (avail, total) = parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)?;
            Some((avail.saturating_mul(100) / total).min(100) as u8)
        }
    }

    pub fn mem_total_bytes() -> Option<u64> {
        #[cfg(target_os = "macos")]
        {
            sysctl_int::<u64>(c"hw.memsize").filter(|t| *t > 0)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let (_, total_kb) = parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)?;
            Some(total_kb.saturating_mul(1024))
        }
    }

    pub fn exe_path(pid: u32) -> Option<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            let mut buf = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
            // SAFETY: buf is valid for buf.len() bytes.
            let n = unsafe {
                libc::proc_pidpath(pid as i32, buf.as_mut_ptr().cast(), buf.len() as u32)
            };
            (n > 0).then(|| PathBuf::from(String::from_utf8_lossy(&buf[..n as usize]).as_ref()))
        }
        #[cfg(not(target_os = "macos"))]
        {
            std::fs::read_link(format!("/proc/{pid}/exe")).ok()
        }
    }

    pub fn command_line(pid: u32) -> Option<String> {
        let out = std::process::Command::new("ps")
            .args(["-o", "command=", "-p"])
            .arg(pid.to_string())
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!s.is_empty()).then_some(s)
    }

    pub fn detach(cmd: &mut std::process::Command) {
        // SAFETY: setsid is async-signal-safe and touches no parent state.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }

    pub fn detach_tokio(cmd: &mut tokio::process::Command) {
        // SAFETY: as in `detach`.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }

    pub fn shell(script: &str) -> std::process::Command {
        let mut c = std::process::Command::new("sh");
        c.arg("-c").arg(script);
        c
    }

    pub fn shell_tokio(script: &str) -> tokio::process::Command {
        let mut c = tokio::process::Command::new("sh");
        c.arg("-c").arg(script);
        c
    }

    pub fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }

    pub fn fallback_replace(from: &Path, to: &Path) -> io::Result<()> {
        std::fs::rename(from, to)
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
        QueryFullProcessImageNameW, TerminateProcess,
    };

    pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    /// Out of a job object that would kill the child with us (terminals and IDEs run their
    /// children in one). Spawning fails when the job forbids it: retry without.
    pub const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

    /// `CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW`: no Ctrl-C from our console group, and a
    /// console of its own that nobody sees, which its own children inherit (so they never flash
    /// a window either). Not `DETACHED_PROCESS`: a console-less parent makes every console child
    /// open a visible window.
    pub const DETACHED: u32 = 0x0000_0200 | CREATE_NO_WINDOW;

    /// An open process handle, closed on drop.
    pub(crate) struct Proc(HANDLE);

    impl Proc {
        pub(crate) fn open(pid: u32, access: u32) -> Option<Self> {
            if pid == 0 {
                return None;
            }
            // SAFETY: OpenProcess has no memory preconditions; a null handle means failure.
            let h = unsafe { OpenProcess(access, 0, pid) };
            (!h.is_null()).then_some(Self(h))
        }
        pub(crate) fn raw(&self) -> HANDLE {
            self.0
        }
    }

    impl Drop for Proc {
        fn drop(&mut self) {
            // SAFETY: we own the handle.
            unsafe { CloseHandle(self.0) };
        }
    }

    pub fn pid_alive(pid: u32) -> bool {
        let Some(p) = Proc::open(pid, PROCESS_QUERY_LIMITED_INFORMATION) else {
            // Access denied still means it exists.
            return io::Error::last_os_error().raw_os_error() == Some(5);
        };
        let mut code = 0u32;
        // SAFETY: valid handle and out pointer.
        unsafe { GetExitCodeProcess(p.raw(), &mut code) != 0 && code == STILL_ACTIVE as u32 }
    }

    pub fn terminate(pid: u32) -> bool {
        force_kill(pid)
    }

    pub fn force_kill(pid: u32) -> bool {
        if pid == std::process::id() {
            return false;
        }
        let Some(p) = Proc::open(pid, PROCESS_TERMINATE) else {
            return false;
        };
        // SAFETY: valid handle opened with PROCESS_TERMINATE.
        unsafe { TerminateProcess(p.raw(), 1) != 0 }
    }

    pub fn file_identity(f: &File) -> Option<(u64, u64)> {
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        // SAFETY: zeroed out struct; the handle is open for the lifetime of `f`.
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(f.as_raw_handle() as HANDLE, &mut info) } == 0 {
            return None;
        }
        let index = ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64;
        Some((info.dwVolumeSerialNumber as u64, index))
    }

    /// Rename `from` over `to` with POSIX semantics (`FILE_RENAME_FLAG_POSIX_SEMANTICS`): one step in which the
    /// name `to` goes from the old file to the new one, even while readers hold the old one open. `std::fs::rename`
    /// is `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`, which Windows documents without that guarantee: a reader opening
    /// the name meanwhile can find it missing (`NotFound`) or, the old file being delete-pending, be refused (`Access
    /// is denied`) (#545). The retry loop and what follows a refusal are in [`super::replace_file_via`].
    pub fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
        super::replace_file_via(from, to, posix_rename)
    }

    /// What is left when the POSIX rename cannot be done (an old Windows, FAT or a network share) or the target is
    /// readonly: `ReplaceFileW`, which swaps the contents under the name that stays (the name is not given up for the
    /// new file as `MoveFileExW` does), and `std::fs::rename` when that is refused too or there is nothing to replace.
    /// Neither is a promise to a reader opening the name at that instant, so readers keep [`crate::fs::read_replaced`].
    pub fn fallback_replace(from: &Path, to: &Path) -> io::Result<()> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;
        let wide = |p: &Path| -> Vec<u16> { p.as_os_str().encode_wide().chain(Some(0)).collect() };
        if to.exists() {
            let (replaced, replacement) = (wide(to), wide(from));
            // SAFETY: both are NUL-terminated and live for the call; the backup, exclude and reserved arguments are
            // null, as documented.
            let ok = unsafe {
                ReplaceFileW(
                    replaced.as_ptr(),
                    replacement.as_ptr(),
                    std::ptr::null(),
                    0,
                    std::ptr::null(),
                    std::ptr::null(),
                )
            };
            if ok != 0 {
                return Ok(());
            }
        }
        std::fs::rename(from, to)
    }

    fn posix_rename(from: &Path, to: &Path) -> io::Result<()> {
        use std::os::windows::ffi::OsStrExt;
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            DELETE, FILE_RENAME_INFO, FileRenameInfoEx, SetFileInformationByHandle,
        };
        const REPLACE_IF_EXISTS: u32 = 0x1; // FILE_RENAME_FLAG_REPLACE_IF_EXISTS
        const POSIX_SEMANTICS: u32 = 0x2; // FILE_RENAME_FLAG_POSIX_SEMANTICS

        // A rename by handle takes a full name: the parent in its verbatim form, then the file.
        let name = to
            .file_name()
            .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        let parent = to.parent().filter(|p| !p.as_os_str().is_empty());
        let full = std::fs::canonicalize(parent.unwrap_or(Path::new(".")))?.join(name);
        let wide: Vec<u16> = full.as_os_str().encode_wide().collect();
        let name_bytes = wide.len() * 2;

        // The struct ends in a variable-length name: a u64-aligned buffer big enough for the header and the name.
        let header = std::mem::offset_of!(FILE_RENAME_INFO, FileName);
        let size = header + name_bytes + 2;
        let mut buf = vec![0u64; size.div_ceil(8)];
        let info = buf.as_mut_ptr().cast::<FILE_RENAME_INFO>();
        // SAFETY: `buf` is zeroed, aligned for the struct and `size` bytes long; every write stays inside it, and
        // the name (a NUL after it already, the buffer being zeroed) is copied to where `FileName` starts.
        unsafe {
            (*info).Anonymous.Flags = REPLACE_IF_EXISTS | POSIX_SEMANTICS;
            (*info).FileNameLength = name_bytes as u32;
            std::ptr::copy_nonoverlapping(
                wide.as_ptr(),
                buf.as_mut_ptr().cast::<u8>().add(header).cast::<u16>(),
                wide.len(),
            );
        }
        // DELETE is the access a rename needs; the share mode stays the default (read, write, delete) so a
        // concurrent reader of `from` is not locked out either.
        let src = std::fs::OpenOptions::new().access_mode(DELETE).open(from)?;
        // SAFETY: the handle is open for the lifetime of `src`; `buf` holds a complete FILE_RENAME_INFO of `size` bytes.
        let ok = unsafe {
            SetFileInformationByHandle(
                src.as_raw_handle() as HANDLE,
                FileRenameInfoEx,
                buf.as_ptr().cast(),
                size as u32,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn parent_pid() -> u32 {
        let me = std::process::id();
        process_rows()
            .into_iter()
            .find(|r| r.pid == me)
            .map_or(0, |r| r.ppid)
    }

    pub fn try_lock(file: &File) -> io::Result<bool> {
        use windows_sys::Win32::Storage::FileSystem::{
            LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx,
        };
        use windows_sys::Win32::System::IO::OVERLAPPED;
        // SAFETY: zeroed OVERLAPPED is valid; offset set below.
        let mut ov: OVERLAPPED = unsafe { std::mem::zeroed() };
        // One byte at offset 2^63: past any real content, so readers of the file are not blocked.
        ov.Anonymous.Anonymous.Offset = 0;
        ov.Anonymous.Anonymous.OffsetHigh = 0x8000_0000;
        // SAFETY: the handle is open for the lifetime of `file`; `ov` outlives the call.
        let ok = unsafe {
            LockFileEx(
                file.as_raw_handle() as HANDLE,
                LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                0,
                1,
                0,
                &mut ov,
            )
        };
        if ok != 0 {
            return Ok(true);
        }
        let e = io::Error::last_os_error();
        // ERROR_LOCK_VIOLATION
        if e.raw_os_error() == Some(33) {
            Ok(false)
        } else {
            Err(e)
        }
    }

    pub fn random_bytes(buf: &mut [u8]) -> io::Result<()> {
        use windows_sys::Win32::Security::Cryptography::{
            BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
        };
        for chunk in buf.chunks_mut(u32::MAX as usize) {
            // SAFETY: chunk is valid for its length.
            let st = unsafe {
                BCryptGenRandom(
                    std::ptr::null_mut(),
                    chunk.as_mut_ptr(),
                    chunk.len() as u32,
                    BCRYPT_USE_SYSTEM_PREFERRED_RNG,
                )
            };
            if st != 0 {
                return Err(io::Error::other(format!("BCryptGenRandom: {st:#x}")));
            }
        }
        Ok(())
    }

    pub fn hostname() -> Option<String> {
        std::env::var("COMPUTERNAME").ok().filter(|s| !s.is_empty())
    }

    /// Windows has no load average.
    pub fn load_avg() -> Option<(f64, f64, f64)> {
        None
    }

    fn mem_status() -> Option<windows_sys::Win32::System::SystemInformation::MEMORYSTATUSEX> {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        // SAFETY: an all-zero MEMORYSTATUSEX is valid; dwLength is set before the call as required.
        let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        // SAFETY: `m` is a valid, correctly sized MEMORYSTATUSEX.
        (unsafe { GlobalMemoryStatusEx(&mut m) } != 0).then_some(m)
    }

    pub fn mem_available_pct() -> Option<u8> {
        let m = mem_status()?;
        (m.ullTotalPhys > 0)
            .then(|| (m.ullAvailPhys.saturating_mul(100) / m.ullTotalPhys).min(100) as u8)
    }

    pub fn mem_total_bytes() -> Option<u64> {
        mem_status().map(|m| m.ullTotalPhys).filter(|t| *t > 0)
    }

    pub fn exe_path(pid: u32) -> Option<PathBuf> {
        let p = Proc::open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        let mut buf = vec![0u16; 32768];
        let mut len = buf.len() as u32;
        // SAFETY: buf is valid for `len` u16s; len is updated to the written length.
        let ok = unsafe { QueryFullProcessImageNameW(p.raw(), 0, buf.as_mut_ptr(), &mut len) };
        (ok != 0).then(|| std::ffi::OsString::from_wide(&buf[..len as usize]).into())
    }

    pub fn command_line(pid: u32) -> Option<String> {
        use windows_sys::Wdk::System::Threading::NtQueryInformationProcess;
        use windows_sys::Win32::Foundation::UNICODE_STRING;
        // ProcessCommandLineInformation (Windows 8.1+): a UNICODE_STRING followed by its buffer.
        const PROCESS_COMMAND_LINE_INFORMATION: i32 = 60;
        let p = Proc::open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        let mut need = 0u32;
        // SAFETY: a null buffer of length 0 only asks for the size.
        unsafe {
            NtQueryInformationProcess(
                p.raw(),
                PROCESS_COMMAND_LINE_INFORMATION,
                std::ptr::null_mut(),
                0,
                &mut need,
            )
        };
        if need == 0 {
            return None;
        }
        // u64 storage keeps the UNICODE_STRING header aligned.
        let mut buf = vec![0u64; (need as usize).div_ceil(8)];
        // SAFETY: buf holds `need` bytes.
        let st = unsafe {
            NtQueryInformationProcess(
                p.raw(),
                PROCESS_COMMAND_LINE_INFORMATION,
                buf.as_mut_ptr().cast(),
                need,
                &mut need,
            )
        };
        if st != 0 {
            return None;
        }
        // SAFETY: on success the buffer starts with a UNICODE_STRING whose Buffer points inside it.
        let s = unsafe { &*(buf.as_ptr() as *const UNICODE_STRING) };
        if s.Buffer.is_null() {
            return None;
        }
        // SAFETY: Length bytes of UTF-16 at Buffer, inside `buf`.
        let wide = unsafe { std::slice::from_raw_parts(s.Buffer, s.Length as usize / 2) };
        Some(String::from_utf16_lossy(wide))
    }

    pub fn detach(cmd: &mut std::process::Command) {
        cmd.creation_flags(DETACHED);
    }

    pub fn detach_tokio(cmd: &mut tokio::process::Command) {
        cmd.creation_flags(DETACHED);
    }

    fn comspec() -> std::ffi::OsString {
        std::env::var_os("ComSpec").unwrap_or_else(|| "cmd.exe".into())
    }

    /// `/s /c "<script>"`: with `/s`, cmd strips exactly the outer quotes and runs the rest
    /// verbatim, so the script keeps its own quoting. `raw_arg` stops Rust from re-escaping it.
    fn cmd_tail(script: &str) -> String {
        format!("/d /s /c \"{script}\"")
    }

    pub fn shell(script: &str) -> std::process::Command {
        let mut c = std::process::Command::new(comspec());
        c.raw_arg(cmd_tail(script));
        c
    }

    pub fn shell_tokio(script: &str) -> tokio::process::Command {
        let mut c = tokio::process::Command::new(comspec());
        c.raw_arg(cmd_tail(script));
        c
    }

    /// Snapshot of every process (`CreateToolhelp32Snapshot`). Empty on failure.
    pub fn process_rows() -> Vec<ProcRow> {
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        };
        // SAFETY: no preconditions; INVALID_HANDLE_VALUE means failure.
        let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snap == INVALID_HANDLE_VALUE {
            return Vec::new();
        }
        let mut out = Vec::new();
        // SAFETY: zeroed PROCESSENTRY32W with dwSize set is the documented input.
        let mut e: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        // SAFETY: valid snapshot handle and entry.
        let mut ok = unsafe { Process32FirstW(snap, &mut e) };
        while ok != 0 {
            let end = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
            out.push(ProcRow {
                pid: e.th32ProcessID,
                ppid: e.th32ParentProcessID,
                exe: String::from_utf16_lossy(&e.szExeFile[..end]),
            });
            // SAFETY: as above.
            ok = unsafe { Process32NextW(snap, &mut e) };
        }
        // SAFETY: we own the snapshot handle.
        unsafe { CloseHandle(snap) };
        out
    }

    /// Every live descendant of `root` (not `root`), parents before children; none when `root` is not
    /// running. See [`super::descendants_by`].
    pub fn descendants(root: u32, rows: &[ProcRow]) -> Vec<u32> {
        super::descendants_by(root, rows, &start_time_secs)
    }

    /// End `root` and its whole tree (Windows has no process groups to signal). Never ourselves or
    /// an ancestor of ours, and nothing when no process object has `root`'s pid: after the last handle of
    /// a child is closed its number is anyone's (see [`super::tree_targets`]), so call this while you still
    /// hold the `Child` (a running or exited one), never after it was consumed or dropped.
    pub fn kill_tree(root: u32) {
        for pid in super::tree_targets(root, std::process::id(), &process_rows(), &start_time_secs)
        {
            force_kill(pid);
        }
    }

    /// Creation time of `pid` as Unix seconds.
    pub fn start_time_secs(pid: u32) -> Option<u64> {
        use windows_sys::Win32::System::Threading::GetProcessTimes;
        let p = Proc::open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        let z = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut c, mut e, mut k, mut u) = (z, z, z, z);
        // SAFETY: valid handle and out pointers.
        if unsafe { GetProcessTimes(p.raw(), &mut c, &mut e, &mut k, &mut u) } == 0 {
            return None;
        }
        let ticks = ((c.dwHighDateTime as u64) << 32) | c.dwLowDateTime as u64;
        // FILETIME counts 100 ns since 1601-01-01; Unix time starts 11644473600 s later.
        (ticks / 10_000_000).checked_sub(11_644_473_600)
    }

    /// CPU time (user + kernel) in milliseconds and working set in bytes of `pid`.
    pub fn usage(pid: u32) -> Option<(u64, u64)> {
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        };
        use windows_sys::Win32::System::Threading::GetProcessTimes;
        let p = Proc::open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        let z = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut c, mut e, mut k, mut u) = (z, z, z, z);
        // SAFETY: valid handle and out pointers.
        if unsafe { GetProcessTimes(p.raw(), &mut c, &mut e, &mut k, &mut u) } == 0 {
            return None;
        }
        let ft = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
        let cpu_ms = (ft(k) + ft(u)) / 10_000;
        // SAFETY: zeroed counters with cb set is the documented input.
        let mut m: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
        m.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        // SAFETY: valid handle and out struct of size cb.
        let ok = unsafe { GetProcessMemoryInfo(p.raw(), &mut m, m.cb) };
        Some((cpu_ms, if ok != 0 { m.WorkingSetSize as u64 } else { 0 }))
    }
}

/// One row of the process table.
#[cfg(any(windows, test))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcRow {
    pub pid: u32,
    pub ppid: u32,
    /// Executable file name (`node.exe`), as the snapshot reports it.
    pub exe: String,
}

/// Every live descendant of `root` (not `root`), parents before children, with `born` the creation time of a
/// pid (`None`: no such process object). A recorded parent pid survives its process on Windows and pids are
/// reused, so a child counts only when it was created no earlier than its parent. A `root` with no creation time
/// has no descendants: read as 0, every process that ever had a parent with that number (any older holder of
/// the pid, the CI runner's own children included) would pass the test above. (An exited process whose handle
/// the caller still holds keeps its creation time, so its real orphans are still found.)
#[cfg(any(windows, test))]
fn descendants_by(root: u32, rows: &[ProcRow], born: &dyn Fn(u32) -> Option<u64>) -> Vec<u32> {
    let Some(root_born) = born(root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut queue = std::collections::VecDeque::from([(root, root_born)]);
    while let Some((p, p_born)) = queue.pop_front() {
        for r in rows {
            if r.ppid == p
                && r.pid != p
                && r.pid != root
                && !out.contains(&r.pid)
                && let Some(b) = born(r.pid)
                && b >= p_born
            {
                out.push(r.pid);
                queue.push_back((r.pid, b));
            }
        }
    }
    out
}

/// The pids [`kill_tree`] ends for `root`: `root` and its descendants, and nothing when `root` is not running
/// (its number may belong to a stranger by now) or is `me` or one of `me`'s ancestors (by the table's recorded
/// parents: the test runner, the CI runner). The process table is never trusted to name a pid that is not ours.
#[cfg(any(windows, test))]
fn tree_targets(
    root: u32,
    me: u32,
    rows: &[ProcRow],
    born: &dyn Fn(u32) -> Option<u64>,
) -> Vec<u32> {
    let mut protected = vec![me];
    let mut at = me;
    while let Some(r) = rows.iter().find(|r| r.pid == at)
        && r.ppid != 0
        && !protected.contains(&r.ppid)
    {
        protected.push(r.ppid);
        at = r.ppid;
    }
    if protected.contains(&root) || born(root).is_none() {
        return Vec::new();
    }
    let mut out = vec![root];
    out.extend(
        descendants_by(root, rows, born)
            .into_iter()
            .filter(|p| !protected.contains(p)),
    );
    out
}

#[cfg(windows)]
pub use imp::{
    CREATE_BREAKAWAY_FROM_JOB, CREATE_NO_WINDOW, DETACHED, descendants, kill_tree, process_rows,
    start_time_secs, usage,
};

/// Windows: open `target` (an `http`, `https` or `mailto` URL the caller already checked) with the user's default
/// handler, through `ShellExecuteW` as a browser link does. The URL is ONE string: nothing parses a command line out
/// of it, unlike `explorer.exe <string>`, which reads commas and switches (`/select,`).
#[cfg(windows)]
pub fn shell_open(target: &str) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    const SW_SHOWNORMAL: i32 = 1;
    if target.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a link has no NUL in it",
        ));
    }
    let wide = |s: &str| -> Vec<u16> {
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(Some(0))
            .collect()
    };
    let (verb, file) = (wide("open"), wide(target));
    // SAFETY: both strings are NUL-terminated and outlive the call; the optional arguments are null.
    let code = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    } as usize;
    // Values above 32 mean it started; the rest are error codes.
    if code > 32 {
        Ok(())
    } else {
        Err(io::Error::other(format!("ShellExecuteW returned {code}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #545 again, after the POSIX rename landed: `replacing_a_file_never_leaves_its_name_missing_for_a_reader_that_
    /// opens_it` still failed on a Windows runner, the reader getting "Access is denied". That is the old file in
    /// delete-pending, which only the `MoveFileExW` fallback leaves behind: the POSIX rename had failed for a moment
    /// (something such as the antivirus holding the new file without delete sharing) and the fallback ran at once.
    /// A transient refusal is now retried, a little longer each time and briefly in total; only what will not get
    /// better by waiting goes straight to the fallback.
    #[test]
    fn a_refused_posix_rename_is_retried_briefly_and_only_an_unsupported_one_falls_back_at_once() {
        const ACCESS_DENIED: i32 = 5;
        const SHARING_VIOLATION: i32 = 32;
        for code in [ACCESS_DENIED, SHARING_VIOLATION] {
            let mut waits = Vec::new();
            let mut attempt = 0;
            while let RenameStep::Retry(w) = after_posix_rename_error(Some(code), attempt) {
                waits.push(w);
                attempt += 1;
                assert!(attempt < 100, "never stops retrying");
            }
            assert!(
                waits.len() >= 5,
                "{code}: too few retries to outlast a scan: {waits:?}"
            );
            assert!(
                waits.windows(2).all(|w| w[0] <= w[1]),
                "{code}: waits shrink: {waits:?}"
            );
            let total: std::time::Duration = waits.iter().sum();
            assert!(
                total <= std::time::Duration::from_millis(1500),
                "{code}: a write may stall {total:?}"
            );
        }
        // Past the last retry the replace gives up: the old file keeps its name.
        assert_eq!(
            after_posix_rename_error(Some(ACCESS_DENIED), 28),
            RenameStep::GiveUp
        );
        // Not supported (old Windows, not NTFS: invalid function, not supported, invalid parameter, not implemented),
        // write-protected media, or no OS code at all: the fallback, without waiting. (A readonly target also says
        // "access denied"; `replace_file_via` checks the attribute and skips the retries.)
        for code in [Some(1), Some(50), Some(87), Some(120), Some(19), None] {
            assert_eq!(
                after_posix_rename_error(code, 0),
                RenameStep::Fallback,
                "{code:?}"
            );
        }
        // Anything else (a lost race, a path that is gone, a full disk) is the caller's to see: the legacy rename would
        // only fail the same way, or replace the name non-atomically.
        for code in [Some(2), Some(3), Some(112), Some(1224)] {
            assert_eq!(
                after_posix_rename_error(code, 0),
                RenameStep::GiveUp,
                "{code:?}"
            );
        }
    }

    /// #743 (`status_write_is_atomic_under_concurrent_reads`, Windows, `NotFound` at the reader): the fallback after a
    /// refusal that outlasted the retries was `std::fs::rename`, the legacy replace that can leave the name missing
    /// (or the old file delete-pending) for a reader. Now a refusal that does not end fails the replace instead, and
    /// the old file keeps its name for the whole time, the new file stays beside it and is the caller's to clean up.
    #[test]
    fn a_refusal_that_outlasts_the_retries_fails_the_replace_and_the_name_never_leaves_the_old_file()
     {
        let dir = tempfile::tempdir().unwrap();
        let (to, from) = (
            dir.path().join("status.json"),
            dir.path().join("status.json.tmp"),
        );
        std::fs::write(&to, "old").unwrap();
        std::fs::write(&from, "new").unwrap();
        let missing = watch_name(&to);
        let mut tries = 0;
        let err = replace_file_via(&from, &to, |_, _| {
            tries += 1;
            Err(io::Error::from_raw_os_error(5)) // ERROR_ACCESS_DENIED, every time
        })
        .unwrap_err();
        let missing = missing.stop();
        assert_eq!(err.raw_os_error(), Some(5));
        assert!(tries > 5, "no retries: {tries}");
        assert_eq!(
            missing, 0,
            "the name was missing for a reader {missing} times"
        );
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "old");
        assert_eq!(std::fs::read_to_string(&from).unwrap(), "new");
    }

    /// The fallback itself (the POSIX rename "not supported", forced on every OS): the replace happens, and on Windows,
    /// where the fallback is `ReplaceFileW` (or `MoveFileExW`), a reader opening the name all along never finds it missing. (A refusal
    /// "access denied" of the old file delete-pending is the other window; `read_replaced` covers it.)
    #[test]
    fn the_fallback_replace_never_leaves_the_name_missing_for_a_reader_that_opens_it() {
        let dir = tempfile::tempdir().unwrap();
        let to = dir.path().join("status.json");
        std::fs::write(&to, "0").unwrap();
        let watch = watch_name(&to);
        for n in 1..300 {
            let from = dir.path().join(format!("status.json.tmp-{n}"));
            std::fs::write(&from, n.to_string()).unwrap();
            // ERROR_NOT_SUPPORTED: the case for which the fallback exists.
            replace_file_via(&from, &to, |_, _| Err(io::Error::from_raw_os_error(50))).unwrap();
        }
        let missing = watch.stop();
        assert_eq!(
            missing, 0,
            "the name was missing for a reader {missing} times"
        );
        assert_eq!(std::fs::read_to_string(&to).unwrap(), "299");
    }

    /// A thread that opens `path` in a loop, as a reader does, and counts the opens that found no file.
    struct NameWatch {
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        thread: std::thread::JoinHandle<(u32, u32)>,
    }

    fn watch_name(path: &Path) -> NameWatch {
        use std::sync::atomic::{AtomicBool, Ordering};
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let (path, flag) = (path.to_path_buf(), stop.clone());
        let thread = std::thread::spawn(move || {
            let (mut opens, mut missing) = (0, 0);
            while !flag.load(Ordering::Relaxed) {
                match File::open(&path) {
                    Err(e) if e.kind() == io::ErrorKind::NotFound => missing += 1,
                    _ => opens += 1,
                }
            }
            (opens, missing)
        });
        NameWatch { stop, thread }
    }

    impl NameWatch {
        /// How many opens found the name missing; at least one open must have happened.
        fn stop(self) -> u32 {
            self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            let (opens, missing) = self.thread.join().unwrap();
            assert!(opens > 0, "the watcher never opened the file");
            missing
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_child_started_with_no_new_privs_reports_the_flag_and_a_plain_one_does_not() {
        // Regression: a sway with file capabilities hid its environment from its owner, so a crash left it unreapable.
        let status = |harden: bool| {
            let mut c = std::process::Command::new("grep");
            c.args(["NoNewPrivs", "/proc/self/status"]);
            if harden {
                no_new_privs(&mut c);
            }
            String::from_utf8(c.output().unwrap().stdout).unwrap()
        };
        assert!(status(true).contains("1"), "{}", status(true));
        // The test runner itself may already run under no_new_privs (containers); only compare when it does not.
        if status(false).contains('0') {
            assert!(!status(false).contains('1'));
        }
    }

    #[cfg(unix)]
    #[test]
    fn sigint_and_sigterm_set_a_flag_once_watched_instead_of_ending_the_process() {
        watch_interrupts();
        assert!(!interrupted());
        // SAFETY: raise(3) to ourselves with the handler installed above.
        unsafe { libc::raise(libc::SIGTERM) };
        assert!(interrupted());
    }

    #[test]
    fn our_own_pid_is_alive_and_a_freed_one_is_not() {
        assert!(pid_alive(std::process::id()));
        assert!(!pid_alive(0));
        let mut child = shell("exit 0").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        // Reaped and gone (a recycled pid within this window is vanishingly unlikely).
        assert!(!pid_alive(pid));
    }

    #[test]
    fn the_shell_runs_a_script_with_its_own_quoting_and_exit_code() {
        let out = shell("echo \"a b\"&& exit 3").output().unwrap();
        assert_eq!(out.status.code(), Some(3));
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("a b"), "{text:?}");
    }

    #[test]
    fn a_shell_argv_is_recognised_only_in_its_exact_windows_form() {
        let argv = shell_argv("echo \"a b\"");
        assert_eq!(argv.last().map(String::as_str), Some("echo \"a b\""));
        if cfg!(windows) {
            assert_eq!(shell_script(&argv), Some("echo \"a b\""));
            let upper: Vec<String> = ["C:\\Windows\\System32\\CMD.EXE", "/D", "/S", "/C", "x"]
                .map(String::from)
                .into();
            assert_eq!(shell_script(&upper), Some("x"));
        } else {
            assert_eq!(argv[..2], ["sh", "-c"]);
        }
        // Anything else is an ordinary program with arguments.
        for other in [
            vec!["cmd.exe", "/c", "x"],
            vec!["cmd.exe", "/d", "/s", "/c", "x", "extra"],
            vec!["node", "/d", "/s", "/c", "x"],
            vec!["sh", "-c", "x"],
        ] {
            let v: Vec<String> = other.iter().map(|s| s.to_string()).collect();
            assert_eq!(shell_script(&v), None, "{other:?}");
        }
    }

    #[test]
    fn a_second_lock_on_the_same_file_fails_and_the_contents_stay_readable() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x.lock");
        std::fs::write(&p, "1234").unwrap();
        let a = File::options().read(true).write(true).open(&p).unwrap();
        let b = File::options().read(true).write(true).open(&p).unwrap();
        assert!(try_lock(&a).unwrap());
        assert!(!try_lock(&b).unwrap());
        // The loser reads the holder's pid (Windows byte-range locks are mandatory).
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "1234");
        drop(a);
        // A sibling test that forks between `open` and `exec` holds the open file description for
        // that instant, so the release is bounded, not instant (the lock is per description).
        let end = std::time::Instant::now() + crate::scaling::HANG;
        while !try_lock(&b).unwrap() {
            assert!(
                std::time::Instant::now() < end,
                "the lock was never released"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn a_spawned_child_is_found_by_exe_and_command_line_and_can_be_killed() {
        let mut child = if cfg!(windows) {
            shell("ping -n 30 127.0.0.1 >NUL").spawn().unwrap()
        } else {
            shell("sleep 30").spawn().unwrap()
        };
        let pid = child.id();
        let exe = exe_path(pid).expect("exe path");
        let name = exe.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            is_program(&name, "cmd") || is_program(&name, "sh") || name.contains("sh"),
            "{name}"
        );
        assert!(command_line(pid).is_some());
        assert!(force_kill(pid));
        child.wait().unwrap();
        assert!(!pid_alive(pid));
    }

    #[cfg(windows)]
    #[test]
    fn kill_tree_ends_the_grandchildren_of_a_shell() {
        // cmd -> ping: killing cmd alone would orphan ping (no process groups on Windows).
        let mut child = shell("ping -n 60 127.0.0.1 >NUL").spawn().unwrap();
        let pid = child.id();
        let mut kids = Vec::new();
        let end = std::time::Instant::now() + crate::scaling::HANG;
        while std::time::Instant::now() < end {
            kids = descendants(pid, &process_rows());
            if !kids.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(!kids.is_empty(), "ping never showed up under cmd");
        kill_tree(pid);
        child.wait().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        for k in kids {
            assert!(!pid_alive(k), "descendant {k} survived");
        }
    }

    fn row(pid: u32, ppid: u32) -> ProcRow {
        ProcRow {
            pid,
            ppid,
            exe: String::new(),
        }
    }

    /// `born` for a made-up machine: the listed pids run, with that creation time; every other pid is gone.
    fn running(born: &[(u32, u64)]) -> impl Fn(u32) -> Option<u64> {
        let born: std::collections::HashMap<u32, u64> = born.iter().copied().collect();
        move |pid| born.get(&pid).copied()
    }

    // Incident (Windows CI jobs that end in "The hosted runner lost communication with the server"): callers
    // ended the tree of a child they had already waited for. With the root gone its creation time read as 0, so
    // every process whose recorded parent had that number, an older holder of the pid included (Windows keeps
    // `ppid` after the parent ends and reuses the number), counted as its descendant and was killed. Here 300 is
    // such a process: started by an earlier process numbered 100, long before the child that got 100 later.
    #[test]
    fn a_root_that_already_exited_has_no_descendants_and_nothing_to_kill() {
        let rows = [row(7000, 500), row(500, 4), row(300, 100)];
        let born = running(&[(7000, 90), (500, 10), (4, 1), (300, 5)]);
        assert!(descendants_by(100, &rows, &born).is_empty());
        assert!(tree_targets(100, 7000, &rows, &born).is_empty());
    }

    #[test]
    fn a_running_root_takes_its_descendants_but_not_older_strangers_with_its_number() {
        // 100 (born 50) -> 101 (60) -> 102 (70); 300 (born 5) names 100 as its parent but is older: a stranger.
        // 103 names 101 but is gone.
        let rows = [
            row(100, 1),
            row(101, 100),
            row(102, 101),
            row(300, 100),
            row(103, 101),
        ];
        let born = running(&[(100, 50), (101, 60), (102, 70), (300, 5)]);
        assert_eq!(descendants_by(100, &rows, &born), vec![101, 102]);
        assert_eq!(tree_targets(100, 7000, &rows, &born), vec![100, 101, 102]);
    }

    #[test]
    fn the_tree_never_includes_us_or_an_ancestor_of_ours() {
        // 7000 (us) <- 500 (test runner) <- 4 (CI runner): "500's tree" would end us and the job.
        let rows = [row(7000, 500), row(500, 4), row(4, 1), row(100, 500)];
        let born = running(&[(7000, 90), (500, 10), (4, 1), (100, 80)]);
        for root in [7000, 500, 4] {
            assert!(tree_targets(root, 7000, &rows, &born).is_empty(), "{root}");
        }
        // A child of the runner that is not on our own chain is an ordinary root.
        assert_eq!(tree_targets(100, 7000, &rows, &born), vec![100]);
        // A tree with us or an ancestor under it leaves them out.
        let under = [row(7000, 101), row(101, 100), row(100, 4), row(4, 1)];
        let born = running(&[(7000, 90), (101, 80), (100, 10), (4, 1)]);
        assert!(tree_targets(100, 7000, &under, &born).is_empty());
    }

    #[test]
    fn program_names_match_exactly_and_windows_accepts_the_exe_suffix() {
        assert!(is_program("atlas", "atlas"));
        assert!(!is_program("atlas-mcp", "atlas"));
        assert!(!is_program("xatlas", "atlas"));
        assert_eq!(is_program("atlas.exe", "atlas"), cfg!(windows));
        assert_eq!(is_program("ATLAS.EXE", "atlas"), cfg!(windows));
    }

    #[test]
    fn file_identity_follows_the_file_across_a_rename_and_tells_files_apart() {
        let d = tempfile::tempdir().unwrap();
        let (a, b) = (d.path().join("a.log"), d.path().join("b.log"));
        std::fs::write(&a, "1").unwrap();
        std::fs::write(&b, "2").unwrap();
        let held = File::open(&a).unwrap();
        let id = file_identity(&held).unwrap();
        assert_ne!(Some(id), file_identity(&File::open(&b).unwrap()));
        let moved = d.path().join("a.prev.log");
        std::fs::rename(&a, &moved).unwrap();
        assert_eq!(Some(id), file_identity(&File::open(&moved).unwrap()));
        // A new file at the old name is a different file.
        std::fs::write(&a, "3").unwrap();
        assert_ne!(Some(id), file_identity(&File::open(&a).unwrap()));
    }

    #[test]
    fn random_bytes_are_not_all_zero() {
        let mut b = [0u8; 32];
        random_bytes(&mut b).unwrap();
        assert!(b.iter().any(|&x| x != 0));
    }

    #[test]
    fn meminfo_gives_available_and_total_and_rejects_what_it_cannot_use() {
        let text = "MemTotal:       16384000 kB\nMemFree:          100000 kB\nMemAvailable:    4096000 kB\nBuffers: 1 kB\n";
        assert_eq!(parse_meminfo(text), Some((4_096_000, 16_384_000)));
        // A name that merely starts with the field name is not the field.
        assert_eq!(parse_meminfo("MemTotalX: 5 kB\nMemAvailable: 1 kB\n"), None);
        // Old kernels have no MemAvailable: unknown, never a made-up figure.
        assert_eq!(parse_meminfo("MemTotal: 8 kB\nMemFree: 1 kB\n"), None);
        assert_eq!(parse_meminfo("MemTotal: 0 kB\nMemAvailable: 0 kB\n"), None);
        assert_eq!(parse_meminfo(""), None);
    }

    #[test]
    fn this_machines_memory_is_readable_and_sane() {
        let total = mem_total_bytes().expect("every supported OS reports its memory");
        assert!(total >= 256 << 20, "{total}");
        let pct = mem_available_pct().expect("every supported OS reports free memory");
        assert!(pct <= 100, "{pct}");
    }
}
