//! Dictation on the crew server (#647): the commands `crew_voice_status`, `crew_voice_set`,
//! `crewmate_voice_put` and `crewmate_transcribe`, and the one door every way in goes through ([`run`]): the
//! apps' uploads and a Telegram voice note are held to the same rules.
//!
//! | module   | what it owns                                                                          |
//! |----------|---------------------------------------------------------------------------------------|
//! | `audio`  | the upload: format, the 300 s cap, the 10 min TTL, deletion                            |
//! | `engine` | whisper.cpp (pinned, built by `atlas crew voice install`) and the hosted endpoint      |
//! | `vocab`  | hints for the engine and the replacements after it                                     |
//!
//! The principles of the issue, each held here and tested:
//! - dictation only fills a composer: nothing here creates a task (Telegram's Send does, on a tap);
//! - the engine in use is always in the reply, and a failed engine is `crew.voice.engine_failed`: no other engine
//!   is ever tried;
//! - the audio is deleted after the transcription, on failure, and when the upload is abandoned.

pub(crate) mod audio;
pub(crate) mod engine;
pub(crate) mod vocab;

use std::time::SystemTime;

use atlas_core::progress::Coded;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::protocol::{Failure, HostCmd, invalid_argument};
use crate::server::Hub;

pub use engine::{
    DEFAULT_MODEL, EngineSetting, MODELS, MODELS_REVISION, ModelPin, WHISPER_SOURCE_SHA256,
    WHISPER_SOURCE_URL, WHISPER_VERSION, model, model_url,
};
pub use vocab::Replacement;

pub const OFF: &str = "crew.voice.off";
pub const NOT_INSTALLED: &str = "crew.voice.not_installed";
pub const ENGINE_FAILED: &str = "crew.voice.engine_failed";
pub const TOO_LONG: &str = "crew.voice.too_long";
pub const BAD_AUDIO: &str = "crew.voice.bad_audio";
pub const FORBIDDEN: &str = "crew.voice.forbidden";

/// What the crew chose about voice, kept in `crew.json`. Off until the owner turns it on.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VoiceSettings {
    /// The crew-wide switch: off, every transcription is refused.
    #[serde(default)]
    pub enabled: bool,
    /// `None`: whisper.cpp with the best model installed here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<EngineSetting>,
    /// The crew's own rows of the replacement table (the defaults are never stored).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replacements: Vec<Replacement>,
}

/// The line the agent gets with a dictated prompt. Ours, so it goes before the task text, and never shown as the
/// user's words: it is built at start from `Task::dictated`, not stored in the prompt.
pub const NOTE: &str =
    "(This request was dictated by voice; expect homophones in identifiers and file names.)";

/// [`NOTE`] and a blank line for a dictated task, nothing for any other.
pub(crate) fn prompt_note(task: &super::store::Task) -> String {
    if task.dictated {
        format!("{NOTE}\n\n")
    } else {
        String::new()
    }
}

// ── coded failures ───────────────────────────────────────────────────────────

/// `crew.voice.off`: the crew-wide switch is off.
pub(crate) fn off() -> Coded {
    Coded::new(OFF, json!({}), "voice is turned off on this crew")
}

/// `crew.voice.not_installed`: no engine is configured or installed.
pub(crate) fn not_installed() -> Coded {
    Coded::new(
        NOT_INSTALLED,
        json!({}),
        "no voice engine is installed on this crew server (atlas crew voice install)",
    )
}

/// `crew.voice.engine_failed`: the engine ran and failed. It is never retried on another engine.
pub(crate) fn engine_failed(engine: &str, detail: &str) -> Coded {
    Coded::new(
        ENGINE_FAILED,
        json!({"engine": engine, "detail": detail}),
        format!("{engine} failed: {detail}"),
    )
}

/// `crew.voice.too_long`: over the cap.
pub(crate) fn too_long() -> Coded {
    Coded::new(
        TOO_LONG,
        json!({"max_seconds": audio::MAX_SECONDS}),
        format!("dictation is capped at {} seconds", audio::MAX_SECONDS),
    )
}

/// `crew.voice.bad_audio`: not the declared format.
pub(crate) fn bad_audio() -> Coded {
    Coded::new(
        BAD_AUDIO,
        json!({}),
        "the audio is not 16 kHz mono 16-bit PCM WAV",
    )
}

/// `crew.voice.forbidden`: a non-owner changing the voice settings.
pub(crate) fn forbidden() -> Coded {
    Coded::new(
        FORBIDDEN,
        json!({}),
        "only the owner can change the voice settings",
    )
}

// ── status and set ───────────────────────────────────────────────────────────

/// The value of the secret `name`: the dictating crewmate's own first, then any crewmate's (the crew's
/// secrets are its crewmates' secrets: there is no other store).
fn find_secret(hub: &Hub, name: &str, prefer: Option<&str>) -> Option<String> {
    let store = &hub.crew.store;
    let own = prefer.and_then(|id| store.secret_env(id).remove(name));
    own.or_else(|| {
        store
            .list_crewmates()
            .iter()
            .find_map(|c| store.secret_env(&c.id).remove(name))
    })
    .filter(|v| !v.is_empty())
}

/// The engine this crew uses now: what it chose, else whisper.cpp with the best model installed here.
fn effective_engine(hub: &Hub, settings: &VoiceSettings) -> Option<EngineSetting> {
    settings
        .engine
        .clone()
        .or_else(|| engine::installed_default(&engine::voice_dir(&hub.crew.dir)))
}

/// `crew_voice_status`: the reply of `crew_voice_status` and of `crew_voice_set`.
pub(crate) fn status(hub: &Hub) -> Value {
    let settings = hub.crew.store.load_crew_settings().voice;
    let voice = engine::voice_dir(&hub.crew.dir);
    let engine = effective_engine(hub, &settings).map(|e| {
        let has_key = match &e {
            EngineSetting::OpenaiCompatible { key_secret, .. } => {
                find_secret(hub, key_secret, None).is_some()
            }
            EngineSetting::WhisperCpp { .. } => false,
        };
        engine::status(&voice, e, has_key).json()
    });
    json!({
        "enabled": settings.enabled,
        "engine": engine,
        "max_seconds": audio::MAX_SECONDS,
        "audio": {
            "format": "wav_pcm_s16le",
            "sample_rate": audio::SAMPLE_RATE,
            "channels": audio::CHANNELS,
        },
        "languages": ["en", "pt"],
        "replacements": vocab::effective(&settings.replacements),
    })
}

/// Only the owner changes the voice settings: the desktop (or the CLI) and the owner's Telegram chat. A phone, a
/// crewmate's process, anything else named on the connection is refused.
pub(crate) fn is_owner(origin: &str) -> bool {
    origin == "desktop" || origin.starts_with("telegram:")
}

/// `crew_voice_set`.
fn set(
    hub: &Hub,
    origin: &str,
    enabled: Option<bool>,
    engine: Option<EngineSetting>,
    replacements: Option<Vec<Replacement>>,
) -> Result<Value, Failure> {
    if !is_owner(origin) {
        return Err(forbidden().into());
    }
    if let Some(e) = &engine {
        e.validate()
            .map_err(|why| invalid_argument("engine", "invalid", why))?;
    }
    if let Some(rows) = &replacements {
        vocab::validate(rows).map_err(|why| invalid_argument("replacements", "invalid", why))?;
    }
    // Load, change, save: the file also holds `paused`, which this command must never reset.
    let mut crew = hub.crew.store.load_crew_settings();
    if let Some(on) = enabled {
        crew.voice.enabled = on;
    }
    if let Some(e) = engine {
        crew.voice.engine = Some(e);
    }
    if let Some(rows) = replacements {
        crew.voice.replacements = rows;
    }
    hub.crew
        .store
        .save_crew_settings(&crew)
        .map_err(|e| format!("save_crew_settings: {e}"))?;
    // What changed, never a key: the secret's name only.
    hub.crew.audit.record(
        None,
        "config_changed",
        json!({
            "field": "voice",
            "enabled": crew.voice.enabled,
            "engine": crew.voice.engine,
            "replacements": crew.voice.replacements.len(),
            "by": origin,
        }),
    );
    Ok(status(hub))
}

// ── transcription ────────────────────────────────────────────────────────────

/// What a transcription returns: the contract's `Transcript`.
pub(crate) struct Transcript {
    pub text: String,
    pub raw: String,
    pub kind: &'static str,
    pub label: String,
    pub language: String,
    pub duration_ms: u64,
    pub partial: bool,
}

impl Transcript {
    pub fn json(&self) -> Value {
        json!({
            "text": self.text,
            "raw": self.raw,
            "engine": {"kind": self.kind, "label": self.label},
            "language": self.language,
            "duration_ms": self.duration_ms,
            "partial": self.partial,
        })
    }
}

/// One transcription at a time: the engine uses every core, and two at once would each take twice as long.
static ENGINE: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

/// Audio shorter than this is silence, not speech: no engine is run for it (a tap on the mic).
const MIN_PCM: usize = audio::BYTES_PER_SECOND / 10;

/// Transcribe `pcm` (16 kHz mono s16le) for `crewmate_id`. The one door: the apps' uploads and Telegram both come
/// here, so the switch, the engine, the hints and the replacements are the same for both.
///
/// `language` is `en`/`pt`, or `None` to detect. With `partial` an engine that is busy is not waited for (the
/// dimmed preview is best effort; a final transcription queues).
pub(crate) async fn run(
    hub: &Hub,
    crewmate_id: &str,
    pcm: &[u8],
    language: Option<&str>,
    partial: bool,
) -> Result<Transcript, Failure> {
    let settings = hub.crew.store.load_crew_settings().voice;
    if !settings.enabled {
        return Err(off().into());
    }
    let setting = effective_engine(hub, &settings).ok_or_else(|| Failure::from(not_installed()))?;
    let label = setting.label();
    let key = match &setting {
        EngineSetting::OpenaiCompatible { key_secret, .. } => Some(
            find_secret(hub, key_secret, Some(crewmate_id))
                .ok_or_else(|| Failure::from(not_installed()))?,
        ),
        EngineSetting::WhisperCpp { .. } => None,
    };
    let done = |raw: String, heard_language: Option<String>| {
        let table = vocab::effective(&settings.replacements);
        Transcript {
            text: vocab::apply(&raw, &table, language),
            raw,
            kind: setting.kind(),
            label: label.clone(),
            language: heard_language
                .or_else(|| language.map(str::to_string))
                .unwrap_or_else(|| "auto".into()),
            duration_ms: audio::duration_ms(pcm.len()),
            partial,
        }
    };
    if pcm.len() < MIN_PCM {
        return Ok(done(String::new(), None));
    }
    let _turn = if partial {
        ENGINE
            .try_acquire()
            .map_err(|_| Failure::from("the voice engine is busy"))?
    } else {
        ENGINE
            .acquire()
            .await
            .map_err(|_| Failure::from("the voice engine is stopped"))?
    };
    let hints = vocab::hints(&hub.crew.store, crewmate_id);
    let voice = engine::voice_dir(&hub.crew.dir);
    match engine::transcribe(&voice, &setting, key.as_deref(), pcm, language, &hints).await {
        Ok(heard) => Ok(done(heard.text, heard.language)),
        Err(engine::Failure::NotInstalled) => Err(not_installed().into()),
        // Never another engine: the person chose where their audio goes.
        Err(engine::Failure::Failed(detail)) => Err(engine_failed(&label, &detail).into()),
    }
}

fn parse_language(language: Option<&str>) -> Result<Option<&'static str>, Failure> {
    match language {
        None | Some("auto") => Ok(None),
        Some("en") => Ok(Some("en")),
        Some("pt") => Ok(Some("pt")),
        Some(_) => {
            Err(invalid_argument("language", "invalid", "language is en, pt or auto").into())
        }
    }
}

/// `crewmate_voice_put`.
fn put(hub: &Hub, upload: &str, piece_b64: &str, last: bool) -> Result<Value, Failure> {
    if upload.is_empty() {
        // Opening one: refuse now what every transcription would refuse, before the audio is uploaded.
        let settings = hub.crew.store.load_crew_settings().voice;
        if !settings.enabled {
            return Err(off().into());
        }
        if effective_engine(hub, &settings).is_none() {
            return Err(not_installed().into());
        }
    }
    let voice = engine::voice_dir(&hub.crew.dir);
    match audio::put_piece(&voice, upload, piece_b64, last, SystemTime::now()) {
        Ok((id, bytes)) => Ok(json!({"upload": id, "bytes": bytes})),
        Err(audio::Refused::BadAudio) => Err(bad_audio().into()),
        Err(audio::Refused::TooLong) => Err(too_long().into()),
        Err(audio::Refused::Other(e)) => Err(e.into()),
    }
}

/// `crewmate_transcribe`.
async fn transcribe(
    hub: &Hub,
    id: &str,
    upload: &str,
    language: Option<&str>,
    partial: bool,
) -> Result<Value, Failure> {
    let language = parse_language(language)?;
    hub.crew
        .store
        .load_crewmate(id)
        .map_err(|_| format!("crewmate {id} not found"))?;
    let voice = engine::voice_dir(&hub.crew.dir);
    let mut rec = audio::Recording::open(&voice, upload, SystemTime::now())?;
    if !partial {
        if !rec.complete {
            return Err(
                "the upload is not complete (send the last piece, or ask for a partial)".into(),
            );
        }
        // From here the audio is deleted whatever happens: the answer, a failure, or a client that went away.
        rec.arm();
    }
    let window = partial.then_some(audio::PARTIAL_WINDOW_S);
    let pcm = rec.pcm(window).map_err(|e| format!("upload: {e}"))?;
    let out = run(hub, id, &pcm, language, partial).await;
    drop(rec);
    out.map(|t| t.json())
}

/// The voice commands. `origin` is who is on the connection (`Hub::origin_of`).
pub(crate) async fn handle(hub: &Hub, cmd: HostCmd, origin: &str) -> Result<Value, Failure> {
    if !hub.crew_host {
        return Err("this computer is not a crew server (atlas crew install)".into());
    }
    match cmd {
        HostCmd::CrewVoiceStatus {} => Ok(status(hub)),
        HostCmd::CrewVoiceSet {
            enabled,
            engine,
            replacements,
        } => set(hub, origin, enabled, engine, replacements),
        HostCmd::CrewmateVoicePut {
            upload,
            piece_b64,
            last,
        } => put(hub, &upload, &piece_b64, last),
        HostCmd::CrewmateTranscribe {
            id,
            upload,
            language,
            partial,
        } => transcribe(hub, &id, &upload, language.as_deref(), partial).await,
        _ => Err("not a voice command".into()),
    }
}

/// Run when the crew server starts.
pub(crate) fn sweep_at_start(hostd_dir: &std::path::Path) {
    audio::sweep_at_start(&engine::voice_dir(hostd_dir));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crew::store::{CrewStore, Crewmate, Mode, Task};

    fn crewmate(
        store: &CrewStore,
        id: &str,
        name: &str,
        project: Option<crate::crew::store::ProjectRef>,
    ) {
        let c: Crewmate = serde_json::from_value(json!({
            "id": id, "name": name, "role": "r", "agent": "claude", "mode": Mode::Ask,
            "created_at": 1, "project": project,
        }))
        .unwrap();
        store.save_crewmate(&c).unwrap();
    }

    fn task(store: &CrewStore, id: &str, prompt: &str, result: Option<&str>) {
        let mut t = Task::new("test", prompt);
        t.result = result.map(str::to_string);
        store.append_task(id, &t).unwrap();
    }

    use super::vocab::hints;

    // The hints are what the engine is told to expect: this work's repo, branch, the crew's names, and the files and
    // symbols of this crewmate's newest task.
    #[test]
    fn hints_name_the_repo_the_branch_the_crew_and_the_tasks_symbols() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CrewStore::new(tmp.path());
        crewmate(
            &store,
            "eyes",
            "Eyes",
            Some(crate::crew::store::ProjectRef::Git {
                url: "https://github.com/codeatlasdev/atlas.git".into(),
                branch: Some("feat/crew-voice".into()),
            }),
        );
        crewmate(&store, "ana", "Ana", None);
        task(
            &store,
            "eyes",
            "Fix `parse_config` in src/config/mod.rs",
            Some("renamed FooBar"),
        );
        let h = hints(&store, "eyes");
        for want in [
            "atlas",
            "branch feat/crew-voice",
            "Eyes, Ana",
            "parse_config",
            "src/config/mod.rs",
            "FooBar",
        ] {
            assert!(h.contains(want), "{want} missing from {h:?}");
        }
        assert!(
            !h.contains("github.com") && !h.contains(".git"),
            "the repo name, not its URL: {h:?}"
        );
        assert!(h.chars().count() <= vocab::MAX_HINT_CHARS);
    }

    // Another crewmate's tasks are somebody else's conversation: only its NAME (a word the owner says aloud) is a
    // hint, never a file, a symbol or a word of what it was asked.
    #[test]
    fn hints_never_carry_another_crewmates_task_text() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CrewStore::new(tmp.path());
        crewmate(&store, "eyes", "Eyes", None);
        crewmate(&store, "ana", "Ana", None);
        task(
            &store,
            "ana",
            "Rotate the key in secret_vault.rs for the Initech account",
            Some("done in billing/keys.rs"),
        );
        task(&store, "eyes", "Look at layout.css", None);
        let h = hints(&store, "eyes");
        assert!(h.contains("layout.css"), "{h:?}");
        for leaked in ["secret_vault", "Initech", "billing", "keys.rs"] {
            assert!(
                !h.contains(leaked),
                "{leaked} of Ana's task leaked into Eyes' hints: {h:?}"
            );
        }
        // And the other way round: Ana's own hints have Ana's task.
        let a = hints(&store, "ana");
        assert!(
            a.contains("secret_vault.rs") && !a.contains("layout.css"),
            "{a:?}"
        );
    }

    #[test]
    fn a_crewmate_with_nothing_yet_has_only_the_crews_names_and_an_unknown_one_has_none() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CrewStore::new(tmp.path());
        assert_eq!(hints(&store, "nobody"), "");
        crewmate(&store, "eyes", "Eyes", None);
        assert_eq!(hints(&store, "eyes"), "crewmates Eyes");
    }

    // A name is the owner's text and ends up in the engine's prompt: control characters never do.
    #[test]
    fn a_hostile_name_cannot_put_control_characters_in_the_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CrewStore::new(tmp.path());
        crewmate(&store, "eyes", "Ey\nes\u{7}", None);
        let h = hints(&store, "eyes");
        assert!(!h.chars().any(char::is_control), "{h:?}");
    }

    #[test]
    fn every_failure_code_is_a_reply_code_with_the_args_the_contract_names() {
        for (c, args) in [
            (off(), vec![]),
            (not_installed(), vec![]),
            (
                engine_failed("whisper.cpp (small)", "boom"),
                vec!["engine", "detail"],
            ),
            (too_long(), vec!["max_seconds"]),
            (bad_audio(), vec![]),
            (forbidden(), vec![]),
        ] {
            assert!(crate::protocol::REPLY_CODES.contains(&c.code), "{}", c.code);
            let mut got: Vec<_> = c.args.keys().cloned().collect();
            got.sort();
            let mut want: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            want.sort();
            assert_eq!(got, want, "{}", c.code);
        }
        assert_eq!(too_long().args["max_seconds"], 300);
    }
}
