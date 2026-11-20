//! The engines that turn audio into text (#647).
//!
//! **whisper.cpp** is the default: it runs on the crew server, as the crew user, so the audio never leaves the
//! owner's machines. whisper.cpp publishes no Linux binaries, so `atlas crew voice install` builds the pinned
//! source release with cmake (see `docs/crew.md`) into `<hostd dir>/voice/`:
//!
//! ```text
//! voice/bin/whisper-cli          built from WHISPER_SOURCE_URL, sha256 checked before it is unpacked
//! voice/models/ggml-<name>.bin   WHISPER_MODELS, sha256 checked before it is moved into place
//! ```
//!
//! **An OpenAI-compatible endpoint** is the optional hosted engine: the owner's own URL and key (a secret of the
//! crew, never in a reply). Its label says it is remote.
//!
//! Neither engine is ever a fallback for the other: an engine that fails is an error (`crew.voice.engine_failed`),
//! because a person who chose "on my machines" must never find their audio went to a cloud.

use std::path::{Path, PathBuf};
use std::time::Duration;

use atlas_core::sys::CommandExt as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::audio;

/// The whisper.cpp release `atlas crew voice install` builds, and the sha256 of its GitHub source tarball.
/// Moving to another release is one edit here (and the docs), nothing else.
pub const WHISPER_VERSION: &str = "1.9.5";
pub const WHISPER_SOURCE_URL: &str =
    "https://github.com/ggml-org/whisper.cpp/archive/refs/tags/v1.9.5.tar.gz";
pub const WHISPER_SOURCE_SHA256: &str =
    "ff1a9053feb509ff9d7729703355541ae9690073a6b1c40eb692c962e0dc1720";
/// The commit of `ggerganov/whisper.cpp` on Hugging Face the models are fetched at: a URL that names a commit
/// serves the same bytes for ever, and the hashes below say so.
pub const MODELS_REVISION: &str = "5359861c739e955e79d9a303bcbc70fb988958b1";

/// A multilingual ggml model (English and Portuguese both need it: the `.en` models do not speak Portuguese).
pub struct ModelPin {
    pub name: &'static str,
    pub file: &'static str,
    pub sha256: &'static str,
    pub bytes: u64,
}

/// `base` (about 140 MB, fast, rougher) and `small` (about 470 MB, the default).
pub const MODELS: [ModelPin; 2] = [
    ModelPin {
        name: "base",
        file: "ggml-base.bin",
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
        bytes: 147_951_465,
    },
    ModelPin {
        name: "small",
        file: "ggml-small.bin",
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        bytes: 487_601_967,
    },
];

/// The model installed when none is asked for.
pub const DEFAULT_MODEL: &str = "small";

pub fn model(name: &str) -> Option<&'static ModelPin> {
    MODELS.iter().find(|m| m.name == name)
}

pub fn model_url(m: &ModelPin) -> String {
    format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/{MODELS_REVISION}/{}",
        m.file
    )
}

/// A transcription that runs longer than this is killed: 5 minutes of audio on a slow core is minutes, not
/// a quarter hour.
const RUN_DEADLINE: Duration = Duration::from_secs(15 * 60);
/// A hosted engine that does not answer in this long has failed.
const REMOTE_DEADLINE: Duration = Duration::from_secs(120);

/// What the crew chose, as `crew.json` keeps it and `crew_voice_set` sends it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EngineSetting {
    WhisperCpp {
        model: String,
    },
    OpenaiCompatible {
        base_url: String,
        model: String,
        /// The name of the secret that holds the key. The key itself is never here.
        key_secret: String,
    },
}

impl EngineSetting {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::WhisperCpp { .. } => "whisper_cpp",
            Self::OpenaiCompatible { .. } => "openai_compatible",
        }
    }

    pub fn model_name(&self) -> &str {
        match self {
            Self::WhisperCpp { model } | Self::OpenaiCompatible { model, .. } => model,
        }
    }

    /// What the apps show next to the mic, verbatim: it names where the audio goes.
    pub fn label(&self) -> String {
        match self {
            Self::WhisperCpp { model } => format!("whisper.cpp ({model})"),
            Self::OpenaiCompatible {
                base_url, model, ..
            } => format!("OpenAI-compatible ({model} @ {})", host_of(base_url)),
        }
    }

    /// Check what the owner sent. Err is the text they are told.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::WhisperCpp { model: m } => model(m)
                .map(|_| ())
                .ok_or_else(|| format!("model must be one of: {}", model_names())),
            Self::OpenaiCompatible {
                base_url,
                model,
                key_secret,
            } => {
                if !endpoint_ok(base_url) {
                    return Err(
                        "base_url must be https (http only for this computer, 127.0.0.1 or localhost)"
                            .into(),
                    );
                }
                let named = |s: &str| {
                    !s.is_empty() && s.chars().count() <= 100 && !s.chars().any(char::is_control)
                };
                if !named(model) {
                    return Err("model must be 1 to 100 characters".into());
                }
                if !named(key_secret) {
                    return Err(
                        "key_secret must be the name of a secret, 1 to 100 characters".into(),
                    );
                }
                Ok(())
            }
        }
    }
}

fn model_names() -> String {
    MODELS.iter().map(|m| m.name).collect::<Vec<_>>().join(", ")
}

/// `https://host/...`, or `http://` to this computer only: the key goes in a header, so it never crosses a
/// network in the clear.
fn endpoint_ok(url: &str) -> bool {
    let (rest, loopback_only) = match url.strip_prefix("https://") {
        Some(r) => (r, false),
        None => match url.strip_prefix("http://") {
            Some(r) => (r, true),
            None => return false,
        },
    };
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() || host.contains('@') {
        return false;
    }
    !loopback_only || is_loopback(host)
}

fn is_loopback(host_port: &str) -> bool {
    let host = match host_port.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => host_port.rsplit_once(':').map_or(host_port, |(h, _)| h),
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

/// `api.openai.com` of `https://api.openai.com/v1`.
fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split(['/', '?', '#'])
        .next()
        .unwrap_or(rest)
        .to_string()
}

/// `<hostd dir>/voice`
pub(crate) fn voice_dir(hostd_dir: &Path) -> PathBuf {
    hostd_dir.join("voice")
}

pub(crate) fn whisper_bin(voice: &Path) -> PathBuf {
    voice.join("bin").join(bin_name("whisper-cli"))
}

pub(crate) fn model_path(voice: &Path, name: &str) -> Option<PathBuf> {
    model(name).map(|m| voice.join("models").join(m.file))
}

/// `name` or `name.exe`.
fn bin_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// `ffmpeg` for a Telegram voice note (Opus in an Ogg file, which whisper.cpp does not read): one in
/// `voice/bin/` when the owner put one there, else the server's own.
fn ffmpeg_program(voice: &Path) -> PathBuf {
    let own = voice.join("bin").join(bin_name("ffmpeg"));
    // tree-access: hostd's voice dir (`voice/bin`), which only the owner and `atlas crew voice install` write
    if own.is_file() {
        own
    } else {
        PathBuf::from("ffmpeg")
    }
}

/// What an audio file of any common format (a Telegram voice note: Opus in Ogg) becomes: 16 kHz mono s16le PCM, at
/// most `max_seconds` of it. The second value is whether the file had more than that. The file is written to a
/// scratch dir that is removed before this returns. Err is a short English line.
pub(crate) async fn decode_to_pcm(
    voice: &Path,
    bytes: &[u8],
    max_seconds: u32,
) -> Result<(Vec<u8>, bool), String> {
    let scratch = Scratch::new(voice).map_err(|e| e.to_string())?;
    let input = scratch.path().join("in.audio");
    crate::paths::write_private(&input, bytes).map_err(|e| e.to_string())?;
    // One second past the cap: if it is there, the note was longer than the cap.
    let take = max_seconds + 1;
    // tree-access: ffmpeg with hostd's own arguments, on a file hostd just wrote in its own scratch dir
    let mut cmd = tokio::process::Command::new(ffmpeg_program(voice));
    cmd.args(["-nostdin", "-v", "error", "-i"])
        .arg(&input)
        .args([
            "-vn",
            "-t",
            &take.to_string(),
            "-ar",
            "16000",
            "-ac",
            "1",
            "-f",
            "s16le",
            "-",
        ])
        .no_window();
    let out = crate::crew::proc::run_bounded(&mut cmd, "ffmpeg", Duration::from_secs(120)).await?;
    if !out.status.success() {
        return Err(short(&String::from_utf8_lossy(&out.stderr)));
    }
    let mut pcm = out.stdout;
    if pcm.len() % 2 == 1 {
        pcm.pop();
    }
    let cap = max_seconds as usize * audio::BYTES_PER_SECOND;
    let longer = pcm.len() > cap;
    pcm.truncate(cap);
    Ok((pcm, longer))
}

/// The engine of a crew that never chose: whisper.cpp with the best model that is installed here. `None` when
/// the binary or every model is missing.
pub(crate) fn installed_default(voice: &Path) -> Option<EngineSetting> {
    // tree-access: the engine hostd installed in its own voice dir
    if !whisper_bin(voice).is_file() {
        return None;
    }
    [DEFAULT_MODEL, "base"].into_iter().find_map(|m| {
        model_path(voice, m)
            // tree-access: a model file in hostd's own voice dir
            .filter(|p| p.is_file())
            .map(|_| EngineSetting::WhisperCpp { model: m.into() })
    })
}

/// What `crew_voice_status.engine` says.
pub(crate) struct EngineStatus {
    pub setting: EngineSetting,
    pub ready: bool,
    pub detail: Option<String>,
}

impl EngineStatus {
    pub fn json(&self) -> Value {
        json!({
            "kind": self.setting.kind(),
            "label": self.setting.label(),
            "model": self.setting.model_name(),
            "ready": self.ready,
            "detail": self.detail,
        })
    }
}

/// Whether `setting` can run right now, and if not, why (a short English line for logs). `has_key` says whether
/// a hosted engine's secret is set.
pub(crate) fn status(voice: &Path, setting: EngineSetting, has_key: bool) -> EngineStatus {
    let detail = match &setting {
        EngineSetting::WhisperCpp { model } => {
            // tree-access: the engine and its models in hostd's own voice dir
            if !whisper_bin(voice).is_file() {
                Some(
                    "whisper.cpp is not installed on this server (run: atlas crew voice install)"
                        .to_string(),
                )
            // tree-access: a model file in hostd's own voice dir
            } else if !model_path(voice, model).is_some_and(|p| p.is_file()) {
                Some(format!(
                    "the {model} model is not installed on this server (run: atlas crew voice install --model {model})"
                ))
            } else {
                None
            }
        }
        EngineSetting::OpenaiCompatible { key_secret, .. } => {
            (!has_key).then(|| format!("the secret {key_secret} is not set"))
        }
    };
    EngineStatus {
        ready: detail.is_none(),
        detail,
        setting,
    }
}

/// What an engine heard.
pub(crate) struct Heard {
    pub text: String,
    /// The language it recognised (or was told), `en`, `pt`, ...
    pub language: Option<String>,
}

/// An engine that ran and failed, or could not run. `detail` is a short English line.
#[derive(Debug)]
pub(crate) enum Failure {
    /// Nothing is installed or configured to run (`crew.voice.not_installed`).
    NotInstalled,
    /// It ran and failed (`crew.voice.engine_failed`).
    Failed(String),
}

fn failed(e: impl std::fmt::Display) -> Failure {
    Failure::Failed(short(&e.to_string()))
}

/// A detail that stays one short line of the engine's own words.
fn short(s: &str) -> String {
    let line = s.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
    line.chars().filter(|c| !c.is_control()).take(200).collect()
}

/// Run `setting` over `pcm` (16 kHz mono s16le). `language` is `en`/`pt` or `None` to detect; `prompt` the hints.
/// `key` is the hosted engine's secret. The audio is written to a scratch dir that is removed whatever happens.
pub(crate) async fn transcribe(
    voice: &Path,
    setting: &EngineSetting,
    key: Option<&str>,
    pcm: &[u8],
    language: Option<&str>,
    prompt: &str,
) -> Result<Heard, Failure> {
    match setting {
        EngineSetting::WhisperCpp { model } => {
            let bin = whisper_bin(voice);
            let weights = model_path(voice, model).ok_or(Failure::NotInstalled)?;
            // tree-access: the engine and the model hostd installed in its own voice dir
            if !bin.is_file() || !weights.is_file() {
                return Err(Failure::NotInstalled);
            }
            run_whisper(voice, &bin, &weights, pcm, language, prompt).await
        }
        EngineSetting::OpenaiCompatible {
            base_url, model, ..
        } => {
            let key = key.ok_or(Failure::NotInstalled)?;
            run_remote(base_url, model, key, pcm, language, prompt).await
        }
    }
}

/// A scratch dir that is removed when it goes out of scope: success, failure and a dropped future alike.
pub(crate) struct Scratch(PathBuf);

impl Scratch {
    pub fn new(voice: &Path) -> std::io::Result<Self> {
        let id = crate::paths::random_hex(8)?;
        let dir = voice.join("tmp").join(id);
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
        std::fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn run_whisper(
    voice: &Path,
    bin: &Path,
    weights: &Path,
    pcm: &[u8],
    language: Option<&str>,
    prompt: &str,
) -> Result<Heard, Failure> {
    let scratch = Scratch::new(voice).map_err(failed)?;
    let input = scratch.path().join("in.wav");
    let out = scratch.path().join("out");
    crate::paths::write_private(&input, &audio::wav(pcm)).map_err(failed)?;
    // tree-access: whisper-cli, the engine hostd installed itself, with hostd's own arguments
    let mut cmd = tokio::process::Command::new(bin);
    cmd.arg("-m")
        .arg(weights)
        .arg("-f")
        .arg(&input)
        .args(["-l", language.unwrap_or("auto"), "-nt", "-np", "-oj", "-of"])
        .arg(&out)
        .no_window();
    if !prompt.is_empty() {
        cmd.arg("--prompt").arg(prompt);
    }
    let output = crate::crew::proc::run_bounded(&mut cmd, "whisper-cli", RUN_DEADLINE)
        .await
        .map_err(failed)?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(failed(if err.trim().is_empty() {
            format!("whisper-cli exited with {}", output.status)
        } else {
            err.into_owned()
        }));
    }
    // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
    let raw = std::fs::read(out.with_extension("json"))
        .map_err(|_| failed("whisper-cli wrote no result"))?;
    parse_whisper(&raw).map_err(failed)
}

/// whisper-cli's `-oj` file: `{"result": {"language": "en"}, "transcription": [{"text": " ..."}]}`.
pub(crate) fn parse_whisper(raw: &[u8]) -> Result<Heard, String> {
    let v: Value = serde_json::from_slice(raw).map_err(|e| format!("whisper-cli result: {e}"))?;
    let segments = v["transcription"]
        .as_array()
        .ok_or("whisper-cli result has no transcription")?;
    let text = clean(
        segments
            .iter()
            .filter_map(|s| s["text"].as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .as_str(),
    );
    Ok(Heard {
        text,
        language: v["result"]["language"].as_str().map(str::to_lowercase),
    })
}

/// The text without the engine's own markers (`[BLANK_AUDIO]`, `(music)`, `[ Silence ]`) and with single spaces.
/// A marker stands on its own between spaces: `a[N]` and `f(X)` are code a person dictated, and stay.
pub(crate) fn clean(text: &str) -> String {
    let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut kept = String::new();
    let mut rest = joined.as_str();
    let mut at_word_start = true;
    while let Some(start) = rest.find(['[', '(']) {
        let word_start = if start == 0 {
            at_word_start
        } else {
            rest[..start].ends_with(' ')
        };
        let close = if rest[start..].starts_with('[') {
            ']'
        } else {
            ')'
        };
        let end = rest[start..].find(close).map(|e| start + e);
        let alone = end.is_some_and(|e| rest[e + 1..].is_empty() || rest[e + 1..].starts_with(' '));
        match end {
            Some(e) if word_start && alone && is_marker(&rest[start + 1..e]) => {
                kept.push_str(&rest[..start]);
                rest = &rest[e + 1..];
                at_word_start = true;
            }
            _ => {
                kept.push_str(&rest[..start + 1]);
                rest = &rest[start + 1..];
                at_word_start = false;
            }
        }
    }
    kept.push_str(rest);
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `BLANK_AUDIO`, `MUSIC`, `Music`, `inaudible`: a known word, or capital letters and underscores of four or more.
fn is_marker(inner: &str) -> bool {
    let t = inner.trim();
    matches!(
        t.to_lowercase().as_str(),
        "music" | "silence" | "inaudible" | "noise" | "applause" | "laughter" | "blank_audio"
    ) || (t.len() >= 4 && t.chars().all(|c| c.is_ascii_uppercase() || c == '_'))
}

// ── hosted ───────────────────────────────────────────────────────────────────

/// A `multipart/form-data` body of text fields and one file.
fn multipart(boundary: &str, fields: &[(&str, &str)], file: (&str, &str, &[u8])) -> Vec<u8> {
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }
    let (field, filename, bytes) = file;
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"{filename}\"\r\nContent-Type: audio/wav\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

async fn run_remote(
    base_url: &str,
    model: &str,
    key: &str,
    pcm: &[u8],
    language: Option<&str>,
    prompt: &str,
) -> Result<Heard, Failure> {
    let client = crate::crew::push::HTTP
        .as_ref()
        .ok_or_else(|| failed("no http client"))?;
    let boundary = format!("atlas{}", crate::paths::random_hex(8).map_err(failed)?);
    let mut fields = vec![("model", model), ("response_format", "verbose_json")];
    if let Some(l) = language {
        fields.push(("language", l));
    }
    if !prompt.is_empty() {
        fields.push(("prompt", prompt));
    }
    let body = multipart(&boundary, &fields, ("file", "audio.wav", &audio::wav(pcm)));
    let url = format!("{}/audio/transcriptions", base_url.trim_end_matches('/'));
    let resp = client
        .post(url)
        .bearer_auth(key)
        .header(
            "content-type",
            format!("multipart/form-data; boundary={boundary}"),
        )
        .timeout(REMOTE_DEADLINE)
        .body(body)
        .send()
        .await
        // The error text of a failed request names the URL, never the key (it is in a header).
        .map_err(|e| {
            failed(format!(
                "the endpoint could not be reached: {}",
                e.without_url()
            ))
        })?;
    let status = resp.status();
    if !status.is_success() {
        // The body is not shown: a provider's error text can echo part of the key.
        return Err(failed(format!(
            "the endpoint answered HTTP {}",
            status.as_u16()
        )));
    }
    let v: Value = resp
        .json()
        .await
        .map_err(|_| failed("the endpoint did not answer with JSON"))?;
    let text = v["text"]
        .as_str()
        .ok_or_else(|| failed("the endpoint's answer has no text"))?;
    Ok(Heard {
        text: clean(text),
        language: v["language"].as_str().map(language_code),
    })
}

/// OpenAI's `verbose_json` names the language in English ("portuguese"); the contract speaks codes.
fn language_code(name: &str) -> String {
    match name.to_lowercase().as_str() {
        "english" => "en".into(),
        "portuguese" => "pt".into(),
        other => other.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_models_are_the_multilingual_ones_with_pinned_hashes() {
        assert_eq!(DEFAULT_MODEL, "small");
        for m in &MODELS {
            assert!(
                !m.file.contains(".en."),
                "{}: Portuguese needs the multilingual model",
                m.file
            );
            assert_eq!(m.sha256.len(), 64);
            assert!(
                m.sha256
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            );
            let url = model_url(m);
            assert!(
                url.starts_with("https://") && url.contains(MODELS_REVISION),
                "{url}"
            );
            assert!(!url.contains("/main/"), "a branch name is not a pin: {url}");
        }
        assert_eq!(WHISPER_SOURCE_SHA256.len(), 64);
        assert!(WHISPER_SOURCE_URL.contains(&format!("v{WHISPER_VERSION}")));
    }

    #[test]
    fn the_label_names_where_the_audio_goes() {
        let w = EngineSetting::WhisperCpp {
            model: "small".into(),
        };
        assert_eq!(w.label(), "whisper.cpp (small)");
        let o = EngineSetting::OpenaiCompatible {
            base_url: "https://api.openai.com/v1".into(),
            model: "whisper-1".into(),
            key_secret: "OPENAI_KEY".into(),
        };
        assert_eq!(o.label(), "OpenAI-compatible (whisper-1 @ api.openai.com)");
    }

    // The key goes in a header: only https, or a server on this very computer.
    #[test]
    fn a_hosted_endpoint_is_https_or_loopback() {
        for ok in [
            "https://api.openai.com/v1",
            "https://stt.example.org:8443/v1",
            "http://127.0.0.1:9000/v1",
            "http://localhost/v1",
            "http://[::1]:8080/v1",
        ] {
            assert!(endpoint_ok(ok), "{ok}");
        }
        for bad in [
            "http://api.openai.com/v1",
            "http://10.0.0.5/v1",
            "ftp://x/y",
            "https://",
            "https://user:pw@host/v1",
            "https://a b/v1",
            "api.openai.com/v1",
            "http://localhost.evil.com/v1",
            "http://127.0.0.1.evil.com/v1",
        ] {
            assert!(!endpoint_ok(bad), "{bad}");
        }
    }

    #[test]
    fn settings_are_validated() {
        assert!(
            EngineSetting::WhisperCpp {
                model: "base".into()
            }
            .validate()
            .is_ok()
        );
        assert!(
            EngineSetting::WhisperCpp {
                model: "large".into()
            }
            .validate()
            .is_err()
        );
        assert!(
            EngineSetting::WhisperCpp {
                model: "small.en".into()
            }
            .validate()
            .is_err()
        );
        let o = |url: &str, model: &str, key: &str| EngineSetting::OpenaiCompatible {
            base_url: url.into(),
            model: model.into(),
            key_secret: key.into(),
        };
        assert!(o("https://x.io/v1", "whisper-1", "K").validate().is_ok());
        assert!(o("http://x.io/v1", "whisper-1", "K").validate().is_err());
        assert!(o("https://x.io/v1", "", "K").validate().is_err());
        assert!(o("https://x.io/v1", "whisper-1", "").validate().is_err());
    }

    // The JSON of the real whisper-cli 1.9.5 (`-oj`), cut to what it carries.
    #[test]
    fn whisper_clis_json_is_read() {
        let raw = br#"{"systeminfo":"x","result":{"language":"en"},"transcription":[
            {"timestamps":{"from":"00:00:00,000","to":"00:00:30,000"},"offsets":{"from":0,"to":30000},
             "text":" OpenSource/main.rs and rename foo_bar."}]}"#;
        let h = parse_whisper(raw).unwrap();
        assert_eq!(h.text, "OpenSource/main.rs and rename foo_bar.");
        assert_eq!(h.language.as_deref(), Some("en"));
        assert!(parse_whisper(b"{}").is_err());
        assert!(parse_whisper(b"not json").is_err());
    }

    #[test]
    fn the_engines_markers_are_not_text() {
        assert_eq!(clean(" [BLANK_AUDIO]"), "");
        assert_eq!(clean("(music) hello [ Silence ] world"), "hello world");
        assert_eq!(clean("  rename  the\nfunction "), "rename the function");
        // Brackets in speech stay.
        assert_eq!(clean("use a[0] and f(x) here"), "use a[0] and f(x) here");
        assert_eq!(
            clean("segment one [BLANK_AUDIO] segment two"),
            "segment one segment two"
        );
    }

    #[test]
    fn a_detail_is_one_short_line() {
        let long = format!("first line\n{}\n", "x".repeat(500));
        assert_eq!(short(&long).len(), 200);
        assert_eq!(short("a\nlast words\n\n"), "last words");
        assert_eq!(short("bad\u{7}byte"), "badbyte");
    }

    #[test]
    fn the_status_says_why_an_engine_is_not_ready() {
        let tmp = tempfile::tempdir().unwrap();
        let s = status(
            tmp.path(),
            EngineSetting::WhisperCpp {
                model: "small".into(),
            },
            false,
        );
        assert!(!s.ready);
        assert!(s.detail.unwrap().contains("not installed"));
        std::fs::create_dir_all(tmp.path().join("bin")).unwrap();
        std::fs::write(whisper_bin(tmp.path()), b"").unwrap();
        let s = status(
            tmp.path(),
            EngineSetting::WhisperCpp {
                model: "small".into(),
            },
            false,
        );
        assert!(s.detail.unwrap().contains("small model"));
        std::fs::create_dir_all(tmp.path().join("models")).unwrap();
        std::fs::write(model_path(tmp.path(), "small").unwrap(), b"").unwrap();
        assert!(
            status(
                tmp.path(),
                EngineSetting::WhisperCpp {
                    model: "small".into()
                },
                false
            )
            .ready
        );
        // Without a model chosen, the best installed one is the engine.
        assert_eq!(
            installed_default(tmp.path()),
            Some(EngineSetting::WhisperCpp {
                model: "small".into()
            })
        );
        std::fs::remove_file(model_path(tmp.path(), "small").unwrap()).unwrap();
        assert_eq!(installed_default(tmp.path()), None);
        std::fs::write(model_path(tmp.path(), "base").unwrap(), b"").unwrap();
        assert_eq!(
            installed_default(tmp.path()),
            Some(EngineSetting::WhisperCpp {
                model: "base".into()
            })
        );
        // The hosted engine needs its secret.
        let o = EngineSetting::OpenaiCompatible {
            base_url: "https://x.io".into(),
            model: "m".into(),
            key_secret: "K".into(),
        };
        assert!(!status(tmp.path(), o.clone(), false).ready);
        assert!(status(tmp.path(), o, true).ready);
    }

    #[test]
    fn the_status_json_is_the_contracts() {
        let tmp = tempfile::tempdir().unwrap();
        let v = status(
            tmp.path(),
            EngineSetting::WhisperCpp {
                model: "base".into(),
            },
            false,
        )
        .json();
        assert_eq!(v["kind"], "whisper_cpp");
        assert_eq!(v["label"], "whisper.cpp (base)");
        assert_eq!(v["model"], "base");
        assert_eq!(v["ready"], false);
        assert!(v["detail"].is_string());
    }

    #[test]
    fn the_hosted_body_is_a_multipart_with_the_audio() {
        let body = multipart(
            "B",
            &[("model", "whisper-1"), ("language", "pt")],
            ("file", "audio.wav", b"RIFFdata"),
        );
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("name=\"model\"\r\n\r\nwhisper-1\r\n"));
        assert!(text.contains("name=\"language\"\r\n\r\npt\r\n"));
        assert!(text.contains("name=\"file\"; filename=\"audio.wav\""));
        assert!(text.ends_with("\r\n--B--\r\n"));
        assert!(body.windows(8).any(|w| w == b"RIFFdata"));
    }

    #[test]
    fn openai_names_languages_in_english() {
        assert_eq!(language_code("Portuguese"), "pt");
        assert_eq!(language_code("english"), "en");
        assert_eq!(language_code("german"), "german");
    }
}
