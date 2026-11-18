//! Images on tasks (#209) against a real hostd: stored with the task, copied into the work dir and named in
//! the prompt, refused when they break a rule, fetched back for the apps, and sent from Telegram. The
//! rules themselves are unit-tested in `attachments.rs`; these check that every door applies them.

use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};

use super::fixture::{create_crewmate, daemon, fake_agent, lock_agent_bin, wait_task_state};
use crate::HostClient;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01";
const JPG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F'];

fn sent(name: &str, bytes: &[u8]) -> Value {
    json!({"name": name, "mime": "image/png", "data_b64": B64.encode(bytes)})
}

async fn send(client: &HostClient, id: &str, attachments: Value) -> Result<Value, String> {
    client
        .call(
            "crewmate_task",
            json!({"id": id, "prompt": "fix this", "source": "test", "attachments": attachments}),
        )
        .await
        .map_err(|e| format!("{e:#}"))
}

/// All the bytes of an attachment, fetched the way a client does: piece by piece until `last`.
async fn fetch(client: &HostClient, id: &str, task: &str, name: &str) -> Vec<u8> {
    let mut out = vec![];
    loop {
        let v = client
            .call(
                "crewmate_attachment",
                json!({"id": id, "task": task, "name": name, "offset": out.len()}),
            )
            .await
            .unwrap_or_else(|e| panic!("crewmate_attachment: {e:#}"));
        out.extend(B64.decode(v["data_b64"].as_str().unwrap()).unwrap());
        if v["last"] == true {
            return out;
        }
    }
}

// The agent is a headless claude engine: it runs in the work dir with the prompt as its last argument, so
// the fake records both (what it was told, and what it can open from where it stands).
#[tokio::test(flavor = "multi_thread")]
async fn a_task_with_two_images_is_stored_staged_in_the_work_dir_and_named_in_the_prompt() {
    let _bin_lock = lock_agent_bin().await;
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let work = tmp.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    let script = format!(
        "for a in \"$@\"; do last=\"$a\"; done\n\
         printf '%s' \"$last\" > {out}/prompt.txt\n\
         ls -R .atlas/attachments > {out}/ls.txt 2>&1\n\
         printf '{{\"type\":\"result\",\"session_id\":\"s1\",\"result\":\"seen\",\"is_error\":false}}\\n'\n\
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

    let v = send(
        &client,
        &id,
        json!([sent("a.png", PNG), sent("b.jpg", JPG)]),
    )
    .await
    .unwrap();
    let task_id = v["task"]["id"].as_str().unwrap().to_string();

    // The reply and the stored task describe both images, never carrying their bytes.
    let atts = v["task"]["attachments"].as_array().unwrap();
    assert_eq!(atts.len(), 2, "{v}");
    assert_eq!(atts[0]["name"], "a.png");
    assert_eq!(atts[0]["mime"], "image/png");
    assert_eq!(atts[0]["bytes_len"], PNG.len());
    assert_eq!(atts[1]["mime"], "image/jpeg", "type from the bytes: {v}");
    assert_eq!(atts[0]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(
        atts[0]["path"],
        format!(".atlas/attachments/{task_id}/a.png")
    );
    assert!(!v.to_string().contains(&B64.encode(PNG)), "bytes leaked");

    wait_task_state(&client, &id, &task_id, "done", atlas_core::scaling::HANG).await;

    // The task keeps its images on the crew server...
    assert_eq!(fetch(&client, &id, &task_id, "a.png").await, PNG);
    assert_eq!(fetch(&client, &id, &task_id, "b.jpg").await, JPG);
    let stored = dir.join(format!("crew/{id}/attachments/{task_id}/a.png"));
    assert_eq!(std::fs::read(&stored).unwrap(), PNG);
    // ...and the agent found them in its work dir, where the prompt said they were.
    for n in ["a.png", "b.jpg"] {
        let staged = work.join(format!(".atlas/attachments/{task_id}/{n}"));
        assert!(staged.exists(), "{staged:?} was not staged");
    }
    let prompt = std::fs::read_to_string(tmp.path().join("prompt.txt")).unwrap();
    for n in ["a.png", "b.jpg"] {
        assert!(
            prompt.contains(&format!(".atlas/attachments/{task_id}/{n}")),
            "the prompt does not name {n}: {prompt}"
        );
    }
    // What the agent sees from its work dir is exactly the two files.
    let ls = std::fs::read_to_string(tmp.path().join("ls.txt")).unwrap();
    assert!(ls.contains("a.png") && ls.contains("b.jpg"), "{ls}");
    // The list the client reads back carries the records too (the thread renders from it).
    let listed = client
        .call("crewmate_tasks", json!({"id": id, "limit": 5}))
        .await
        .unwrap();
    assert_eq!(
        listed["tasks"][0]["attachments"].as_array().unwrap().len(),
        2
    );

    crate::crew::engine::set_test_engine_bin(None);
    client.call("shutdown", json!({})).await.ok();
}

// Every refusal leaves nothing behind: no task, no files.
#[tokio::test(flavor = "multi_thread")]
async fn a_task_that_breaks_a_rule_is_refused_whole_and_leaves_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Strict").await;

    let cases: Vec<(&str, Value, &str)> = vec![
        (
            "too many",
            json!(
                (0..5)
                    .map(|i| sent(&format!("{i}.png"), PNG))
                    .collect::<Vec<_>>()
            ),
            "at most 4",
        ),
        (
            "wrong magic bytes, declared png",
            json!([sent("shell.png", b"#!/bin/sh\nrm -rf ~")]),
            "not a png, jpeg, webp or gif",
        ),
        (
            "svg",
            json!([sent("x.svg", b"<svg onload=alert(1)/>")]),
            "not a png",
        ),
        ("empty", json!([sent("e.png", b"")]), "empty"),
        (
            "bad base64",
            json!([{"name": "a.png", "data_b64": "***"}]),
            "base64",
        ),
        (
            "neither bytes nor upload",
            json!([{"name": "a.png"}]),
            "data_b64 or upload",
        ),
        (
            "unknown upload",
            json!([{"name": "a.png", "upload": "doesnotexist1"}]),
            "was not found",
        ),
    ];
    for (what, atts, want) in cases {
        let e = send(&client, &id, atts).await.unwrap_err();
        assert!(e.contains(want), "{what}: {e}");
    }
    let listed = client
        .call("crewmate_tasks", json!({"id": id, "limit": 50}))
        .await
        .unwrap();
    assert_eq!(listed["tasks"], json!([]), "a refused task was queued");
    assert!(
        !dir.join(format!("crew/{id}/attachments")).exists()
            || std::fs::read_dir(dir.join(format!("crew/{id}/attachments")))
                .unwrap()
                .next()
                .is_none(),
        "files left behind"
    );
    client.call("shutdown", json!({})).await.ok();
}

// A protocol line is at most 1 MiB, so an image over 5 MiB sent inline (about 7 MB of base64) never reaches the
// crew code: hostd drops the line, says so, and keeps serving. Nothing is queued and nothing is stored. This is
// why large images go in pieces (`a_big_image_travels_in_pieces_both_ways`).
#[tokio::test(flavor = "multi_thread")]
async fn an_oversize_image_sent_inline_is_dropped_by_the_line_limit_and_leaves_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Line").await;

    let mut big = PNG.to_vec();
    big.resize(5 * 1024 * 1024 + 1, 0);
    let mut events = client.events();
    // Never answered: the reply id cannot be read from a dropped line. Not awaited.
    let c = client.clone();
    let (inline_id, inline) = (id.clone(), sent("big.png", &big));
    let stuck = tokio::spawn(async move { send(&c, &inline_id, json!([inline])).await });

    let said = tokio::time::timeout(atlas_core::scaling::HANG, async {
        loop {
            let ev = events.recv().await.expect("events closed");
            if ev["ev"] == "error" {
                return ev["message"].as_str().unwrap_or_default().to_string();
            }
        }
    })
    .await
    .expect("hostd never said it dropped the line");
    assert!(said.contains("bytes dropped"), "{said}");
    stuck.abort();

    // Still serving, and nothing was created.
    let listed = client
        .call("crewmate_tasks", json!({"id": id, "limit": 50}))
        .await
        .unwrap();
    assert_eq!(listed["tasks"], json!([]));
    assert!(!dir.join(format!("crew/{id}/attachments")).exists());
    client.call("shutdown", json!({})).await.ok();
}

// A name from the sender is never a path: `../` and absolute names end up inside the task's folder only.
#[tokio::test(flavor = "multi_thread")]
async fn a_path_in_the_name_cannot_leave_the_tasks_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Paths").await;

    let v = send(
        &client,
        &id,
        json!([
            sent("../../../../escape.png", PNG),
            sent("/tmp/atlas-attachment-escape.png", PNG),
            sent("..\\..\\win.png", PNG),
        ]),
    )
    .await
    .unwrap();
    let task_id = v["task"]["id"].as_str().unwrap();
    let task_dir = dir.join(format!("crew/{id}/attachments/{task_id}"));
    for a in v["task"]["attachments"].as_array().unwrap() {
        let name = a["name"].as_str().unwrap();
        assert!(!name.contains(['/', '\\']), "{name}");
        assert!(task_dir.join(name).exists(), "{name} not in the task dir");
    }
    assert!(!std::path::Path::new("/tmp/atlas-attachment-escape.png").exists());
    assert!(!dir.join("escape.png").exists() && !tmp.path().join("escape.png").exists());
    // Reading is by the recorded name only: a path is a miss, not a file.
    for hostile in ["../../tasks.jsonl", "/etc/passwd", "..", ""] {
        let e = client
            .call(
                "crewmate_attachment",
                json!({"id": id, "task": task_id, "name": hostile}),
            )
            .await
            .unwrap_err();
        assert!(
            format!("{e:#}").contains("no attachment"),
            "{hostile}: {e:#}"
        );
    }
    client.call("shutdown", json!({})).await.ok();
}

// An image over one protocol line goes in pieces and comes back in pieces; the result is byte-exact.
#[tokio::test(flavor = "multi_thread")]
async fn a_big_image_travels_in_pieces_both_ways() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Big").await;

    let mut img = PNG.to_vec();
    img.extend((0..3_000_000u32).map(|i| (i % 251) as u8)); // ~3 MB: three pieces
    let upload = "up-0123456789abcdef";
    let pieces: Vec<&[u8]> = img.chunks(crate::crew::attachments::CHUNK).collect();
    for (seq, p) in pieces.iter().enumerate() {
        let r = client
            .call(
                "crewmate_attachment_put",
                json!({"id": id, "upload": upload, "seq": seq, "data_b64": B64.encode(p),
                       "last": seq == pieces.len() - 1}),
            )
            .await
            .unwrap();
        assert!(r["staged"].as_u64().unwrap() > 0);
    }
    let v = send(&client, &id, json!([{"name": "big.png", "upload": upload}]))
        .await
        .unwrap();
    let task_id = v["task"]["id"].as_str().unwrap();
    assert_eq!(v["task"]["attachments"][0]["bytes_len"], img.len());
    assert_eq!(fetch(&client, &id, task_id, "big.png").await, img);
    // The staged upload is gone: it belongs to the task now.
    assert!(!dir.join(format!("crew/{id}/uploads/{upload}")).exists());
    client.call("shutdown", json!({})).await.ok();
}

// Pieces out of order, an upload id that is a path, and an upload that never stops growing are refused.
#[tokio::test(flavor = "multi_thread")]
async fn uploads_are_ordered_named_safely_and_capped() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let id = create_crewmate(&client, "Up").await;
    let put = |upload: &str, seq: u32, data: &[u8]| {
        let client = &client;
        let args = json!({"id": id, "upload": upload, "seq": seq, "data_b64": B64.encode(data)});
        async move {
            client
                .call("crewmate_attachment_put", args)
                .await
                .map_err(|e| format!("{e:#}"))
        }
    };
    assert!(
        put("../../etc/x", 0, PNG)
            .await
            .unwrap_err()
            .contains("upload must be")
    );
    assert!(
        put("short", 0, PNG)
            .await
            .unwrap_err()
            .contains("upload must be")
    );
    assert!(
        put("order-test-0001", 1, PNG)
            .await
            .unwrap_err()
            .contains("expected piece 0")
    );
    put("order-test-0001", 0, PNG).await.unwrap();
    assert!(
        put("order-test-0001", 0, PNG)
            .await
            .unwrap_err()
            .contains("expected piece 1")
    );
    // A piece over the chunk size is refused outright.
    let over = vec![0u8; crate::crew::attachments::CHUNK + 1000];
    assert!(
        put("order-test-0002", 0, &over)
            .await
            .unwrap_err()
            .contains("at most")
    );
    // Ten chunk-sized pieces pass 5 MiB: the upload is dropped at the one that crosses it.
    let full = vec![1u8; crate::crew::attachments::CHUNK];
    let mut refused = None;
    for seq in 0..11 {
        if let Err(e) = put("endless-up-0001", seq, &full).await {
            refused = Some(e);
            break;
        }
    }
    assert!(
        refused
            .expect("the upload was never capped")
            .contains("over 5 MiB")
    );
    assert!(
        !dir.join(format!("crew/{id}/uploads/endless-up-0001"))
            .exists()
    );
    client.call("shutdown", json!({})).await.ok();
}

// A phone without `read` cannot fetch an image (scope arms, the relay path included); with `read` but not
// `act` it can look but neither upload nor send.
#[test]
fn fetching_an_image_needs_read_and_sending_one_needs_act() {
    use crate::mobile::{Scope, required_scope};
    use crate::protocol::HostCmd;
    let parse = |v: Value| -> HostCmd { serde_json::from_value(v).unwrap() };
    let fetch =
        json!({"cmd": "crewmate_attachment", "args": {"id": "c", "task": "t", "name": "a.png"}});
    let put = json!({"cmd": "crewmate_attachment_put",
                     "args": {"id": "c", "upload": "up-0123456789", "seq": 0, "data_b64": ""}});
    let task = json!({"cmd": "crewmate_task", "args": {"id": "c", "prompt": "p", "source": "mobile",
                      "attachments": [{"name": "a.png", "data_b64": "AA=="}]}});
    assert_eq!(required_scope(&parse(fetch.clone())), Ok(Scope::Read));
    assert_eq!(required_scope(&parse(put.clone())), Ok(Scope::Act));
    assert_eq!(required_scope(&parse(task.clone())), Ok(Scope::Act));
    // The same through the Mac's relay.
    for (inner, want) in [
        (&fetch, Scope::Read),
        (&put, Scope::Act),
        (&task, Scope::Act),
    ] {
        let relayed = json!({"cmd": "crew_call", "args": {"server": "s",
            "cmd": inner["cmd"], "args": inner["args"]}});
        assert_eq!(required_scope(&parse(relayed)), Ok(want), "{inner}");
    }
}

// A paired phone that was not granted `read` is refused by the real gateway, not just by the table.
#[tokio::test(flavor = "multi_thread")]
async fn a_phone_without_read_cannot_fetch_an_attachment_through_the_gateway() {
    use crate::mobile::tests::{Phone, pair_start};
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let client = daemon(&dir).await;
    let info = pair_start(&client).await;
    let mut phone = Phone::dial(&info).await;
    let ok = phone
        .call("pair", json!({"code": info["code"], "name": "iPhone"}))
        .await
        .unwrap();
    assert_eq!(ok["ok"], true, "{ok}");
    let node = client.call("mobile_devices", json!({})).await.unwrap()["devices"][0]["node_id"]
        .as_str()
        .unwrap()
        .to_string();
    let fetch = json!({"id": "c", "task": "t", "name": "a.png"});

    // `act` alone: it may send a task, never read an image back.
    client
        .call(
            "mobile_set_scopes",
            json!({"node_id": node, "scopes": ["act"]}),
        )
        .await
        .unwrap();
    let r = phone
        .call("crewmate_attachment", fetch.clone())
        .await
        .unwrap();
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(r["error"], "forbidden: read", "{r}");

    // With `read` it gets past the gateway and reaches the crew handler (which has no such crewmate).
    client
        .call(
            "mobile_set_scopes",
            json!({"node_id": node, "scopes": ["read"]}),
        )
        .await
        .unwrap();
    let r = phone.call("crewmate_attachment", fetch).await.unwrap();
    assert_eq!(r["error"], "crewmate c not found", "{r}");

    // `read` without `act` cannot upload a piece.
    let r = phone
        .call(
            "crewmate_attachment_put",
            json!({"id": "c", "upload": "up-0123456789", "seq": 0, "data_b64": ""}),
        )
        .await
        .unwrap();
    assert_eq!(r["error"], "forbidden: act", "{r}");
    client.call("shutdown", json!({})).await.ok();
}

// An upload abandoned before a restart is thrown away when the crew server starts: nothing else would ever
// notice it if its crewmate never uploaded again. A fresh one (a phone still sending) survives the restart.
#[tokio::test(flavor = "multi_thread")]
async fn a_crew_server_start_removes_uploads_abandoned_before_it() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let first = daemon(&dir).await;
    let id = create_crewmate(&first, "Sweep").await;
    for upload in ["stale-upload-01", "fresh-upload-01"] {
        first
            .call(
                "crewmate_attachment_put",
                json!({"id": id, "upload": upload, "seq": 0, "data_b64": B64.encode(PNG)}),
            )
            .await
            .unwrap();
    }
    first.call("shutdown", json!({})).await.ok();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let uploads = dir.join(format!("crew/{id}/uploads"));
    let hour_and_a_bit = crate::crew::attachments::UPLOAD_TTL + Duration::from_secs(60);
    std::fs::OpenOptions::new()
        .write(true)
        .open(uploads.join("stale-upload-01/seq"))
        .unwrap()
        .set_modified(std::time::SystemTime::now() - hour_and_a_bit)
        .unwrap();

    let second = daemon(&dir).await;
    assert!(
        !uploads.join("stale-upload-01").exists(),
        "stale upload survived the start"
    );
    assert!(
        uploads.join("fresh-upload-01").exists(),
        "a fresh upload was swept"
    );
    second.call("shutdown", json!({})).await.ok();
}

// ── Telegram ─────────────────────────────────────────────────────────────────

/// A fake Telegram Bot API on loopback: `getUpdates` hands over `updates` once, `getFile` names a path,
/// `/file/bot<token>/<path>` serves `photo`, and every `sendMessage` text is recorded. Returns the base URL,
/// the paths requested (in order) and the texts sent.
pub(super) async fn fake_telegram(
    updates: Value,
    photo: Vec<u8>,
    file_size: u64,
) -> (
    String,
    std::sync::Arc<parking_lot::Mutex<Vec<String>>>,
    std::sync::Arc<parking_lot::Mutex<Vec<String>>>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let api = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", api.local_addr().unwrap());
    let paths = std::sync::Arc::new(parking_lot::Mutex::new(vec![]));
    let sent = std::sync::Arc::new(parking_lot::Mutex::new(vec![]));
    let (p, s) = (paths.clone(), sent.clone());
    let handed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = api.accept().await else {
                return;
            };
            let (p, s, handed, updates, photo) = (
                p.clone(),
                s.clone(),
                handed.clone(),
                updates.clone(),
                photo.clone(),
            );
            tokio::spawn(async move {
                let mut buf = vec![0u8; 65536];
                let mut n = sock.read(&mut buf).await.unwrap_or(0);
                // The body may arrive after the headers.
                let head_end = |b: &[u8]| b.windows(4).position(|w| w == b"\r\n\r\n");
                while head_end(&buf[..n]).is_none() && n < buf.len() {
                    let m = sock.read(&mut buf[n..]).await.unwrap_or(0);
                    if m == 0 {
                        break;
                    }
                    n += m;
                }
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let line = req.lines().next().unwrap_or("").to_string();
                let path = line.split(' ').nth(1).unwrap_or("").to_string();
                p.lock().push(path.clone());
                let (ctype, body): (&str, Vec<u8>) = if path.contains("/getUpdates") {
                    let first = !handed.swap(true, std::sync::atomic::Ordering::SeqCst);
                    let r = if first { updates } else { json!([]) };
                    if !first {
                        // Hold the next long poll: nothing new comes.
                        tokio::time::sleep(Duration::from_secs(30)).await;
                    }
                    (
                        "application/json",
                        json!({"ok": true, "result": r}).to_string().into_bytes(),
                    )
                } else if path.contains("/getFile") {
                    (
                        "application/json",
                        json!({"ok": true, "result": {"file_id": "f", "file_size": file_size,
                               "file_path": "photos/file_1.jpg"}})
                        .to_string()
                        .into_bytes(),
                    )
                } else if path.contains("/file/bot") {
                    ("image/jpeg", photo)
                } else {
                    if path.contains("/sendMessage") {
                        let body = req.split("\r\n\r\n").nth(1).unwrap_or("");
                        if let Ok(v) = serde_json::from_str::<Value>(body) {
                            s.lock().push(v["text"].as_str().unwrap_or("").to_string());
                        }
                    }
                    ("application/json", br#"{"ok":true,"result":{}}"#.to_vec())
                };
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: {ctype}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(&body).await;
            });
        }
    });
    (base, paths, sent)
}

/// A hostd whose Telegram bridge talks to `base`, with chat 42 already linked. `ATLAS_TELEGRAM_API` is read
/// when the daemon starts and is process-wide, so it is held only for that moment, under one lock.
pub(super) async fn daemon_with_telegram(dir: &std::path::Path, base: &str) -> HostClient {
    static ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("telegram.json"),
        json!({"token": "123:abc", "chats": [42]}).to_string(),
    )
    .unwrap();
    let _g = ENV.lock().await;
    // SAFETY: serialised by ENV; nothing else in the test binary reads it between set and the daemon's start.
    unsafe { std::env::set_var("ATLAS_TELEGRAM_API", base) };
    let client = daemon(dir).await;
    unsafe { std::env::remove_var("ATLAS_TELEGRAM_API") };
    client
}

pub(super) async fn wait_for<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    for _ in 0..1200 {
        if let Some(v) = f() {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for {what}");
}

fn photo_update(caption: Option<&str>) -> Value {
    let mut msg = json!({
        "message_id": 7, "chat": {"id": 42},
        // Telegram sends every size of one photo, smallest first.
        "photo": [
            {"file_id": "small", "width": 90, "height": 90, "file_size": 1000},
            {"file_id": "large", "width": 1280, "height": 960, "file_size": 90000},
        ],
    });
    if let Some(c) = caption {
        msg["caption"] = json!(c);
    }
    json!([{"update_id": 1, "message": msg}])
}

// A photo sent to the linked chat becomes a task with the image; the largest size is the one downloaded,
// through Telegram's own `/bot<token>/` paths.
#[tokio::test(flavor = "multi_thread")]
async fn a_telegram_photo_creates_a_task_with_the_image() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let (base, paths, sent) = fake_telegram(
        photo_update(Some("the header is cut off")),
        PNG.to_vec(),
        PNG.len() as u64,
    )
    .await;
    // The crewmate has to exist before the photo arrives: create it in a first daemon, then restart on the dir.
    let first = daemon(&dir).await;
    let id = create_crewmate(&first, "Eyes").await;
    first.call("shutdown", json!({})).await.ok();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let client = daemon_with_telegram(&dir, &base).await;
    let task = wait_for("the photo's task", || {
        let t = std::fs::read_to_string(dir.join(format!("crew/{id}/tasks.jsonl"))).ok()?;
        t.lines()
            .next()
            .and_then(|l| serde_json::from_str::<Value>(l).ok())
    })
    .await;
    assert_eq!(task["source"], "telegram");
    assert_eq!(task["prompt"], "the header is cut off");
    let atts = task["attachments"].as_array().unwrap();
    assert_eq!(atts.len(), 1, "{task}");
    assert_eq!(atts[0]["mime"], "image/png", "type from the bytes: {task}");
    let task_id = task["id"].as_str().unwrap();
    assert_eq!(
        fetch(&client, &id, task_id, atts[0]["name"].as_str().unwrap()).await,
        PNG
    );

    let seen = paths.lock().clone();
    assert!(
        seen.iter().any(|p| p == "/bot123:abc/getFile"),
        "getFile must go through /bot<token>/: {seen:?}"
    );
    assert!(
        seen.iter()
            .any(|p| p == "/file/bot123:abc/photos/file_1.jpg"),
        "the file must come from /file/bot<token>/: {seen:?}"
    );
    let queued = wait_for("the confirmation", || sent.lock().first().cloned()).await;
    assert!(queued.contains("queued"), "{queued}");
    client.call("shutdown", json!({})).await.ok();
}

// A photo whose file Telegram says is over the cap is refused without being downloaded, and the chat is told.
#[tokio::test(flavor = "multi_thread")]
async fn a_telegram_photo_over_the_cap_is_refused_and_the_chat_is_told() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let (base, paths, sent) =
        fake_telegram(photo_update(None), PNG.to_vec(), 5 * 1024 * 1024 + 1).await;
    let first = daemon(&dir).await;
    let id = create_crewmate(&first, "Eyes").await;
    first.call("shutdown", json!({})).await.ok();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let client = daemon_with_telegram(&dir, &base).await;
    let said = wait_for("the refusal", || sent.lock().first().cloned()).await;
    assert!(said.contains("5 MiB"), "{said}");
    assert!(
        !paths.lock().iter().any(|p| p.contains("/file/bot")),
        "an oversize file must not be downloaded"
    );
    let listed = client
        .call("crewmate_tasks", json!({"id": id, "limit": 5}))
        .await
        .unwrap();
    assert_eq!(listed["tasks"], json!([]));
    client.call("shutdown", json!({})).await.ok();
}

// What Telegram serves is not trusted either: a "photo" that is not an image by its bytes is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_telegram_photo_that_is_not_an_image_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hostd");
    let body = b"#!/bin/sh\necho pwned".to_vec();
    let (base, _paths, sent) =
        fake_telegram(photo_update(None), body.clone(), body.len() as u64).await;
    let first = daemon(&dir).await;
    let id = create_crewmate(&first, "Eyes").await;
    first.call("shutdown", json!({})).await.ok();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let client = daemon_with_telegram(&dir, &base).await;
    let said = wait_for("the refusal", || sent.lock().first().cloned()).await;
    assert!(said.contains("not a png, jpeg, webp or gif"), "{said}");
    let listed = client
        .call("crewmate_tasks", json!({"id": id, "limit": 5}))
        .await
        .unwrap();
    assert_eq!(listed["tasks"], json!([]));
    client.call("shutdown", json!({})).await.ok();
}
