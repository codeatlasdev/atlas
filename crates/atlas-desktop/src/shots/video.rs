//! Reading and joining the videos capture writes (macOS, AVFoundation; always present, no ffmpeg needed):
//! the duration and pixel size of a clip, a frame at a time (the poster of the quick card and the source of a GIF),
//! and the join of the segments a paused recording is made of.

use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_av_foundation::{
    AVAsset, AVAssetExportPresetPassthrough, AVAssetExportSession, AVAssetExportSessionStatus,
    AVAssetImageGenerator, AVAssetTrack, AVFileTypeMPEG4, AVMediaTypeVideo, AVMutableComposition,
    AVURLAsset,
};
use objc2_core_foundation::CGSize;
use objc2_core_media::{CMTime, CMTimeRange};
use objc2_foundation::{NSString, NSURL};

const EXPORT_WAIT: Duration = Duration::from_secs(600);

fn url(p: &Path) -> Retained<NSURL> {
    NSURL::fileURLWithPath(&NSString::from_str(&p.to_string_lossy()))
}

fn asset(p: &Path) -> Retained<AVURLAsset> {
    // SAFETY: plain constructor with a file URL.
    unsafe { AVURLAsset::assetWithURL(&url(p)) }
}

fn secs(t: CMTime) -> f64 {
    if t.timescale > 0 && t.flags.0 & 1 != 0 {
        t.value as f64 / f64::from(t.timescale)
    } else {
        0.0
    }
}

/// What a clip is: seconds, pixel size, nominal frame rate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Info {
    pub secs: f64,
    pub size: (u32, u32),
    pub fps: u32,
}

// The synchronous track and frame readers are deprecated in favour of async loaders; these helpers run on worker
// threads that block on the result anyway, so the synchronous forms are the simple, correct ones.
#[allow(deprecated)]
fn video_track(a: &AVAsset) -> Result<Retained<AVAssetTrack>, String> {
    // SAFETY: plain getters on a live asset.
    let media = unsafe { AVMediaTypeVideo }.ok_or("AVFoundation has no video media type")?;
    unsafe { a.tracksWithMediaType(media) }
        .iter()
        .next()
        .ok_or_else(|| "the file has no video track".to_string())
}

pub fn info(p: &Path) -> Result<Info, String> {
    let a = asset(p);
    let track = video_track(&a)?;
    // SAFETY: plain getters.
    let (d, size, fps) = unsafe { (a.duration(), track.naturalSize(), track.nominalFrameRate()) };
    Ok(Info {
        secs: secs(d),
        size: (size.width.round() as u32, size.height.round() as u32),
        fps: fps.round().max(1.0) as u32,
    })
}

/// Reads frames of one clip: the asset and the image generator are built once, so a GIF export of 150 frames parses
/// the file once instead of 150 times.
pub struct FrameReader {
    generator: Retained<AVAssetImageGenerator>,
}

impl FrameReader {
    /// A reader of `p` whose frames fit `max_side` (0 = full size).
    #[allow(deprecated)]
    pub fn open(p: &Path, max_side: u32) -> Self {
        let a = asset(p);
        // SAFETY: documented AVAssetImageGenerator setup on a live asset.
        let generator = unsafe {
            let g = AVAssetImageGenerator::assetImageGeneratorWithAsset(&a);
            g.setAppliesPreferredTrackTransform(true);
            // exact frames: the default tolerance snaps to keyframes, which would make a GIF stutter
            g.setRequestedTimeToleranceBefore(CMTime::new(0, 600));
            g.setRequestedTimeToleranceAfter(CMTime::new(0, 600));
            if max_side > 0 {
                g.setMaximumSize(CGSize::new(f64::from(max_side), f64::from(max_side)));
            }
            g
        };
        Self { generator }
    }

    /// The frame at `at` seconds as RGBA.
    #[allow(deprecated)]
    pub fn at(&self, at: f64) -> Result<super::pixels::Rgba, String> {
        // SAFETY: the CGImage is read (copied into RGBA) before it is released.
        let img = unsafe {
            self.generator
                .copyCGImageAtTime_actualTime_error(
                    CMTime::with_seconds(at, 600),
                    std::ptr::null_mut(),
                )
                .map_err(|e| e.localizedDescription().to_string())?
        };
        let (data, size) = super::sck::rgba_of(&img)?;
        Ok(super::pixels::Rgba { data, size })
    }
}

/// The frame at `at` seconds as RGBA, scaled to fit `max_side` (0 = full size).
pub fn frame_at(p: &Path, at: f64, max_side: u32) -> Result<super::pixels::Rgba, String> {
    FrameReader::open(p, max_side).at(at)
}

/// Join `parts` (in order) into one MP4 at `out`, without re-encoding. One part is a plain copy.
pub fn join(parts: &[std::path::PathBuf], out: &Path) -> Result<(), String> {
    match parts {
        [] => return Err("there is nothing to join".into()),
        [one] => {
            std::fs::rename(one, out)
                .or_else(|_| std::fs::copy(one, out).map(|_| ()))
                .map_err(|e| format!("{}: {e}", out.display()))?;
            return Ok(());
        }
        _ => {}
    }
    let _ = std::fs::remove_file(out);
    // SAFETY: documented composition + export sequence; every object is alive until the export finishes.
    unsafe {
        let comp = AVMutableComposition::composition();
        let media = AVMediaTypeVideo.ok_or("AVFoundation has no video media type")?;
        let track = comp
            .addMutableTrackWithMediaType_preferredTrackID(media, 0)
            .ok_or("could not add a video track")?;
        let mut at = CMTime::new(0, 600);
        for part in parts {
            let a = asset(part);
            let src = video_track(&a)?;
            let range = CMTimeRange {
                start: CMTime::new(0, 600),
                duration: a.duration(),
            };
            track
                .insertTimeRange_ofTrack_atTime_error(range, &src, at)
                .map_err(|e| e.localizedDescription().to_string())?;
            at = CMTime::with_seconds(secs(at) + secs(a.duration()), 600);
        }
        let export = AVAssetExportSession::initWithAsset_presetName(
            AVAssetExportSession::alloc(),
            &comp,
            AVAssetExportPresetPassthrough,
        )
        .ok_or("could not start the export")?;
        export.setOutputURL(Some(&url(out)));
        export.setOutputFileType(AVFileTypeMPEG4);
        let (tx, rx) = mpsc::channel();
        let ex = export.clone();
        let block = RcBlock::new(move || {
            let st = ex.status();
            let r = if st == AVAssetExportSessionStatus::Completed {
                Ok(())
            } else {
                Err(ex
                    .error()
                    .map(|e| e.localizedDescription().to_string())
                    .unwrap_or_else(|| format!("export ended as {st:?}")))
            };
            let _ = tx.send(r);
        });
        export.exportAsynchronouslyWithCompletionHandler(&block);
        rx.recv_timeout(EXPORT_WAIT)
            .map_err(|_| "joining the recording took too long".to_string())??;
    }
    Ok(())
}

/// Write the part `start..end` (seconds) of `src` to `out` as MP4 without re-encoding (the export session's
/// passthrough preset with a time range). Passthrough cannot cut between key frames: the picture starts at the
/// requested instant (the edit list hides what precedes it) but the file holds the frames from the key frame before.
pub fn trim(src: &Path, out: &Path, start: f64, end: f64) -> Result<(), String> {
    if end.partial_cmp(&start) != Some(std::cmp::Ordering::Greater) || start < 0.0 {
        return Err("that trim keeps nothing".into());
    }
    let _ = std::fs::remove_file(out);
    let a = asset(src);
    // SAFETY: documented export sequence; every object is alive until the export finishes.
    unsafe {
        let export = AVAssetExportSession::initWithAsset_presetName(
            AVAssetExportSession::alloc(),
            &a,
            AVAssetExportPresetPassthrough,
        )
        .ok_or("could not start the export")?;
        export.setOutputURL(Some(&url(out)));
        export.setOutputFileType(AVFileTypeMPEG4);
        export.setTimeRange(CMTimeRange {
            start: CMTime::with_seconds(start, 600),
            duration: CMTime::with_seconds(end - start, 600),
        });
        let (tx, rx) = mpsc::channel();
        let ex = export.clone();
        let block = RcBlock::new(move || {
            let st = ex.status();
            let r = if st == AVAssetExportSessionStatus::Completed {
                Ok(())
            } else {
                Err(ex
                    .error()
                    .map(|e| e.localizedDescription().to_string())
                    .unwrap_or_else(|| format!("export ended as {st:?}")))
            };
            let _ = tx.send(r);
        });
        export.exportAsynchronouslyWithCompletionHandler(&block);
        rx.recv_timeout(EXPORT_WAIT)
            .map_err(|_| "trimming the recording took too long".to_string())??;
    }
    Ok(())
}
