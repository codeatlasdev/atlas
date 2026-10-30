//! The pure decisions of a recording's options, with no screen or AppKit in them so they are tested everywhere:
//! what a quality preset does to the picture, the size estimate shown while recording, which part a trim keeps, and
//! what this Mac can record besides the picture.

use serde::Serialize;

use super::prefs::VideoQuality;

/// What a quality preset does to the recording. ScreenCaptureKit's `SCRecordingOutput` takes no bitrate (the system
/// encoder picks it), so the preset acts on what *does* decide the size: the number of pixels and the frame rate.
/// `bitrate` (bits per second) is the model of what the encoder then spends, used only for the size estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate: u64,
}

/// Longest side (pixels) a preset keeps; `High` records at the display's own pixels.
const MEDIUM_MAX_SIDE: u32 = 1920;
const LOW_MAX_SIDE: u32 = 1280;

/// The recording config for `quality` on a region of `pixels` (w, h) with `fps` as the frame-rate cap.
///
/// Never scales up, keeps the aspect ratio, both sides stay even (video encoders reject odd sizes) and at least 2.
/// Low also drops to 24 fps. The bitrate is `width * height * fps * bits-per-pixel`, with bits per pixel per frame
/// of screen content in HEVC of 0.10 high, 0.07 medium, 0.05 low: a model, not a measurement of every clip.
pub fn video_config(quality: VideoQuality, pixels: (u32, u32), fps: u32) -> VideoConfig {
    let (max_side, fps, bpp) = match quality {
        VideoQuality::High => (u32::MAX, fps, 0.10),
        VideoQuality::Medium => (MEDIUM_MAX_SIDE, fps, 0.07),
        VideoQuality::Low => (LOW_MAX_SIDE, fps.min(24), 0.05),
    };
    let (w, h) = (pixels.0.max(2), pixels.1.max(2));
    let long = w.max(h);
    let (w, h) = if long > max_side {
        let k = f64::from(max_side) / f64::from(long);
        let side = |v: u32| (((f64::from(v) * k).round() as u32) / 2 * 2).max(2);
        (side(w), side(h))
    } else {
        ((w / 2 * 2).max(2), (h / 2 * 2).max(2))
    };
    VideoConfig {
        width: w,
        height: h,
        fps,
        bitrate: (f64::from(w) * f64::from(h) * f64::from(fps) * bpp).round() as u64,
    }
}

/// About how many bytes `secs` of recording at `bitrate` (bits per second) take: `bitrate * secs / 8`.
pub fn estimate_bytes(bitrate: u64, secs: f64) -> u64 {
    if !secs.is_finite() || secs <= 0.0 {
        return 0;
    }
    (bitrate as f64 * secs / 8.0).round() as u64
}

/// A trim never leaves less than this.
pub const MIN_TRIM_SECS: f64 = 0.5;
/// A handle this close to either end of the clip sits on it (a drag that stops a hair short keeps the whole edge).
const TRIM_SNAP_SECS: f64 = 0.15;

/// The range to keep of a clip of `duration` seconds when the handles were dragged to `start` and `end`.
///
/// Both are clamped into `0..=duration` and snap to the bounds when within [`TRIM_SNAP_SECS`]; handles that crossed
/// are put in order; a kept part under [`MIN_TRIM_SECS`] is grown around its middle and slid back inside the clip.
/// `None` when the clip itself is shorter than the minimum or an input is not a number: nothing sensible to trim to.
pub fn clamp_trim(duration: f64, start: f64, end: f64) -> Option<(f64, f64)> {
    if !duration.is_finite() || !start.is_finite() || !end.is_finite() || duration < MIN_TRIM_SECS {
        return None;
    }
    let snap = |v: f64| {
        let v = v.clamp(0.0, duration);
        if v <= TRIM_SNAP_SECS {
            0.0
        } else if duration - v <= TRIM_SNAP_SECS {
            duration
        } else {
            v
        }
    };
    let (mut s, mut e) = (snap(start.min(end)), snap(start.max(end)));
    if e - s < MIN_TRIM_SECS {
        let mid = (s + e) / 2.0;
        s = mid - MIN_TRIM_SECS / 2.0;
        e = s + MIN_TRIM_SECS;
        if s < 0.0 {
            s = 0.0;
            e = MIN_TRIM_SECS;
        } else if e > duration {
            e = duration;
            s = duration - MIN_TRIM_SECS;
        }
    }
    Some((s, e))
}

/// Whether `range` (from [`clamp_trim`]) is the whole clip: nothing to cut, the file is kept as it is.
pub fn is_whole(duration: f64, range: (f64, f64)) -> bool {
    range.0 <= 0.0 && range.1 >= duration
}

/// Height (points) of the HUD window for a pill of `base` points when it also shows `notes`. A note is a sentence
/// that wraps in the 96-point text column (about 19 characters of the 10-point note font per line, 13 points a
/// line), so the window grows by what its notes need and no more; no notes, no growth. Capped so a long list never
/// outgrows a small display.
pub fn hud_height(base: f64, notes: &[String]) -> f64 {
    const CHARS_PER_LINE: usize = 19;
    const LINE: f64 = 13.0;
    const MAX_EXTRA: f64 = 240.0;
    let lines: usize = notes
        .iter()
        .map(|n| n.chars().count().div_ceil(CHARS_PER_LINE).max(1))
        .sum();
    base + (lines as f64 * LINE + if lines > 0 { 4.0 } else { 0.0 }).min(MAX_EXTRA)
}

/// What this Mac can record besides the picture (`shots_record_capabilities`), camelCase. Settings enables its
/// toggles from it; `reason` is the sentence for whichever is off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub system_audio: bool,
    pub microphone: bool,
    pub reason: Option<String>,
}

/// The microphone grant of this process as the system reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicAccess {
    /// Never asked: the first recording with the microphone on asks.
    NotAsked,
    Granted,
    /// Refused or restricted by policy: the system never asks again, so the option stays off until it is changed in
    /// System Settings.
    Refused,
}

/// What can be recorded on macOS `major` with the microphone in state `mic`. Recording with ScreenCaptureKit's
/// recording output needs macOS 15, and so do both audio sources of it.
pub fn capabilities(major: i64, mic: MicAccess) -> Capabilities {
    if major < 15 {
        return Capabilities {
            system_audio: false,
            microphone: false,
            reason: Some("Recording with sound needs macOS 15 or newer.".into()),
        };
    }
    if mic == MicAccess::Refused {
        return Capabilities {
            system_audio: true,
            microphone: false,
            reason: Some(MIC_OFF.into()),
        };
    }
    Capabilities {
        system_audio: true,
        microphone: true,
        reason: None,
    }
}

/// Why the microphone toggle is off while access is refused.
pub const MIC_OFF: &str = "Microphone access is off for Atlas. Turn it on in System Settings > Privacy & Security > Microphone.";
/// The HUD note for a microphone that was on but could not be recorded.
pub const MIC_SKIPPED: &str = "Microphone skipped: Atlas has no microphone access. Turn it on in System Settings > Privacy & Security > Microphone.";
/// The HUD note for keystrokes that were on but could not be shown.
pub const KEYS_SKIPPED: &str = "Keystrokes skipped: macOS has not given Atlas Input Monitoring. Turn it on in System Settings > Privacy & Security > Input Monitoring, then record again.";
/// The HUD note for audio asked on a macOS without it.
pub const AUDIO_SKIPPED: &str = "Sound skipped: recording with sound needs macOS 15 or newer.";

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(q: VideoQuality, w: u32, h: u32) -> VideoConfig {
        video_config(q, (w, h), 30)
    }

    #[test]
    fn high_keeps_the_pixels_of_the_region_and_the_frame_rate() {
        let c = cfg(VideoQuality::High, 3024, 1964);
        assert_eq!((c.width, c.height, c.fps), (3024, 1964, 30));
    }

    #[test]
    fn medium_and_low_cap_the_long_side_and_keep_the_aspect() {
        let m = cfg(VideoQuality::Medium, 3024, 1964);
        assert_eq!(m.width, 1920);
        assert!((f64::from(m.width) / f64::from(m.height) - 3024.0 / 1964.0).abs() < 0.01);
        let l = cfg(VideoQuality::Low, 3024, 1964);
        assert_eq!(l.width, 1280);
        // a portrait region is capped on its height
        let p = cfg(VideoQuality::Low, 1000, 3000);
        assert_eq!(p.height, 1280);
        assert!(p.width < 1000);
    }

    #[test]
    fn a_preset_never_scales_up() {
        for q in [VideoQuality::High, VideoQuality::Medium, VideoQuality::Low] {
            let c = cfg(q, 800, 600);
            assert_eq!((c.width, c.height), (800, 600), "{q:?}");
        }
    }

    #[test]
    fn sides_are_even_and_at_least_two() {
        for q in [VideoQuality::High, VideoQuality::Medium, VideoQuality::Low] {
            for (w, h) in [(3001, 1999), (1, 1), (2561, 7), (9999, 3)] {
                let c = cfg(q, w, h);
                assert!(
                    c.width.is_multiple_of(2) && c.height.is_multiple_of(2),
                    "{q:?} {w}x{h} -> {c:?}"
                );
                assert!(c.width >= 2 && c.height >= 2, "{q:?} {w}x{h} -> {c:?}");
            }
        }
    }

    #[test]
    fn low_records_at_most_24_fps_and_never_raises_a_slower_source() {
        assert_eq!(video_config(VideoQuality::Low, (800, 600), 30).fps, 24);
        assert_eq!(video_config(VideoQuality::Low, (800, 600), 15).fps, 15);
        assert_eq!(video_config(VideoQuality::Medium, (800, 600), 30).fps, 30);
    }

    /// The point of the presets: each one costs less than the one above it on the same region.
    #[test]
    fn the_bitrate_goes_down_with_the_preset() {
        let (h, m, l) = (
            cfg(VideoQuality::High, 2880, 1800).bitrate,
            cfg(VideoQuality::Medium, 2880, 1800).bitrate,
            cfg(VideoQuality::Low, 2880, 1800).bitrate,
        );
        assert!(h > m && m > l, "{h} > {m} > {l}");
    }

    #[test]
    fn the_hud_grows_for_its_notes_and_only_for_them() {
        assert_eq!(hud_height(120.0, &[]), 120.0);
        let one = vec!["x".repeat(19)];
        assert_eq!(hud_height(120.0, &one), 120.0 + 13.0 + 4.0);
        let two_lines = vec!["x".repeat(20)];
        assert_eq!(hud_height(120.0, &two_lines), 120.0 + 26.0 + 4.0);
        // the real sentences are several lines: more notes, more height, never past the cap
        let real = vec![MIC_SKIPPED.to_string(), KEYS_SKIPPED.to_string()];
        let h = hud_height(120.0, &real);
        assert!(h > hud_height(120.0, &real[..1]) && h <= 120.0 + 240.0);
        assert_eq!(hud_height(120.0, &vec!["x".repeat(5000); 3]), 120.0 + 240.0);
    }

    #[test]
    fn the_estimate_is_bitrate_times_seconds_over_eight() {
        assert_eq!(estimate_bytes(8_000_000, 10.0), 10_000_000);
        assert_eq!(estimate_bytes(4_000_000, 0.5), 250_000);
        assert_eq!(estimate_bytes(8_000_000, 0.0), 0);
        assert_eq!(estimate_bytes(8_000_000, -3.0), 0);
        assert_eq!(estimate_bytes(8_000_000, f64::NAN), 0);
    }

    #[test]
    fn a_trim_inside_the_clip_is_kept_as_dragged() {
        assert_eq!(clamp_trim(10.0, 2.0, 7.5), Some((2.0, 7.5)));
    }

    #[test]
    fn handles_snap_to_the_bounds_of_the_clip() {
        assert_eq!(clamp_trim(10.0, 0.1, 9.9), Some((0.0, 10.0)));
        assert_eq!(clamp_trim(10.0, 0.16, 9.84), Some((0.16, 9.84)));
        // dragged past the ends
        assert_eq!(clamp_trim(10.0, -5.0, 99.0), Some((0.0, 10.0)));
    }

    #[test]
    fn crossed_handles_are_put_in_order() {
        assert_eq!(clamp_trim(10.0, 8.0, 3.0), Some((3.0, 8.0)));
    }

    #[test]
    fn a_trim_never_keeps_less_than_half_a_second_and_stays_inside_the_clip() {
        let (s, e) = clamp_trim(10.0, 5.0, 5.1).unwrap();
        assert!((e - s - MIN_TRIM_SECS).abs() < 1e-9, "{s}..{e}");
        assert!(s >= 0.0 && e <= 10.0);
        // at the start and at the end of the clip it slides instead of leaving
        assert_eq!(clamp_trim(10.0, 0.0, 0.2), Some((0.0, MIN_TRIM_SECS)));
        assert_eq!(
            clamp_trim(10.0, 9.8, 10.0),
            Some((10.0 - MIN_TRIM_SECS, 10.0))
        );
        // equal handles
        let (s, e) = clamp_trim(10.0, 4.0, 4.0).unwrap();
        assert!(e - s >= MIN_TRIM_SECS - 1e-9);
    }

    #[test]
    fn there_is_nothing_to_trim_in_a_clip_shorter_than_the_minimum_or_from_junk() {
        assert_eq!(clamp_trim(0.4, 0.0, 0.4), None);
        assert_eq!(clamp_trim(f64::NAN, 0.0, 1.0), None);
        assert_eq!(clamp_trim(10.0, f64::NAN, 1.0), None);
        assert_eq!(clamp_trim(10.0, 0.0, f64::INFINITY), None);
        // exactly the minimum is a valid clip
        assert_eq!(clamp_trim(0.5, 0.0, 0.5), Some((0.0, 0.5)));
    }

    #[test]
    fn the_whole_clip_is_recognised_so_the_file_is_not_rewritten() {
        let r = clamp_trim(10.0, 0.05, 9.95).unwrap();
        assert!(is_whole(10.0, r));
        assert!(!is_whole(10.0, (0.0, 9.0)));
        assert!(!is_whole(10.0, (1.0, 10.0)));
    }

    #[test]
    fn capabilities_need_macos_15_for_any_sound() {
        let c = capabilities(14, MicAccess::Granted);
        assert!(!c.system_audio && !c.microphone);
        assert!(c.reason.unwrap().contains("macOS 15"));
        let c = capabilities(15, MicAccess::NotAsked);
        assert!(c.system_audio && c.microphone && c.reason.is_none());
    }

    /// A refused microphone cannot be asked again, so the toggle is off and says where to turn it on, while system
    /// audio (no extra permission) stays available.
    #[test]
    fn a_refused_microphone_turns_only_the_microphone_off() {
        let c = capabilities(26, MicAccess::Refused);
        assert!(c.system_audio && !c.microphone);
        assert!(c.reason.unwrap().contains("System Settings"));
    }

    /// Settings reads exactly these keys: serialize the real value.
    #[test]
    fn capabilities_serialize_with_the_keys_settings_reads() {
        let v = serde_json::to_value(capabilities(15, MicAccess::Refused)).unwrap();
        assert_eq!(v["systemAudio"], true);
        assert_eq!(v["microphone"], false);
        assert!(v["reason"].is_string());
        let ok = serde_json::to_value(capabilities(15, MicAccess::Granted)).unwrap();
        assert!(ok["reason"].is_null());
        assert!(v.get("system_audio").is_none(), "camelCase only");
    }
}
