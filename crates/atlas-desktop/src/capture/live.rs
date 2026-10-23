//! Live frames of the main window for `bun run desktop:mirror` (debug builds): one `SCStream` at up to 30 fps whose
//! frames are encoded to JPEG in memory as they arrive, so a viewer pulls the newest picture over the debug bridge
//! instead of a screenshot per frame (PNG on disk + re-encode: ~5 fps). The stream starts on the first pull and
//! stops by itself when nobody has pulled for `IDLE`, so an unwatched instance pays nothing.

use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AnyThread, DefinedClass, define_class, msg_send};
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImageCompressionFactor};
use objc2_core_foundation::CFData;
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo,
};
use objc2_core_media::CMSampleBuffer;
use objc2_core_video::{
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSDictionary, NSError, NSNumber, NSObject, NSObjectProtocol};
use objc2_screen_capture_kit::{
    SCContentFilter, SCStream, SCStreamConfiguration, SCStreamOutput, SCStreamOutputType,
};
use parking_lot::{Condvar, Mutex};

/// Stop the stream after this long without a pull.
pub const IDLE: Duration = Duration::from_secs(5);
const QUALITY: f64 = 0.6;

#[derive(Default)]
pub struct Latest {
    pub seq: u64,
    pub jpeg: Arc<Vec<u8>>,
    pub size: (usize, usize),
}

pub struct Shared {
    latest: Mutex<Latest>,
    fresh: Condvar,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "AtlasLiveSink"]
    #[ivars = Arc<Shared>]
    struct Sink;

    unsafe impl NSObjectProtocol for Sink {}

    unsafe impl SCStreamOutput for Sink {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn did_output(&self, _s: &SCStream, sb: &CMSampleBuffer, kind: SCStreamOutputType) {
            if kind != SCStreamOutputType::Screen {
                return;
            }
            // SAFETY: `sb` is the live buffer handed to this callback.
            let Some(px) = (unsafe { sb.image_buffer() }) else { return };
            let Some((jpeg, size)) = encode(&px) else { return };
            let sh = self.ivars();
            let mut l = sh.latest.lock();
            l.seq += 1;
            l.jpeg = Arc::new(jpeg);
            l.size = size;
            sh.fresh.notify_all();
        }
    }
);

/// BGRA pixel buffer (SCStream's default format) to JPEG.
fn encode(px: &objc2_core_video::CVPixelBuffer) -> Option<(Vec<u8>, (usize, usize))> {
    if objc2_core_video::CVPixelBufferGetPixelFormatType(px)
        != objc2_core_video::kCVPixelFormatType_32BGRA
    {
        return None;
    }
    let ro = CVPixelBufferLockFlags::ReadOnly;
    // SAFETY: lock/unlock pair around reading the base address of a live buffer.
    if unsafe { CVPixelBufferLockBaseAddress(px, ro) } != 0 {
        return None;
    }
    let (w, h, bpr) = (
        CVPixelBufferGetWidth(px),
        CVPixelBufferGetHeight(px),
        CVPixelBufferGetBytesPerRow(px),
    );
    let base = CVPixelBufferGetBaseAddress(px) as *const u8;
    let bytes =
        (!base.is_null()).then(|| unsafe { std::slice::from_raw_parts(base, bpr * h) }.to_vec());
    unsafe { CVPixelBufferUnlockBaseAddress(px, ro) };
    let bytes = bytes?;
    let data = CFData::from_bytes(&bytes);
    let provider = CGDataProvider::with_cf_data(Some(&data))?;
    let space = CGColorSpace::new_device_rgb()?;
    let info = CGBitmapInfo(
        CGImageByteOrderInfo::Order32Little.0 | CGImageAlphaInfo::PremultipliedFirst.0,
    );
    // SAFETY: the provider holds w*h BGRA pixels with `bpr` bytes per row; null decode array is allowed.
    let img: objc2_core_foundation::CFRetained<CGImage> = unsafe {
        CGImage::new(
            w,
            h,
            8,
            32,
            bpr,
            Some(&space),
            info,
            Some(&provider),
            std::ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }?;
    let rep = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &img);
    let props = NSDictionary::from_slices(
        &[unsafe { NSImageCompressionFactor }],
        &[&*NSNumber::new_f64(QUALITY) as &objc2::runtime::AnyObject],
    );
    // SAFETY: encode of a live bitmap rep with a compression-factor property.
    let data =
        unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::JPEG, &props) }?;
    Some((data.to_vec(), (w, h)))
}

/// The newest frame after `since`, waiting up to `wait` for one (a still window sends no new frames: the current
/// one comes back after the wait so the viewer keeps a picture).
pub fn pull(shared: &Shared, since: u64, wait: Duration) -> super::Frame {
    let mut l = shared.latest.lock();
    if l.seq <= since {
        let _ = shared.fresh.wait_for(&mut l, wait);
    }
    (l.seq, l.jpeg.clone(), l.size)
}

pub struct Live {
    stream: Retained<SCStream>,
    _sink: Retained<Sink>,
    shared: Arc<Shared>,
    window: u32,
    last_pull: Instant,
}

// SAFETY: SCStream and the sink are thread-safe Objective-C objects (SCK calls the sink on its own queue); `Live`
// is only touched behind the `LIVE` mutex, the same contract as `sck::Recorder`.
unsafe impl Send for Live {}

impl Live {
    /// Start a 30 fps stream of window `id` at `pixels`.
    pub fn start(id: u32, pixels: (usize, usize)) -> Result<Self, String> {
        let window = super::sck::own_window(id)?;
        let shared = Arc::new(Shared {
            latest: Mutex::default(),
            fresh: Condvar::new(),
        });
        let sink: Retained<Sink> =
            unsafe { msg_send![super(Sink::alloc().set_ivars(shared.clone())), init] };
        // SAFETY: documented SCK setup; the sink outlives the stream through `Live`.
        let stream = unsafe {
            let filter = SCContentFilter::initWithDesktopIndependentWindow(
                SCContentFilter::alloc(),
                &window,
            );
            let c: Retained<SCStreamConfiguration> = super::sck::config(pixels, 30, false);
            // SCStream defaults to YUV 4:2:0 (`420v`); `encode` reads BGRA.
            c.setPixelFormat(objc2_core_video::kCVPixelFormatType_32BGRA);
            let stream = SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                &filter,
                &c,
                None,
            );
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(
                    ProtocolObject::from_ref(&*sink),
                    SCStreamOutputType::Screen,
                    None,
                )
                .map_err(|e| e.localizedDescription().to_string())?;
            stream
        };
        let (tx, rx) = mpsc::channel();
        let block = RcBlock::new(move |e: *mut NSError| {
            let _ = tx.send(unsafe { e.as_ref() }.map(|e| e.localizedDescription().to_string()));
        });
        unsafe { stream.startCaptureWithCompletionHandler(Some(&block)) };
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(None) => {}
            Ok(Some(e)) => return Err(super::sck::permission_hint(&e)),
            Err(_) => return Err("ScreenCaptureKit did not start the live stream".into()),
        }
        Ok(Self {
            stream,
            _sink: sink,
            shared,
            window: id,
            last_pull: Instant::now(),
        })
    }

    pub fn window(&self) -> u32 {
        self.window
    }

    pub fn touch(&mut self) {
        self.last_pull = Instant::now();
    }

    pub fn idle_for(&self) -> Duration {
        self.last_pull.elapsed()
    }

    pub fn shared(&self) -> Arc<Shared> {
        self.shared.clone()
    }

    pub fn stop(self) {
        let block = RcBlock::new(|_e: *mut NSError| {});
        unsafe { self.stream.stopCaptureWithCompletionHandler(Some(&block)) };
    }
}
