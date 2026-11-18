//! Recording on Linux: `grim` stills of an agent instance's window, piped to `ffmpeg` (#282).
//!
//! The instance's private sway offers the wlroots screencopy protocol to its own clients only, and `grim` is its
//! client: on the pixman renderer a 1280x820 frame takes ~16 ms (measured: 30 grabs in 0.48 s), which is the 60 fps
//! the macOS recorder offers at that size, with no portal, no PipeWire and no second compositor tool to install (CI's
//! "Desktop opens (Linux)" image already has `grim`; `ffmpeg` is the one addition). Frames are PPM (no PNG
//! compression on the hot path) and stamped by arrival: ffmpeg turns the wall clock into a constant-rate H.264 file
//! (`fps` filter), so a slow grab repeats a frame instead of speeding the video up, and the stats report what was
//! really captured (`frames`, `max_gap_ms`, `avg_fps`), not what was asked.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::request;

/// What the capture thread saw: how many frames it took and when (offsets from the start call).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    pub frames: u64,
    pub first_ms: u64,
    pub last_ms: u64,
    pub max_gap_ms: u64,
}

impl Tally {
    /// A frame taken `at` after the start call.
    pub fn push(&mut self, at: Duration) {
        let ms = at.as_millis() as u64;
        if self.frames == 0 {
            self.first_ms = ms;
        } else {
            self.max_gap_ms = self.max_gap_ms.max(ms.saturating_sub(self.last_ms));
        }
        self.last_ms = ms;
        self.frames += 1;
    }

    pub fn duration_ms(&self) -> u64 {
        self.last_ms - self.first_ms
    }
}

/// `(width, height)` of a binary PPM (`P6\n<w> <h>\n255\n…`), the format `grim -t ppm` writes.
pub fn ppm_size(frame: &[u8]) -> Option<(u32, u32)> {
    let mut parts = frame.splitn(4, |b| b.is_ascii_whitespace());
    (parts.next()? == b"P6").then_some(())?;
    let num = |p: Option<&[u8]>| std::str::from_utf8(p?).ok()?.parse::<u32>().ok();
    Some((num(parts.next())?, num(parts.next())?)).filter(|&(w, h)| w > 0 && h > 0)
}

/// The ffmpeg command line: PPM frames on stdin stamped by the wall clock, out as constant-rate H.264 with even
/// sides (the encoder refuses odd ones; one pixel is cropped, never resampled).
pub fn ffmpeg_args(fps: u32, out: &Path) -> Vec<OsString> {
    let vf = format!("crop=trunc(iw/2)*2:trunc(ih/2)*2,fps={fps}");
    let mut a: Vec<OsString> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostats",
        "-y",
        "-use_wallclock_as_timestamps",
        "1",
        "-f",
        "image2pipe",
        "-c:v",
        "ppm",
        "-i",
        "-",
        "-vf",
    ]
    .map(OsString::from)
    .to_vec();
    a.push(vf.into());
    a.extend(
        [
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "18",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
        ]
        .map(OsString::from),
    );
    a.push(out.into());
    a
}

/// A recording in progress: the capture thread owns ffmpeg and the frame source.
pub struct Recorder {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Result<Tally, String>>,
    pub out: PathBuf,
    pub fps: u32,
    pub pixels: (u32, u32),
    pub started: Instant,
    pub started_epoch_ms: u128,
}

fn ffmpeg_error(e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound {
        "ffmpeg not found: recording needs ffmpeg with libx264 (Arch: `pacman -S ffmpeg`; Debian/Ubuntu: `apt install ffmpeg`)".into()
    } else {
        format!("ffmpeg: {e}")
    }
}

impl Recorder {
    /// Start encoding to `out`: `first` is the frame already grabbed (so a missing tool fails the start call, and
    /// the picture size is known), `grab` takes the next ones, `fps` is the constant rate of the file.
    pub fn start(
        first: Vec<u8>,
        mut grab: impl FnMut() -> Result<Vec<u8>, String> + Send + 'static,
        fps: u32,
        out: &Path,
    ) -> Result<Self, String> {
        let pixels = ppm_size(&first).ok_or("grim did not return a PPM picture")?;
        let mut child = Command::new("ffmpeg")
            .args(ffmpeg_args(fps, out))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ffmpeg_error(&e))?;
        let started = Instant::now();
        let started_epoch_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut tally = Tally::default();
            let stdin = child.stdin.take().expect("spawned with a piped stdin");
            // `feed` drops stdin on return, which is what ends the file; then ffmpeg's verdict is the recording's.
            let fed = feed(stdin, first, &mut grab, fps, started, &flag, &mut tally);
            let mut log = String::new();
            if let Some(mut e) = child.stderr.take() {
                let _ = e.read_to_string(&mut log);
            }
            let status = child.wait().map_err(|e| ffmpeg_error(&e))?;
            fed?;
            if !status.success() {
                return Err(format!("ffmpeg failed: {}", log.trim()));
            }
            Ok(tally)
        });
        Ok(Self {
            stop,
            thread,
            out: out.to_path_buf(),
            fps,
            pixels,
            started,
            started_epoch_ms,
        })
    }

    /// Finish the file: stop grabbing, let ffmpeg close it, and say what was captured.
    pub fn stop(self) -> Result<Tally, String> {
        self.stop.store(true, Ordering::Relaxed);
        self.thread
            .join()
            .map_err(|_| "the recording thread panicked".to_string())?
    }
}

/// Write `first`, then a new grab every `1/fps` until stopped (or [`request::MAX_RECORD_SECS`]), then one last; a late
/// grab is not followed by a burst to catch up.
fn feed(
    mut stdin: ChildStdin,
    first: Vec<u8>,
    grab: &mut impl FnMut() -> Result<Vec<u8>, String>,
    fps: u32,
    started: Instant,
    stop: &AtomicBool,
    tally: &mut Tally,
) -> Result<(), String> {
    let period = Duration::from_secs_f64(1.0 / f64::from(fps));
    let write = |stdin: &mut ChildStdin, frame: &[u8]| {
        stdin.write_all(frame).map_err(|e| {
            format!("ffmpeg stopped reading frames ({e}); see its message in the stop result")
        })
    };
    write(&mut stdin, &first)?;
    tally.push(started.elapsed());
    let mut next = Instant::now() + period;
    let limit = Duration::from_secs(request::MAX_RECORD_SECS);
    loop {
        // One more frame after the stop is seen: whatever ended the recording (`--until <selector>` showing) was
        // painted just before it, and the compositor draws it a beat later than the page reports it.
        let last = stop.load(Ordering::Relaxed) || started.elapsed() >= limit;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        }
        next = next.max(now) + period;
        write(&mut stdin, &grab()?)?;
        tally.push(started.elapsed());
        if last {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A solid-colour PPM of `w`x`h`.
    fn ppm(w: u32, h: u32, shade: u8) -> Vec<u8> {
        let mut f = format!("P6\n{w} {h}\n255\n").into_bytes();
        f.extend(std::iter::repeat_n(shade, (w * h * 3) as usize));
        f
    }

    #[test]
    fn grims_ppm_header_gives_the_picture_size() {
        assert_eq!(ppm_size(&ppm(1280, 820, 0)), Some((1280, 820)));
        // What grim really writes for a 2x output: same shape, bigger numbers.
        assert_eq!(ppm_size(b"P6\n2560 1640\n255\n\0\0\0"), Some((2560, 1640)));
        for bad in [
            &b""[..],
            b"P5\n2 2\n255\n",
            b"P6\n0 4\n255\n",
            b"\x89PNG\r\n",
            b"P6\nx y\n",
        ] {
            assert_eq!(ppm_size(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_tally_reports_what_was_captured_and_its_worst_gap() {
        let mut t = Tally::default();
        for ms in [40, 56, 72, 200, 216] {
            t.push(Duration::from_millis(ms));
        }
        assert_eq!(
            t,
            Tally {
                frames: 5,
                first_ms: 40,
                last_ms: 216,
                max_gap_ms: 128
            }
        );
        assert_eq!(t.duration_ms(), 176);
    }

    #[test]
    fn ffmpeg_gets_wallclock_ppm_frames_and_writes_constant_rate_even_sided_h264() {
        let a: Vec<String> = ffmpeg_args(30, Path::new("/tmp/x.mov"))
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        let has = |pair: [&str; 2]| a.windows(2).any(|w| w == pair);
        assert!(has(["-use_wallclock_as_timestamps", "1"]));
        assert!(has(["-vf", "crop=trunc(iw/2)*2:trunc(ih/2)*2,fps=30"]));
        assert!(has(["-c:v", "libx264"]) && has(["-pix_fmt", "yuv420p"]));
        assert_eq!(a.last().map(String::as_str), Some("/tmp/x.mov"));
    }

    /// Whether `tool` runs; CI installs ffmpeg (a missing one there fails the test), a bare dev box skips.
    fn tool(tool: &str) -> bool {
        let ok = Command::new(tool).arg("-version").output().is_ok();
        assert!(
            ok || std::env::var_os("CI").is_none(),
            "{tool} must be installed in CI"
        );
        ok
    }

    #[test]
    fn a_recording_is_a_playable_file_of_the_asked_rate_and_length_with_the_captured_frames_counted()
     {
        if !tool("ffmpeg") || !tool("ffprobe") {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("rec.mp4");
        // An odd-sided picture (grim gives whatever the window is): the encoder would refuse it uncropped.
        let n = std::sync::atomic::AtomicU8::new(0);
        let rec = Recorder::start(
            ppm(101, 61, 10),
            move || Ok(ppm(101, 61, n.fetch_add(40, Ordering::Relaxed))),
            30,
            &out,
        )
        .unwrap();
        assert_eq!(
            rec.pixels,
            (101, 61),
            "the start result tells the size grim gave"
        );
        std::thread::sleep(Duration::from_millis(1000));
        let t = rec.stop().unwrap();

        // The grab loop paces itself: ~1 s at 30/s is ~30 frames, never an unthrottled flood or a trickle.
        assert!((20..=40).contains(&t.frames), "frames: {t:?}");
        assert!(t.duration_ms() >= 800, "duration: {t:?}");
        let probe = Command::new("ffprobe")
            .args(["-v", "error", "-count_frames", "-select_streams", "v:0"])
            .args([
                "-show_entries",
                "stream=width,height,nb_read_frames,r_frame_rate,codec_name",
            ])
            .args(["-of", "default=nw=1"])
            .arg(&out)
            .output()
            .unwrap();
        let info = String::from_utf8_lossy(&probe.stdout);
        assert!(info.contains("codec_name=h264"), "{info}");
        assert!(
            info.contains("width=100") && info.contains("height=60"),
            "cropped to even sides: {info}"
        );
        assert!(info.contains("r_frame_rate=30/1"), "constant 30: {info}");
        let frames: u64 = info
            .lines()
            .find_map(|l| l.strip_prefix("nb_read_frames="))
            .and_then(|v| v.parse().ok())
            .unwrap();
        assert!((20..=45).contains(&frames), "encoded frames: {info}");
    }

    #[test]
    fn the_last_frame_is_grabbed_after_the_stop_so_what_ended_the_recording_is_in_it() {
        if !tool("ffmpeg") {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let grabs = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let count = grabs.clone();
        // 1 fps: the loop is asleep for a second after the start frame, so stopping now is a stop seen before any
        // periodic grab; the one grab that still happens is the final one.
        let rec = Recorder::start(
            ppm(64, 64, 0),
            move || {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(ppm(64, 64, 9))
            },
            1,
            &dir.path().join("x.mp4"),
        )
        .unwrap();
        let tally = rec.stop().unwrap();
        assert_eq!(grabs.load(Ordering::SeqCst), 1, "one grab after the stop");
        assert_eq!(tally.frames, 2, "the start frame and the final one");
    }

    #[test]
    fn a_frame_source_that_fails_ends_the_recording_with_its_error() {
        if !tool("ffmpeg") {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let rec = Recorder::start(
            ppm(64, 64, 0),
            || Err("the compositor has no window of this process".into()),
            30,
            &dir.path().join("x.mp4"),
        )
        .unwrap();
        let err = rec.stop().unwrap_err();
        assert!(err.contains("no window of this process"), "{err}");
    }
}
