//! Images on a task (#209): what a user sends with a prompt (a screenshot, a photo of a screen), kept on the
//! crew server next to the task and shown to the agent as files in its work dir.
//!
//! The rules live here and nowhere else, so every way in (the apps' `crewmate_task`, a Telegram photo) is
//! held to the same ones:
//! - at most [`MAX_ATTACHMENTS`] per task, [`MAX_BYTES`] each;
//! - png, jpeg, webp or gif **by the file's own first bytes**: the declared mime is never believed;
//! - the name is the sender's, so it is reduced to one safe path component ([`safe_name`]): a `../` or an
//!   absolute path can never leave the task's folder.
//!
//! On disk: `<crewmate dir>/attachments/<task id>/<name>` is the task's copy (the source of truth the apps
//! fetch with `crewmate_attachment`); `<work dir>/.atlas/attachments/<task id>/<name>` is the agent's copy,
//! rewritten each time the task starts so a restart or an edited work tree never loses it.
//!
//! A line on the hostd protocol is at most 1 MiB (`protocol::MAX_LINE_BYTES`, both ways), so bytes travel in
//! pieces of [`CHUNK`]: `crewmate_attachment_put` uploads, `crewmate_attachment` downloads. An image small
//! enough for one line may instead ride inline in `crewmate_task` itself.

use std::io;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::store::CrewStore;

/// Images a task takes.
pub(crate) const MAX_ATTACHMENTS: usize = 4;
/// The largest image, in bytes (5 MiB).
pub(crate) const MAX_BYTES: usize = 5 * 1024 * 1024;
/// Raw bytes per protocol line: base64 makes 4/3 of it, well under `MAX_LINE_BYTES`.
pub(crate) const CHUNK: usize = 512 * 1024;
/// The longest file name kept, in bytes.
const MAX_NAME: usize = 100;

/// An image kept with a task: what `Task.attachments` holds and the apps read. The bytes are never in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    /// A single safe path component, unique within its task.
    pub name: String,
    /// `image/png|jpeg|webp|gif`, from the bytes.
    pub mime: String,
    pub bytes_len: u64,
    /// Lowercase hex SHA-256 of the bytes.
    pub sha256: String,
    /// Where the agent finds it, relative to its work dir: `.atlas/attachments/<task id>/<name>`.
    pub path: String,
    /// Who attached it: `"agent"` for a capture the crewmate took, empty for what the owner sent (left out of
    /// the JSON then, so records and clients that predate this read exactly what they always did).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

/// [`Attachment::source`] of the captures an agent took.
pub(crate) const SOURCE_AGENT: &str = "agent";
/// Images the agent may attach to one task (captures: a matrix is up to 16 a call). Not the owner's cap.
pub(crate) const MAX_AGENT_ATTACHMENTS: usize = 32;
/// Agent images a `ui` task needs before it is `done` (before and after).
pub(crate) const PROOF_IMAGES: usize = 2;

/// The image kind of `bytes` by its signature, `None` for anything else (an SVG, a PDF, a renamed `.exe`).
pub(crate) fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// The extension a kind gets when the sender's name has none that fits.
fn ext(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        _ => "webp",
    }
}

/// `name` as one path component that stays inside a folder: its last path segment, with every character
/// outside `[A-Za-z0-9._-]` turned into `_`, no leading dot, cut to [`MAX_NAME`]. Its extension is made to
/// match `mime` (the bytes decide what the file is, not what the sender called it).
pub(crate) fn safe_name(name: &str, mime: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = last
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let (stem, _) = cleaned.rsplit_once('.').unwrap_or((&cleaned, ""));
    let stem = stem.trim_matches(|c| c == '.' || c == '_');
    let stem = if stem.is_empty() { "image" } else { stem };
    let mut stem: String = stem.chars().take(MAX_NAME - 5).collect();
    stem.push('.');
    stem.push_str(ext(mime));
    stem
}

/// One image as it arrives, before it is stored.
pub(crate) struct Incoming {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// Check `images` and turn them into the records the task keeps. Refused with a message the sender can
/// read: too many, empty, over [`MAX_BYTES`], not an image by its bytes. Names are made safe and unique
/// (`a.png`, `a-2.png`) in order. Nothing is written.
#[cfg(test)]
pub(crate) fn check(
    task_id: &str,
    images: &[Incoming],
) -> Result<Vec<(Attachment, Vec<u8>)>, String> {
    if images.len() > MAX_ATTACHMENTS {
        return Err(format!(
            "a task takes at most {MAX_ATTACHMENTS} images, got {}",
            images.len()
        ));
    }
    check_more(task_id, &[], images)
}

/// [`check`] for images that join the `taken` ones: the names it makes are unique against those too, and
/// the count is not capped here (the caller owns the cap: [`add`]).
fn check_more(
    task_id: &str,
    taken: &[Attachment],
    images: &[Incoming],
) -> Result<Vec<(Attachment, Vec<u8>)>, String> {
    let mut out: Vec<(Attachment, Vec<u8>)> = Vec::with_capacity(images.len());
    for img in images {
        if img.bytes.is_empty() {
            return Err(format!("{} is empty", shown(&img.name)));
        }
        if img.bytes.len() > MAX_BYTES {
            return Err(format!(
                "{} is over {} MiB",
                shown(&img.name),
                MAX_BYTES / (1024 * 1024)
            ));
        }
        let Some(mime) = sniff(&img.bytes) else {
            return Err(format!(
                "{} is not a png, jpeg, webp or gif image",
                shown(&img.name)
            ));
        };
        let mut name = safe_name(&img.name, mime);
        let mut n = 2;
        while out.iter().any(|(a, _)| a.name == name) || taken.iter().any(|a| a.name == name) {
            let (stem, e) = name.rsplit_once('.').unwrap_or((&name, ""));
            let stem = stem.trim_end_matches(|c: char| c.is_ascii_digit() || c == '-');
            name = format!("{stem}-{n}.{e}");
            n += 1;
        }
        let mut sha = Sha256::new();
        sha.update(&img.bytes);
        out.push((
            Attachment {
                path: format!("{}/{task_id}/{name}", WORK_SUBDIR),
                name,
                mime: mime.to_string(),
                bytes_len: img.bytes.len() as u64,
                sha256: hex::encode(sha.finalize()),
                source: String::new(),
            },
            img.bytes.clone(),
        ));
    }
    Ok(out)
}

/// A name for an error message: the sender's, shortened and printable.
fn shown(name: &str) -> String {
    let n: String = name.chars().filter(|c| !c.is_control()).take(40).collect();
    if n.is_empty() {
        "an image".to_string()
    } else {
        format!("\"{n}\"")
    }
}

/// Where the agent's copies live inside its work dir.
pub(crate) const WORK_SUBDIR: &str = ".atlas/attachments";

/// `<crewmate dir>/attachments/<task id>/`
pub(crate) fn task_dir(store: &CrewStore, crewmate_id: &str, task_id: &str) -> PathBuf {
    store
        .crewmate_dir(crewmate_id)
        .join("attachments")
        .join(task_id)
}

/// Write the checked images under the task's folder (0600). Either all land or the folder is removed.
pub(crate) fn save(
    store: &CrewStore,
    crewmate_id: &str,
    task_id: &str,
    files: &[(Attachment, Vec<u8>)],
) -> io::Result<()> {
    let dir = task_dir(store, crewmate_id, task_id);
    for (a, bytes) in files {
        if let Err(e) = crate::paths::write_private(&dir.join(&a.name), bytes) {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
    }
    Ok(())
}

/// The bytes of one stored image, `name` being one of the task's recorded names (anything else is a miss:
/// the name is looked up in the record, never joined to a path as given).
pub(crate) fn read(
    store: &CrewStore,
    crewmate_id: &str,
    task: &super::store::Task,
    name: &str,
) -> Result<(Attachment, Vec<u8>), String> {
    let a = task
        .attachments
        .iter()
        .find(|a| a.name == name)
        .ok_or_else(|| format!("task {} has no attachment {name:?}", task.id))?;
    let bytes = std::fs::read(task_dir(store, crewmate_id, &task.id).join(&a.name))
        .map_err(|e| format!("attachment {name:?} cannot be read: {e}"))?;
    Ok((a.clone(), bytes))
}

/// Copy the task's images into `work_dir/.atlas/attachments/<task id>/` for the agent, and return the
/// paths (relative to the work dir) that now exist. An image whose stored copy is gone is skipped and
/// named in hostd's log: the task still runs, the prompt only lists what the agent can open.
pub(crate) fn stage(
    store: &CrewStore,
    crewmate_id: &str,
    task: &super::store::Task,
    work_dir: &Path,
) -> Vec<String> {
    let mut staged = vec![];
    for a in &task.attachments {
        let src = task_dir(store, crewmate_id, &task.id).join(&a.name);
        let rel = format!("{WORK_SUBDIR}/{}/{}", task.id, a.name);
        let dst = work_dir.join(&rel);
        let copied = dst
            .parent()
            .map(std::fs::create_dir_all)
            .transpose()
            .and_then(|_| std::fs::copy(&src, &dst));
        match copied {
            Ok(_) => staged.push(rel),
            Err(e) => eprintln!(
                "atlas crew: attachment {} of task {} not staged: {e}",
                a.name, task.id
            ),
        }
    }
    if !staged.is_empty() {
        keep_out_of_git(work_dir);
    }
    staged
}

/// Add `.atlas/attachments/` to the checkout's local `info/exclude`, so an agent's `git add -A` never commits
/// the user's screenshots (the project's own `.gitignore` is never touched). Best effort: a work dir that is
/// not a git checkout has nothing to exclude from.
fn keep_out_of_git(work_dir: &Path) {
    let Ok(out) = std::process::Command::new("git")
        .args(["rev-parse", "--git-path", "info/exclude"])
        .current_dir(work_dir)
        .output()
    else {
        return;
    };
    if !out.status.success() {
        return;
    }
    let rel = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let path = if Path::new(&rel).is_absolute() {
        PathBuf::from(rel)
    } else {
        work_dir.join(rel)
    };
    let line = format!("/{WORK_SUBDIR}/");
    let have = std::fs::read_to_string(&path).unwrap_or_default();
    if have.lines().any(|l| l.trim() == line) {
        return;
    }
    let sep = if have.is_empty() || have.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let _ = path
        .parent()
        .map(std::fs::create_dir_all)
        .transpose()
        .and_then(|_| std::fs::write(&path, format!("{have}{sep}{line}\n")));
}

/// The line of the prompt that names the task's images, empty without any. Ours, not the sender's: it holds
/// only paths built from sanitized names, so it goes before the (possibly untrusted) task text.
pub(crate) fn prompt_block(staged: &[String]) -> String {
    if staged.is_empty() {
        return String::new();
    }
    let list = staged.join(", ");
    format!(
        "The user attached {} image{} to this task, in your working directory: {list}. Open and look at {} before you answer.\n\n",
        staged.len(),
        if staged.len() == 1 { "" } else { "s" },
        if staged.len() == 1 { "it" } else { "them" },
    )
}

/// A reply piece of a stored image, for `crewmate_attachment`: `limit` is clamped to [`CHUNK`].
pub(crate) fn piece(bytes: &[u8], offset: u64, limit: Option<u64>) -> (String, bool) {
    let start = usize::try_from(offset)
        .unwrap_or(usize::MAX)
        .min(bytes.len());
    let n = usize::try_from(limit.unwrap_or(CHUNK as u64))
        .unwrap_or(CHUNK)
        .clamp(1, CHUNK);
    let end = (start + n).min(bytes.len());
    (B64.encode(&bytes[start..end]), end >= bytes.len())
}

/// The `crewmate_attachment` reply for the piece of `bytes` at `offset`. Built here for the handler and for
/// the contract fixtures both apps read, so they cannot drift.
pub(crate) fn reply(
    a: &Attachment,
    bytes: &[u8],
    offset: u64,
    limit: Option<u64>,
) -> serde_json::Value {
    let (data_b64, last) = piece(bytes, offset, limit);
    serde_json::json!({
        "name": a.name,
        "mime": a.mime,
        "bytes_len": a.bytes_len,
        "offset": offset,
        "data_b64": data_b64,
        "last": last,
    })
}

/// Decode a sender's base64, refusing early what could never fit so a huge string is not decoded at all.
pub(crate) fn decode(name: &str, data_b64: &str) -> Result<Vec<u8>, String> {
    if data_b64.len() > MAX_BYTES.div_ceil(3) * 4 + 4 {
        return Err(format!(
            "{} is over {} MiB",
            shown(name),
            MAX_BYTES / (1024 * 1024)
        ));
    }
    B64.decode(data_b64.trim())
        .map_err(|_| format!("{} is not valid base64", shown(name)))
}

/// One image in a `crewmate_task`: its bytes inline (`data_b64`, for one that fits a protocol line) or
/// by the id of an upload (`crewmate_attachment_put`). The `mime` a client declares is ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct Sent {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub mime: Option<String>,
    #[serde(default)]
    pub data_b64: Option<String>,
    #[serde(default)]
    pub upload: Option<String>,
}

/// An upload id is a client-chosen token that becomes a folder name: only `[A-Za-z0-9_-]`, 8 to 64 long.
fn valid_upload(id: &str) -> bool {
    (8..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// `<crewmate dir>/uploads/<upload id>/`: pieces of images on their way, before a task owns them.
fn upload_dir(store: &CrewStore, crewmate_id: &str, upload: &str) -> PathBuf {
    store.crewmate_dir(crewmate_id).join("uploads").join(upload)
}

/// How long an upload may sit without a new piece before it is thrown away (an app killed mid-send, a network
/// that dropped). Each piece renews it.
pub(crate) const UPLOAD_TTL: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// Unfinished uploads one crewmate may hold at once: two tasks' worth of images. A new upload past this is
/// refused; continuing one that exists never is. Together with the 5 MiB cap per upload and the TTL it bounds
/// what a client (or bad luck over months) can leave on the crew server's disk.
const MAX_LIVE_UPLOADS: usize = MAX_ATTACHMENTS * 2;

/// The upload dirs of `uploads` (a crewmate's `uploads/`): real directories with a valid upload id for a name.
/// A stray file or an odd name is not ours and is never listed, so never removed.
fn upload_dirs(uploads: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(uploads) else {
        return vec![];
    };
    rd.filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| e.file_name().to_str().is_some_and(valid_upload))
        .map(|e| e.path())
        .collect()
}

fn live_uploads(uploads: &Path) -> usize {
    upload_dirs(uploads).len()
}

/// When an upload last got a piece: its `seq` file is rewritten by every one. A dir without it (a crash between
/// creating the dir and the first write) falls back to the dir's own time, so it too ages out.
fn upload_touched(dir: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(dir.join("seq"))
        .or_else(|_| std::fs::metadata(dir))
        .and_then(|m| m.modified())
        .ok()
}

/// Remove the uploads of one `uploads/` dir that are older than [`UPLOAD_TTL`]. Best effort, logged: a sweep
/// that cannot remove something must never fail the upload that triggered it.
fn sweep_dir(uploads: &Path) {
    let now = std::time::SystemTime::now();
    for dir in upload_dirs(uploads) {
        let stale = upload_touched(&dir)
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > UPLOAD_TTL);
        if stale && let Err(e) = std::fs::remove_dir_all(&dir) {
            eprintln!(
                "atlas crew: stale upload {} not removed: {e}",
                dir.display()
            );
        }
    }
}

/// Remove every crewmate's stale uploads. Run when the crew server starts: an upload abandoned before a restart
/// would otherwise wait for its crewmate's next upload to be noticed, and a crewmate that never uploads again
/// would keep it forever.
pub(crate) fn sweep_stale_uploads(store: &CrewStore) {
    for c in store.list_crewmate_dirs() {
        sweep_dir(&c.join("uploads"));
    }
}

/// Stage piece `seq` of upload `upload`. Pieces arrive in order from 0 (a gap, a repeat or a piece over
/// [`CHUNK`] is refused), and the whole may never pass [`MAX_BYTES`]: a client cannot fill the disk with
/// an upload it never finishes. Returns the bytes staged so far.
pub(crate) fn put_piece(
    store: &CrewStore,
    crewmate_id: &str,
    upload: &str,
    seq: u32,
    data_b64: &str,
) -> Result<u64, String> {
    use std::io::Write as _;
    if !valid_upload(upload) {
        return Err("upload must be 8 to 64 letters, digits, - or _".into());
    }
    if data_b64.len() > CHUNK.div_ceil(3) * 4 + 4 {
        return Err(format!("a piece is at most {} KiB", CHUNK / 1024));
    }
    let bytes = B64
        .decode(data_b64.trim())
        .map_err(|_| "piece is not valid base64".to_string())?;
    // Before this one is looked at: whatever was abandoned is thrown away, so a stale upload never counts
    // against the limit below. A crewmate can only ever hold so many unfinished uploads.
    let uploads = store.crewmate_dir(crewmate_id).join("uploads");
    sweep_dir(&uploads);
    let dir = upload_dir(store, crewmate_id, upload);
    if !dir.exists() && live_uploads(&uploads) >= MAX_LIVE_UPLOADS {
        return Err(format!(
            "too many unfinished uploads ({MAX_LIVE_UPLOADS}); send the task or wait"
        ));
    }
    let file = dir.join("data");
    let have = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
    let expected = std::fs::read_to_string(dir.join("seq"))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);
    if seq != expected {
        return Err(format!("expected piece {expected}, got {seq}"));
    }
    if have + bytes.len() as u64 > MAX_BYTES as u64 {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(format!("upload is over {} MiB", MAX_BYTES / (1024 * 1024)));
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    atlas_core::sys::owner_only(std::fs::OpenOptions::new().create(true).append(true))
        .open(&file)
        .and_then(|mut f| f.write_all(&bytes))
        .map_err(|e| format!("upload: {e}"))?;
    std::fs::write(dir.join("seq"), (seq + 1).to_string()).map_err(|e| e.to_string())?;
    Ok(have + bytes.len() as u64)
}

/// The staged bytes of `upload`, which is then removed (an upload belongs to one task).
fn take_upload(store: &CrewStore, crewmate_id: &str, upload: &str) -> Result<Vec<u8>, String> {
    if !valid_upload(upload) {
        return Err("upload must be 8 to 64 letters, digits, - or _".into());
    }
    let dir = upload_dir(store, crewmate_id, upload);
    let bytes = std::fs::read(dir.join("data"))
        .map_err(|_| format!("upload {upload} was not found (send its pieces first)"))?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(bytes)
}

/// Remove the staged pieces of `uploads` (a task that was refused must not leave its images on disk).
pub(crate) fn discard_uploads(store: &CrewStore, crewmate_id: &str, sent: &[Sent]) {
    for u in sent.iter().filter_map(|s| s.upload.as_deref()) {
        if valid_upload(u) {
            let _ = std::fs::remove_dir_all(upload_dir(store, crewmate_id, u));
        }
    }
}

/// The images of a `crewmate_task`, decoded and read from their uploads, ready for [`check`]. An item with
/// neither bytes nor an upload, or with both, is refused.
pub(crate) fn resolve(
    store: &CrewStore,
    crewmate_id: &str,
    sent: &[Sent],
) -> Result<Vec<Incoming>, String> {
    if sent.len() > MAX_ATTACHMENTS {
        return Err(format!(
            "a task takes at most {MAX_ATTACHMENTS} images, got {}",
            sent.len()
        ));
    }
    sent.iter()
        .map(|s| {
            let bytes = match (&s.data_b64, &s.upload) {
                (Some(d), None) => decode(&s.name, d)?,
                (None, Some(u)) => take_upload(store, crewmate_id, u)?,
                _ => {
                    return Err(format!(
                        "{} needs data_b64 or upload, not both or neither",
                        shown(&s.name)
                    ));
                }
            };
            Ok(Incoming {
                name: s.name.clone(),
                bytes,
            })
        })
        .collect()
}

/// Check `images`, store them and record them on `task` (`task.attachments`). The one door every way in
/// goes through (the apps' `crewmate_task`, a Telegram photo). Nothing is left on disk when it fails.
pub(crate) fn attach(
    store: &CrewStore,
    crewmate_id: &str,
    task: &mut super::store::Task,
    images: &[Incoming],
) -> Result<(), String> {
    add(store, crewmate_id, task, images, "")
}

/// [`attach`] for images that join the ones the task already has: the owner's marked screenshot with a
/// retry (`source` ""), or the agent's captures ([`SOURCE_AGENT`]). Names stay unique against what is
/// there, and the cap is the task's total for that source ([`MAX_ATTACHMENTS`] for the owner,
/// [`MAX_AGENT_ATTACHMENTS`] for the agent). Nothing is left on disk when it fails.
pub(crate) fn add(
    store: &CrewStore,
    crewmate_id: &str,
    task: &mut super::store::Task,
    images: &[Incoming],
    source: &str,
) -> Result<(), String> {
    if images.is_empty() {
        return Ok(());
    }
    let cap = if source == SOURCE_AGENT {
        MAX_AGENT_ATTACHMENTS
    } else {
        MAX_ATTACHMENTS
    };
    let have = task
        .attachments
        .iter()
        .filter(|a| a.source == source)
        .count();
    if have + images.len() > cap {
        return Err(format!(
            "a task takes at most {cap} {}images, it has {have} and {} more were sent",
            if source == SOURCE_AGENT {
                "capture "
            } else {
                ""
            },
            images.len()
        ));
    }
    let mut files = check_more(&task.id, &task.attachments, images)?;
    for (a, _) in &mut files {
        a.source = source.to_string();
    }
    save(store, crewmate_id, &task.id, &files).map_err(|e| format!("attachment: {e}"))?;
    task.attachments.extend(files.into_iter().map(|(a, _)| a));
    Ok(())
}

/// Store the agent's `files` (PNGs a capture wrote) as images of the task `task_id`, source
/// [`SOURCE_AGENT`], and return the names they are kept under. Runs from the hook route while the runner
/// holds its own copy of the task: it records on the stored task in one locked step
/// (`CrewStore::update_task`), and the runner's own writes keep the attachments it finds there
/// (`CrewStore::append_runner_state`).
pub(crate) fn store_agent(
    store: &CrewStore,
    crewmate_id: &str,
    task_id: &str,
    files: &[PathBuf],
) -> Result<Vec<String>, String> {
    let incoming = files
        .iter()
        .map(|f| {
            Ok(Incoming {
                name: f
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| format!("bad file name: {}", f.display()))?
                    .to_string(),
                bytes: std::fs::read(f).map_err(|e| format!("{}: {e}", f.display()))?,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    store
        .update_task(crewmate_id, task_id, |task| {
            let before = task.attachments.len();
            add(store, crewmate_id, task, &incoming, SOURCE_AGENT)?;
            Ok(task.attachments[before..]
                .iter()
                .map(|a| a.name.clone())
                .collect())
        })?
        .map(|(_, names)| names)
        .ok_or_else(|| format!("task {task_id} not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
    const JPG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10];

    fn img(name: &str, bytes: &[u8]) -> Incoming {
        Incoming {
            name: name.into(),
            bytes: bytes.to_vec(),
        }
    }

    // The declared mime is the sender's word; the bytes are the file. Each kind by its signature,
    // and the formats that look like images but are not allowed (SVG can carry script).
    #[test]
    fn the_type_comes_from_the_bytes() {
        assert_eq!(sniff(PNG), Some("image/png"));
        assert_eq!(sniff(JPG), Some("image/jpeg"));
        assert_eq!(sniff(b"GIF89a\x01\0"), Some("image/gif"));
        assert_eq!(sniff(b"GIF87a\x01\0"), Some("image/gif"));
        assert_eq!(sniff(b"RIFF\x24\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff(b"RIFF\x24\0\0\0WAVEfmt "), None, "RIFF is not WebP");
        assert_eq!(sniff(b"<svg xmlns='http://www.w3.org/2000/svg'/>"), None);
        assert_eq!(sniff(b"%PDF-1.7"), None);
        assert_eq!(sniff(b"MZ\x90\0"), None);
        assert_eq!(sniff(b""), None);
        assert_eq!(
            sniff(b"\x89PNG"),
            None,
            "a truncated signature is not a PNG"
        );
    }

    // A path in the name must never leave the task's folder, on either separator.
    #[test]
    fn a_name_is_one_safe_component() {
        for hostile in [
            "../../etc/passwd",
            "..\\..\\windows\\system32\\x.png",
            "/etc/shadow",
            "C:\\Users\\me\\a.png",
            "a/b/c.png",
            "..",
            ".",
            "",
            "....png",
            "a\0b.png",
            "sub/../..",
        ] {
            let n = safe_name(hostile, "image/png");
            assert!(
                !n.contains('/') && !n.contains('\\') && !n.contains('\0') && !n.starts_with('.'),
                "{hostile:?} -> {n:?}"
            );
            assert!(n.ends_with(".png"), "{hostile:?} -> {n:?}");
            assert!(
                Path::new(&n).components().count() == 1,
                "{hostile:?} -> {n:?}"
            );
        }
        assert_eq!(
            safe_name("shot 1 (final).PNG", "image/png"),
            "shot_1__final.png"
        );
        // The extension follows the bytes, not the sender: a JPEG called .png is saved .jpg.
        assert_eq!(safe_name("a.png", "image/jpeg"), "a.jpg");
        let long = "x".repeat(500);
        assert!(safe_name(&long, "image/png").len() <= MAX_NAME);
    }

    #[test]
    fn five_images_are_refused_and_four_are_taken() {
        let four: Vec<_> = (0..4).map(|i| img(&format!("{i}.png"), PNG)).collect();
        assert_eq!(check("t", &four).unwrap().len(), 4);
        let five: Vec<_> = (0..5).map(|i| img(&format!("{i}.png"), PNG)).collect();
        let e = check("t", &five).unwrap_err();
        assert!(e.contains("at most 4"), "{e}");
    }

    // The edge: exactly the cap passes, one byte over is refused.
    #[test]
    fn the_size_cap_is_five_mib_exactly() {
        let mut at = PNG.to_vec();
        at.resize(MAX_BYTES, 0);
        assert!(check("t", &[img("a.png", &at)]).is_ok());
        at.push(0);
        let e = check("t", &[img("a.png", &at)]).unwrap_err();
        assert!(e.contains("over 5 MiB"), "{e}");
    }

    #[test]
    fn an_empty_or_non_image_file_is_refused_by_name() {
        assert!(
            check("t", &[img("a.png", b"")])
                .unwrap_err()
                .contains("empty")
        );
        // Named .png, declared image/png by the client, but it is a script.
        let e = check("t", &[img("evil.png", b"#!/bin/sh\nrm -rf /")]).unwrap_err();
        assert!(e.contains("not a png, jpeg, webp or gif"), "{e}");
        assert!(e.contains("evil.png"), "{e}");
    }

    // Two screenshots with the same name (every phone camera roll does this) both survive.
    #[test]
    fn equal_names_are_made_unique_in_order() {
        let got = check(
            "t",
            &[
                img("shot.png", PNG),
                img("shot.png", PNG),
                img("shot.png", PNG),
            ],
        )
        .unwrap();
        let names: Vec<_> = got.iter().map(|(a, _)| a.name.as_str()).collect();
        assert_eq!(names, ["shot.png", "shot-2.png", "shot-3.png"]);
    }

    #[test]
    fn the_record_describes_the_bytes() {
        let got = check("task9", &[img("a.png", PNG)]).unwrap();
        let (a, bytes) = &got[0];
        assert_eq!(a.mime, "image/png");
        assert_eq!(a.bytes_len, PNG.len() as u64);
        assert_eq!(a.path, ".atlas/attachments/task9/a.png");
        assert_eq!(a.sha256.len(), 64);
        assert_eq!(bytes, PNG);
        // Same bytes, same hash; other bytes, other hash.
        let again = check("task9", &[img("b.png", PNG)]).unwrap();
        assert_eq!(again[0].0.sha256, a.sha256);
        let other = check("task9", &[img("c.jpg", JPG)]).unwrap();
        assert_ne!(other[0].0.sha256, a.sha256);
    }

    #[test]
    fn a_piece_is_at_most_a_chunk_and_the_last_says_so() {
        let bytes = vec![7u8; CHUNK * 2 + 10];
        let (a, last) = piece(&bytes, 0, None);
        assert!(!last);
        assert_eq!(B64.decode(a).unwrap().len(), CHUNK);
        let (b, last) = piece(&bytes, (CHUNK * 2) as u64, None);
        assert!(last);
        assert_eq!(B64.decode(b).unwrap().len(), 10);
        // A limit over the chunk is clamped; an offset past the end is an empty last piece.
        let (c, _) = piece(&bytes, 0, Some(u64::MAX));
        assert_eq!(B64.decode(c).unwrap().len(), CHUNK);
        let (d, last) = piece(&bytes, u64::MAX, None);
        assert!(last && d.is_empty());
    }

    #[test]
    fn a_huge_base64_string_is_refused_before_it_is_decoded() {
        let huge = "A".repeat(MAX_BYTES * 2);
        assert!(decode("a.png", &huge).unwrap_err().contains("over 5 MiB"));
        assert!(
            decode("a.png", "!!!not base64")
                .unwrap_err()
                .contains("base64")
        );
        assert_eq!(decode("a.png", &B64.encode(PNG)).unwrap(), PNG);
    }

    // ── abandoned uploads: an app killed mid-send must not keep a piece on the crew server forever ──

    fn upload_exists(store: &CrewStore, crewmate: &str, upload: &str) -> bool {
        upload_dir(store, crewmate, upload).exists()
    }

    /// Make `upload` look as if its last piece arrived `age` ago (the clock the sweep reads is its `seq` file).
    fn age_upload(store: &CrewStore, crewmate: &str, upload: &str, age: std::time::Duration) {
        let seq = upload_dir(store, crewmate, upload).join("seq");
        let f = std::fs::OpenOptions::new().write(true).open(seq).unwrap();
        f.set_modified(std::time::SystemTime::now() - age).unwrap();
    }

    fn piece_b64() -> String {
        B64.encode(PNG)
    }

    // The bug: nothing ever removed an upload whose task was never sent.
    #[test]
    fn an_upload_older_than_the_ttl_is_gone_after_the_next_put_of_another_upload() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CrewStore::new(tmp.path());
        put_piece(&store, "c1", "stale-upload-01", 0, &piece_b64()).unwrap();
        put_piece(&store, "c1", "fresh-upload-01", 0, &piece_b64()).unwrap();
        age_upload(
            &store,
            "c1",
            "stale-upload-01",
            UPLOAD_TTL + std::time::Duration::from_secs(60),
        );

        put_piece(&store, "c1", "other-upload-01", 0, &piece_b64()).unwrap();

        assert!(
            !upload_exists(&store, "c1", "stale-upload-01"),
            "stale upload kept"
        );
        assert!(upload_exists(&store, "c1", "fresh-upload-01"));
        assert!(upload_exists(&store, "c1", "other-upload-01"));
    }

    // The edge: just inside the TTL is kept; and a slow but alive upload (a piece every few minutes) is never
    // swept, because each piece renews its clock.
    #[test]
    fn an_upload_inside_the_ttl_is_kept_and_each_piece_renews_it() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CrewStore::new(tmp.path());
        put_piece(&store, "c1", "slow-upload-001", 0, &piece_b64()).unwrap();
        age_upload(
            &store,
            "c1",
            "slow-upload-001",
            UPLOAD_TTL - std::time::Duration::from_secs(60),
        );
        put_piece(&store, "c1", "other-upload-01", 0, &piece_b64()).unwrap();
        assert!(upload_exists(&store, "c1", "slow-upload-001"));

        // Its next piece renews the clock: aged to the brink again, still not stale after another put.
        put_piece(&store, "c1", "slow-upload-001", 1, &piece_b64()).unwrap();
        put_piece(&store, "c1", "other-upload-02", 0, &piece_b64()).unwrap();
        assert!(upload_exists(&store, "c1", "slow-upload-001"));
    }

    // One crewmate's stale uploads are not another's business, and never another crewmate's files.
    #[test]
    fn a_put_sweeps_only_its_own_crewmates_uploads() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CrewStore::new(tmp.path());
        put_piece(&store, "c1", "stale-upload-01", 0, &piece_b64()).unwrap();
        age_upload(
            &store,
            "c1",
            "stale-upload-01",
            UPLOAD_TTL + std::time::Duration::from_secs(60),
        );
        put_piece(&store, "c2", "other-upload-01", 0, &piece_b64()).unwrap();
        assert!(upload_exists(&store, "c1", "stale-upload-01"));
    }

    #[test]
    fn the_new_upload_past_the_live_limit_is_refused_but_an_existing_one_continues() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CrewStore::new(tmp.path());
        let limit = MAX_ATTACHMENTS * 2;
        for i in 0..limit {
            put_piece(
                &store,
                "c1",
                &format!("live-upload-{i:02}"),
                0,
                &piece_b64(),
            )
            .unwrap();
        }
        let e = put_piece(&store, "c1", "one-too-many-01", 0, &piece_b64()).unwrap_err();
        assert!(e.contains("too many unfinished uploads"), "{e}");
        assert!(
            !upload_exists(&store, "c1", "one-too-many-01"),
            "refused upload left a dir"
        );

        // Continuing any of the live ones is always allowed.
        put_piece(&store, "c1", "live-upload-00", 1, &piece_b64()).unwrap();
        put_piece(
            &store,
            "c1",
            &format!("live-upload-{:02}", limit - 1),
            1,
            &piece_b64(),
        )
        .unwrap();
        // Another crewmate has its own budget.
        put_piece(&store, "c2", "one-too-many-01", 0, &piece_b64()).unwrap();
    }

    // Taking an upload for a task frees its place; so does the sweep of a stale one.
    #[test]
    fn a_taken_or_swept_upload_gives_its_place_back() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CrewStore::new(tmp.path());
        let limit = MAX_ATTACHMENTS * 2;
        for i in 0..limit {
            put_piece(
                &store,
                "c1",
                &format!("live-upload-{i:02}"),
                0,
                &piece_b64(),
            )
            .unwrap();
        }
        assert_eq!(take_upload(&store, "c1", "live-upload-00").unwrap(), PNG);
        put_piece(&store, "c1", "new-upload-0001", 0, &piece_b64()).unwrap();

        // Everything else goes stale: the next new upload sweeps them all first, so it is not refused.
        for i in 1..limit {
            age_upload(
                &store,
                "c1",
                &format!("live-upload-{i:02}"),
                UPLOAD_TTL + std::time::Duration::from_secs(60),
            );
        }
        put_piece(&store, "c1", "newer-upload-01", 0, &piece_b64()).unwrap();
        assert!(!upload_exists(&store, "c1", "live-upload-01"));
    }

    #[test]
    fn the_startup_sweep_removes_stale_uploads_of_every_crewmate_and_keeps_fresh_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CrewStore::new(tmp.path());
        put_piece(&store, "c1", "stale-upload-01", 0, &piece_b64()).unwrap();
        put_piece(&store, "c1", "fresh-upload-01", 0, &piece_b64()).unwrap();
        put_piece(&store, "c2", "stale-upload-02", 0, &piece_b64()).unwrap();
        for (c, u) in [("c1", "stale-upload-01"), ("c2", "stale-upload-02")] {
            age_upload(
                &store,
                c,
                u,
                UPLOAD_TTL + std::time::Duration::from_secs(60),
            );
        }
        // A crewmate dir with no uploads, and a stray file in uploads/, must not trip the sweep.
        std::fs::create_dir_all(store.crewmate_dir("c3")).unwrap();
        std::fs::write(
            store.crewmate_dir("c1").join("uploads").join("stray.txt"),
            b"x",
        )
        .unwrap();

        sweep_stale_uploads(&store);

        assert!(!upload_exists(&store, "c1", "stale-upload-01"));
        assert!(!upload_exists(&store, "c2", "stale-upload-02"));
        assert!(upload_exists(&store, "c1", "fresh-upload-01"));
        assert!(
            store
                .crewmate_dir("c1")
                .join("uploads")
                .join("stray.txt")
                .exists()
        );
    }

    // A directory with no `seq` (a crash between create_dir and the first write) has no clock: it is judged by
    // the directory's own mtime, so it is swept once old instead of living forever.
    #[test]
    fn an_upload_dir_without_a_seq_file_is_swept_by_its_directory_age() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CrewStore::new(tmp.path());
        let dir = upload_dir(&store, "c1", "no-seq-upload-01");
        std::fs::create_dir_all(&dir).unwrap();
        set_dir_mtime(
            &dir,
            std::time::SystemTime::now() - UPLOAD_TTL - std::time::Duration::from_secs(60),
        );
        sweep_stale_uploads(&store);
        assert!(!dir.exists());
    }

    /// Set a directory's mtime. Windows opens a directory only with FILE_FLAG_BACKUP_SEMANTICS
    /// (a plain open is "Access is denied").
    fn set_dir_mtime(dir: &std::path::Path, t: std::time::SystemTime) {
        let mut o = std::fs::OpenOptions::new();
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            o.write(true).custom_flags(0x0200_0000); // FILE_FLAG_BACKUP_SEMANTICS
        }
        #[cfg(not(windows))]
        o.read(true);
        o.open(dir).unwrap().set_modified(t).unwrap();
    }
}
