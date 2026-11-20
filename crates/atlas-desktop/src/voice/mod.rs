//! Dictation capture (#647): the native half of the composer's microphone button.
//!
//! The page cannot use the Web Speech API (it sends audio to a vendor and is missing in two of the three webview
//! engines), so the shell records the microphone itself: 16 kHz mono s16le WAV in memory ([`wav`], [`pipe`]), one
//! recording at a time ([`recorder`]), the system's default input ([`device`]), the permission explained when it is
//! refused ([`access`]). The page pulls the audio in pieces (`voice_read`) and uploads it to the crew server, which
//! transcribes it; nothing is written to disk and nothing leaves the machine from here.
//!
//! The microphone is released as soon as the recording ends: the person releases the key, the 300 s cap is reached,
//! the window goes to the background, the composer goes away (`voice_cancel`), or the device disappears. The three
//! ways that happen without the page asking are announced with `voice://stopped` (what was said is kept: the page
//! transcribes it); a meter reading arrives as `voice://level` (0..1, 20 a second).

pub mod access;
mod device;
mod pipe;
mod recorder;
mod wav;

use base64::Engine as _;
use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use access::Access;
use recorder::{Event, Reason, Recorder, StartError, Take};

/// Every meter reading, 0..1.
const LEVEL: &str = "voice://level";
/// A recording that ended without the page asking: the payload is a [`Take`].
const STOPPED: &str = "voice://stopped";

/// The one recorder of the app.
pub struct Voice {
    rec: Mutex<Recorder<device::Mic>>,
}

impl Voice {
    pub fn new(app: &AppHandle) -> Self {
        let app = app.clone();
        Self {
            rec: Mutex::new(Recorder::new(device::Mic::default(), move |ev| match ev {
                Event::Level(l) => {
                    let _ = app.emit(LEVEL, l);
                }
                // Stopping closes the device from its own thread, never from the audio callback that said it.
                Event::Ended(reason) => {
                    let app = app.clone();
                    std::thread::spawn(move || end(&app, reason));
                }
            })),
        }
    }
}

/// End the recording (if one is live) and tell the page, which takes what was said from here.
fn end(app: &AppHandle, reason: Reason) {
    let take = app.state::<Voice>().rec.lock().stop(reason);
    if let Some(take) = take {
        let _ = app.emit(STOPPED, take);
    }
}

/// The window left the foreground: give the microphone back. Safe to call when nothing is recording.
pub fn release_in_background(app: &AppHandle) {
    let app = app.clone();
    // A stop joins the audio thread: keep it off the thread that delivers window events.
    std::thread::spawn(move || {
        if let Some(v) = app.try_state::<Voice>()
            && v.rec.lock().is_live()
        {
            end(&app, Reason::Background);
        }
    });
}

/// The permission state, for the composer to explain before the first recording and after a failed one.
#[tauri::command]
pub fn voice_access() -> Access {
    access::current()
}

/// Open the system page where the microphone is allowed.
#[tauri::command]
pub fn voice_open_settings() -> Result<(), String> {
    access::open_settings()
}

/// Open the microphone. The first time on macOS this shows the system prompt and waits for the answer.
#[tauri::command]
pub async fn voice_start(app: AppHandle) -> Result<(), StartError> {
    // The recording must never hear a reply being read aloud.
    crate::speech::stop(&app);
    tauri::async_runtime::spawn_blocking(move || {
        match access::current() {
            Access::Denied => return Err(StartError::Denied),
            Access::NotAsked => {
                // The prompt is the OS's: an agent instance must not raise it on the user's screen.
                if crate::native_ui::gate(
                    crate::native_ui::Kind::Alert,
                    "microphone permission",
                    "dictation",
                ) == crate::native_ui::Verdict::Suppressed
                {
                    return Err(StartError::Unavailable {
                        detail: "an agent instance never shows the microphone prompt".into(),
                    });
                }
                if access::request() != Access::Granted {
                    return Err(StartError::Denied);
                }
            }
            Access::Granted | Access::Unknown => {}
        }
        app.state::<Voice>().rec.lock().start()
    })
    .await
    .map_err(|e| StartError::Failed {
        detail: e.to_string(),
    })?
}

/// Release the microphone; what was said stays readable. `null` when nothing was recording (it ended by itself a
/// moment before: `voice://stopped` carries that take).
#[tauri::command]
pub async fn voice_stop(app: AppHandle) -> Option<Take> {
    app.state::<Voice>().rec.lock().stop(Reason::Requested)
}

/// Release the microphone and throw the audio away (Esc, the composer unmounting, a refused start).
#[tauri::command]
pub async fn voice_cancel(app: AppHandle) {
    app.state::<Voice>().rec.lock().cancel();
}

/// One piece of the WAV file (see [`wav`] for the header while it is still growing).
#[derive(Serialize)]
pub struct Piece {
    /// Base64 of the bytes.
    data_b64: String,
    /// Bytes of the file so far.
    total: usize,
    /// Still recording: `total` can grow.
    live: bool,
}

/// Up to `max` bytes of the recording from `offset`.
#[tauri::command]
pub async fn voice_read(app: AppHandle, offset: usize, max: usize) -> Piece {
    let p = app.state::<Voice>().rec.lock().read(offset, max);
    Piece {
        data_b64: base64::engine::general_purpose::STANDARD.encode(&p.data),
        total: p.total,
        live: p.live,
    }
}
