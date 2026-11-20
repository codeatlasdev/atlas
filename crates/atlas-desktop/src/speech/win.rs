//! Windows: SAPI (`ISpVoice`), asynchronous. The person's default voice and rate from Speech settings are used as
//! they are; a language other than the voice's gets an installed voice for it (SAPI's `Language` attribute is a
//! hexadecimal LCID: 409 English, 416 Portuguese Brazil, 816 Portuguese Portugal) and the default when there is none.

use windows::Win32::Media::Speech::{
    ISpObjectTokenCategory, ISpVoice, SPF_ASYNC, SPF_IS_NOT_XML, SPF_PURGEBEFORESPEAK, SPRS_DONE,
    SPVOICESTATUS, SpObjectTokenCategory, SpVoice,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::core::{PCWSTR, PWSTR, w};

use super::speaker::{Engine, Lang};

pub fn available() -> bool {
    true
}

pub struct Sapi {
    voice: Option<ISpVoice>,
    /// The language the voice was last set for (`None`: the person's default).
    set_for: Option<Lang>,
}

impl Sapi {
    /// Must run on the speech thread: COM is initialised for it and the voice never leaves it.
    pub fn new() -> Self {
        // SAFETY: COM initialisation for this thread; a second call on the same thread is harmless.
        let voice = unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            CoCreateInstance::<_, ISpVoice>(&SpVoice, None, CLSCTX_ALL).ok()
        };
        Self {
            voice,
            set_for: None,
        }
    }
}

/// The voices category of the registry, where SAPI lists the installed voices.
const VOICES: PCWSTR = w!("HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Speech\\Voices");

/// The `Language` requirement that selects an installed voice for `lang`.
fn requirement(lang: Lang) -> PCWSTR {
    match lang {
        Lang::En => w!("Language=409"),
        Lang::PtBr => w!("Language=416"),
    }
}

impl Engine for Sapi {
    fn speak(&mut self, text: &str, lang: Lang) -> Result<(), String> {
        let voice = self
            .voice
            .as_ref()
            .ok_or("the Windows speech engine is not available")?;
        if self.set_for != Some(lang) {
            // SAFETY: COM calls on interfaces this thread created.
            unsafe {
                let token = CoCreateInstance::<_, ISpObjectTokenCategory>(
                    &SpObjectTokenCategory,
                    None,
                    CLSCTX_ALL,
                )
                .and_then(|c| {
                    c.SetId(VOICES, false)?;
                    c.EnumTokens(requirement(lang), PCWSTR::null())?.Item(0)
                });
                // No installed voice for this language: the default voice speaks, as the person set it.
                if let Ok(t) = token {
                    let _ = voice.SetVoice(&t);
                }
            }
            self.set_for = Some(lang);
        }
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `wide` is NUL-terminated and outlives the call; the text is copied by SAPI before it returns.
        unsafe {
            voice
                .Speak(
                    PCWSTR(wide.as_ptr()),
                    (SPF_ASYNC.0 | SPF_PURGEBEFORESPEAK.0 | SPF_IS_NOT_XML.0) as u32,
                    None,
                )
                .map_err(|e| format!("SAPI could not speak: {e}"))
        }
    }

    fn stop(&mut self) {
        if let Some(v) = &self.voice {
            // SAFETY: an empty purging call is how SAPI is silenced.
            unsafe {
                let _ = v.Speak(
                    PCWSTR::null(),
                    (SPF_ASYNC.0 | SPF_PURGEBEFORESPEAK.0) as u32,
                    None,
                );
            }
        }
    }

    fn speaking(&mut self) -> bool {
        let Some(v) = &self.voice else { return false };
        let mut status = SPVOICESTATUS::default();
        let mut bookmark = PWSTR::null();
        // SAFETY: both out-pointers are valid for the call.
        let got = unsafe { v.GetStatus(&mut status, &mut bookmark) };
        got.is_ok() && status.dwRunningState & SPRS_DONE.0 as u32 == 0
    }
}
