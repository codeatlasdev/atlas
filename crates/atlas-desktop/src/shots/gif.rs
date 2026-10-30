//! GIF export: the presets, the size estimate, and an encoder with a real palette.
//!
//! A GIF is 256 colours per frame, so the work is choosing the palette and the frames. [`plan`] decides the
//! frame rate and the pixel size from a preset (pure, tested), [`estimate_bytes`] tells the person what it will
//! cost before they commit, and [`encode`] writes the file: one global palette trained on frames sampled across
//! the clip (a per-frame palette flickers; a palette from frame 0 alone posterises everything that appears later),
//! frame differencing so unchanged pixels cost nothing, and delays that sum to the real duration.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

use color_quant::NeuQuant;

/// The sentence of an encode the person stopped (the HUD and the card show it as the end of the export, not as a
/// failure).
pub const CANCELLED: &str = "GIF export cancelled.";

/// Set by [`request_cancel`] (Esc in the HUD or the card), checked before every frame of the running encode.
static CANCEL: AtomicBool = AtomicBool::new(false);
/// An encode is running: a cancel with none running must not poison the next one.
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Ask the running encode to stop after its current frame. `false` when none is running (nothing is remembered).
pub fn request_cancel() -> bool {
    if RUNNING.load(Ordering::SeqCst) {
        CANCEL.store(true, Ordering::SeqCst);
        true
    } else {
        false
    }
}

/// Marks an encode as running for its lifetime and clears the cancel flag on both ends, so the flag of one export
/// never stops the next.
pub struct Encoding;

impl Encoding {
    pub fn begin() -> Self {
        CANCEL.store(false, Ordering::SeqCst);
        RUNNING.store(true, Ordering::SeqCst);
        Encoding
    }

    /// The flag the frame loop checks.
    pub fn flag(&self) -> &'static AtomicBool {
        &CANCEL
    }
}

impl Drop for Encoding {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::SeqCst);
        CANCEL.store(false, Ordering::SeqCst);
    }
}

/// A preset the person picks in the export sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    /// Frames per second of the GIF (a source at 60 is decimated).
    pub fps: u32,
    /// Longest side in pixels; the clip is never scaled up.
    pub max_side: u32,
}

pub const PRESETS: [Preset; 3] = [
    Preset {
        id: "small",
        label: "Small (10 fps, 480 px)",
        fps: 10,
        max_side: 480,
    },
    Preset {
        id: "balanced",
        label: "Balanced (15 fps, 720 px)",
        fps: 15,
        max_side: 720,
    },
    Preset {
        id: "sharp",
        label: "Sharp (20 fps, 1080 px)",
        fps: 20,
        max_side: 1080,
    },
];

pub fn preset(id: &str) -> Option<Preset> {
    PRESETS.iter().copied().find(|p| p.id == id)
}

/// What a preset does to a clip of `src` pixels at `src_fps`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Keep every n-th source frame.
    pub every: u32,
}

/// The output size and rate for `preset` on a `src` (w, h) video at `src_fps`. Never scales up, never exceeds the
/// source rate, keeps the aspect ratio, and keeps both sides at least 1.
pub fn plan(src: (u32, u32), src_fps: u32, p: Preset) -> Plan {
    let longest = src.0.max(src.1).max(1);
    let scale = (f64::from(p.max_side) / f64::from(longest)).min(1.0);
    let width = ((f64::from(src.0) * scale).round() as u32).max(1);
    let height = ((f64::from(src.1) * scale).round() as u32).max(1);
    let src_fps = src_fps.max(1);
    let fps = p.fps.min(src_fps);
    // the nearest integer decimation: 60 → 15 is every 4th; 30 → 20 is every 2nd (=15), never faster than asked
    let every = (src_fps as f64 / f64::from(fps)).round().max(1.0) as u32;
    Plan {
        width,
        height,
        fps: (src_fps / every).max(1),
        every,
    }
}

/// A rough size of the encoded file, in bytes, for the "≈ 4.2 MB" shown before exporting. LZW on screen
/// content with differencing lands near 0.18 bytes per pixel per frame for UI recordings (measured on the clips of
/// `encode`'s test); it is an estimate and the sheet says "about".
pub fn estimate_bytes(plan: Plan, secs: f64) -> u64 {
    let frames = (secs * f64::from(plan.fps)).ceil().max(1.0);
    (frames * f64::from(plan.width) * f64::from(plan.height) * 0.18) as u64
}

/// GIF delays are in hundredths of a second, so 15 fps (6.67 cs) cannot be written exactly. The per-frame delays
/// alternate between the two nearest integers so that the clip's total duration is exact instead of drifting.
pub fn delays(frames: usize, fps: u32) -> Vec<u16> {
    let per = 100.0 / f64::from(fps.max(1));
    let mut out = Vec::with_capacity(frames);
    let mut carried = 0.0;
    for _ in 0..frames {
        let want = per + carried;
        let d = want.round().max(2.0); // browsers clamp delays under 2 cs to 10: never write them
        carried = want - d;
        out.push(d as u16);
    }
    out
}

/// An RGBA frame, `width * height * 4` bytes.
pub struct Frame {
    pub rgba: Vec<u8>,
}

/// Nearest-neighbour-free box downscale of an RGBA frame to `(w, h)`. Averaging (not picking) keeps text legible.
pub fn downscale(rgba: &[u8], src: (u32, u32), dst: (u32, u32)) -> Vec<u8> {
    if src == dst {
        return rgba.to_vec();
    }
    let (sw, sh, dw, dh) = (
        src.0 as usize,
        src.1 as usize,
        dst.0 as usize,
        dst.1 as usize,
    );
    let mut out = vec![0u8; dw * dh * 4];
    for dy in 0..dh {
        let y0 = dy * sh / dh;
        let y1 = ((dy + 1) * sh / dh).max(y0 + 1).min(sh);
        for dx in 0..dw {
            let x0 = dx * sw / dw;
            let x1 = ((dx + 1) * sw / dw).max(x0 + 1).min(sw);
            let mut acc = [0u32; 4];
            for y in y0..y1 {
                let row = &rgba[(y * sw + x0) * 4..(y * sw + x1) * 4];
                for px in row.as_chunks::<4>().0 {
                    acc[0] += u32::from(px[0]);
                    acc[1] += u32::from(px[1]);
                    acc[2] += u32::from(px[2]);
                    acc[3] += u32::from(px[3]);
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            let o = (dy * dw + dx) * 4;
            for (c, sum) in acc.iter().enumerate() {
                out[o + c] = (sum / n) as u8;
            }
        }
    }
    out
}

/// `k` frame indices spread evenly over `n` frames, always including the first and the last (a colour that only
/// appears at the end of the clip must reach the palette). Fewer than `k` frames: all of them.
pub fn sample_indices(n: usize, k: usize) -> Vec<usize> {
    if n <= k {
        return (0..n).collect();
    }
    if k <= 1 {
        return vec![0];
    }
    (0..k).map(|i| i * (n - 1) / (k - 1)).collect()
}

/// A multiplicative hasher for the packed colour keys of the exact palette: the clip's pixels are looked up millions
/// of times, and SipHash (std's default, built against hash flooding) is several times slower for a `u32` that no
/// one controls adversarially.
#[derive(Default, Clone, Copy)]
struct ColourHash(u64);

impl std::hash::Hasher for ColourHash {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 ^ u64::from(*b)).wrapping_mul(0x100000001b3);
        }
    }
    fn write_u32(&mut self, v: u32) {
        self.0 = u64::from(v).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
}

type ColourMap = std::collections::HashMap<u32, u8, std::hash::BuildHasherDefault<ColourHash>>;

/// An opaque pixel's colour as one key.
#[inline]
fn key(p: &[u8]) -> u32 {
    u32::from(p[0]) << 16 | u32::from(p[1]) << 8 | u32::from(p[2])
}

/// Palette index of a colour, remembered: a direct-mapped table keyed by the packed colour. It stores exactly what
/// [`Palette::index`] answered, so using it never changes the output; it only skips repeating the search.
struct IndexCache {
    /// `(key + 1, index)`; key 0 (black) is stored as 1, so an empty slot (0) is never a hit.
    slots: Vec<(u32, u8)>,
}

impl IndexCache {
    const BITS: u32 = 16;

    fn new() -> Self {
        Self {
            slots: vec![(0, 0); 1 << Self::BITS],
        }
    }

    #[inline]
    fn index(&mut self, palette: &Palette, k: u32, p: &[u8]) -> u8 {
        // Fibonacci hashing spreads the packed colour over the slots
        let slot = (k.wrapping_mul(0x9E37_79B1) >> (32 - Self::BITS)) as usize;
        let e = &mut self.slots[slot];
        if e.0 == k + 1 {
            return e.1;
        }
        let i = palette.index(p);
        *e = (k + 1, i);
        i
    }
}

/// The 256-entry palette of a clip. Index 255 is never a colour: it is the transparent "unchanged" index.
enum Palette {
    /// The clip has at most 255 distinct colours (a UI recording usually does): they are the palette, so every
    /// pixel round-trips exactly and no quantiser can blend two colours into a third.
    Exact(ColourMap, Vec<u8>),
    /// More colours than fit: NeuQuant trained on frames spread over the clip, last frame included.
    Trained(Box<NeuQuant>),
}

impl Palette {
    fn for_clip(frames: &[Frame]) -> Self {
        let mut seen = ColourMap::default();
        let mut rgb: Vec<u8> = Vec::new();
        // One pass: the 256th distinct colour means the clip does not fit an exact palette (what a second
        // "is every pixel known" pass used to find out by hashing every pixel again).
        let mut last: Option<u32> = None;
        let mut fits = true;
        'scan: for f in frames {
            for p in f.rgba.as_chunks::<4>().0 {
                let k = key(p);
                // a run of one colour (flat UI) costs a compare, not a lookup
                if last == Some(k) {
                    continue;
                }
                last = Some(k);
                if !seen.contains_key(&k) {
                    if seen.len() == 255 {
                        fits = false;
                        break 'scan;
                    }
                    seen.insert(k, seen.len() as u8);
                    rgb.extend_from_slice(&[p[0], p[1], p[2]]);
                }
            }
        }
        if fits {
            rgb.resize(256 * 3, 0);
            return Palette::Exact(seen, rgb);
        }
        const SAMPLES: usize = 8;
        let mut train = Vec::new();
        for f in sample_indices(frames.len(), SAMPLES)
            .into_iter()
            .map(|i| &frames[i])
        {
            // every 3rd pixel: palette quality barely changes, training time drops 3x
            for p in f.rgba.as_chunks::<4>().0.iter().step_by(3) {
                train.extend_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
        Palette::Trained(Box::new(NeuQuant::new(10, 255, &train)))
    }

    fn rgb(&self) -> Vec<u8> {
        match self {
            Palette::Exact(_, rgb) => rgb.clone(),
            Palette::Trained(nq) => {
                let mut p = nq.color_map_rgb();
                p.resize(256 * 3, 0);
                p
            }
        }
    }

    fn index(&self, p: &[u8]) -> u8 {
        match self {
            Palette::Exact(map, _) => map[&key(p)],
            Palette::Trained(nq) => nq.index_of(&[p[0], p[1], p[2], 255]) as u8,
        }
    }
}

/// Encode `frames` (all `size` pixels, RGBA) as an endlessly looping GIF with `delays` per frame.
///
/// * Palette: trained once on up to `SAMPLES` frames spread over the clip, at a sampling step that keeps training
///   fast on a 1080p clip.
/// * Differencing: a frame is stored as the bounding box of the pixels that changed from the previous one, and
///   unchanged pixels inside it are the transparent index, so a still screen with a blinking cursor costs bytes,
///   not megabytes.
pub fn encode<W: Write>(
    out: W,
    size: (u32, u32),
    frames: &[Frame],
    delays: &[u16],
) -> Result<(), String> {
    let (w, h) = (size.0 as u16, size.1 as u16);
    if frames.is_empty() {
        return Err("there are no frames to encode".into());
    }
    if frames.len() != delays.len() {
        return Err("every frame needs a delay".into());
    }
    let px = size.0 as usize * size.1 as usize;
    if frames.iter().any(|f| f.rgba.len() != px * 4) {
        return Err("a frame does not match the clip size".into());
    }

    let palette = Palette::for_clip(frames);

    let mut enc = gif::Encoder::new(out, w, h, &palette.rgb()).map_err(|e| e.to_string())?;
    enc.set_repeat(gif::Repeat::Infinite)
        .map_err(|e| e.to_string())?;

    let width = size.0 as usize;
    let mut cache = IndexCache::new();
    // Two index planes swapped each frame and one output buffer reused: no per-frame allocation of a full frame.
    let mut cur = vec![0u8; px];
    let mut prev = vec![0u8; px];
    let mut buf: Vec<u8> = Vec::new();
    for (n, (f, delay)) in frames.iter().zip(delays).enumerate() {
        let first = n == 0;
        // index the frame: a run of one colour reuses the last answer, and a colour seen before in the clip (a
        // quantised clip repeats a few thousand colours millions of times) is one cache probe, not a palette search
        let mut memo: Option<(u32, u8)> = None;
        for (d, p) in cur.iter_mut().zip(f.rgba.as_chunks::<4>().0) {
            let k = key(p);
            *d = match memo {
                Some((mk, mi)) if mk == k => mi,
                _ => {
                    let i = cache.index(&palette, k, p);
                    memo = Some((k, i));
                    i
                }
            };
        }
        let (bx, by, bw, bh) = if first {
            (0, 0, width, size.1 as usize)
        } else {
            // identical to the previous frame: a 1×1 transparent pixel keeps the delay without any data
            diff_box(&prev, &cur, width, size.1 as usize).unwrap_or((0, 0, 1, 1))
        };
        buf.clear();
        buf.reserve(bw * bh);
        for y in by..by + bh {
            let row = y * width;
            for x in bx..bx + bw {
                let i = row + x;
                buf.push(if !first && prev[i] == cur[i] {
                    255
                } else {
                    cur[i]
                });
            }
        }
        let frame = gif::Frame {
            width: bw as u16,
            height: bh as u16,
            left: bx as u16,
            top: by as u16,
            delay: *delay,
            dispose: gif::DisposalMethod::Keep,
            transparent: (!first).then_some(255),
            buffer: std::borrow::Cow::Borrowed(&buf),
            ..gif::Frame::default()
        };
        enc.write_frame(&frame).map_err(|e| e.to_string())?;
        std::mem::swap(&mut cur, &mut prev);
    }
    Ok(())
}

/// The bounding box (x, y, w, h) of the indices that differ, `None` when the frames are identical.
fn diff_box(a: &[u8], b: &[u8], w: usize, h: usize) -> Option<(usize, usize, usize, usize)> {
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
    for y in 0..h {
        let (ra, rb) = (&a[y * w..(y + 1) * w], &b[y * w..(y + 1) * w]);
        // an unchanged row (most of a UI frame) is one slice compare
        if ra == rb {
            continue;
        }
        let first = ra.iter().zip(rb).position(|(p, q)| p != q).unwrap_or(0);
        let last = ra.iter().zip(rb).rposition(|(p, q)| p != q).unwrap_or(0);
        x0 = x0.min(first);
        x1 = x1.max(last);
        y0 = y0.min(y);
        y1 = y1.max(y);
    }
    (x1 >= x0 && y1 >= y0 && x0 < w).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

/// Why a frame loop ended early.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stopped {
    Cancelled,
    Failed(String),
}

impl Stopped {
    /// The sentence for the person.
    pub fn into_message(self) -> String {
        match self {
            Stopped::Cancelled => CANCELLED.to_string(),
            Stopped::Failed(e) => e,
        }
    }
}

/// The frame loop of an export: `grab(t)` for each of `times`, `progress(done, total)` after each, and `cancel`
/// checked before every frame (so Esc stops within one frame, not at the end). Pure over its closures: the
/// ScreenCaptureKit/AVFoundation reader is `grab`.
pub fn collect_frames(
    times: &[f64],
    cancel: &AtomicBool,
    mut grab: impl FnMut(f64) -> Result<Frame, String>,
    mut progress: impl FnMut(usize, usize),
) -> Result<Vec<Frame>, Stopped> {
    let mut frames = Vec::with_capacity(times.len());
    for (i, t) in times.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            return Err(Stopped::Cancelled);
        }
        frames.push(grab(*t).map_err(Stopped::Failed)?);
        progress(i + 1, times.len());
    }
    if cancel.load(Ordering::SeqCst) {
        return Err(Stopped::Cancelled);
    }
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Frame {
        Frame { rgba: vec![0; 4] }
    }

    #[test]
    fn the_frame_loop_reads_every_time_in_order_and_reports_progress() {
        let cancel = AtomicBool::new(false);
        let mut seen = Vec::new();
        let mut prog = Vec::new();
        let out = collect_frames(
            &[0.0, 0.5, 1.0],
            &cancel,
            |t| {
                seen.push(t);
                Ok(frame())
            },
            |d, n| prog.push((d, n)),
        )
        .unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(seen, [0.0, 0.5, 1.0]);
        assert_eq!(prog, [(1, 3), (2, 3), (3, 3)]);
    }

    /// Esc must stop the encode within one frame: with the flag set after the second frame no third frame is read.
    #[test]
    fn cancelling_stops_before_the_next_frame_is_read() {
        let cancel = AtomicBool::new(false);
        let mut reads = 0;
        let r = collect_frames(
            &[0.0, 0.1, 0.2, 0.3, 0.4],
            &cancel,
            |_| {
                reads += 1;
                if reads == 2 {
                    cancel.store(true, Ordering::SeqCst);
                }
                Ok(frame())
            },
            |_, _| {},
        );
        assert_eq!(r.err(), Some(Stopped::Cancelled));
        assert_eq!(reads, 2, "no frame is read after the cancel");
    }

    #[test]
    fn a_flag_set_before_the_first_frame_reads_nothing() {
        let cancel = AtomicBool::new(true);
        let mut reads = 0;
        let r = collect_frames(
            &[0.0],
            &cancel,
            |_| {
                reads += 1;
                Ok(frame())
            },
            |_, _| {},
        );
        assert_eq!(r.err(), Some(Stopped::Cancelled));
        assert_eq!(reads, 0);
    }

    /// A cancel that lands during the very last frame still cancels: the file must not be written afterwards.
    #[test]
    fn a_cancel_during_the_last_frame_still_cancels() {
        let cancel = AtomicBool::new(false);
        let r = collect_frames(
            &[0.0, 0.1],
            &cancel,
            |t| {
                if t > 0.05 {
                    cancel.store(true, Ordering::SeqCst);
                }
                Ok(frame())
            },
            |_, _| {},
        );
        assert_eq!(r.err(), Some(Stopped::Cancelled));
    }

    #[test]
    fn a_failing_frame_ends_the_loop_with_its_sentence() {
        let cancel = AtomicBool::new(false);
        let r = collect_frames(
            &[0.0, 0.1, 0.2],
            &cancel,
            |t| {
                if t > 0.05 {
                    Err("no frame there".into())
                } else {
                    Ok(frame())
                }
            },
            |_, _| {},
        );
        assert_eq!(r.err(), Some(Stopped::Failed("no frame there".into())));
        assert_eq!(Stopped::Cancelled.into_message(), CANCELLED);
    }

    /// The flag of one export never stops the next, and a cancel with nothing running is dropped, not remembered.
    /// (One test: the flag is process-wide, so parallel tests of it would race.)
    #[test]
    fn the_process_flag_is_cleared_between_exports_and_ignored_when_idle() {
        assert!(!request_cancel(), "nothing running");
        {
            let e = Encoding::begin();
            assert!(!e.flag().load(Ordering::SeqCst));
            assert!(request_cancel());
            assert!(e.flag().load(Ordering::SeqCst));
        }
        assert!(!request_cancel(), "the encode ended");
        let e = Encoding::begin();
        assert!(
            !e.flag().load(Ordering::SeqCst),
            "a fresh encode starts uncancelled"
        );
    }

    #[test]
    fn a_preset_never_scales_up_and_keeps_the_aspect_ratio() {
        let p = preset("balanced").unwrap();
        assert_eq!(plan((1440, 900), 60, p).width, 720);
        assert_eq!(plan((1440, 900), 60, p).height, 450);
        let small = plan((400, 300), 60, p);
        assert_eq!(
            (small.width, small.height),
            (400, 300),
            "a small region is kept as is"
        );
        let tall = plan((300, 1200), 30, p);
        assert_eq!(
            (tall.width, tall.height),
            (180, 720),
            "the longest side is the one capped"
        );
    }

    #[test]
    fn the_gif_rate_never_exceeds_the_source_and_decimates_by_whole_frames() {
        let b = preset("balanced").unwrap();
        let p = plan((800, 600), 60, b);
        assert_eq!((p.fps, p.every), (15, 4), "60 → 15 keeps every 4th frame");
        let p = plan((800, 600), 30, b);
        assert_eq!((p.fps, p.every), (15, 2));
        let sharp = preset("sharp").unwrap();
        let p = plan((800, 600), 24, sharp);
        assert!(p.fps <= 24, "{p:?}");
        let p = plan((800, 600), 10, sharp);
        assert_eq!((p.fps, p.every), (10, 1), "a 10 fps source stays at 10");
    }

    #[test]
    fn degenerate_sizes_still_plan() {
        let p = plan((0, 0), 0, preset("small").unwrap());
        assert!(p.width >= 1 && p.height >= 1 && p.fps >= 1 && p.every >= 1);
    }

    #[test]
    fn the_estimate_grows_with_time_and_with_size() {
        let b = plan((1440, 900), 60, preset("balanced").unwrap());
        let s = plan((1440, 900), 60, preset("small").unwrap());
        assert!(estimate_bytes(b, 10.0) > estimate_bytes(b, 5.0));
        assert!(estimate_bytes(b, 5.0) > estimate_bytes(s, 5.0));
        assert!(estimate_bytes(s, 0.0) > 0, "even an instant has a frame");
    }

    /// 15 fps is 6.67 cs per frame; writing 7 cs each would make a 10 s clip 10.5 s long. The delays must add up.
    #[test]
    fn delays_sum_to_the_real_duration_instead_of_drifting() {
        let d = delays(150, 15);
        assert_eq!(d.iter().map(|d| u32::from(*d)).sum::<u32>(), 1000);
        assert!(d.iter().all(|d| *d == 6 || *d == 7), "{d:?}");
        let d = delays(100, 10);
        assert!(d.iter().all(|d| *d == 10));
        // browsers show delays under 2 cs as 10 cs: a 100 fps request must not write 1
        assert!(delays(10, 100).iter().all(|d| *d >= 2));
    }

    #[test]
    fn a_box_downscale_averages_instead_of_picking() {
        // 2×2 black/white checker → 1×1 grey
        let src = [
            0, 0, 0, 255, 255, 255, 255, 255, //
            255, 255, 255, 255, 0, 0, 0, 255,
        ];
        let out = downscale(&src, (2, 2), (1, 1));
        assert_eq!(out, [127, 127, 127, 255]);
        assert_eq!(downscale(&src, (2, 2), (2, 2)), src);
    }

    /// The box filter against the plain definition (the mean of every source pixel whose cell maps to the output
    /// pixel, per channel, integer division), on ratios that do not divide evenly, a one-pixel output and a stretch.
    #[test]
    fn the_box_downscale_matches_the_plain_mean_of_each_cell() {
        fn reference(rgba: &[u8], src: (u32, u32), dst: (u32, u32)) -> Vec<u8> {
            let (sw, sh, dw, dh) = (
                src.0 as usize,
                src.1 as usize,
                dst.0 as usize,
                dst.1 as usize,
            );
            let mut out = Vec::new();
            for dy in 0..dh {
                let y0 = dy * sh / dh;
                let y1 = ((dy + 1) * sh / dh).max(y0 + 1).min(sh);
                for dx in 0..dw {
                    let x0 = dx * sw / dw;
                    let x1 = ((dx + 1) * sw / dw).max(x0 + 1).min(sw);
                    for c in 0..4 {
                        let mut sum = 0u32;
                        for y in y0..y1 {
                            for x in x0..x1 {
                                sum += u32::from(rgba[(y * sw + x) * 4 + c]);
                            }
                        }
                        out.push((sum / ((y1 - y0) * (x1 - x0)) as u32) as u8);
                    }
                }
            }
            out
        }
        for (src, dst) in [
            ((7, 5), (3, 2)),
            ((1200, 800), (440, 293)),
            ((13, 13), (1, 1)),
            ((3, 3), (5, 4)),
            ((64, 1), (10, 1)),
        ] {
            let px: Vec<u8> = (0..src.0 * src.1 * 4)
                .map(|i: u32| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
                .collect();
            assert_eq!(
                downscale(&px, src, dst),
                reference(&px, src, dst),
                "{src:?} -> {dst:?}"
            );
        }
    }

    fn solid(w: u32, h: u32, rgb: [u8; 3]) -> Frame {
        Frame {
            rgba: (0..w * h)
                .flat_map(|_| [rgb[0], rgb[1], rgb[2], 255])
                .collect(),
        }
    }

    fn decode(bytes: &[u8]) -> (gif::Decoder<&[u8]>, Vec<gif::Frame<'static>>) {
        let mut opts = gif::DecodeOptions::new();
        opts.set_color_output(gif::ColorOutput::Indexed);
        let mut d = opts.read_info(bytes).unwrap();
        let mut frames = Vec::new();
        while let Some(f) = d.read_next_frame().unwrap() {
            frames.push(f.clone());
        }
        (d, frames)
    }

    /// The bytes of an encode are pinned: speeding the encoder up must not change a single output byte (palette
    /// order, frame boxes, transparency and delays all show in the hash). Clips: a flat two-panel UI with a moving
    /// block (exact palette) and a gradient with noise (NeuQuant).
    #[test]
    fn the_encoded_bytes_do_not_change_when_the_encoder_gets_faster() {
        fn clip(n: usize, colours: bool) -> Vec<Frame> {
            let (w, h) = (160u32, 100u32);
            let mut seed = 7u32;
            (0..n)
                .map(|f| {
                    let mut rgba = vec![0u8; (w * h * 4) as usize];
                    for y in 0..h {
                        for x in 0..w {
                            let i = ((y * w + x) * 4) as usize;
                            let moving = x / 40 == (f as u32 % 17) && y / 40 == 3;
                            let (r, g, b) = if moving {
                                (250, 80, 40)
                            } else if colours {
                                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                                (
                                    (x * 255 / w) as u8,
                                    (y * 255 / h) as u8,
                                    90 + ((seed >> 28) as u8),
                                )
                            } else if (x / 120 + y / 120) % 2 == 0 {
                                (240, 240, 240)
                            } else {
                                (30, 30, 34)
                            };
                            rgba[i..i + 4].copy_from_slice(&[r, g, b, 255]);
                        }
                    }
                    Frame { rgba }
                })
                .collect()
        }
        fn fnv(bytes: &[u8]) -> u64 {
            bytes.iter().fold(0xcbf29ce484222325, |h, b| {
                (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
            })
        }
        for (colours, want_hash, want_len) in [
            (false, 0x76d5ee250007321f_u64, 1427),
            (true, 0xf6e4f67f910f9f6a_u64, 32905),
        ] {
            let mut out = Vec::new();
            encode(&mut out, (160, 100), &clip(12, colours), &delays(12, 15)).unwrap();
            assert_eq!(
                (fnv(&out), out.len()),
                (want_hash, want_len),
                "colours={colours}"
            );
        }
    }

    #[test]
    fn the_encoder_writes_every_frame_with_its_delay_and_loops_forever() {
        let frames = vec![solid(16, 16, [255, 0, 0]), solid(16, 16, [0, 0, 255])];
        let mut out = Vec::new();
        encode(&mut out, (16, 16), &frames, &[7, 6]).unwrap();
        assert_eq!(&out[..6], b"GIF89a");
        let (d, fs) = decode(&out);
        assert_eq!((d.width(), d.height()), (16, 16));
        assert_eq!(fs.len(), 2);
        assert_eq!((fs[0].delay, fs[1].delay), (7, 6));
    }

    /// A still screen costs almost nothing: the second identical frame is a single transparent pixel that only
    /// carries the delay. This is what makes a 20 s recording of a terminal a few hundred KB.
    #[test]
    fn identical_frames_cost_one_pixel_and_only_the_changed_box_is_stored() {
        let a = solid(64, 64, [10, 200, 10]);
        let same = solid(64, 64, [10, 200, 10]);
        let mut changed = solid(64, 64, [10, 200, 10]);
        for y in 20..24 {
            for x in 30..38 {
                let i = (y * 64 + x) * 4;
                changed.rgba[i..i + 3].copy_from_slice(&[250, 250, 250]);
            }
        }
        let mut out = Vec::new();
        encode(&mut out, (64, 64), &[a, same, changed], &[5, 5, 5]).unwrap();
        let (_, fs) = decode(&out);
        assert_eq!((fs[1].width, fs[1].height), (1, 1), "unchanged frame");
        assert_eq!(
            (fs[2].left, fs[2].top, fs[2].width, fs[2].height),
            (30, 20, 8, 4)
        );
        assert_eq!(fs[2].transparent, Some(255));
    }

    /// Incident: a clip of two colours (dark UI, one red flash) decoded its late frame as grey (100,100,100):
    /// NeuQuant on a tiny, skewed training set returned blended entries. A clip with at most 255 distinct colours
    /// needs no quantiser at all: its palette is those colours, and every pixel round-trips exactly.
    #[test]
    fn a_clip_with_few_colours_round_trips_exactly() {
        let colours = [
            [20u8, 20, 20],
            [240, 30, 30],
            [30, 200, 90],
            [255, 255, 255],
        ];
        let frames: Vec<Frame> = colours.iter().map(|c| solid(8, 8, *c)).collect();
        let mut out = Vec::new();
        encode(&mut out, (8, 8), &frames, &[5; 4]).unwrap();
        let mut opts = gif::DecodeOptions::new();
        opts.set_color_output(gif::ColorOutput::RGBA);
        let mut d = opts.read_info(&out[..]).unwrap();
        let mut canvas = vec![0u8; 8 * 8 * 4];
        let mut n = 0;
        while let Some(f) = d.read_next_frame().unwrap() {
            // composite onto the canvas the way a viewer does: transparent pixels keep what was there
            let (l, t, w, h) = (
                usize::from(f.left),
                usize::from(f.top),
                usize::from(f.width),
                usize::from(f.height),
            );
            for y in 0..h {
                for x in 0..w {
                    let s = (y * w + x) * 4;
                    if f.buffer[s + 3] == 255 {
                        let o = ((t + y) * 8 + l + x) * 4;
                        canvas[o..o + 4].copy_from_slice(&f.buffer[s..s + 4]);
                    }
                }
            }
            let c = colours[n];
            assert_eq!(&canvas[..3], &c, "frame {n} is not its exact colour");
            n += 1;
        }
        assert_eq!(n, 4);
    }

    /// The palette is trained on the whole clip: a colour that only appears in the last frame must still be
    /// represented, not collapsed to whatever frame 0 had.
    #[test]
    fn a_colour_that_only_appears_late_survives_the_palette() {
        let mut frames: Vec<Frame> = (0..16).map(|_| solid(8, 8, [20, 20, 20])).collect();
        frames.push(solid(8, 8, [240, 30, 30]));
        let mut out = Vec::new();
        encode(&mut out, (8, 8), &frames, &[5; 17]).unwrap();
        let mut opts = gif::DecodeOptions::new();
        opts.set_color_output(gif::ColorOutput::RGBA);
        let mut d = opts.read_info(&out[..]).unwrap();
        let mut last = None;
        while let Some(f) = d.read_next_frame().unwrap() {
            last = Some(f.buffer.to_vec());
        }
        let px = last.unwrap();
        // the last frame is a transparent-diff of the previous; the changed pixels must be red-ish, not dark grey
        let red = px
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] == 255 && p[0] > 150 && p[1] < 90)
            .count();
        assert!(
            red > 0,
            "the late colour was lost in the palette: {:?}",
            &px[..16.min(px.len())]
        );
    }

    /// Incident for this fix: `step_by(n / 8).take(8)` over 17 frames stopped at frame 14 and never reached the
    /// last one, so a colour that appears only at the end of a clip was quantised away.
    #[test]
    fn palette_samples_always_include_the_first_and_the_last_frame() {
        let s = sample_indices(17, 8);
        assert_eq!((s.first(), s.last()), (Some(&0), Some(&16)));
        assert_eq!(s.len(), 8);
        assert!(
            s.windows(2).all(|w| w[0] < w[1]),
            "strictly increasing: {s:?}"
        );
        for n in [1, 2, 7, 8, 9, 100, 1801] {
            let s = sample_indices(n, 8);
            assert_eq!(s.last(), Some(&(n - 1)), "n={n}");
            assert!(s.iter().all(|i| *i < n), "n={n}");
        }
        assert!(sample_indices(0, 8).is_empty());
        assert_eq!(sample_indices(5, 8), [0, 1, 2, 3, 4]);
        assert_eq!(sample_indices(10, 1), [0]);
    }

    /// More than 255 colours (a photo, a gradient) takes the quantiser: it must stay close to the source and keep
    /// a colour that appears only in the last frame (the sampling covers the whole clip).
    #[test]
    fn a_clip_with_many_colours_is_quantised_close_to_the_source_including_a_late_colour() {
        let (w, h) = (32u32, 32u32);
        let gradient = |shift: u32| Frame {
            rgba: (0..w * h)
                .flat_map(|i| {
                    let (x, y) = (i % w, i / w);
                    [
                        ((x * 8 + shift) % 256) as u8,
                        (y * 8) as u8,
                        ((x + y) * 4) as u8,
                        255,
                    ]
                })
                .collect(),
        };
        let mut frames: Vec<Frame> = (0..20).map(|_| gradient(0)).collect();
        frames.push(solid(w, h, [250, 10, 10]));
        assert!(matches!(Palette::for_clip(&frames), Palette::Trained(_)));
        let mut out = Vec::new();
        encode(&mut out, (w, h), &frames, &[5; 21]).unwrap();
        let mut opts = gif::DecodeOptions::new();
        opts.set_color_output(gif::ColorOutput::RGBA);
        let mut d = opts.read_info(&out[..]).unwrap();
        let mut canvas = vec![0u8; (w * h * 4) as usize];
        let mut n = 0;
        while let Some(f) = d.read_next_frame().unwrap() {
            let (l, t, fw, fh) = (
                usize::from(f.left),
                usize::from(f.top),
                usize::from(f.width),
                usize::from(f.height),
            );
            for y in 0..fh {
                for x in 0..fw {
                    let s = (y * fw + x) * 4;
                    if f.buffer[s + 3] == 255 {
                        let o = ((t + y) * w as usize + l + x) * 4;
                        canvas[o..o + 4].copy_from_slice(&f.buffer[s..s + 4]);
                    }
                }
            }
            n += 1;
        }
        assert_eq!(n, 21);
        let p = &canvas[..3];
        let dist = (i32::from(p[0]) - 250).abs()
            + (i32::from(p[1]) - 10).abs()
            + (i32::from(p[2]) - 10).abs();
        assert!(dist < 60, "the late red came out as {p:?}");
    }

    #[test]
    fn bad_input_is_refused_not_encoded() {
        let mut out = Vec::new();
        assert!(encode(&mut out, (4, 4), &[], &[]).is_err());
        assert!(encode(&mut out, (4, 4), &[solid(4, 4, [0; 3])], &[]).is_err());
        assert!(encode(&mut out, (4, 4), &[solid(2, 2, [0; 3])], &[5]).is_err());
    }
}
