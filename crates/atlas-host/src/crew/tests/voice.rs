//! Dictation (#647) against a real hostd: `crew_voice_*`, `crewmate_voice_put`, `crewmate_transcribe`, the
//! engines (a fake `whisper-cli` and a fake hosted endpoint, both recording what ran), the audio's lifetime, the
//! Telegram voice note, and the contract fixtures both apps read.
//!
//! The rules themselves are unit-tested in `voice/{audio,vocab,engine}.rs`; these check that every door applies
//! them. A real whisper.cpp run is not here: it needs a 150 MB model.
//!
//! Regenerate the fixtures: `UPDATE_FIXTURES=1 cargo test -p atlas-host crew::tests::voice`.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use atlas_core::progress::Coded;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use parking_lot::Mutex;
use serde_json::{Value, json};

use super::attachments::{daemon_with_telegram, wait_for};
use super::fixture::{create_crewmate, daemon, fake_agent, lock_agent_bin, wait_task_state};
use crate::HostClient;
use crate::crew::attachments::CHUNK;
use crate::crew::voice::audio::{self, BYTES_PER_SECOND, HEADER};

// ── the audio a client sends ─────────────────────────────────────────────────

/// What the desktop's recorder sends first: the canonical header, both sizes `0xFFFFFFFF`.
fn live_header() -> Vec<u8> {
    audio::live_header()
}

/// `seconds` of audio (a constant tone's worth of bytes: the fake engines do not listen).
fn pcm(seconds: usize) -> Vec<u8> {
    vec![3u8; seconds * BYTES_PER_SECOND]
}

/// One call of `crewmate_voice_put` (a piece is at most [`CHUNK`] raw bytes: a protocol line is 1 MiB).
async fn put_one(
    client: &HostClient,
    upload: &str,
    bytes: &[u8],
    last: bool,
) -> anyhow::Result<Value> {
    client
        .call(
            "crewmate_voice_put",
            json!({"upload": upload, "piece_b64": B64.encode(bytes), "last": last}),
        )
        .await
}

/// `bytes` in pieces of at most [`CHUNK`], the way a client sends them; `last` flags the final piece. The first
/// refusal is returned as it is, and the reply is the last piece's (its `upload` is the id of the whole).
async fn put(client: &HostClient, upload: &str, bytes: &[u8], last: bool) -> anyhow::Result<Value> {
    let pieces: Vec<&[u8]> = if bytes.is_empty() {
        vec![&[][..]]
    } else {
        bytes.chunks(CHUNK).collect()
    };
    let mut id = upload.to_string();
    let mut reply = Value::Null;
    for (i, piece) in pieces.iter().enumerate() {
        reply = put_one(client, &id, piece, last && i + 1 == pieces.len()).await?;
        id = reply["upload"].as_str().unwrap().to_string();
    }
    Ok(reply)
}

/// A whole dictation: header and `pcm` in pieces of at most [`CHUNK`], the last one flagged. Returns its id.
async fn upload(client: &HostClient, pcm: &[u8]) -> String {
    let mut all = live_header();
    all.extend_from_slice(pcm);
    put(client, "", &all, true).await.unwrap()["upload"]
        .as_str()
        .unwrap()
        .to_string()
}

fn coded(e: &anyhow::Error) -> Coded {
    e.chain()
        .find_map(|c| c.downcast_ref::<Coded>().cloned())
        .unwrap_or_else(|| panic!("not a coded failure: {e:#}"))
}

async fn transcribe(
    client: &HostClient,
    id: &str,
    upload: &str,
    extra: Value,
) -> anyhow::Result<Value> {
    let mut args = json!({"id": id, "upload": upload});
    args.as_object_mut()
        .unwrap()
        .extend(extra.as_object().cloned().unwrap_or_default());
    client.call("crewmate_transcribe", args).await
}

async fn enable(client: &HostClient) -> Value {
    client
        .call("crew_voice_set", json!({"enabled": true}))
        .await
        .unwrap()
}

// ── the fake engines ─────────────────────────────────────────────────────────

/// A `whisper-cli` that records what it was run with (under `voice/log/`) and answers `text`, or fails with
/// `fail`. A model file is put next to it, so the engine is "installed".
fn install_whisper(dir: &Path, text: &str, fail: Option<&str>) {
    let voice = dir.join("voice");
    std::fs::create_dir_all(voice.join("models")).unwrap();
    std::fs::write(voice.join("models/ggml-small.bin"), b"model").unwrap();
    let body = match fail {
        Some(why) => format!("echo '{why}' >&2\nexit 1"),
        None => format!(
            "printf '%s' '{{\"result\":{{\"language\":\"en\"}},\"transcription\":[{{\"text\":\" {text}\"}}]}}' > \"$out.json\""
        ),
    };
    let script = format!(
        "LOG='{log}'\nmkdir -p \"$LOG\"\nout=''; inp=''; prompt=''; lang=''\n\
         while [ $# -gt 0 ]; do case \"$1\" in -of) out=\"$2\"; shift;; -f) inp=\"$2\"; shift;; \
         --prompt) prompt=\"$2\"; shift;; -l) lang=\"$2\"; shift;; esac; shift; done\n\
         echo \"$inp\" >> \"$LOG/inputs\"\nprintf '%s' \"$prompt\" > \"$LOG/prompt\"\n\
         printf '%s' \"$lang\" > \"$LOG/lang\"\n\
         [ -f \"$inp\" ] && wc -c < \"$inp\" >> \"$LOG/sizes\"\n{body}",
        log = voice.join("log").display(),
    );
    let bin_dir = voice.join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let bin = bin_dir.join("whisper-cli");
    std::fs::write(&bin, format!("#!/bin/sh\n{script}\n")).unwrap();
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn ran(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("voice/log/inputs"))
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

fn log(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join("voice/log").join(name)).unwrap_or_default()
}

/// Nothing of any audio is left under the voice dir: no upload, no scratch file.
fn no_audio_left(dir: &Path) {
    for sub in ["uploads", "tmp"] {
        let p = dir.join("voice").join(sub);
        let left: Vec<_> = std::fs::read_dir(&p)
            .map(|rd| rd.filter_map(Result::ok).map(|e| e.path()).collect())
            .unwrap_or_default();
        assert!(left.is_empty(), "audio left in {sub}: {left:?}");
    }
}

/// A loopback OpenAI-compatible endpoint: answers `status` and `body` to every request and records the
/// `authorization` header and the number of requests.
async fn fake_endpoint(status: u16, body: Value) -> (String, Arc<Mutex<Vec<String>>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", l.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(vec![]));
    let s = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = l.accept().await else {
                return;
            };
            let (s, body) = (s.clone(), body.to_string());
            tokio::spawn(async move {
                let mut buf = vec![0u8; 1 << 20];
                let mut n = 0;
                // Read until the whole request (headers + content-length bytes) is in.
                loop {
                    let m = sock.read(&mut buf[n..]).await.unwrap_or(0);
                    if m == 0 {
                        break;
                    }
                    n += m;
                    let text = String::from_utf8_lossy(&buf[..n]).to_string();
                    if let Some(head_end) = text.find("\r\n\r\n") {
                        let len = text[..head_end]
                            .lines()
                            .find_map(|l| {
                                l.to_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if n >= head_end + 4 + len {
                            break;
                        }
                    }
                }
                let text = String::from_utf8_lossy(&buf[..n]).to_string();
                let line = text.lines().next().unwrap_or("").to_string();
                let auth = text
                    .lines()
                    .find(|l| l.to_lowercase().starts_with("authorization:"))
                    .unwrap_or("")
                    .to_string();
                s.lock().push(format!("{line} | {auth}"));
                let head = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(body.as_bytes()).await;
            });
        }
    });
    (base, seen)
}

// ── the switch, and what is not installed ────────────────────────────────────

// Voice is off until the owner turns it on: nothing can be uploaded, and the status says what it can say.
#[tokio::test(flavor = "multi_thread")]
async fn voice_is_off_until_the_owner_turns_it_on_and_refuses_without_an_engine() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;

    let off = client.call("crew_voice_status", json!({})).await.unwrap();
    assert_eq!(off["enabled"], false, "{off}");
    assert_eq!(off["engine"], Value::Null, "{off}");
    assert_eq!(off["max_seconds"], 300);
    assert_eq!(off["languages"], json!(["en", "pt"]));
    assert_eq!(
        off["audio"],
        json!({"format": "wav_pcm_s16le", "sample_rate": 16000, "channels": 1})
    );
    let table = off["replacements"].as_array().unwrap();
    assert!(
        table
            .iter()
            .any(|r| r["say"] == "slash" && r["write"] == "/" && r["lang"] == "en")
    );
    assert!(
        table
            .iter()
            .any(|r| r["say"] == "barra" && r["write"] == "/" && r["lang"] == "pt")
    );

    let e = put(&client, "", &live_header(), false).await.unwrap_err();
    assert_eq!(coded(&e).code, "crew.voice.off", "{e:#}");
    no_audio_left(&dir);

    // On, but nothing installed: another coded reason, still before any audio is accepted.
    assert_eq!(enable(&client).await["enabled"], true);
    let e = put(&client, "", &live_header(), false).await.unwrap_err();
    assert_eq!(coded(&e).code, "crew.voice.not_installed", "{e:#}");
    no_audio_left(&dir);

    // The switch survives a restart (it is in crew.json), and is audited.
    client.call("shutdown", json!({})).await.ok();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let again = daemon(&dir).await;
    let s = again.call("crew_voice_status", json!({})).await.unwrap();
    assert_eq!(s["enabled"], true, "{s}");
    let audit = again
        .call("crew_audit", json!({"kind": "config_changed"}))
        .await
        .unwrap();
    assert!(audit.to_string().contains("\"voice\""), "{audit}");
    again.call("shutdown", json!({})).await.ok();
}

// A crew's own replacement rows are stored, extend the effective table and override a default of the same words.
#[tokio::test(flavor = "multi_thread")]
async fn the_crews_replacements_extend_the_table_and_bad_rows_are_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let client = daemon(&tmp.path().join("hostd")).await;
    let s = client
        .call(
            "crew_voice_set",
            json!({"replacements": [
                {"say": "hash", "write": "#", "lang": "any"},
                {"say": "slash", "write": " / ", "lang": "en"},
            ]}),
        )
        .await
        .unwrap();
    let table = s["replacements"].as_array().unwrap();
    assert!(
        table
            .iter()
            .any(|r| r["say"] == "hash" && r["write"] == "#")
    );
    assert_eq!(
        table
            .iter()
            .filter(|r| r["say"] == "slash" && r["lang"] == "en")
            .count(),
        1
    );
    assert!(
        table
            .iter()
            .any(|r| r["say"] == "slash" && r["write"] == " / "),
        "the crew's row wins: {s}"
    );

    for bad in [
        json!({"replacements": [{"say": "", "write": "x", "lang": "en"}]}),
        json!({"replacements": [{"say": "x", "write": "y", "lang": "fr"}]}),
        json!({"engine": {"kind": "whisper_cpp", "model": "large"}}),
        json!({"engine": {"kind": "openai_compatible", "base_url": "http://example.com/v1", "model": "m", "key_secret": "K"}}),
    ] {
        let e = client
            .call("crew_voice_set", bad.clone())
            .await
            .unwrap_err();
        assert_eq!(coded(&e).code, "hostd.invalid_argument", "{bad}: {e:#}");
    }
    // A refused set changed nothing.
    let after = client.call("crew_voice_status", json!({})).await.unwrap();
    assert_eq!(after["replacements"], s["replacements"]);
    client.call("shutdown", json!({})).await.ok();
}

// ── a dictation, end to end, and the fixtures ────────────────────────────────

pub(super) fn fixture(path: &Path, live: &Value, tabs: bool) {
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        let text = if tabs {
            let mut buf = Vec::new();
            let mut ser = serde_json::Serializer::with_formatter(
                &mut buf,
                serde_json::ser::PrettyFormatter::with_indent(b"\t"),
            );
            serde::Serialize::serialize(live, &mut ser).unwrap();
            String::from_utf8(buf).unwrap() + "\n"
        } else {
            serde_json::to_string_pretty(live).unwrap() + "\n"
        };
        std::fs::write(path, text).unwrap();
    }
    let stored: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap_or_else(|_| {
        panic!("{path:?} missing: UPDATE_FIXTURES=1 cargo test -p atlas-host crew::tests::voice")
    }))
    .unwrap();
    assert_eq!(
        &stored, live,
        "{path:?} changed: rerun with UPDATE_FIXTURES=1"
    );
}

// The whole path of the issue: the desktop's recording (header with unknown sizes, pieces, a partial while it
// records, the last piece) becomes text with the table applied, the engine named, hints from the crew; the audio
// is gone; and every payload the apps parse is written from these real replies.
#[tokio::test(flavor = "multi_thread")]
async fn a_dictation_becomes_text_with_hints_and_replacements_and_leaves_no_audio() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    install_whisper(
        &dir,
        "open src slash main dot rs and rename foo underscore bar",
        None,
    );
    let client = daemon(&dir).await;
    let eyes = create_crewmate(&client, "Eyes").await;
    create_crewmate(&client, "Ana").await;
    let status_on = enable(&client).await;
    assert_eq!(
        status_on["engine"],
        json!({"kind": "whisper_cpp", "label": "whisper.cpp (small)", "model": "small", "ready": true, "detail": null}),
        "an installed whisper.cpp is the engine of a crew that chose none"
    );

    // 70 s in two pieces, a partial between them (the dimmed preview), then the last piece.
    let mut first = live_header();
    first.extend(pcm(40));
    let id1 = put(&client, "", &first, false).await.unwrap();
    let up = id1["upload"].as_str().unwrap().to_string();
    assert_eq!(up.len(), 32);
    assert_eq!(id1["bytes"], (HEADER + 40 * BYTES_PER_SECOND) as u64);
    let partial = transcribe(
        &client,
        &eyes,
        &up,
        json!({"partial": true, "language": "en"}),
    )
    .await
    .unwrap();
    assert_eq!(partial["partial"], true, "{partial}");
    assert_eq!(
        partial["duration_ms"], 30_000,
        "a partial listens to the last 30 s: {partial}"
    );
    assert!(
        dir.join(format!("voice/uploads/{up}")).exists(),
        "a partial leaves the upload open"
    );
    // A final one needs the last piece first.
    let e = transcribe(&client, &eyes, &up, json!({}))
        .await
        .unwrap_err();
    assert!(format!("{e:#}").contains("not complete"), "{e:#}");
    assert!(
        dir.join(format!("voice/uploads/{up}")).exists(),
        "refused, so nothing was consumed"
    );

    let put_last = put(&client, &up, &pcm(30), true).await.unwrap();
    assert_eq!(put_last["upload"], up);
    let done = transcribe(&client, &eyes, &up, json!({"language": "en"}))
        .await
        .unwrap();
    assert_eq!(
        done["text"], "open src/main.rs and rename foo_bar",
        "{done}"
    );
    assert_eq!(
        done["raw"],
        "open src slash main dot rs and rename foo underscore bar"
    );
    assert_eq!(
        done["engine"],
        json!({"kind": "whisper_cpp", "label": "whisper.cpp (small)"})
    );
    assert_eq!(done["language"], "en");
    assert_eq!(done["duration_ms"], 70_000);
    assert_eq!(done["partial"], false);

    // The engine heard the hints of this crew and the language it was told; the file it read had real sizes.
    let prompt = log(&dir, "prompt");
    assert!(
        prompt.contains("Eyes") && prompt.contains("Ana"),
        "crewmate names are hints: {prompt:?}"
    );
    assert_eq!(log(&dir, "lang"), "en");
    let inputs = ran(&dir);
    assert_eq!(
        inputs.len(),
        2,
        "one run for the partial, one for the final"
    );
    for input in &inputs {
        assert!(
            !Path::new(input).exists(),
            "the audio the engine read is gone: {input}"
        );
    }
    no_audio_left(&dir);

    // The upload was consumed: asking again is an error, not a second run.
    let e = transcribe(&client, &eyes, &up, json!({}))
        .await
        .unwrap_err();
    assert!(format!("{e:#}").contains("unknown upload"), "{e:#}");
    assert_eq!(ran(&dir).len(), 2);

    client.call("shutdown", json!({})).await.ok();
}

// The cap by the declared format: 300 s are taken, one sample more is `too_long`, and what was said is still
// transcribed (reaching the cap discards nothing).
#[tokio::test(flavor = "multi_thread")]
async fn the_cap_is_300_seconds_and_reaching_it_still_transcribes_what_was_said() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    install_whisper(&dir, "five minutes of words", None);
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Eyes").await;
    enable(&client).await;

    let up = upload_without_last(&client, &pcm(300)).await;
    let e = put(&client, &up, &[0, 0], false).await.unwrap_err();
    let c = coded(&e);
    assert_eq!(c.code, "crew.voice.too_long", "{e:#}");
    assert_eq!(c.args["max_seconds"], 300);
    // The refused piece did not cost the 300 s already there: closing the upload transcribes all of it.
    put(&client, &up, &[], true).await.unwrap();
    let t = transcribe(&client, &id, &up, json!({})).await.unwrap();
    assert_eq!(t["text"], "five minutes of words");
    assert_eq!(t["duration_ms"], 300_000, "{t}");
    no_audio_left(&dir);
    client.call("shutdown", json!({})).await.ok();
}

/// A dictation whose last piece has not been sent: header and `pcm` in pieces of at most [`CHUNK`].
async fn upload_without_last(client: &HostClient, pcm: &[u8]) -> String {
    let mut all = live_header();
    all.extend_from_slice(pcm);
    put(client, "", &all, false).await.unwrap()["upload"]
        .as_str()
        .unwrap()
        .to_string()
}

// Audio that is not the declared format is refused with its code, at the piece where the header is complete, and
// nothing of it stays.
#[tokio::test(flavor = "multi_thread")]
async fn audio_that_is_not_16khz_mono_pcm_is_refused_and_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    install_whisper(&dir, "x", None);
    let client = daemon(&dir).await;
    enable(&client).await;
    let mut stereo = live_header();
    stereo[22] = 2;
    stereo.extend(pcm(1));
    let e = put(&client, "", &stereo, false).await.unwrap_err();
    assert_eq!(coded(&e).code, "crew.voice.bad_audio", "{e:#}");
    // An mp3 and an empty "last" piece are not audio either.
    for junk in [
        &b"ID3\x04\0\0\0\0\0\0 and more bytes after the id3 tag header"[..],
        &b""[..],
    ] {
        let e = put(&client, "", junk, true).await.unwrap_err();
        assert_eq!(coded(&e).code, "crew.voice.bad_audio", "{junk:?}: {e:#}");
    }
    no_audio_left(&dir);
    assert!(
        ran(&dir).is_empty(),
        "no engine ran for audio it was never given"
    );
    client.call("shutdown", json!({})).await.ok();
}

// The apps' contract: the replies and failures below are the REAL ones of a hostd, in the order a person meets
// them, written to the fixtures the desktop and the phone read (`UPDATE_FIXTURES=1`).
#[tokio::test(flavor = "multi_thread")]
async fn the_voice_fixtures_are_real_replies() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let eyes = create_crewmate(&client, "Eyes").await;

    let status_off = client.call("crew_voice_status", json!({})).await.unwrap();
    let off = put(&client, "", &live_header(), false).await.unwrap_err();
    let status_enabled_no_engine = enable(&client).await;
    let not_installed = put(&client, "", &live_header(), false).await.unwrap_err();

    install_whisper(
        &dir,
        "open src slash main dot rs and rename foo underscore bar",
        None,
    );
    let status_on = client.call("crew_voice_status", json!({})).await.unwrap();
    let mut first = live_header();
    first.extend(pcm(40));
    let opened = put(&client, "", &first, false).await.unwrap();
    let up = opened["upload"].as_str().unwrap().to_string();
    let transcript_partial = transcribe(
        &client,
        &eyes,
        &up,
        json!({"partial": true, "language": "en"}),
    )
    .await
    .unwrap();
    let closed = put(&client, &up, &pcm(30), true).await.unwrap();
    let transcript = transcribe(&client, &eyes, &up, json!({"language": "en"}))
        .await
        .unwrap();

    let mut stereo = live_header();
    stereo[22] = 2;
    let bad_audio = put(&client, "", &stereo, false).await.unwrap_err();
    let long_up = upload_without_last(&client, &pcm(300)).await;
    let too_long = put(&client, &long_up, &[0, 0], false).await.unwrap_err();
    install_whisper(&dir, "x", Some("ggml_backend_alloc failed: out of memory"));
    let failing = upload(&client, &pcm(2)).await;
    let engine_failed = transcribe(&client, &eyes, &failing, json!({}))
        .await
        .unwrap_err();

    // The id is random: the fixture holds a stand-in of the same shape.
    let stand_in =
        |v: &Value| json!({"upload": "0123456789abcdef0123456789abcdef", "bytes": v["bytes"]});
    let value = json!({
        "status_off": status_off,
        "status_enabled_no_engine": status_enabled_no_engine,
        "status_on": status_on,
        "put_open": stand_in(&opened),
        "put_last": stand_in(&closed),
        "transcript": transcript,
        "transcript_partial": transcript_partial,
        "errors": {
            "off": coded(&off).wire(),
            "not_installed": coded(&not_installed).wire(),
            "engine_failed": coded(&engine_failed).wire(),
            "too_long": coded(&too_long).wire(),
            "bad_audio": coded(&bad_audio).wire(),
            "forbidden": crate::crew::voice::forbidden().wire(),
        },
    });
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fixture(
        &root.join("apps/desktop/src/lib/__fixtures__/crew-voice.json"),
        &value,
        true,
    );
    fixture(
        &root.join("apps/mobile/test/fixtures/crew_voice.json"),
        &value,
        false,
    );
    client.call("shutdown", json!({})).await.ok();
}

// ── engines: no silent fallback ──────────────────────────────────────────────

// A failing engine is `engine_failed`, with its label and its own words, and the audio is gone. The other engine
// the crew has set up is never asked: the person chose where their audio goes.
#[tokio::test(flavor = "multi_thread")]
async fn a_failing_engine_is_a_coded_failure_and_no_other_engine_runs() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Eyes").await;
    client
        .call(
            "crewmate_secret_set",
            json!({"id": id, "key": "STT_KEY", "value": "sk-test-key-123"}),
        )
        .await
        .unwrap();
    enable(&client).await;

    // whisper.cpp chosen and failing; a hosted endpoint exists and would answer. It must not be asked.
    let (base, hits) = fake_endpoint(
        200,
        json!({"text": "from the cloud", "language": "english"}),
    )
    .await;
    install_whisper(&dir, "x", Some("ggml_backend_alloc failed: out of memory"));
    client
        .call(
            "crew_voice_set",
            json!({"engine": {"kind": "whisper_cpp", "model": "small"}}),
        )
        .await
        .unwrap();
    let up = upload(&client, &pcm(2)).await;
    let e = transcribe(&client, &id, &up, json!({})).await.unwrap_err();
    let c = coded(&e);
    assert_eq!(c.code, "crew.voice.engine_failed", "{e:#}");
    assert_eq!(c.args["engine"], "whisper.cpp (small)");
    assert!(
        c.args["detail"].as_str().unwrap().contains("out of memory"),
        "{c:?}"
    );
    assert!(
        hits.lock().is_empty(),
        "the hosted engine was asked after whisper.cpp failed: {:?}",
        hits.lock()
    );
    no_audio_left(&dir);

    // The other way round: the hosted engine chosen and failing (HTTP 500); the local whisper.cpp, which is
    // installed and would work, never runs.
    install_whisper(&dir, "from the machine", None);
    let runs_before = ran(&dir).len();
    let (bad, bad_hits) = fake_endpoint(
        500,
        json!({"error": {"message": "sk-test-key-123 is wrong"}}),
    )
    .await;
    client
        .call(
            "crew_voice_set",
            json!({"engine": {"kind": "openai_compatible", "base_url": bad, "model": "whisper-1", "key_secret": "STT_KEY"}}),
        )
        .await
        .unwrap();
    let up = upload(&client, &pcm(2)).await;
    let e = transcribe(&client, &id, &up, json!({})).await.unwrap_err();
    let c = coded(&e);
    assert_eq!(c.code, "crew.voice.engine_failed", "{e:#}");
    assert!(
        c.args["engine"]
            .as_str()
            .unwrap()
            .starts_with("OpenAI-compatible (whisper-1 @ 127.0.0.1"),
        "{c:?}"
    );
    assert!(
        c.args["detail"].as_str().unwrap().contains("HTTP 500"),
        "{c:?}"
    );
    assert!(
        !format!("{c:?}").contains("sk-test-key-123"),
        "a provider's error text can echo the key: {c:?}"
    );
    assert_eq!(bad_hits.lock().len(), 1);
    assert_eq!(
        ran(&dir).len(),
        runs_before,
        "whisper.cpp ran after the hosted engine failed"
    );
    no_audio_left(&dir);
    drop(base);
    client.call("shutdown", json!({})).await.ok();
}

// The hosted engine is the user's own endpoint with a key from the crew's secrets: the key goes in the header
// only, the label names the host, and no reply or audit line ever carries the key.
#[tokio::test(flavor = "multi_thread")]
async fn a_hosted_engine_uses_the_crews_key_and_the_key_never_leaves() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Eyes").await;
    client
        .call(
            "crewmate_secret_set",
            json!({"id": id, "key": "STT_KEY", "value": "sk-test-key-123"}),
        )
        .await
        .unwrap();
    let (base, hits) = fake_endpoint(
        200,
        json!({"text": " src slash lib dot rs", "language": "english"}),
    )
    .await;
    let set = client
        .call(
            "crew_voice_set",
            json!({"enabled": true,
                   "engine": {"kind": "openai_compatible", "base_url": base, "model": "whisper-1", "key_secret": "STT_KEY"}}),
        )
        .await
        .unwrap();
    assert_eq!(set["engine"]["kind"], "openai_compatible");
    assert_eq!(set["engine"]["ready"], true, "the secret is there: {set}");
    assert!(
        set["engine"]["label"]
            .as_str()
            .unwrap()
            .contains("127.0.0.1"),
        "{set}"
    );

    let up = upload(&client, &pcm(2)).await;
    let t = transcribe(&client, &id, &up, json!({"language": "en"}))
        .await
        .unwrap();
    assert_eq!(t["text"], "src/lib.rs");
    assert_eq!(t["engine"]["kind"], "openai_compatible");
    assert_eq!(t["language"], "en");
    let seen = hits.lock().clone();
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert!(
        seen[0].starts_with("POST /v1/audio/transcriptions"),
        "{seen:?}"
    );
    assert!(
        seen[0].to_lowercase().contains("bearer sk-test-key-123"),
        "the key goes in the header: {seen:?}"
    );

    let everything = format!(
        "{set}{t}{}{}",
        client.call("crew_voice_status", json!({})).await.unwrap(),
        client.call("crew_audit", json!({})).await.unwrap()
    );
    assert!(
        !everything.contains("sk-test-key-123"),
        "the key leaked into a reply or the audit"
    );
    assert!(
        std::fs::read_to_string(dir.join("crew.json"))
            .unwrap()
            .contains("STT_KEY"),
        "the secret's name is kept"
    );
    assert!(
        !std::fs::read_to_string(dir.join("crew.json"))
            .unwrap()
            .contains("sk-test-key-123")
    );
    no_audio_left(&dir);

    // Without its secret the hosted engine is not ready, and a transcription says not_installed.
    let (other, _) = fake_endpoint(200, json!({"text": "x"})).await;
    let s = client
        .call(
            "crew_voice_set",
            json!({"engine": {"kind": "openai_compatible", "base_url": other, "model": "m", "key_secret": "MISSING"}}),
        )
        .await
        .unwrap();
    assert_eq!(s["engine"]["ready"], false);
    let up = upload(&client, &pcm(2)).await;
    let e = transcribe(&client, &id, &up, json!({})).await.unwrap_err();
    assert_eq!(coded(&e).code, "crew.voice.not_installed", "{e:#}");
    no_audio_left(&dir);
    client.call("shutdown", json!({})).await.ok();
}

// ── the audio's lifetime ─────────────────────────────────────────────────────

// An upload nobody finished is thrown away: by the sweep at start (here), and by the next put (unit-tested in
// `voice/audio.rs` with an injected clock).
#[tokio::test(flavor = "multi_thread")]
async fn an_abandoned_dictation_is_removed_when_the_crew_server_starts() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    install_whisper(&dir, "x", None);
    let first = daemon(&dir).await;
    enable(&first).await;
    let stale = upload_without_last(&first, &pcm(2)).await;
    let fresh = upload_without_last(&first, &pcm(2)).await;
    first.call("shutdown", json!({})).await.ok();
    tokio::time::sleep(Duration::from_millis(300)).await;
    std::fs::create_dir_all(dir.join("voice/tmp/crashed")).unwrap();
    std::fs::write(dir.join("voice/tmp/crashed/in.wav"), b"audio").unwrap();

    let eleven = audio::UPLOAD_TTL + Duration::from_secs(60);
    std::fs::OpenOptions::new()
        .write(true)
        .open(dir.join(format!("voice/uploads/{stale}/data")))
        .unwrap()
        .set_modified(std::time::SystemTime::now() - eleven)
        .unwrap();

    let second = daemon(&dir).await;
    assert!(
        !dir.join(format!("voice/uploads/{stale}")).exists(),
        "the abandoned upload survived the start"
    );
    assert!(
        dir.join(format!("voice/uploads/{fresh}")).exists(),
        "a fresh upload was swept"
    );
    assert!(
        !dir.join("voice/tmp").exists(),
        "scratch audio from a crash survived the start"
    );
    second.call("shutdown", json!({})).await.ok();
}

// ── who may change it, and the phone ─────────────────────────────────────────

#[test]
fn only_the_desktop_and_the_owners_chat_change_voice() {
    use crate::crew::voice;
    // The origins `Hub::origin_of` can name: the desktop and the CLI, a phone, the owner's chat, a crewmate.
    for (origin, owner) in [
        ("desktop", true),
        ("telegram:42", true),
        ("phone:abcd", false),
        ("crewmate:xyz", false),
        ("", false),
        ("desktop2", false),
    ] {
        assert_eq!(voice::is_owner(origin), owner, "{origin:?}");
    }
    let f = voice::forbidden();
    assert_eq!((f.code, f.args.len()), ("crew.voice.forbidden", 0));
}

// A phone recognises speech itself and never sends audio to hostd: it may read whether voice is on, nothing more.
#[test]
fn a_phone_may_read_the_voice_status_and_nothing_else() {
    use crate::mobile::{Scope, required_scope};
    use crate::protocol::HostCmd;
    let parse = |v: Value| -> HostCmd { serde_json::from_value(v).unwrap() };
    let status = json!({"cmd": "crew_voice_status", "args": {}});
    let set = json!({"cmd": "crew_voice_set", "args": {"enabled": true}});
    let put = json!({"cmd": "crewmate_voice_put", "args": {"upload": "", "piece_b64": "", "last": false}});
    let tr = json!({"cmd": "crewmate_transcribe", "args": {"id": "c", "upload": "u"}});
    assert_eq!(required_scope(&parse(status.clone())), Ok(Scope::Read));
    for cmd in [&set, &put, &tr] {
        assert!(required_scope(&parse(cmd.clone())).is_err(), "{cmd}");
    }
    // The same through the Mac's relay.
    for (inner, want) in [
        (&status, Ok(Scope::Read)),
        (&set, Err("not available on mobile")),
    ] {
        let relayed = json!({"cmd": "crew_call", "args": {"server": "s", "cmd": inner["cmd"], "args": inner["args"]}});
        assert_eq!(required_scope(&parse(relayed)), want, "{inner}");
    }
}

// ── a dictated task ──────────────────────────────────────────────────────────

// `dictated: true` tells the agent, once, to expect homophones; the stored prompt (what the thread shows) is
// the person's words and nothing else, and a task that was typed carries no such line.
#[tokio::test(flavor = "multi_thread")]
async fn a_dictated_task_appends_the_note_for_the_agent_and_never_to_the_prompt() {
    let _bin = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let work = tmp.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let script = format!(
        "for a in \"$@\"; do last=\"$a\"; done\n\
         printf '%s' \"$last\" > {out}/prompt-$$.txt\n\
         printf '{{\"type\":\"result\",\"session_id\":\"s1\",\"result\":\"ok\",\"is_error\":false}}\\n'\n\
         exit 0",
        out = tmp.path().display(),
    );
    let bin = fake_agent(tmp.path(), "claude-engine", &script);
    crate::crew::engine::set_test_engine_bin(Some(&bin.to_string_lossy()));
    let client = daemon(&dir).await;
    let id = client
        .call(
            "crewmate_create",
            json!({"name": "Eyes", "role": "r", "agent": "claude", "approval": "auto",
                   "engine": {"kind": "claude"}, "workdir": work.to_string_lossy()}),
        )
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let say = |dictated: Value| {
        let c = &client;
        let id = &id;
        async move {
            let mut args = json!({"id": id, "prompt": "open src/main.rs and rename foo_bar", "source": "test"});
            if !dictated.is_null() {
                args["dictated"] = dictated;
            }
            let v = c.call("crewmate_task", args).await.unwrap();
            let task = v["task"]["id"].as_str().unwrap().to_string();
            wait_task_state(c, id, &task, "done", atlas_core::scaling::HANG).await;
            v["task"].clone()
        }
    };
    let dictated = say(json!(true)).await;
    assert_eq!(
        dictated["prompt"], "open src/main.rs and rename foo_bar",
        "{dictated}"
    );
    assert_eq!(dictated["dictated"], true);
    let typed = say(Value::Null).await;
    assert!(
        typed.get("dictated").is_none(),
        "a typed task says nothing: {typed}"
    );

    let mut prompts: Vec<String> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("prompt-"))
        .map(|e| std::fs::read_to_string(e.path()).unwrap())
        .collect();
    prompts.sort();
    assert_eq!(prompts.len(), 2, "{prompts:?}");
    let note = crate::crew::voice::NOTE;
    let with: Vec<_> = prompts.iter().filter(|p| p.contains(note)).collect();
    assert_eq!(
        with.len(),
        1,
        "exactly the dictated task carries the note: {prompts:?}"
    );
    assert_eq!(
        note,
        "(This request was dictated by voice; expect homophones in identifiers and file names.)"
    );
    // The note comes before the person's words, so nothing they said can imitate or follow it.
    let p = with[0];
    assert!(
        p.find(note).unwrap() < p.find("open src/main.rs").unwrap(),
        "{p}"
    );
    crate::crew::engine::set_test_engine_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// ── Telegram voice notes ─────────────────────────────────────────────────────

pub(super) type Calls = Arc<Mutex<Vec<(String, Value)>>>;

/// A live fake Telegram Bot API: `getUpdates` hands over whatever the test queued since the last poll, `getFile`
/// names a path, `/file/bot…` serves `file`, and every other method is recorded with its JSON body.
async fn fake_telegram_live(file: Vec<u8>) -> (String, Arc<Mutex<Vec<Value>>>, Calls) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let api = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", api.local_addr().unwrap());
    let queue: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(vec![]));
    let calls: Calls = Arc::new(Mutex::new(vec![]));
    let (q, c) = (queue.clone(), calls.clone());
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = api.accept().await else {
                return;
            };
            let (q, c, file) = (q.clone(), c.clone(), file.clone());
            tokio::spawn(async move {
                let mut buf = vec![0u8; 65536];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req
                    .lines()
                    .next()
                    .unwrap_or("")
                    .split(' ')
                    .nth(1)
                    .unwrap_or("")
                    .to_string();
                let method = path
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .split('?')
                    .next()
                    .unwrap_or("")
                    .to_string();
                let body: Value = serde_json::from_str(req.split("\r\n\r\n").nth(1).unwrap_or(""))
                    .unwrap_or(Value::Null);
                let (ctype, out): (&str, Vec<u8>) = if path.contains("/file/bot") {
                    c.lock().push(("file".into(), json!({"path": path})));
                    ("audio/ogg", file)
                } else if method == "getUpdates" {
                    let updates: Vec<Value> = std::mem::take(&mut *q.lock());
                    if updates.is_empty() {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    (
                        "application/json",
                        json!({"ok": true, "result": updates})
                            .to_string()
                            .into_bytes(),
                    )
                } else if method == "getFile" {
                    c.lock().push((method, body));
                    ("application/json", json!({"ok": true, "result": {"file_id": "f", "file_size": 1000, "file_path": "voice/file_1.oga"}}).to_string().into_bytes())
                } else {
                    c.lock().push((method, body));
                    (
                        "application/json",
                        br#"{"ok":true,"result":{"message_id":500}}"#.to_vec(),
                    )
                };
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: {ctype}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    out.len()
                );
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(&out).await;
            });
        }
    });
    (base, queue, calls)
}

/// A `ffmpeg` that records its arguments and "decodes" any note to two seconds of silence, or fails.
fn install_ffmpeg(dir: &Path, fail: bool) {
    let bin_dir = dir.join("voice/bin");
    std::fs::create_dir_all(&bin_dir).unwrap();
    let body = if fail {
        "echo 'Invalid data found when processing input' >&2\nexit 1".to_string()
    } else {
        "head -c 64000 /dev/zero".to_string()
    };
    let log = dir.join("voice/log");
    let bin = bin_dir.join("ffmpeg");
    std::fs::write(
        &bin,
        format!(
            "#!/bin/sh\nmkdir -p '{l}'\necho \"$@\" >> '{l}/ffmpeg'\n{body}\n",
            l = log.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
}

pub(super) struct Rig {
    pub(super) dir: std::path::PathBuf,
    pub(super) client: HostClient,
    queue: Arc<Mutex<Vec<Value>>>,
    pub(super) calls: Calls,
    pub(super) crewmate: String,
    next_update: i64,
    _tmp: tempfile::TempDir,
}

impl Rig {
    /// A crew server with crewmate Eyes, voice on, a fake whisper.cpp and ffmpeg, and chat 42 linked to a
    /// fake Telegram.
    pub(super) async fn new(transcript: &str, whisper_fails: bool, voice_on: bool) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("hostd");
        let first = daemon(&dir).await;
        let crewmate = create_crewmate(&first, "Eyes").await;
        first.call("shutdown", json!({})).await.ok();
        tokio::time::sleep(Duration::from_millis(300)).await;
        install_whisper(
            &dir,
            transcript,
            whisper_fails.then_some("ggml_backend_alloc failed: out of memory"),
        );
        install_ffmpeg(&dir, false);
        if voice_on {
            std::fs::write(
                dir.join("crew.json"),
                json!({"voice": {"enabled": true}}).to_string(),
            )
            .unwrap();
        }
        let (base, queue, calls) = fake_telegram_live(b"OggS fake opus".to_vec()).await;
        let client = daemon_with_telegram(&dir, &base).await;
        Self {
            dir,
            client,
            queue,
            calls,
            crewmate,
            next_update: 1,
            _tmp: tmp,
        }
    }

    fn push(&mut self, update: Value) {
        let mut u = update;
        u["update_id"] = json!(self.next_update);
        self.next_update += 1;
        self.queue.lock().push(u);
    }

    /// A voice note of `size` bytes from `chat`.
    pub(super) fn note(&mut self, chat: i64, size: u64, caption: Option<&str>) {
        let mut msg = json!({"message_id": 7, "chat": {"id": chat},
                             "voice": {"file_id": "v1", "file_size": size, "duration": 3, "mime_type": "audio/ogg"}});
        if let Some(c) = caption {
            msg["caption"] = json!(c);
        }
        self.push(json!({"message": msg}));
    }

    pub(super) fn text(&mut self, chat: i64, text: &str) {
        self.push(json!({"message": {"message_id": 8, "chat": {"id": chat}, "text": text}}));
    }

    fn tap(&mut self, chat: i64, data: &str) {
        self.push(json!({"callback_query": {"id": "cb", "data": data, "message": {"message_id": 500, "chat": {"id": chat}}}}));
    }

    /// Wait for the `n`th call of `method` (1-based) and return its body.
    pub(super) async fn nth(&self, method: &str, n: usize) -> Value {
        let m = method.to_string();
        wait_for(&format!("{method} #{n}"), || {
            self.calls
                .lock()
                .iter()
                .filter(|(k, _)| *k == m)
                .nth(n - 1)
                .map(|(_, v)| v.clone())
        })
        .await
    }

    pub(super) fn count(&self, method: &str) -> usize {
        self.calls
            .lock()
            .iter()
            .filter(|(k, _)| k == method)
            .count()
    }

    /// The callback data of the button called `label` in `echo`.
    pub(super) fn button(echo: &Value, label: &str) -> String {
        echo["reply_markup"]["inline_keyboard"][0]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["text"] == label)
            .unwrap_or_else(|| panic!("no {label} button in {echo}"))["callback_data"]
            .as_str()
            .unwrap()
            .to_string()
    }

    /// The tasks of the crewmate: the log is append-only (a task has a record per change), so the last of each id.
    pub(super) fn tasks(&self) -> Vec<Value> {
        let all: Vec<Value> =
            std::fs::read_to_string(self.dir.join(format!("crew/{}/tasks.jsonl", self.crewmate)))
                .map(|t| {
                    t.lines()
                        .filter_map(|l| serde_json::from_str(l).ok())
                        .collect()
                })
                .unwrap_or_default();
        let mut out: Vec<Value> = vec![];
        for t in all {
            match out.iter().position(|o| o["id"] == t["id"]) {
                Some(i) => out[i] = t,
                None => out.push(t),
            }
        }
        out
    }

    pub(super) async fn done(self) {
        self.client.call("shutdown", json!({})).await.ok();
    }
}

// A voice note from the owner's chat is transcribed, echoed with the engine named and three buttons, and creates
// nothing until Send as task is tapped; then the task carries the dictated note. The audio is gone before the
// echo is sent.
#[tokio::test(flavor = "multi_thread")]
async fn a_telegram_voice_note_is_echoed_and_only_send_creates_a_dictated_task() {
    let mut rig = Rig::new(
        "open src slash main dot rs and rename foo underscore bar",
        false,
        true,
    )
    .await;
    rig.note(42, 40_000, None);
    let echo = rig.nth("sendMessage", 1).await;
    let text = echo["text"].as_str().unwrap();
    assert!(
        text.starts_with("open src/main.rs and rename foo_bar"),
        "the table applies on Telegram too: {text}"
    );
    assert!(
        text.contains("whisper.cpp (small)"),
        "the engine is shown: {text}"
    );
    assert!(text.contains("Eyes"), "{text}");
    assert_eq!(echo["chat_id"], 42);
    let labels: Vec<_> = echo["reply_markup"]["inline_keyboard"][0]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["text"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(labels, ["Send as task", "Edit", "Cancel"]);
    // Decoded as the contract's format, capped one second past 300 s (so a longer note is known to be longer).
    let ffmpeg = log(&rig.dir, "ffmpeg");
    assert!(
        ffmpeg.contains("-ar 16000 -ac 1 -f s16le") && ffmpeg.contains("-t 301"),
        "{ffmpeg}"
    );
    assert!(
        rig.tasks().is_empty(),
        "nothing is created by the note itself: {:?}",
        rig.tasks()
    );
    no_audio_left(&rig.dir);
    assert!(
        ran(&rig.dir).iter().all(|p| !Path::new(p).exists()),
        "the audio the engine read is gone"
    );

    let send = Rig::button(&echo, "Send as task");
    rig.tap(42, &send);
    let task = wait_for("the dictated task", || rig.tasks().into_iter().next()).await;
    assert_eq!(task["source"], "telegram");
    assert_eq!(task["prompt"], "open src/main.rs and rename foo_bar");
    assert_eq!(task["dictated"], true, "{task}");
    let edited = rig.nth("editMessageText", 1).await;
    assert!(
        edited["text"].as_str().unwrap().contains("Sent as a task"),
        "{edited}"
    );
    assert!(
        edited.get("reply_markup").is_none(),
        "the buttons are taken away"
    );
    // A second tap on the same button is not a second task.
    rig.tap(42, &send);
    let gone = rig.nth("editMessageText", 2).await;
    assert!(
        gone["text"].as_str().unwrap().contains("no longer waiting"),
        "{gone}"
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(rig.tasks().len(), 1);
    rig.done().await;
}

// Cancel drops it: no task, and the buttons of an old echo say it no longer waits.
#[tokio::test(flavor = "multi_thread")]
async fn cancelling_a_voice_note_creates_no_task() {
    let mut rig = Rig::new("delete everything", false, true).await;
    rig.note(42, 40_000, None);
    let echo = rig.nth("sendMessage", 1).await;
    rig.tap(42, &Rig::button(&echo, "Cancel"));
    let said = rig.nth("editMessageText", 1).await;
    assert!(
        said["text"].as_str().unwrap().contains("Nothing was sent"),
        "{said}"
    );
    // Send after Cancel finds nothing waiting.
    rig.tap(42, &Rig::button(&echo, "Send as task"));
    let gone = rig.nth("editMessageText", 2).await;
    assert!(
        gone["text"].as_str().unwrap().contains("no longer waiting"),
        "{gone}"
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(rig.tasks().is_empty(), "{:?}", rig.tasks());
    rig.done().await;
}

// Edit asks for the corrected text; the next message is echoed again with the buttons and is sent only by Send.
#[tokio::test(flavor = "multi_thread")]
async fn editing_a_voice_note_takes_the_next_message_as_the_text_and_still_waits_for_send() {
    let mut rig = Rig::new("open src slash lib dot rss", false, true).await;
    rig.note(42, 40_000, None);
    let echo = rig.nth("sendMessage", 1).await;
    rig.tap(42, &Rig::button(&echo, "Edit"));
    let ask = rig.nth("editMessageText", 1).await;
    assert!(
        ask["text"].as_str().unwrap().contains("corrected text"),
        "{ask}"
    );
    rig.text(42, "open src/lib.rs");
    let again = rig.nth("sendMessage", 2).await;
    assert!(
        again["text"]
            .as_str()
            .unwrap()
            .starts_with("open src/lib.rs"),
        "{again}"
    );
    assert!(rig.tasks().is_empty(), "a correction is not sent by itself");
    rig.tap(42, &Rig::button(&again, "Send as task"));
    let task = wait_for("the corrected task", || rig.tasks().into_iter().next()).await;
    assert_eq!(task["prompt"], "open src/lib.rs");
    assert_eq!(task["dictated"], true);
    rig.done().await;
}

// A note from a chat that is not the owner's is refused like any other message: no download, no engine, no reply,
// and its tap on someone else's echo changes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_voice_note_from_another_chat_is_ignored() {
    let mut rig = Rig::new("hello", false, true).await;
    rig.note(99, 40_000, None);
    rig.note(42, 40_000, None);
    let echo = rig.nth("sendMessage", 1).await;
    assert_eq!(
        echo["chat_id"], 42,
        "the first reply is to the owner's chat, none went to chat 99"
    );
    // The stranger taps Send on the owner's echo.
    rig.tap(99, &Rig::button(&echo, "Send as task"));
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(
        rig.tasks().is_empty(),
        "a tap from another chat made a task: {:?}",
        rig.tasks()
    );
    assert_eq!(rig.count("getFile"), 1, "only the owner's note was fetched");
    assert_eq!(ran(&rig.dir).len(), 1, "the engine ran once, for the owner");
    assert!(
        rig.calls.lock().iter().all(|(_, v)| v["chat_id"] != 99),
        "something was sent to chat 99"
    );
    rig.done().await;
}

// Telegram's own limit: over 20 MB is refused before it is fetched, and the chat is told.
#[tokio::test(flavor = "multi_thread")]
async fn a_voice_note_over_twenty_megabytes_is_refused_without_being_fetched() {
    let mut rig = Rig::new("hello", false, true).await;
    rig.note(42, 20 * 1024 * 1024 + 1, None);
    let said = rig.nth("sendMessage", 1).await;
    assert!(said["text"].as_str().unwrap().contains("20 MB"), "{said}");
    assert!(said.get("reply_markup").is_none());
    assert_eq!(rig.count("getFile"), 0);
    assert!(ran(&rig.dir).is_empty());
    rig.done().await;
}

// A failing engine is told to the chat with its name; there are no buttons, no task, no other engine, no audio.
#[tokio::test(flavor = "multi_thread")]
async fn a_telegram_voice_note_whose_engine_fails_says_so_and_leaves_nothing() {
    let mut rig = Rig::new("x", true, true).await;
    rig.note(42, 40_000, None);
    let said = rig.nth("sendMessage", 1).await;
    let text = said["text"].as_str().unwrap();
    assert!(
        text.contains("whisper.cpp (small) failed") && text.contains("out of memory"),
        "{text}"
    );
    assert!(text.contains("No other engine was tried"), "{text}");
    assert!(said.get("reply_markup").is_none());
    assert!(rig.tasks().is_empty());
    no_audio_left(&rig.dir);
    rig.done().await;
}

// With voice off the note is not even decoded into an engine run.
#[tokio::test(flavor = "multi_thread")]
async fn a_telegram_voice_note_with_voice_off_is_told_so() {
    let mut rig = Rig::new("x", false, false).await;
    rig.note(42, 40_000, None);
    let said = rig.nth("sendMessage", 1).await;
    assert!(
        said["text"].as_str().unwrap().contains("turned off"),
        "{said}"
    );
    assert!(
        ran(&rig.dir).is_empty(),
        "no engine ran while voice was off"
    );
    no_audio_left(&rig.dir);
    rig.done().await;
}
