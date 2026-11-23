// The spike configuration of docs/terminal-linux-design.md section 5.4, as a scratch crate (not in the workspace): `cat <corpus>`
// writes into a real pty slave with output processing ON (\n becomes \r\n), this process reads the master with one 64 KiB
// `read` per call and feeds `atlas_term::Vt`. Modes: `plain` (read, feed, on one thread), `wait <us>` (sleep after a short read),
// `thread <KiB>` (a reader thread only reads and queues, a parse thread takes everything queued and feeds it: the coalescing
// is the backpressure, KiB is the queue cap). Prints MB/s, reads, mean read size and the process CPU per MB.
//
//   Cargo.toml: [dependencies] atlas-term = { path = "<repo>/crates/atlas-term" }, libc = "0.2"; [workspace]; copy <repo>/Cargo.lock and rust-toolchain.toml
//   cargo run --release --offline -- <corpus.bin> plain|wait 200|thread 64
use std::os::fd::{FromRawFd, OwnedFd, AsRawFd};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let file = &a[1];
    let mode = a.get(2).map(String::as_str).unwrap_or("plain");
    let wait_us: u64 = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(500);
    let (mut m, mut s) = (0, 0);
    unsafe { assert_eq!(libc::openpty(&mut m, &mut s, std::ptr::null_mut(), std::ptr::null(), std::ptr::null()), 0); }
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(m), OwnedFd::from_raw_fd(s)) };
    // Output processing stays on (the shell default: \n becomes \r\n), echo off.
    unsafe { let mut t: libc::termios = std::mem::zeroed(); libc::tcgetattr(slave.as_raw_fd(), &mut t); t.c_lflag &= !(libc::ECHO | libc::ICANON); libc::tcsetattr(slave.as_raw_fd(), libc::TCSANOW, &t); }
    let mut child = Command::new("cat").arg(file).stdout(Stdio::from(slave.try_clone().unwrap())).spawn().unwrap();
    drop(slave);
    let vt = std::sync::Arc::new(atlas_term::Vt::new(120, 40, |_| {}));
    let mut buf = vec![0u8; 64 * 1024];
    let (mut reads, mut total) = (0u64, 0u64);
    let t0 = Instant::now();
    let cpu0 = cpu();
    if mode == "thread" {
        let q = std::sync::Arc::new((std::sync::Mutex::new((Vec::<u8>::new(), false)), std::sync::Condvar::new(), std::sync::Condvar::new()));
        let (q2, vt2) = (q.clone(), vt.clone());
        let cap: usize = wait_us as usize * 1024; // arg3 = KiB of backpressure
        let parser = std::thread::spawn(move || {
            let mut feeds = 0u64; let mut fed = 0u64;
            let mut local: Vec<u8> = Vec::new();
            loop {
                {
                    let (m, c_data, c_room) = &*q2;
                    let mut g = m.lock().unwrap();
                    while g.0.is_empty() && !g.1 { g = c_data.wait(g).unwrap(); }
                    if g.0.is_empty() && g.1 { break; }
                    std::mem::swap(&mut g.0, &mut local);
                    c_room.notify_one();
                }
                for c in local.chunks(64 * 1024) { vt2.feed(c); feeds += 1; fed += c.len() as u64; }
                local.clear();
            }
            (feeds, fed)
        });
        loop {
            let n = unsafe { libc::read(master.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 { break; }
            reads += 1; total += n as u64;
            let (m, c_data, c_room) = &*q;
            let mut g = m.lock().unwrap();
            while g.0.len() >= cap { g = c_room.wait(g).unwrap(); }
            g.0.extend_from_slice(&buf[..n as usize]);
            c_data.notify_one();
        }
        { let (m, c_data, _) = &*q; m.lock().unwrap().1 = true; c_data.notify_one(); }
        let (feeds, _) = parser.join().unwrap();
        println!("  parse thread: {feeds} feeds, mean {} B", total / feeds.max(1));
    } else {
        loop {
            let n = unsafe { libc::read(master.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 { break; }
            reads += 1; total += n as u64;
            vt.feed(&buf[..n as usize]);
            if mode == "wait" && (n as usize) < 8192 { std::thread::sleep(Duration::from_micros(wait_us)); }
        }
    }
    let secs = t0.elapsed().as_secs_f64();
    let _ = child.wait();
    println!("{mode} wait={wait_us}us: {:.1} MB/s, {} reads, mean {} B, cpu {:.2}s ({:.1} ms/MB), ", total as f64 / 1e6 / secs, reads, total / reads.max(1), cpu() - cpu0, (cpu() - cpu0) * 1000.0 / (total as f64 / 1e6));
}
fn cpu() -> f64 { let mut ts: libc::timespec = unsafe { std::mem::zeroed() }; unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut ts); } ts.tv_sec as f64 + ts.tv_nsec as f64 / 1e9 }
