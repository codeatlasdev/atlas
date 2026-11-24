//! k2p: key to composited frame, measured from outside the app on a wlroots compositor (private headless sway).
//!
//! Types a key into the focused surface with a virtual keyboard, stamps CLOCK_MONOTONIC just before the request is
//! flushed, and takes the output through wlr-screencopy (copy_with_damage, so one `ready` per composited frame, stamped
//! by the compositor with its own CLOCK_MONOTONIC commit time). The latency of a key is the stamp of the first frame whose
//! pixels in --region differ from the settled frame before the key, minus the key stamp. Keys alternate `a` and
//! BackSpace so the prompt line stays short. The same program measures xterm.js and the native terminal, so the
//! quantization (headless output refresh) is the same on both sides.
//!
//! k2p --region x,y,w,h [--n 60] [--control] [--settle-ms 350]   (region in output pixels; JSON lines on stdout)
use std::collections::VecDeque;
use std::os::fd::AsFd;

use memmap2::MmapMut;
use rustix::event::{poll, PollFd, PollFlags};
use rustix::time::{clock_gettime, ClockId};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_buffer, wl_output, wl_registry, wl_seat, wl_shm, wl_shm_pool};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};
use wayland_protocols_wlr::screencopy::v1::client::{
    zwlr_screencopy_frame_v1::{self, ZwlrScreencopyFrameV1},
    zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
};

const KEYMAP: &str = r#"xkb_keymap {
 xkb_keycodes "k" { minimum = 8; maximum = 255; <AC01> = 38; <BKSP> = 22; };
 xkb_types "k" { include "complete" };
 xkb_compat "k" { include "complete" };
 xkb_symbols "k" { key <AC01> { [ a, A ] }; key <BKSP> { [ BackSpace ] }; };
};
"#;
const KEY_A: u32 = 30;
const KEY_BACKSPACE: u32 = 14;

struct Frame {
    ts_ns: u64,
    seen_ns: u64,
    region: Vec<u8>,
}

struct State {
    shm: wl_shm::WlShm,
    manager: ZwlrScreencopyManagerV1,
    output: wl_output::WlOutput,
    region: (usize, usize, usize, usize),
    info: Option<(u32, u32, u32)>,
    format: Option<wl_shm::Format>,
    flags: u32,
    buffer: Option<(wl_buffer::WlBuffer, MmapMut, usize)>,
    frames: VecDeque<Frame>,
    failed: u32,
}

fn now_ns() -> u64 {
    let t = clock_gettime(ClockId::Monotonic);
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}

impl State {
    fn request(&self, qh: &QueueHandle<State>) {
        self.manager.capture_output(0, &self.output, qh, ());
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
macro_rules! quiet {
    ($($t:ty),*) => {$(
        impl Dispatch<$t, ()> for State {
            fn event(_: &mut Self, _: &$t, _: <$t as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
        }
    )*};
}
quiet!(wl_shm::WlShm, wl_shm_pool::WlShmPool, wl_buffer::WlBuffer, wl_output::WlOutput, wl_seat::WlSeat, ZwlrScreencopyManagerV1, ZwpVirtualKeyboardManagerV1, ZwpVirtualKeyboardV1);

impl Dispatch<ZwlrScreencopyFrameV1, ()> for State {
    fn event(s: &mut Self, frame: &ZwlrScreencopyFrameV1, ev: zwlr_screencopy_frame_v1::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        use zwlr_screencopy_frame_v1::Event::*;
        match ev {
            Buffer { format, width, height, stride } => {
                s.info = Some((width, height, stride));
                if let wayland_client::WEnum::Value(f) = format {
                    if s.format.is_none() || matches!(f, wl_shm::Format::Xrgb8888 | wl_shm::Format::Argb8888) {
                        s.format = Some(f);
                    }
                }
            }
            Flags { flags } => s.flags = flags.into(),
            BufferDone => {
                let (w, h, stride) = s.info.expect("buffer info");
                let size = stride as usize * h as usize;
                if s.buffer.as_ref().is_none_or(|b| b.2 != size) {
                    let fd = rustix::fs::memfd_create("k2p", rustix::fs::MemfdFlags::CLOEXEC).unwrap();
                    rustix::fs::ftruncate(&fd, size as u64).unwrap();
                    let file = std::fs::File::from(fd);
                    let map = unsafe { MmapMut::map_mut(&file).unwrap() };
                    let pool = s.shm.create_pool(file.as_fd(), size as i32, qh, ());
                    let buf = pool.create_buffer(0, w as i32, h as i32, stride as i32, s.format.unwrap(), qh, ());
                    pool.destroy();
                    s.buffer = Some((buf, map, size));
                }
                frame.copy_with_damage(&s.buffer.as_ref().unwrap().0);
            }
            Ready { tv_sec_hi, tv_sec_lo, tv_nsec } => {
                let seen = now_ns();
                let ts = ((tv_sec_hi as u64) << 32 | tv_sec_lo as u64) * 1_000_000_000 + tv_nsec as u64;
                let (_, h, stride) = s.info.unwrap();
                let (rx, ry, rw, rh) = s.region;
                let map = &s.buffer.as_ref().unwrap().1;
                let mut region = Vec::with_capacity(rw * rh * 4);
                for r in 0..rh {
                    let row = if s.flags & 1 != 0 { h as usize - 1 - (ry + r) } else { ry + r };
                    let o = row * stride as usize + rx * 4;
                    region.extend_from_slice(&map[o..o + rw * 4]);
                }
                s.frames.push_back(Frame { ts_ns: ts, seen_ns: seen, region });
                frame.destroy();
                s.request(qh);
            }
            Failed => {
                s.failed += 1;
                frame.destroy();
                s.request(qh);
            }
            _ => {}
        }
    }
}

fn pump(conn: &Connection, q: &mut EventQueue<State>, s: &mut State, ms: u16) {
    q.dispatch_pending(s).unwrap();
    conn.flush().unwrap();
    if let Some(guard) = conn.prepare_read() {
        let fd = guard.connection_fd();
        let mut fds = [PollFd::new(&fd, PollFlags::IN)];
        let ts = rustix::time::Timespec { tv_sec: 0, tv_nsec: ms as i64 * 1_000_000 };
        let n = poll(&mut fds, Some(&ts)).unwrap();
        if n > 0 {
            let _ = guard.read();
        }
    }
    q.dispatch_pending(s).unwrap();
}

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

fn main() {
    let region: Vec<usize> = arg("--region").expect("--region x,y,w,h").split(',').map(|v| v.parse().unwrap()).collect();
    let n: usize = arg("--n").map_or(60, |v| v.parse().unwrap());
    let settle_ms: u64 = arg("--settle-ms").map_or(350, |v| v.parse().unwrap());
    let control = std::env::args().any(|a| a == "--control");

    let conn = Connection::connect_to_env().expect("connect");
    let (globals, mut q) = registry_queue_init::<State>(&conn).unwrap();
    let qh = q.handle();
    let shm: wl_shm::WlShm = globals.bind(&qh, 1..=1, ()).unwrap();
    let manager: ZwlrScreencopyManagerV1 = globals.bind(&qh, 1..=3, ()).expect("wlr-screencopy");
    let output: wl_output::WlOutput = globals.bind(&qh, 1..=1, ()).unwrap();
    let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=1, ()).unwrap();
    let vkm: ZwpVirtualKeyboardManagerV1 = globals.bind(&qh, 1..=1, ()).expect("virtual keyboard");
    let vk = vkm.create_virtual_keyboard(&seat, &qh, ());
    let kfd = rustix::fs::memfd_create("km", rustix::fs::MemfdFlags::CLOEXEC).unwrap();
    let mut bytes = KEYMAP.as_bytes().to_vec();
    bytes.push(0);
    rustix::fs::ftruncate(&kfd, bytes.len() as u64).unwrap();
    rustix::io::write(&kfd, &bytes).unwrap();
    let kfile = std::fs::File::from(kfd);
    let mut map = unsafe { MmapMut::map_mut(&kfile).unwrap() };
    map.copy_from_slice(&bytes);
    vk.keymap(1, kfile.as_fd(), bytes.len() as u32);

    let mut s = State { shm, manager, output, region: (region[0], region[1], region[2], region[3]), info: None, format: None, flags: 0, buffer: None, frames: VecDeque::new(), failed: 0 };
    s.request(&qh);

    // Warm up: the first frames, and the clock sanity (a frame's stamp is within a refresh of "now" when the clocks agree).
    let t0 = now_ns();
    while now_ns() - t0 < 1_500_000_000 {
        pump(&conn, &mut q, &mut s, 50);
    }
    if let Some(f) = s.frames.back() {
        eprintln!("clock check: last frame stamp is {:.2} ms before it was read ({} frames, {} failed)", (f.seen_ns as f64 - f.ts_ns as f64) / 1e6, s.frames.len(), s.failed);
    }

    if control {
        // No keys: how often does the region change by itself (a blinking cursor, a clock)?
        s.frames.clear();
        let t = now_ns();
        let mut changes = 0;
        let mut last: Option<Vec<u8>> = None;
        while now_ns() - t < 4_000_000_000 {
            pump(&conn, &mut q, &mut s, 50);
            while let Some(f) = s.frames.pop_front() {
                if last.as_ref().is_some_and(|l| *l != f.region) {
                    changes += 1;
                }
                last = Some(f.region);
            }
        }
        println!("{{\"control_region_changes_in_4s\":{changes}}}");
        return;
    }

    let mut lat: Vec<f64> = vec![];
    let mut last_change = now_ns();
    let mut baseline = s.frames.back().map(|f| f.region.clone()).expect("no frame");
    let mut rng: u64 = 0x9E3779B97F4A7C15;
    for i in 0..n {
        // Settle: no region change for settle_ms (plus a jittered wait so the key does not lock to the refresh phase).
        loop {
            pump(&conn, &mut q, &mut s, 20);
            while let Some(f) = s.frames.pop_front() {
                if f.region != baseline {
                    last_change = f.seen_ns;
                    baseline = f.region;
                }
            }
            if now_ns() - last_change > settle_ms * 1_000_000 {
                break;
            }
        }
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let jitter = rng % 16_000_000;
        let until = now_ns() + jitter;
        while now_ns() < until {
            pump(&conn, &mut q, &mut s, 2);
        }
        s.frames.clear();
        let key = if i % 2 == 0 { KEY_A } else { KEY_BACKSPACE };
        let t_key = now_ns();
        vk.key((t_key / 1_000_000) as u32, key, 1);
        conn.flush().unwrap();
        vk.key((t_key / 1_000_000) as u32 + 1, key, 0);
        let mut got: Option<f64> = None;
        let deadline = now_ns() + 800_000_000;
        'wait: while now_ns() < deadline {
            pump(&conn, &mut q, &mut s, 5);
            while let Some(f) = s.frames.pop_front() {
                if f.ts_ns >= t_key && f.region != baseline {
                    got = Some((f.ts_ns - t_key) as f64 / 1e6);
                    last_change = f.seen_ns;
                    baseline = f.region;
                    break 'wait;
                }
            }
        }
        match got {
            Some(ms) => {
                println!("{{\"i\":{i},\"key\":\"{}\",\"ms\":{ms:.2}}}", if key == KEY_A { "a" } else { "bksp" });
                lat.push(ms);
            }
            None => println!("{{\"i\":{i},\"missed\":true}}"),
        }
    }
    lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pct = |p: f64| lat.get(((lat.len() as f64 - 1.0) * p).round() as usize).copied().unwrap_or(f64::NAN);
    println!("{{\"summary\":{{\"n\":{},\"missed\":{},\"p50\":{:.2},\"p95\":{:.2},\"max\":{:.2},\"min\":{:.2}}}}}", lat.len(), n - lat.len(), pct(0.5), pct(0.95), lat.last().copied().unwrap_or(f64::NAN), lat.first().copied().unwrap_or(f64::NAN));
}
