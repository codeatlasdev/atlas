//! OS primitives the CLI, runtime, hostd and app share, so their call sites stay platform-neutral:
//! pid probes and kills, advisory file locks, randomness, detached spawns and the command shell.
//! Unix keeps its signal and `flock` semantics; Windows maps them onto Win32.

use std::fs::File;
use std::io;
use std::path::PathBuf;

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

/// Fill `buf` from the OS CSPRNG.
pub fn random_bytes(buf: &mut [u8]) -> io::Result<()> {
    imp::random_bytes(buf)
}

/// This machine's host name, `None` when unreadable.
pub fn hostname() -> Option<String> {
    imp::hostname()
}

/// The executable file of `pid`, `None` when it is gone or unreadable.
pub fn exe_path(pid: u32) -> Option<PathBuf> {
    imp::exe_path(pid)
}

/// The full command line of `pid` (program and arguments, space separated as the OS shows it).
pub fn command_line(pid: u32) -> Option<String> {
    imp::command_line(pid)
}

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

    /// One row of the process table.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ProcRow {
        pub pid: u32,
        pub ppid: u32,
        /// Executable file name (`node.exe`), as the snapshot reports it.
        pub exe: String,
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

    /// Every live descendant of `root` (not `root`), parents before children. A recorded parent
    /// pid survives its process on Windows and pids are reused, so a child counts only when it
    /// was created no earlier than its parent.
    pub fn descendants(root: u32, rows: &[ProcRow]) -> Vec<u32> {
        let born = |pid: u32| start_time_secs(pid).unwrap_or(0);
        let mut out = Vec::new();
        let mut queue = std::collections::VecDeque::from([root]);
        while let Some(p) = queue.pop_front() {
            let p_born = born(p);
            for r in rows {
                if r.ppid == p
                    && r.pid != p
                    && r.pid != root
                    && !out.contains(&r.pid)
                    && born(r.pid) >= p_born
                {
                    out.push(r.pid);
                    queue.push_back(r.pid);
                }
            }
        }
        out
    }

    /// End `root` and its whole tree (Windows has no process groups to signal). Never ourselves.
    pub fn kill_tree(root: u32) {
        let rows = process_rows();
        let tree = descendants(root, &rows);
        force_kill(root);
        for pid in tree {
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

#[cfg(windows)]
pub use imp::{
    CREATE_BREAKAWAY_FROM_JOB, CREATE_NO_WINDOW, DETACHED, ProcRow, descendants, kill_tree,
    process_rows, start_time_secs, usage,
};

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(try_lock(&b).unwrap());
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
        for _ in 0..100 {
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
}
