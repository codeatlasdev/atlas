//! Reading replies aloud (#648): the native half of the chat's "Read replies aloud" toggle.
//!
//! The page cannot use the Web Speech API (it is missing in two of the three webview engines and some engines
//! send the text to a vendor), so the shell speaks with the system's own synthesizer: `AVSpeechSynthesizer` on macOS
//! ([`mac`]), SAPI on Windows ([`win`]), speech-dispatcher's `spd-say` elsewhere ([`linux`], optional). The system's
//! voice and rate are the person's; the language is the app's display language, which the page passes with the text.
//!
//! One utterance at a time ([`speaker`]). It stops at once on `speech_stop` (any key or click in the page, a
//! dictation starting) and whenever the microphone opens, so the recording never hears the reply. The end of an
//! utterance that ran out by itself arrives as `speech://ended` with its id.

#[cfg(any(not(any(target_os = "macos", windows)), all(test, unix)))]
mod linux;
#[cfg(target_os = "macos")]
mod mac;
mod speaker;
#[cfg(windows)]
mod win;

use tauri::{AppHandle, Emitter, Manager};

use speaker::{Lang, Speaker};

/// An utterance ended by itself: the payload is its id (`speech_speak`'s reply).
const ENDED: &str = "speech://ended";

/// The one speaker of the app.
pub struct Speech {
    speaker: Speaker,
}

impl Speech {
    pub fn new(app: &AppHandle) -> Self {
        let app = app.clone();
        let emit = move |id: u64| {
            let _ = app.emit(ENDED, id);
        };
        #[cfg(target_os = "macos")]
        let speaker = Speaker::spawn(mac::Mac::new, emit, speaker::GRACE);
        #[cfg(windows)]
        let speaker = Speaker::spawn(win::Sapi::new, emit, speaker::GRACE);
        #[cfg(not(any(target_os = "macos", windows)))]
        let speaker = Speaker::spawn(linux::Spd::new, emit, speaker::GRACE);
        Self { speaker }
    }
}

/// Silence the speaker if the app has one. Safe from any thread, and when nothing is speaking.
pub fn stop(app: &AppHandle) {
    if let Some(s) = app.try_state::<Speech>() {
        s.speaker.stop();
    }
}

/// Whether this system can read aloud (always on macOS and Windows; on Linux when `spd-say` is installed).
#[tauri::command]
pub fn speech_available() -> bool {
    #[cfg(target_os = "macos")]
    return mac::available();
    #[cfg(windows)]
    return win::available();
    #[cfg(not(any(target_os = "macos", windows)))]
    linux::available()
}

/// Speak `text` in `lang` (`en` or `pt-BR`), cutting what is being said. Returns the utterance's id. A blank text
/// or an engine failure is the error.
#[tauri::command]
pub async fn speech_speak(app: AppHandle, text: String, lang: String) -> Result<u64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Speech>()
            .speaker
            .speak(&text, Lang::parse(&lang))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Silence at once.
#[tauri::command]
pub fn speech_stop(app: AppHandle) {
    stop(&app);
}
