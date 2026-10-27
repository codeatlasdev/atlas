//! ScreenCaptureKit capture of Atlas's own windows (macOS). One `SCStream` filtered to a single
//! window (`desktopIndependentWindow`): the compositor renders that window by itself, so other
//! windows on top of it never appear in the frames, and the stream sends no input anywhere.
//!
//! * Video: `SCRecordingOutput` writes HEVC into a `.mov`/`.mp4` (macOS 15+). A second, cheap
//!   `SCStreamOutput` only reads each frame's timestamp to measure pacing and to report when the
//!   first frame arrived, so the script timeline can be aligned to the picture.
//! * Stills: `SCScreenshotManager` (macOS 14+) and, without any permission, `WKWebView.takeSnapshot`.
//!
//! Limits (Apple's, not ours): a window that is minimized, hidden (`orderOut`) or on another Space
//! produces no frames; occlusion by other windows is fine. Frames are delivered when the picture
//! changes, so the recording is variable frame rate; `Stats::max_gap_ms` measures the worst stall.

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AnyThread, DefinedClass, define_class, msg_send};
use objc2_core_media::{CMSampleBuffer, CMTime};
use objc2_foundation::{NSError, NSObject, NSObjectProtocol, NSString, NSURL};
use objc2_screen_capture_kit::{
    SCContentFilter, SCRecordingOutput, SCRecordingOutputConfiguration, SCRecordingOutputDelegate,
    SCScreenshotManager, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamDelegate,
    SCStreamOutput, SCStreamOutputType, SCWindow,
};
use parking_lot::Mutex;

use super::request::FPS_CHOICES;

const WAIT: Duration = Duration::from_secs(20);

/// Screen Recording state as the OS reports it for this process.
pub fn permission_granted() -> bool {
    objc2_core_graphics::CGPreflightScreenCaptureAccess()
}

/// Shows the system prompt once (macOS remembers the answer); returns the current grant.
pub fn request_permission() -> bool {
    objc2_core_graphics::CGRequestScreenCaptureAccess()
}

/// Seconds on the host clock the stream's presentation timestamps use (`CMClockGetHostTimeClock`).
pub fn host_clock_seconds() -> f64 {
    // SAFETY: the host time clock is a process-wide singleton; CMClockGetTime is a plain getter.
    let t = unsafe { objc2_core_media::CMClock::host_time_clock().time() };
    t.value as f64 / f64::from(t.timescale)
}

/// Frame pacing of one recording, measured from the stream's own presentation timestamps.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Stats {
    pub frames: u64,
    /// Host-clock time (seconds, `mach_absolute_time` based) of the first frame.
    pub first_pts: f64,
    pub last_pts: f64,
    pub max_gap_ms: f64,
}

impl Stats {
    pub fn push(&mut self, pts: f64) {
        if self.frames == 0 {
            self.first_pts = pts;
        } else {
            self.max_gap_ms = self.max_gap_ms.max((pts - self.last_pts) * 1000.0);
        }
        self.last_pts = pts;
        self.frames += 1;
    }
}

struct Ivars {
    stats: Mutex<Stats>,
    started: Mutex<Option<mpsc::Sender<Result<(), String>>>>,
    failed: Mutex<Option<String>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; Ivars is Send + Sync (parking_lot Mutexes).
    #[unsafe(super(NSObject))]
    #[thread_kind = AnyThread]
    #[name = "AtlasCaptureSink"]
    #[ivars = Arc<Ivars>]
    struct Sink;

    unsafe impl NSObjectProtocol for Sink {}

    unsafe impl SCStreamOutput for Sink {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn did_output(&self, _s: &SCStream, sb: &CMSampleBuffer, kind: SCStreamOutputType) {
            if kind != SCStreamOutputType::Screen {
                return;
            }
            // Idle/blank status frames carry no image buffer; only real pictures count.
            // SAFETY: `sb` is the live buffer handed to this callback.
            if unsafe { sb.image_buffer() }.is_none() {
                return;
            }
            // SAFETY: as above.
            let t: CMTime = unsafe { sb.presentation_time_stamp() };
            if t.timescale > 0 {
                self.ivars().stats.lock().push(t.value as f64 / f64::from(t.timescale));
            }
        }
    }

    unsafe impl SCStreamDelegate for Sink {
        #[unsafe(method(stream:didStopWithError:))]
        fn did_stop(&self, _s: &SCStream, e: &NSError) {
            *self.ivars().failed.lock() = Some(e.localizedDescription().to_string());
        }
    }

    unsafe impl SCRecordingOutputDelegate for Sink {
        #[unsafe(method(recordingOutputDidStartRecording:))]
        fn did_start(&self, _o: &SCRecordingOutput) {
            if let Some(tx) = self.ivars().started.lock().take() {
                let _ = tx.send(Ok(()));
            }
        }
        #[unsafe(method(recordingOutput:didFailWithError:))]
        fn did_fail(&self, _o: &SCRecordingOutput, e: &NSError) {
            let msg = e.localizedDescription().to_string();
            *self.ivars().failed.lock() = Some(msg.clone());
            if let Some(tx) = self.ivars().started.lock().take() {
                let _ = tx.send(Err(msg));
            }
        }
        #[unsafe(method(recordingOutputDidFinishRecording:))]
        fn did_finish(&self, _o: &SCRecordingOutput) {}
    }
);

/// A running recording. Dropping it without `stop` leaves the stream running until the app exits:
/// callers keep it in `Capture` state and stop it explicitly (the host also stops on a deadline).
pub struct Recorder {
    stream: Retained<SCStream>,
    output: Retained<SCRecordingOutput>,
    sink: Retained<Sink>,
    ivars: Arc<Ivars>,
    pub pixels: (usize, usize),
}

// SAFETY: the SCK objects are used from the thread that owns the `Capture` mutex; SCK itself is
// thread-safe for start/stop, and the Sink's state is behind parking_lot Mutexes.
unsafe impl Send for Recorder {}

/// The `SCWindow` for CGWindowID `id` among this process's windows (no extra permission to list).
pub(super) fn own_window(id: u32) -> Result<Retained<SCWindow>, String> {
    let (tx, rx) = mpsc::channel();
    let block = RcBlock::new(move |c: *mut SCShareableContent, e: *mut NSError| {
        // SAFETY: SCK passes either a valid content object or a valid error.
        let r = match unsafe { c.as_ref() } {
            Some(c) => {
                let windows = unsafe { c.windows() };
                Ok(windows.iter().find(|w| unsafe { w.windowID() } == id))
            }
            None => Err(unsafe { e.as_ref() }
                .map(|e| e.localizedDescription().to_string())
                .unwrap_or_else(|| "unknown ScreenCaptureKit error".into())),
        };
        let _ = tx.send(r);
    });
    // SAFETY: the block outlives the call (SCK copies it).
    unsafe { SCShareableContent::getCurrentProcessShareableContentWithCompletionHandler(&block) };
    match rx.recv_timeout(WAIT) {
        Ok(Ok(Some(w))) => Ok(w),
        Ok(Ok(None)) => Err(format!(
            "window {id} is not capturable: it is minimized, hidden, or on another Space (show it, or use mode \"snapshot\")"
        )),
        Ok(Err(e)) => Err(permission_hint(&e)),
        Err(_) => Err("ScreenCaptureKit did not answer (is the app responsive?)".into()),
    }
}

pub(super) fn permission_hint(e: &str) -> String {
    if permission_granted() {
        format!("ScreenCaptureKit: {e}")
    } else {
        format!(
            "Screen Recording permission is missing ({e}). Grant it to Atlas (or to the terminal that launched it) in System Settings > Privacy & Security > Screen Recording, then restart. `snapshot` mode needs no permission."
        )
    }
}

pub(super) fn config(
    size: (usize, usize),
    fps: u32,
    cursor: bool,
) -> Retained<SCStreamConfiguration> {
    debug_assert!(FPS_CHOICES.contains(&fps));
    // SAFETY: plain property setters on a fresh configuration.
    unsafe {
        let c = SCStreamConfiguration::new();
        c.setWidth(size.0);
        c.setHeight(size.1);
        c.setMinimumFrameInterval(CMTime::new(1, fps as i32));
        c.setShowsCursor(cursor);
        c.setQueueDepth(6);
        c
    }
}

/// What a recording captures and how it is written: the content filter, the stream configuration, and the file type
/// of the output (`.mov` for scripted window recordings, `.mp4` for the user's screen captures).
pub struct Source {
    pub filter: Retained<SCContentFilter>,
    pub config: Retained<SCStreamConfiguration>,
    pub pixels: (usize, usize),
    /// QuickTime (`.mov`) or MPEG-4 (`.mp4`).
    pub mp4: bool,
}

impl Recorder {
    /// Start recording window `id` (`pixels` = output size) into `out` as HEVC.
    pub fn start(
        id: u32,
        pixels: (usize, usize),
        fps: u32,
        cursor: bool,
        out: &Path,
    ) -> Result<Self, String> {
        let window = own_window(id)?;
        // SAFETY: documented SCK setup; the filter is retained by the stream.
        let filter = unsafe {
            SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &window)
        };
        Self::start_source(
            Source {
                filter,
                config: config(pixels, fps, cursor),
                pixels,
                mp4: false,
            },
            out,
        )
    }

    /// Start recording `source` into `out` as HEVC.
    pub fn start_source(source: Source, out: &Path) -> Result<Self, String> {
        let Source {
            filter,
            config,
            pixels,
            mp4,
        } = source;
        if !objc2_foundation::NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(
            objc2_foundation::NSOperatingSystemVersion {
                majorVersion: 15,
                minorVersion: 0,
                patchVersion: 0,
            },
        ) {
            return Err(
                "video capture needs macOS 15 or newer (stills work on 14+, `snapshot` everywhere)"
                    .into(),
            );
        }
        let _ = std::fs::remove_file(out);
        let ivars = Arc::new(Ivars {
            stats: Mutex::default(),
            started: Mutex::default(),
            failed: Mutex::default(),
        });
        let (tx, rx) = mpsc::channel();
        *ivars.started.lock() = Some(tx);
        let out = out.to_path_buf();
        let iv = ivars.clone();
        let (stream, output, sink) = (|| -> Result<_, String> {
            let sink: Retained<Sink> =
                unsafe { msg_send![super(Sink::alloc().set_ivars(iv)), init] };
            // SAFETY: documented SCK setup sequence; every object outlives the stream through `Recorder`.
            unsafe {
                let stream = SCStream::initWithFilter_configuration_delegate(
                    SCStream::alloc(),
                    &filter,
                    &config,
                    Some(ProtocolObject::from_ref(&*sink)),
                );
                let rc = SCRecordingOutputConfiguration::new();
                let url = NSURL::fileURLWithPath(&NSString::from_str(&out.to_string_lossy()));
                rc.setOutputURL(&url);
                rc.setVideoCodecType(
                    objc2_av_foundation::AVVideoCodecTypeHEVC.ok_or("HEVC is unavailable")?,
                );
                rc.setOutputFileType(if mp4 {
                    objc2_av_foundation::AVFileTypeMPEG4.ok_or("MPEG-4 output is unavailable")?
                } else {
                    objc2_av_foundation::AVFileTypeQuickTimeMovie
                        .ok_or("QuickTime output is unavailable")?
                });
                let output = SCRecordingOutput::initWithConfiguration_delegate(
                    SCRecordingOutput::alloc(),
                    &rc,
                    ProtocolObject::from_ref(&*sink),
                );
                stream
                    .addRecordingOutput_error(&output)
                    .map_err(|e| e.localizedDescription().to_string())?;
                stream
                    .addStreamOutput_type_sampleHandlerQueue_error(
                        ProtocolObject::from_ref(&*sink),
                        SCStreamOutputType::Screen,
                        None,
                    )
                    .map_err(|e| e.localizedDescription().to_string())?;
                Ok((stream, output, sink))
            }
        })()?;
        let (dtx, drx) = mpsc::channel();
        let block = RcBlock::new(move |e: *mut NSError| {
            // SAFETY: SCK passes null on success or a valid error.
            let r = match unsafe { e.as_ref() } {
                None => Ok(()),
                Some(e) => Err(e.localizedDescription().to_string()),
            };
            let _ = dtx.send(r);
        });
        // SAFETY: block is copied by SCK.
        unsafe { stream.startCaptureWithCompletionHandler(Some(&block)) };
        drx.recv_timeout(WAIT)
            .map_err(|_| "ScreenCaptureKit did not start in time".to_string())?
            .map_err(|e| permission_hint(&e))?;
        rx.recv_timeout(WAIT)
            .map_err(|_| "the recording never started".to_string())??;
        Ok(Self {
            stream,
            output,
            sink,
            ivars,
            pixels,
        })
    }

    /// Stop, wait for the file to be finalized, and return the pacing stats.
    pub fn stop(self) -> Result<(Stats, u64), String> {
        let (tx, rx) = mpsc::channel();
        let block = RcBlock::new(move |e: *mut NSError| {
            let r = match unsafe { e.as_ref() } {
                None => Ok(()),
                Some(e) => Err(e.localizedDescription().to_string()),
            };
            let _ = tx.send(r);
        });
        // SAFETY: block is copied by SCK.
        unsafe { self.stream.stopCaptureWithCompletionHandler(Some(&block)) };
        rx.recv_timeout(WAIT)
            .map_err(|_| "ScreenCaptureKit did not stop in time".to_string())?
            .map_err(|e| format!("stopping the capture: {e}"))?;
        // The file is finalized after the stop callback; wait for the delegate's finish via size settling.
        std::thread::sleep(Duration::from_millis(300));
        let bytes = unsafe { self.output.recordedFileSize() }.max(0) as u64;
        let _ = &self.sink;
        if let Some(e) = self.ivars.failed.lock().clone() {
            return Err(format!("the recording failed: {e}"));
        }
        Ok((*self.ivars.stats.lock(), bytes))
    }
}

/// PNG of window `id` via `SCScreenshotManager` (macOS 14+), `pixels` large.
pub fn window_png(id: u32, pixels: (usize, usize), out: &Path) -> Result<(), String> {
    let window = own_window(id)?;
    let (tx, rx) = mpsc::channel();
    let out = out.to_path_buf();
    let block = RcBlock::new(
        move |img: *mut objc2_core_graphics::CGImage, e: *mut NSError| {
            // SAFETY: SCK passes a valid image or a valid error, both live for this callback.
            let r = match unsafe { img.as_ref() } {
                Some(i) => super::png::write_cgimage(i, &out),
                None => Err(unsafe { e.as_ref() }
                    .map(|e| permission_hint(&e.localizedDescription().to_string()))
                    .unwrap_or_else(|| "ScreenCaptureKit returned no image".into())),
            };
            let _ = tx.send(r);
        },
    );
    // SAFETY: documented SCK call; the block is copied.
    unsafe {
        let filter =
            SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &window);
        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
            &filter,
            &config(pixels, 60, false),
            Some(&block),
        );
    }
    rx.recv_timeout(WAIT)
        .map_err(|_| "ScreenCaptureKit did not return the screenshot".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_track_the_worst_gap_between_frames_not_the_mean() {
        let mut s = Stats::default();
        for pts in [10.000, 10.016, 10.033, 10.250, 10.266] {
            s.push(pts);
        }
        assert_eq!(s.frames, 5);
        assert_eq!(s.first_pts, 10.0);
        assert_eq!(s.last_pts, 10.266);
        assert!((s.max_gap_ms - 217.0).abs() < 0.01, "got {}", s.max_gap_ms);
    }

    #[test]
    fn a_single_frame_has_no_gap() {
        let mut s = Stats::default();
        s.push(3.0);
        assert_eq!((s.frames, s.max_gap_ms), (1, 0.0));
    }
}
