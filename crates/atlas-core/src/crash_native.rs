//! Native crashes: a fault in the process itself (SIGSEGV, SIGBUS, SIGILL, SIGFPE, SIGABRT; an access violation on
//! Windows) leaves `crash-native-<unix_ms>-<pid>.toml` next to the panic reports (`crate::crash`), where the app's
//! "closed unexpectedly last time" card, `atlas doctor --bundle` and the bug-report flow already look (#318).
//!
//! Until now only a Rust panic wrote a record. A crash inside WebKitGTK, GTK, the GPU driver or our own `unsafe` code
//! left nothing of ours: on Linux the only trace was systemd-coredump, which a Debian, a container or an AppImage run
//! does not have (#265). The file carries what a maintainer needs and nothing private:
//!
//! - the build (`version`, `channel`, `commit`, `build_id`, `crate::buildinfo`), so the report finds its symbols;
//! - the signal or exception, the faulting address and the time;
//! - `load_bias` and `frames`: raw return addresses. `frames[i] - load_bias` is the address in the executable's own
//!   debug info; `scripts/symbols.sh resolve` turns it into functions and lines with the symbols published for that
//!   release (`docs/release.md`, "Symbols").
//!
//! Never the command line, the environment, memory or paths: the handler cannot run `crate::redact`, so it writes
//! only values it computed itself (numbers) and the header built at install time from the build.
//!
//! # Unix
//!
//! The handler runs in signal context: no allocation, no locks, only `open`/`write`/`close`, `clock_gettime`,
//! `getpid` and `backtrace(3)`, which is warmed up at install so `libgcc` is already loaded. The file name and the
//! fixed header are prepared in [`install`]; the handler formats numbers by hand into a stack buffer. It then hands
//! the signal to whatever was installed before (the standard library's stack-overflow handler, a debugger-friendly
//! default) so the process still dies the way it would have, core dump included.

use std::path::Path;

use crate::buildinfo::BuildInfo;

/// Raw return addresses of the calling thread, innermost first. Empty where the platform has no cheap unwinder
/// (musl, unknown targets).
pub fn frames() -> Vec<usize> {
    imp::frames()
}

/// The difference between where the executable was loaded and the addresses in its debug info (ASLR / PIE slide):
/// `frame - load_bias` is the address to resolve. `None` where it cannot be read.
pub fn load_bias() -> Option<usize> {
    imp::load_bias()
}

/// Install the handler for the signals above, writing into `dir`. Returns whether it is armed: `false` when it
/// already was, or the directory cannot be used (a path too long for the handler's buffer, not creatable).
/// Call once, early; every thread inherits it.
pub fn install_in(dir: &Path, build: &BuildInfo) -> bool {
    imp::install(dir, build)
}

/// [`install_in`] for this process: the crash directory and its own build.
pub fn install() -> bool {
    install_in(&crate::crash::crash_dir(), crate::buildinfo::current())
}

/// `0x1a2b` with no leading zeros; what a frame looks like in the file.
fn hex(v: usize) -> String {
    format!("0x{v:x}")
}

/// `crash-native-<unix_ms>-<pid>.toml`'s name prefix, shared by the writer, `report_time` and the host exit lookup.
pub const NAME_PREFIX: &str = "crash-native-";

#[cfg(unix)]
mod imp {
    use super::*;
    use std::ffi::{c_int, c_void};
    use std::os::unix::ffi::OsStrExt;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Signals whose default action kills the process with a core dump, and the words for the report.
    const SIGNALS: [(c_int, &str, &str); 5] = [
        (libc::SIGSEGV, "SIGSEGV", "invalid memory access"),
        (libc::SIGBUS, "SIGBUS", "bus error"),
        (libc::SIGILL, "SIGILL", "illegal instruction"),
        (libc::SIGFPE, "SIGFPE", "arithmetic exception"),
        (libc::SIGABRT, "SIGABRT", "abort"),
    ];
    /// Deepest stack written.
    const MAX_FRAMES: usize = 64;
    /// Longest crash directory the handler's path buffer takes. Small on purpose: the handler runs on the thread's
    /// signal stack (the standard library gives a thread about 8 KiB), and `backtrace(3)` needs most of it.
    const MAX_DIR: usize = 400;
    /// Room for `/crash-native-<ms>-<pid>-<n>.toml\0` after the directory.
    const NAME_ROOM: usize = 64;

    /// What the handler needs, prepared at install: it cannot build any of it in signal context.
    struct Armed {
        /// `<dir>/crash-native-`, no NUL.
        prefix: Vec<u8>,
        /// `version … arch`, `load_bias`: fixed lines, already TOML.
        header: Vec<u8>,
        /// The actions the signals had before, to hand the signal on to.
        previous: [libc::sigaction; SIGNALS.len()],
    }
    static ARMED: OnceLock<Armed> = OnceLock::new();
    /// A fault inside the handler must not run it again: the second one goes straight to the default action.
    static BUSY: AtomicBool = AtomicBool::new(false);

    pub fn frames() -> Vec<usize> {
        #[cfg(any(target_env = "gnu", target_vendor = "apple"))]
        {
            let mut buf = [std::ptr::null_mut::<c_void>(); MAX_FRAMES];
            // SAFETY: `buf` holds MAX_FRAMES pointers and that is the size passed.
            let n = unsafe { libc::backtrace(buf.as_mut_ptr(), MAX_FRAMES as c_int) };
            buf[..n.max(0) as usize]
                .iter()
                .map(|p| *p as usize)
                .collect()
        }
        #[cfg(not(any(target_env = "gnu", target_vendor = "apple")))]
        Vec::new()
    }

    pub fn load_bias() -> Option<usize> {
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        {
            // The first object `dl_iterate_phdr` reports is the main executable; 0 for a non-PIE one.
            unsafe extern "C" fn first(
                info: *mut libc::dl_phdr_info,
                _size: usize,
                out: *mut c_void,
            ) -> c_int {
                // SAFETY: the loader passes a valid `dl_phdr_info`, and `out` is the `usize` given below.
                unsafe { *(out as *mut usize) = (*info).dlpi_addr as usize };
                1 // stop after the first
            }
            let mut bias = 0usize;
            // SAFETY: `first` matches the callback type and `bias` outlives the call.
            unsafe { libc::dl_iterate_phdr(Some(first), (&mut bias as *mut usize).cast()) };
            Some(bias)
        }
        #[cfg(target_vendor = "apple")]
        {
            // `libc` marks this one deprecated in favour of `mach2`, a dependency for one call: declare it here.
            unsafe extern "C" {
                fn _dyld_get_image_vmaddr_slide(image_index: u32) -> isize;
            }
            // SAFETY: image 0 is the main executable; the call has no other precondition.
            Some(unsafe { _dyld_get_image_vmaddr_slide(0) } as usize)
        }
        #[cfg(not(any(all(target_os = "linux", target_env = "gnu"), target_vendor = "apple")))]
        None
    }

    pub fn install(dir: &Path, build: &BuildInfo) -> bool {
        let bytes = dir.as_os_str().as_bytes();
        if ARMED.get().is_some() || bytes.len() > MAX_DIR || bytes.contains(&0) {
            return false;
        }
        if std::fs::create_dir_all(dir).is_err() {
            return false;
        }
        crate::crash::prune_all(dir);

        let mut header = crate::crash::build_lines(build).into_bytes();
        if let Some(b) = load_bias() {
            header.extend(format!("load_bias = \"{}\"\n", hex(b)).bytes());
        }
        let mut prefix = bytes.to_vec();
        prefix.push(b'/');
        prefix.extend(NAME_PREFIX.bytes());

        // `backtrace` loads libgcc on its first call (a `dlopen`, which allocates): do it now, not in the handler.
        let _ = frames();

        let mut previous = [unsafe { std::mem::zeroed::<libc::sigaction>() }; SIGNALS.len()];
        let mut new = unsafe { std::mem::zeroed::<libc::sigaction>() };
        new.sa_sigaction = handler as *const () as usize;
        new.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
        // SAFETY: `new.sa_mask` is a valid sigset to initialize.
        unsafe { libc::sigemptyset(&mut new.sa_mask) };
        // The handler is read through `ARMED`, so it has to be there before the first signal can arrive. The old
        // actions are only known after `sigaction` returns them: install, then publish them (a signal in that window
        // finds no `ARMED` and takes the default path in `handler`).
        for (i, (sig, ..)) in SIGNALS.iter().enumerate() {
            // SAFETY: both pointers are valid `sigaction`s.
            unsafe { libc::sigaction(*sig, &new, &mut previous[i]) };
        }
        ARMED
            .set(Armed {
                prefix,
                header,
                previous,
            })
            .is_ok()
    }

    /// Buffered `write(2)` to one descriptor, no allocation.
    struct Out {
        fd: c_int,
        buf: [u8; 1024],
        len: usize,
    }
    impl Out {
        fn put(&mut self, mut s: &[u8]) {
            while !s.is_empty() {
                if self.len == self.buf.len() {
                    self.flush();
                }
                let n = s.len().min(self.buf.len() - self.len);
                self.buf[self.len..self.len + n].copy_from_slice(&s[..n]);
                self.len += n;
                s = &s[n..];
            }
        }
        fn flush(&mut self) {
            let mut at = 0;
            while at < self.len {
                // SAFETY: `buf[at..len]` is initialized and within the array.
                let n =
                    unsafe { libc::write(self.fd, self.buf[at..].as_ptr().cast(), self.len - at) };
                if n <= 0 {
                    break;
                }
                at += n as usize;
            }
            self.len = 0;
        }
        fn dec(&mut self, mut v: u64) {
            let mut b = [0u8; 20];
            let mut i = b.len();
            loop {
                i -= 1;
                b[i] = b'0' + (v % 10) as u8;
                v /= 10;
                if v == 0 {
                    break;
                }
            }
            self.put(&b[i..]);
        }
        fn hex(&mut self, mut v: usize) {
            let mut b = [0u8; 16];
            let mut i = b.len();
            loop {
                i -= 1;
                b[i] = b"0123456789abcdef"[v & 0xf];
                v >>= 4;
                if v == 0 {
                    break;
                }
            }
            self.put(b"0x");
            self.put(&b[i..]);
        }
    }

    extern "C" fn handler(sig: c_int, info: *mut libc::siginfo_t, ctx: *mut c_void) {
        let idx = SIGNALS.iter().position(|(s, ..)| *s == sig);
        if let (Some(armed), Some(idx)) = (ARMED.get(), idx)
            && !BUSY.swap(true, Ordering::SeqCst)
        {
            // SAFETY: `info` is the kernel's siginfo for this signal. `si_addr` means something only for a fault the
            // kernel raised (`si_code > 0`); for a `kill`/`raise` the same bytes are a sender's pid.
            let addr = unsafe {
                info.as_ref()
                    .filter(|i| i.si_code > 0)
                    .map_or(0, |i| i.si_addr() as usize)
            };
            write_report(armed, SIGNALS[idx], addr);
            // Held through the chain: the standard library's stack-overflow handler ends in `abort`, and that SIGABRT is
            // the same incident, not a second report. It takes the default path above.
            chain(&armed.previous[idx], sig, info, ctx);
            BUSY.store(false, Ordering::SeqCst);
        } else {
            reset(sig);
        }
    }

    /// Back to the default action and let it happen: a synchronous fault re-executes when the handler returns, and a
    /// `raise`d one is delivered the moment it is unblocked.
    fn reset(sig: c_int) {
        // SAFETY: `signal` and `raise` are async-signal-safe.
        unsafe {
            libc::signal(sig, libc::SIG_DFL);
            libc::raise(sig);
        }
    }

    /// Run what was installed before us (the standard library's stack-overflow handler prints its message there), or
    /// the default action when there was none. A handler that gives up its job by restoring the default action (the
    /// standard library's does, for a fault that is not a stack overflow) leaves a signal that was `raise`d still to
    /// deliver: it is raised again, so the process dies of it. One that stays installed handled the signal.
    fn chain(prev: &libc::sigaction, sig: c_int, info: *mut libc::siginfo_t, ctx: *mut c_void) {
        let f = prev.sa_sigaction;
        if f == libc::SIG_DFL || f == libc::SIG_IGN {
            return reset(sig);
        }
        // SAFETY: a non-default `sa_sigaction` is a handler with the signature its `SA_SIGINFO` flag says.
        unsafe {
            if prev.sa_flags & libc::SA_SIGINFO != 0 {
                let f: extern "C" fn(c_int, *mut libc::siginfo_t, *mut c_void) =
                    std::mem::transmute(f);
                f(sig, info, ctx);
            } else {
                let f: extern "C" fn(c_int) = std::mem::transmute(f);
                f(sig);
            }
            let mut now: libc::sigaction = std::mem::zeroed();
            if libc::sigaction(sig, std::ptr::null(), &mut now) == 0
                && now.sa_sigaction == libc::SIG_DFL
            {
                libc::raise(sig);
            }
        }
    }

    /// Create `<prefix><ms>-<pid>[-n].toml` exclusively; the descriptor, or -1. `create_new` makes the name ours.
    #[inline(never)]
    fn open_report(prefix: &[u8], ms: u64, pid: u64) -> c_int {
        for n in 0..8u64 {
            let mut path = [0u8; MAX_DIR + NAME_ROOM];
            let mut p = PathBuf::new(&mut path);
            p.put(prefix);
            p.dec(ms);
            p.put(b"-");
            p.dec(pid);
            if n > 0 {
                p.put(b"-");
                p.dec(n);
            }
            p.put(b".toml\0");
            // SAFETY: `path` is NUL-terminated by the last `put` (`install` bounds the prefix so it always fits).
            let fd = unsafe {
                libc::open(
                    path.as_ptr().cast(),
                    libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC,
                    0o600 as libc::c_uint,
                )
            };
            if fd >= 0
                || std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists
            {
                return fd;
            }
        }
        -1
    }

    fn write_report(a: &Armed, (sig, name, what): (c_int, &str, &str), addr: usize) {
        // SAFETY: a zeroed timespec is valid, and `clock_gettime` fills it.
        let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
        unsafe { libc::clock_gettime(libc::CLOCK_REALTIME, &mut ts) };
        let ms = ts.tv_sec as u64 * 1000 + ts.tv_nsec as u64 / 1_000_000;
        let pid = unsafe { libc::getpid() } as u64;
        let fd = open_report(&a.prefix, ms, pid);
        if fd < 0 {
            return;
        }

        let mut o = Out {
            fd,
            buf: [0; 1024],
            len: 0,
        };
        o.put(&a.header);
        o.put(b"timestamp = ");
        o.dec(ts.tv_sec as u64);
        o.put(b"\nkind = \"native\"\nsignal = \"");
        o.put(name.as_bytes());
        o.put(b"\"\nsignal_number = ");
        o.dec(sig as u64);
        o.put(b"\nfault_address = \"");
        o.hex(addr);
        // `msg` and `location` are what the summary readers (`atlas_core::probe`, the app's card) print.
        o.put(b"\"\nmsg = \"native crash: ");
        o.put(name.as_bytes());
        o.put(b" (");
        o.put(what.as_bytes());
        o.put(b")\"\nlocation = \"fault address ");
        o.hex(addr);
        o.put(b"\"\nframes = [");
        let mut buf = [std::ptr::null_mut::<c_void>(); MAX_FRAMES];
        #[cfg(any(target_env = "gnu", target_vendor = "apple"))]
        // SAFETY: `buf` holds MAX_FRAMES pointers.
        let n = unsafe { libc::backtrace(buf.as_mut_ptr(), MAX_FRAMES as c_int) }.max(0) as usize;
        #[cfg(not(any(target_env = "gnu", target_vendor = "apple")))]
        let n = 0;
        for (i, f) in buf[..n].iter().enumerate() {
            if i > 0 {
                o.put(b", ");
            }
            o.put(b"\"");
            o.hex(*f as usize);
            o.put(b"\"");
        }
        o.put(b"]\n");
        o.flush();
        // SAFETY: `fd` is the descriptor opened above.
        unsafe { libc::close(fd) };
    }

    /// A NUL-free byte builder over a stack array (the file name), no allocation.
    struct PathBuf<'a> {
        buf: &'a mut [u8],
        len: usize,
    }
    impl<'a> PathBuf<'a> {
        fn new(buf: &'a mut [u8]) -> Self {
            Self { buf, len: 0 }
        }
        fn put(&mut self, s: &[u8]) {
            let n = s.len().min(self.buf.len() - self.len);
            self.buf[self.len..self.len + n].copy_from_slice(&s[..n]);
            self.len += n;
        }
        fn dec(&mut self, mut v: u64) {
            let mut b = [0u8; 20];
            let mut i = b.len();
            loop {
                i -= 1;
                b[i] = b'0' + (v % 10) as u8;
                v /= 10;
                if v == 0 {
                    break;
                }
            }
            self.put(&b[i..]);
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::sync::OnceLock;
    use windows_sys::Win32::Foundation::{
        EXCEPTION_ACCESS_VIOLATION, EXCEPTION_ILLEGAL_INSTRUCTION, EXCEPTION_INT_DIVIDE_BY_ZERO,
        EXCEPTION_STACK_OVERFLOW,
    };
    use windows_sys::Win32::System::Diagnostics::Debug::{
        EXCEPTION_POINTERS, RtlCaptureStackBackTrace, SetUnhandledExceptionFilter,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

    struct Armed {
        dir: std::path::PathBuf,
        header: String,
    }
    static ARMED: OnceLock<Armed> = OnceLock::new();

    pub fn frames() -> Vec<usize> {
        let mut buf = [std::ptr::null_mut::<core::ffi::c_void>(); 62];
        // SAFETY: the buffer holds the 62 entries the call is told about.
        let n = unsafe { RtlCaptureStackBackTrace(0, 62, buf.as_mut_ptr(), std::ptr::null_mut()) };
        buf[..n as usize].iter().map(|p| *p as usize).collect()
    }

    pub fn load_bias() -> Option<usize> {
        // SAFETY: a null name is "the module of the calling process's executable".
        let h = unsafe { GetModuleHandleW(std::ptr::null()) };
        // A PE's debug info (the PDB) holds addresses relative to the image base: frame - base is the RVA.
        (!h.is_null()).then_some(h as usize)
    }

    pub fn install(dir: &Path, build: &BuildInfo) -> bool {
        if ARMED.get().is_some() || std::fs::create_dir_all(dir).is_err() {
            return false;
        }
        crate::crash::prune_all(dir);
        let mut header = crate::crash::build_lines(build);
        if let Some(b) = load_bias() {
            header.push_str(&format!("load_bias = \"{}\"\n", hex(b)));
        }
        let armed = Armed {
            dir: dir.to_path_buf(),
            header,
        };
        if ARMED.set(armed).is_err() {
            return false;
        }
        // SAFETY: `filter` matches the callback type.
        unsafe { SetUnhandledExceptionFilter(Some(filter)) };
        true
    }

    /// The process is already broken here; this is best effort and ordinary Rust (the Windows filter is not a signal
    /// handler). Returns `EXCEPTION_CONTINUE_SEARCH` (0) so Windows Error Reporting and any debugger still see it.
    unsafe extern "system" fn filter(info: *const EXCEPTION_POINTERS) -> i32 {
        let Some(a) = ARMED.get() else { return 0 };
        // SAFETY: Windows passes a valid pointer to the exception record.
        let (code, addr) = unsafe {
            let r = (*info).ExceptionRecord;
            if r.is_null() {
                return 0;
            }
            ((*r).ExceptionCode, (*r).ExceptionAddress as usize)
        };
        let name = match code {
            EXCEPTION_ACCESS_VIOLATION => "EXCEPTION_ACCESS_VIOLATION",
            EXCEPTION_STACK_OVERFLOW => "EXCEPTION_STACK_OVERFLOW",
            EXCEPTION_ILLEGAL_INSTRUCTION => "EXCEPTION_ILLEGAL_INSTRUCTION",
            EXCEPTION_INT_DIVIDE_BY_ZERO => "EXCEPTION_INT_DIVIDE_BY_ZERO",
            _ => "EXCEPTION",
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let frames = frames()
            .iter()
            .map(|f| format!("\"{}\"", hex(*f)))
            .collect::<Vec<_>>()
            .join(", ");
        let text = format!(
            "{header}timestamp = {ts}\nkind = \"native\"\nsignal = \"{name}\"\nsignal_number = {code}\nfault_address = \"{addr}\"\nmsg = \"native crash: {name}\"\nlocation = \"fault address {addr}\"\nframes = [{frames}]\n",
            header = a.header,
            ts = now.as_secs(),
            addr = hex(addr),
            code = code as u32,
        );
        for n in 0..8 {
            let suffix = if n == 0 {
                String::new()
            } else {
                format!("-{n}")
            };
            let path = a.dir.join(format!(
                "{NAME_PREFIX}{}-{}{suffix}.toml",
                now.as_millis(),
                std::process::id()
            ));
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
            {
                use std::io::Write;
                let _ = f.write_all(text.as_bytes());
                break;
            }
        }
        0
    }
}

#[cfg(not(any(unix, windows)))]
mod imp {
    use super::*;
    pub fn frames() -> Vec<usize> {
        Vec::new()
    }
    pub fn load_bias() -> Option<usize> {
        None
    }
    pub fn install(_: &Path, _: &BuildInfo) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The crasher: `#[inline(never)]` so its frame is in the stack the handler writes and in the debug info.
    #[cfg(unix)]
    #[inline(never)]
    #[unsafe(no_mangle)]
    pub extern "C" fn atlas_crash_on_purpose(sig: i32) {
        // SAFETY: raising a signal in the child process, which exists to die of it.
        unsafe { libc::raise(sig) };
    }

    #[inline(never)]
    fn fault_through_a_null_pointer() {
        // SAFETY: none: this is the SIGSEGV the child exists to take.
        unsafe { std::ptr::write_volatile(std::ptr::null_mut::<u8>(), 1) };
    }

    const CHILD_DIR: &str = "ATLAS_NATIVE_CRASH_TEST_DIR";
    const CHILD_HOW: &str = "ATLAS_NATIVE_CRASH_TEST_HOW";

    /// Not a test: the body of the child process the tests below start (`current_exe --exact child`). It installs the
    /// handler for a directory the parent chose, then dies of the signal the parent asked for.
    #[test]
    fn child_that_crashes() {
        let (Ok(dir), Ok(how)) = (std::env::var(CHILD_DIR), std::env::var(CHILD_HOW)) else {
            return; // a normal run of the suite: nothing to do
        };
        // No "program has stopped working" dialog on a CI runner or a desktop: the crash is the test.
        #[cfg(windows)]
        // SAFETY: `SetErrorMode` only sets this process's error-mode flags.
        unsafe {
            windows_sys::Win32::System::Diagnostics::Debug::SetErrorMode(
                windows_sys::Win32::System::Diagnostics::Debug::SEM_NOGPFAULTERRORBOX
                    | windows_sys::Win32::System::Diagnostics::Debug::SEM_FAILCRITICALERRORS,
            )
        };
        assert!(install_in(Path::new(&dir), crate::buildinfo::current()));
        match how.as_str() {
            "null" => fault_through_a_null_pointer(),
            #[cfg(unix)]
            "abort" => atlas_crash_on_purpose(libc::SIGABRT),
            #[cfg(unix)]
            "bus" => atlas_crash_on_purpose(libc::SIGBUS),
            #[cfg(unix)]
            "overflow" => {
                #[inline(never)]
                fn down(n: u64) -> u64 {
                    std::hint::black_box(&[n; 64]);
                    if n == u64::MAX {
                        return 0;
                    }
                    down(n + 1) + 1
                }
                down(0);
            }
            other => panic!("unknown crash {other}"),
        }
        unreachable!("the process should be dead");
    }

    /// Start the child, wait for it, and return how it ended and the directory it wrote into.
    fn crash_a_child(how: &str) -> (std::process::ExitStatus, tempfile::TempDir) {
        crash_a_child_of(&std::env::current_exe().unwrap(), how)
    }

    /// [`crash_a_child`] for another copy of this test executable (the stripped one a release ships).
    fn crash_a_child_of(exe: &Path, how: &str) -> (std::process::ExitStatus, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let st = std::process::Command::new(exe)
            .args([
                "--exact",
                "crash_native::tests::child_that_crashes",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD_DIR, dir.path())
            .env(CHILD_HOW, how)
            .env_remove("RUST_BACKTRACE")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        (st, dir)
    }

    fn reports(dir: &Path) -> Vec<std::path::PathBuf> {
        let mut v: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(NAME_PREFIX) && n.ends_with(".toml"))
            })
            .collect();
        v.sort();
        v
    }

    fn field(text: &str, key: &str) -> Option<String> {
        text.lines()
            .find_map(|l| l.strip_prefix(key)?.strip_prefix(" = "))
            .map(|v| v.trim().trim_matches('"').to_string())
    }

    /// #318: "Forced SIGSEGV in a test binary leaves `crash-native-*.toml`". A real child process dereferences null;
    /// the report names the signal, the build and the fault, and the process still dies of SIGSEGV (a core dump or
    /// systemd-coredump still sees it).
    ///
    /// On Windows the same test is an access violation: the unhandled-exception filter runs in a real child, and the
    /// process still ends with the exception.
    #[test]
    fn a_segfault_leaves_a_native_report_and_the_process_still_dies_of_it() {
        let (st, dir) = crash_a_child("null");
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(st.signal(), Some(libc::SIGSEGV), "{st:?}");
        }
        #[cfg(windows)]
        assert_eq!(st.code(), Some(0xC000_0005u32 as i32), "{st:?}");
        let signal = if cfg!(windows) {
            "EXCEPTION_ACCESS_VIOLATION"
        } else {
            "SIGSEGV"
        };
        let files = reports(dir.path());
        assert_eq!(files.len(), 1, "{files:?}");
        let name = files[0].file_name().unwrap().to_str().unwrap().to_string();
        assert!(
            crate::crash::report_time(&name).is_some(),
            "{name} is not read as a report name"
        );

        let text = std::fs::read_to_string(&files[0]).unwrap();
        assert_eq!(field(&text, "kind").as_deref(), Some("native"), "{text}");
        assert_eq!(field(&text, "signal").as_deref(), Some(signal), "{text}");
        // The address of the data a fault touched is the Unix report's; Windows reports the faulting instruction.
        #[cfg(unix)]
        assert_eq!(
            field(&text, "fault_address").as_deref(),
            Some("0x0"),
            "{text}"
        );
        let b = crate::buildinfo::current();
        // The build id is the one `atlas version` prints and the symbols are keyed by: it must exist where the
        // platform's linker writes one (ELF note, Mach-O UUID, PE CodeView signature of an MSVC build).
        if cfg!(any(
            all(target_os = "linux", target_env = "gnu"),
            target_vendor = "apple",
            target_env = "msvc"
        )) {
            assert!(
                b.build_id.is_some(),
                "no build id read from {:?}",
                std::env::current_exe()
            );
        }
        assert_eq!(field(&text, "version").as_deref(), Some(b.version.as_str()));
        assert_eq!(field(&text, "channel").as_deref(), Some(b.channel.as_str()));
        assert_eq!(field(&text, "commit").as_deref(), Some(b.commit.as_str()));
        assert_eq!(field(&text, "build_id"), b.build_id, "{text}");
        assert!(
            field(&text, "timestamp").is_some_and(|t| t.parse::<u64>().is_ok()),
            "{text}"
        );
        assert!(text.contains("frames = [\"0x"), "{text}");
        // Nothing of the child's command line or environment is in it.
        for private in [CHILD_DIR, CHILD_HOW, "--exact", "args ="] {
            assert!(!text.contains(private), "{private} leaked:\n{text}");
        }
        // The summary readers (doctor bundle, the app's card) take it like any other report.
        let found = crate::probe::crash_files(dir.path(), u64::MAX);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0]
                .summary
                .contains(&format!("native crash: {signal}")),
            "{}",
            found[0].summary
        );
    }

    /// #320: the same crash twice, in two processes the loader places at two addresses (ASLR), is one fingerprint
    /// from the file alone (no symbols), and another crash is another.
    #[cfg(unix)]
    #[test]
    fn a_native_crash_has_one_fingerprint_whatever_the_load_address() {
        let of = |how: &str| {
            let (_, dir) = crash_a_child(how);
            let text = std::fs::read_to_string(reports(dir.path()).pop().unwrap()).unwrap();
            (
                crate::crash_fingerprint::of_report(&text, None),
                field(&text, "load_bias"),
            )
        };
        let (a, bias_a) = of("abort");
        let (b, bias_b) = of("abort");
        assert!(a.is_some(), "no fingerprint for a real abort");
        assert_eq!(a, b, "biases {bias_a:?} {bias_b:?}");
        assert_ne!(a, of("bus").0, "SIGBUS is not SIGABRT");
    }

    #[cfg(unix)]
    #[test]
    fn an_abort_and_a_bus_error_leave_their_own_signal_in_the_report() {
        for (how, sig, name) in [
            ("abort", libc::SIGABRT, "SIGABRT"),
            ("bus", libc::SIGBUS, "SIGBUS"),
        ] {
            let (st, dir) = crash_a_child(how);
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(st.signal(), Some(sig), "{how}: {st:?}");
            let files = reports(dir.path());
            assert_eq!(files.len(), 1, "{how}: {files:?}");
            let text = std::fs::read_to_string(&files[0]).unwrap();
            assert_eq!(field(&text, "signal").as_deref(), Some(name), "{text}");
            assert_eq!(
                field(&text, "signal_number"),
                Some(sig.to_string()),
                "{text}"
            );
        }
    }

    /// The standard library's stack-overflow handler was installed first and prints the message the user and the
    /// maintainer read; ours must run in front of it, leave the report, and still hand the signal on to it.
    #[cfg(unix)]
    #[test]
    fn a_stack_overflow_leaves_a_report_and_the_standard_librarys_message() {
        let dir = tempfile::tempdir().unwrap();
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "crash_native::tests::child_that_crashes",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD_DIR, dir.path())
            .env(CHILD_HOW, "overflow")
            .env_remove("RUST_BACKTRACE")
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(err.contains("has overflowed its stack"), "{err}");
        let files = reports(dir.path());
        assert_eq!(files.len(), 1, "{files:?}\n{err}");
        let text = std::fs::read_to_string(&files[0]).unwrap();
        // macOS reports a guard-page hit as SIGBUS, Linux as SIGSEGV.
        assert!(
            matches!(
                field(&text, "signal").as_deref(),
                Some("SIGSEGV" | "SIGBUS")
            ),
            "{text}"
        );
    }

    /// Installing twice in one process is a no-op, and an unusable directory is reported, not half-armed.
    #[cfg(unix)]
    #[test]
    fn a_directory_the_handler_cannot_use_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let file = d.path().join("a-file");
        std::fs::write(&file, "").unwrap();
        // A path under a regular file cannot be created.
        assert!(!install_in(
            &file.join("crashes"),
            crate::buildinfo::current()
        ));
        assert!(!install_in(
            &d.path().join("x".repeat(500)),
            crate::buildinfo::current()
        ));
    }

    /// #318: "A test resolves a Linux crash address against the published symbols". The crash file the child wrote is
    /// resolved with `scripts/symbols.sh`, the same two commands a release runs: `extract` makes the symbol file for
    /// the executable (named by its build id), `resolve` turns the report's frames into functions and lines. The
    /// crasher's own function must be among them. Skipped where binutils is missing.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_crash_report_resolves_to_the_crashing_function_with_the_published_symbols() {
        let tool = |t: &str| {
            std::process::Command::new(t)
                .arg("--version")
                .output()
                .is_ok()
        };
        if !tool("objcopy") || !tool("addr2line") {
            eprintln!("skipped: binutils (objcopy, addr2line) not installed");
            return;
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let script = root.join("scripts/symbols.sh");
        let (_, dir) = crash_a_child("abort");
        let report = reports(dir.path()).pop().expect("the child left a report");
        let b = crate::buildinfo::current();
        let id = b
            .build_id
            .clone()
            .expect("a Linux test executable has a build id");

        let symbols = tempfile::tempdir().unwrap();
        let exe = std::env::current_exe().unwrap();
        let ex = std::process::Command::new("sh")
            .arg(&script)
            .arg("extract")
            .arg(&exe)
            .arg(symbols.path())
            .output()
            .unwrap();
        assert!(
            ex.status.success(),
            "extract: {}",
            String::from_utf8_lossy(&ex.stderr)
        );
        assert!(
            symbols.path().join(format!("{id}.debug")).is_file(),
            "the symbol file is named by the build id: {:?}",
            std::fs::read_dir(symbols.path())
                .unwrap()
                .flatten()
                .map(|e| e.file_name())
                .collect::<Vec<_>>()
        );

        let rs = std::process::Command::new("sh")
            .arg(&script)
            .arg("resolve")
            .arg(&report)
            .arg(symbols.path())
            .output()
            .unwrap();
        let out = String::from_utf8_lossy(&rs.stdout);
        assert!(
            rs.status.success(),
            "resolve: {out}{}",
            String::from_utf8_lossy(&rs.stderr)
        );
        let line = out
            .lines()
            .find(|l| l.contains("atlas_crash_on_purpose"))
            .unwrap_or_else(|| panic!("no frame resolved to the crasher:\n{out}"));
        assert!(
            line.contains("crash_native.rs:"),
            "file and line are resolved: {line}"
        );
        // #320: with the symbols the fingerprint is of the crashing function and its file.
        let text = std::fs::read_to_string(&report).unwrap();
        assert_eq!(
            crate::crash_fingerprint::of_report(&text, Some(&out)),
            Some(crate::fingerprint::with_key(
                "native crash: SIGABRT",
                "atlas_crash_on_purpose|crates/atlas-core/src/crash_native.rs"
            )),
            "{out}"
        );
    }

    /// What a release ships is the binary with its debug info taken out (`symbols.sh split`), not the one the symbols
    /// came from. A crash of THAT copy must still resolve, with the symbols split off it: same build id, same
    /// addresses, and the shipped file really is smaller and free of DWARF (`verify-stripped` fails on the original).
    #[cfg(target_os = "linux")]
    #[test]
    fn a_crash_of_the_stripped_shipped_binary_resolves_with_the_symbols_split_off_it() {
        let tool = |t: &str| {
            std::process::Command::new(t)
                .arg("--version")
                .output()
                .is_ok()
        };
        if !["objcopy", "addr2line", "readelf"].iter().all(|t| tool(t)) {
            eprintln!("skipped: binutils not installed");
            return;
        }
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/symbols.sh");
        let sh = |args: &[&std::ffi::OsStr]| {
            std::process::Command::new("sh")
                .arg(&script)
                .args(args)
                .output()
                .unwrap()
        };
        let exe = std::env::current_exe().unwrap();
        // Next to the original (the target directory), not in /tmp: a test executable with debug info is large.
        let work = tempfile::tempdir_in(exe.parent().unwrap()).unwrap();
        let shipped = work.path().join("shipped");
        std::fs::copy(&exe, &shipped).unwrap();
        let symbols = work.path().join("symbols");

        assert!(
            !sh(&["verify-stripped".as_ref(), exe.as_os_str()])
                .status
                .success(),
            "the unstripped original has debug info: the check must say so"
        );
        let split = sh(&["split".as_ref(), shipped.as_os_str(), symbols.as_os_str()]);
        assert!(
            split.status.success(),
            "split: {}",
            String::from_utf8_lossy(&split.stderr)
        );
        let v = sh(&["verify-stripped".as_ref(), shipped.as_os_str()]);
        assert!(
            v.status.success(),
            "verify-stripped: {}",
            String::from_utf8_lossy(&v.stderr)
        );
        let (before, after) = (
            std::fs::metadata(&exe).unwrap().len(),
            std::fs::metadata(&shipped).unwrap().len(),
        );
        assert!(
            after < before,
            "the shipped copy must be smaller: {after} >= {before}"
        );
        assert_eq!(
            crate::buildinfo::build_id_of(&shipped),
            crate::buildinfo::build_id_of(&exe),
            "stripping must keep the build id the symbols are named by"
        );

        let (_, dir) = crash_a_child_of(&shipped, "abort");
        let report = reports(dir.path())
            .pop()
            .expect("the stripped child left a report");
        let rs = sh(&["resolve".as_ref(), report.as_os_str(), symbols.as_os_str()]);
        let out = String::from_utf8_lossy(&rs.stdout);
        assert!(
            rs.status.success(),
            "resolve: {out}{}",
            String::from_utf8_lossy(&rs.stderr)
        );
        let line = out
            .lines()
            .find(|l| l.contains("atlas_crash_on_purpose"))
            .unwrap_or_else(|| panic!("no frame resolved to the crasher:\n{out}"));
        assert!(line.contains("crash_native.rs:"), "{line}");
    }
}
