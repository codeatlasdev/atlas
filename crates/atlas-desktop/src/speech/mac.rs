//! macOS: `AVSpeechSynthesizer`. The system's own voice and rate are used: the utterance keeps its default rate (the
//! person's Spoken Content settings), and its voice is left unset when the system language is the language to speak
//! (so the voice they picked in System Settings speaks); another language gets the best installed voice for it.

use objc2::rc::Retained;
use objc2_avf_audio::{
    AVSpeechBoundary, AVSpeechSynthesisVoice, AVSpeechSynthesizer, AVSpeechUtterance,
};
use objc2_foundation::NSString;

use super::speaker::{Engine, Lang};

pub struct Mac {
    synth: Retained<AVSpeechSynthesizer>,
}

pub fn available() -> bool {
    true
}

impl Mac {
    pub fn new() -> Self {
        // SAFETY: a plain allocation of the synthesizer; it lives on the speech thread that made it.
        Self {
            synth: unsafe { AVSpeechSynthesizer::new() },
        }
    }
}

/// The primary language subtag of a BCP-47 tag: `pt-BR` -> `pt`.
fn primary(tag: &str) -> &str {
    tag.split(['-', '_']).next().unwrap_or(tag)
}

impl Engine for Mac {
    fn speak(&mut self, text: &str, lang: Lang) -> Result<(), String> {
        // SAFETY: the Objective-C calls below take owned/borrowed objects that outlive them.
        unsafe {
            let utterance = AVSpeechUtterance::speechUtteranceWithString(&NSString::from_str(text));
            let system = AVSpeechSynthesisVoice::currentLanguageCode().to_string();
            if primary(&system) != primary(lang.tag()) {
                let voice = AVSpeechSynthesisVoice::voiceWithLanguage(Some(&NSString::from_str(
                    lang.tag(),
                )))
                .ok_or_else(|| format!("no voice for {}", lang.tag()))?;
                utterance.setVoice(Some(&voice));
            }
            self.synth.speakUtterance(&utterance);
        }
        Ok(())
    }

    fn stop(&mut self) {
        // SAFETY: a message to a live synthesizer.
        unsafe {
            self.synth
                .stopSpeakingAtBoundary(AVSpeechBoundary::Immediate);
        }
    }

    fn speaking(&mut self) -> bool {
        // SAFETY: a message to a live synthesizer.
        unsafe { self.synth.isSpeaking() }
    }
}

#[cfg(test)]
mod tests {
    use super::primary;

    #[test]
    fn the_primary_language_decides_whether_the_system_voice_speaks() {
        assert_eq!(primary("pt-BR"), "pt");
        assert_eq!(primary("pt_PT"), "pt");
        assert_eq!(primary("en"), "en");
    }
}
