//! Live frames of an agent instance for `bun run desktop:mirror` on Linux (#282): a `grim` loop on the instance's own
//! compositor, JPEG in memory, ~30 fps. The loop starts on the first pull and ends by itself when nobody has pulled
//! for [`IDLE`], so an unwatched instance pays nothing. The picture is the webview rectangle (what `debug shot` shows),
//! so the mirror's click mapping (`image x / image width * page width`) is exact.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tauri::AppHandle;

use super::latest::Shared;
use super::linux::{Rect, grim_frame, view_rect};

/// End the loop after this long without a pull (the same as the macOS stream).
pub const IDLE: Duration = Duration::from_secs(5);
const PERIOD: Duration = Duration::from_millis(33);
/// The window may be resized by the page or by `debug resize`: look its rectangle up again this often.
const RECT_REFRESH: Duration = Duration::from_secs(1);
const JPEG: [&str; 4] = ["-t", "jpeg", "-q", "60"];

/// `(width, height)` of a JPEG, from its first start-of-frame segment.
pub fn jpeg_size(jpeg: &[u8]) -> Option<(usize, usize)> {
    if jpeg.get(..2)? != [0xFF, 0xD8] {
        return None;
    }
    let mut i = 2;
    loop {
        // Marker: 0xFF (possibly repeated as padding) then its code.
        while *jpeg.get(i)? == 0xFF && *jpeg.get(i + 1)? == 0xFF {
            i += 1;
        }
        if *jpeg.get(i)? != 0xFF {
            return None;
        }
        let code = *jpeg.get(i + 1)?;
        i += 2;
        match code {
            // Standalone markers carry no length (restart markers, SOI, TEM).
            0x01 | 0xD0..=0xD7 | 0xD8 => continue,
            // Start of frame (all but DHT 0xC4, JPG 0xC8, DAC 0xCC): precision, height, width.
            0xC0..=0xCF if !matches!(code, 0xC4 | 0xC8 | 0xCC) => {
                let h = u16::from_be_bytes([*jpeg.get(i + 3)?, *jpeg.get(i + 4)?]);
                let w = u16::from_be_bytes([*jpeg.get(i + 5)?, *jpeg.get(i + 6)?]);
                return Some((w.into(), h.into()));
            }
            // Start of scan or end of image before any frame header: not a picture.
            0xD9 | 0xDA => return None,
            _ => {
                let len = u16::from_be_bytes([*jpeg.get(i)?, *jpeg.get(i + 1)?]);
                i += usize::from(len);
            }
        }
    }
}

/// The running loop; `pull` hands out its newest frame.
pub struct Live {
    shared: Arc<Shared>,
    last_pull: Arc<Mutex<Instant>>,
    running: Arc<AtomicBool>,
}

impl Live {
    /// Grab the first frame now (so a missing tool or a window that is not shown fails the pull) and keep going.
    pub fn start(app: &AppHandle) -> Result<Self, String> {
        let rect = view_rect(app)?;
        let first = grim_frame(rect, &JPEG)?;
        let size = jpeg_size(&first).ok_or("grim did not return a JPEG picture")?;
        let shared = Arc::new(Shared::default());
        shared.publish(first, size);
        let live = Self {
            shared: shared.clone(),
            last_pull: Arc::new(Mutex::new(Instant::now())),
            running: Arc::new(AtomicBool::new(true)),
        };
        let (app, last_pull, running) = (app.clone(), live.last_pull.clone(), live.running.clone());
        std::thread::spawn(move || {
            run(&app, rect, &shared, &last_pull);
            running.store(false, Ordering::Relaxed);
        });
        Ok(live)
    }

    pub fn running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// A pull happened now: the loop stays alive another [`IDLE`].
    pub fn touch(&self) -> Arc<Shared> {
        *self.last_pull.lock() = Instant::now();
        self.shared.clone()
    }
}

/// Grab at ~30 fps until a viewer has been gone for [`IDLE`] or a grab fails (the next pull starts a fresh loop,
/// which fails its first grab in the pull, where the viewer sees the reason). A frame equal to the last is not
/// published: a still window sends nothing new, as on macOS.
fn run(app: &AppHandle, mut rect: Rect, shared: &Shared, last_pull: &Mutex<Instant>) {
    let mut rect_at = Instant::now();
    loop {
        let tick = Instant::now();
        if last_pull.lock().elapsed() >= IDLE {
            return;
        }
        if rect_at.elapsed() >= RECT_REFRESH {
            match view_rect(app) {
                Ok(r) => rect = r,
                Err(_) => return,
            }
            rect_at = Instant::now();
        }
        let Ok(frame) = grim_frame(rect, &JPEG) else {
            return;
        };
        if *shared.newest_jpeg() != frame
            && let Some(size) = jpeg_size(&frame)
        {
            shared.publish(frame, size);
        }
        if let Some(rest) = PERIOD.checked_sub(tick.elapsed()) {
            std::thread::sleep(rest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The start of a baseline JPEG as the format defines it: SOI, a JFIF APP0 segment, a DQT, then SOF0.
    fn header(w: u16, h: u16) -> Vec<u8> {
        let mut j = vec![0xFF, 0xD8];
        j.extend([0xFF, 0xE0, 0x00, 0x10]);
        j.extend(b"JFIF\0\x01\x01\x00\x00\x01\x00\x01\x00\x00");
        j.extend([0xFF, 0xDB, 0x00, 0x43, 0x00]);
        j.extend([8u8; 64]);
        j.extend([0xFF, 0xC0, 0x00, 0x11, 0x08]);
        j.extend(h.to_be_bytes());
        j.extend(w.to_be_bytes());
        j.extend([0x03, 0x01, 0x22, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01]);
        j
    }

    #[test]
    fn the_size_comes_from_the_frame_header_after_the_segments_before_it() {
        assert_eq!(jpeg_size(&header(1280, 820)), Some((1280, 820)));
        // A progressive picture (SOF2) is sized the same way.
        let mut p = header(640, 480);
        let at = p.iter().rposition(|&b| b == 0xC0).unwrap();
        p[at] = 0xC2;
        assert_eq!(jpeg_size(&p), Some((640, 480)));
    }

    #[test]
    fn what_is_not_a_jpeg_has_no_size() {
        assert_eq!(jpeg_size(b""), None);
        assert_eq!(jpeg_size(b"\x89PNG\r\n\x1a\n"), None);
        // Cut inside the segments, and a scan that starts before any frame header.
        let h = header(1, 1);
        assert_eq!(jpeg_size(&h[..30]), None);
        assert_eq!(jpeg_size(&[0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x02]), None);
    }
}
