//! The audio of a dictation: what a client uploads, how it is checked, sized and thrown away (#647).
//!
//! The format is fixed by the wire contract: one WAV file of 16 kHz, mono, signed 16-bit PCM, sent in pieces of
//! [`attachments::CHUNK`](super::super::attachments::CHUNK) raw bytes that concatenate to the file. Piece 0 begins
//! with the canonical 44-byte header. A client that is still recording does not know the length, so it writes
//! `0xFFFFFFFF` in both size fields and never fixes them: **the length is the bytes received after the header,
//! never the header's own fields**.
//!
//! Audio is the most private thing hostd holds, so it never outlives its use:
//! - an upload lives in `<voice dir>/uploads/<id>/` and is removed when it is transcribed (the answer or the
//!   failure), and after [`UPLOAD_TTL`] without a piece (an app killed mid-recording, a dropped network);
//! - the id is issued by hostd, never chosen by the client, so it can never name another path;
//! - at most [`MAX_LIVE_UPLOADS`] at once, [`MAX_PCM`] bytes each: a client cannot fill the disk.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;

use super::super::attachments::CHUNK;

/// The longest dictation, by the declared format (the issue: 5 minutes).
pub(crate) const MAX_SECONDS: u32 = 300;
/// The declared format.
pub(crate) const SAMPLE_RATE: u32 = 16_000;
pub(crate) const CHANNELS: u16 = 1;
/// PCM bytes in a second of the declared format (16 kHz, mono, 2 bytes a sample).
pub(crate) const BYTES_PER_SECOND: usize = SAMPLE_RATE as usize * 2;
/// The most PCM bytes an upload holds (after the header).
pub(crate) const MAX_PCM: usize = MAX_SECONDS as usize * BYTES_PER_SECOND;
/// A canonical WAV header.
pub(crate) const HEADER: usize = 44;
/// A `partial` transcription listens to the last this many seconds, however long the upload is.
pub(crate) const PARTIAL_WINDOW_S: usize = 30;
/// How long an upload may sit without a new piece before it is thrown away.
pub(crate) const UPLOAD_TTL: Duration = Duration::from_secs(10 * 60);
/// Unfinished uploads on the whole crew server.
pub(crate) const MAX_LIVE_UPLOADS: usize = 4;

/// Why a piece was refused. The first two are the coded failures of the contract.
#[derive(Debug, PartialEq)]
pub(crate) enum Refused {
    /// Not the declared format (`crew.voice.bad_audio`). The upload is gone.
    BadAudio,
    /// Over [`MAX_SECONDS`] (`crew.voice.too_long`). The upload keeps what it had: it can still be transcribed.
    TooLong,
    /// Anything else, as a text (an unknown upload, a piece that is not base64, too many uploads).
    Other(String),
}

fn other(s: impl Into<String>) -> Refused {
    Refused::Other(s.into())
}

/// `<voice dir>/uploads`
fn uploads(voice_dir: &Path) -> PathBuf {
    voice_dir.join("uploads")
}

/// An upload id is the 32 hex digits hostd made: nothing else is ever joined to a path.
fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn dir_of(voice_dir: &Path, id: &str) -> Result<PathBuf, Refused> {
    if !valid_id(id) {
        return Err(other("unknown upload (open one with an empty `upload`)"));
    }
    Ok(uploads(voice_dir).join(id))
}

/// Whether `head` (the first [`HEADER`] bytes) is the canonical WAV header of the declared format. The two size
/// fields are not read: a recorder still writing leaves them `0xFFFFFFFF`.
pub(crate) fn header_ok(head: &[u8]) -> bool {
    if head.len() < HEADER {
        return false;
    }
    let u16_at = |o: usize| u16::from_le_bytes([head[o], head[o + 1]]);
    let u32_at = |o: usize| u32::from_le_bytes([head[o], head[o + 1], head[o + 2], head[o + 3]]);
    &head[0..4] == b"RIFF"
        && &head[8..12] == b"WAVE"
        && &head[12..16] == b"fmt "
        && u32_at(16) == 16
        && u16_at(20) == 1
        && u16_at(22) == CHANNELS
        && u32_at(24) == SAMPLE_RATE
        && u32_at(28) == SAMPLE_RATE * 2
        && u16_at(32) == 2
        && u16_at(34) == 16
        && &head[36..40] == b"data"
}

/// A WAV file with the real sizes in its header, holding `pcm`: what the engines read.
pub(crate) fn wav(pcm: &[u8]) -> Vec<u8> {
    let len = pcm.len() as u32;
    let mut out = Vec::with_capacity(HEADER + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&CHANNELS.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(pcm);
    out
}

/// Milliseconds of audio in `pcm_bytes` of the declared format.
pub(crate) fn duration_ms(pcm_bytes: usize) -> u64 {
    (pcm_bytes / (BYTES_PER_SECOND / 1000)) as u64
}

/// When an upload last got a piece. A dir without data (a crash between creating it and the first write) falls
/// back to the dir's own time, so it too ages out.
fn touched(dir: &Path) -> Option<SystemTime> {
    // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
    std::fs::metadata(dir.join("data"))
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
        .or_else(|_| std::fs::metadata(dir))
        .and_then(|m| m.modified())
        .ok()
}

fn live(voice_dir: &Path) -> Vec<PathBuf> {
    // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
    let Ok(rd) = std::fs::read_dir(uploads(voice_dir)) else {
        return vec![];
    };
    rd.filter_map(Result::ok)
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| e.file_name().to_str().is_some_and(valid_id))
        .map(|e| e.path())
        .collect()
}

/// Remove every upload that has had no piece for [`UPLOAD_TTL`] as of `now`. Best effort, logged: a sweep that
/// cannot remove something must never fail the call that triggered it.
pub(crate) fn sweep(voice_dir: &Path, now: SystemTime) {
    for dir in live(voice_dir) {
        let stale = touched(&dir)
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > UPLOAD_TTL);
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
        if stale && let Err(e) = std::fs::remove_dir_all(&dir) {
            // i18n-ignore: log
            eprintln!(
                "atlas crew: stale voice upload {} not removed: {e}",
                dir.display()
            );
        }
    }
}

/// Run when the crew server starts: stale uploads, and every scratch dir a crash left behind.
pub(crate) fn sweep_at_start(voice_dir: &Path) {
    sweep(voice_dir, SystemTime::now());
    // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
    let _ = std::fs::remove_dir_all(voice_dir.join("tmp"));
}

/// One put at a time: the check of the cap and the append are one step, and pieces of one upload are ordered.
static PUT: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

/// Stage one piece. `upload` empty opens a new upload; otherwise it is one hostd issued and that still exists.
/// Returns the upload's id and the bytes it holds now (header included).
///
/// - the first [`HEADER`] bytes must be the canonical header of the declared format, else [`Refused::BadAudio`]
///   and the upload is removed;
/// - a piece that would take the PCM past [`MAX_PCM`] is refused whole ([`Refused::TooLong`]) and the upload
///   keeps what it has: reaching the cap transcribes what was said, nothing is discarded;
/// - `last` marks the upload complete (a final transcription needs it). A last piece that leaves less than a
///   header is bad audio.
pub(crate) fn put_piece(
    voice_dir: &Path,
    upload: &str,
    piece_b64: &str,
    last: bool,
    now: SystemTime,
) -> Result<(String, u64), Refused> {
    if piece_b64.len() > CHUNK.div_ceil(3) * 4 + 4 {
        return Err(other(format!("a piece is at most {} KiB", CHUNK / 1024)));
    }
    let bytes = B64
        .decode(piece_b64.trim())
        .map_err(|_| other("piece is not valid base64"))?;
    let _one = PUT.lock();
    sweep(voice_dir, now);
    let (id, dir, have) = if upload.is_empty() {
        if live(voice_dir).len() >= MAX_LIVE_UPLOADS {
            return Err(other(format!(
                "too many unfinished dictations ({MAX_LIVE_UPLOADS}); finish one or wait"
            )));
        }
        let id = crate::paths::random_hex(16).map_err(|e| other(format!("upload: {e}")))?;
        let dir = dir_of(voice_dir, &id)?;
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
        std::fs::create_dir_all(&dir).map_err(|e| other(format!("upload: {e}")))?;
        (id, dir, 0u64)
    } else {
        let dir = dir_of(voice_dir, upload)?;
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
        let have = std::fs::metadata(dir.join("data"))
            .map(|m| m.len())
            .map_err(|_| other("unknown upload (it expired or was never opened)"))?;
        if dir.join("last").exists() {
            return Err(other("the upload is already complete"));
        }
        (upload.to_string(), dir, have)
    };
    let total = have + bytes.len() as u64;
    if (total as usize).saturating_sub(HEADER) > MAX_PCM {
        return Err(Refused::TooLong);
    }
    let file = dir.join("data");
    // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
    let wrote = atlas_core::sys::owner_only(std::fs::OpenOptions::new().create(true).append(true))
        .open(&file)
        .and_then(|mut f| f.write_all(&bytes));
    if let Err(e) = wrote {
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
        let _ = std::fs::remove_dir_all(&dir);
        return Err(other(format!("upload: {e}")));
    }
    // The header is checked once it has arrived whole, so a client that is not sending the contract's audio
    // leaves nothing behind.
    let header_arrived = have < HEADER as u64 && total >= HEADER as u64;
    let bad = (header_arrived && !head_of(&file).is_some_and(|h| header_ok(&h)))
        || (last && total < HEADER as u64);
    if bad {
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
        let _ = std::fs::remove_dir_all(&dir);
        return Err(Refused::BadAudio);
    }
    if last {
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
        if let Err(e) = std::fs::write(dir.join("last"), b"") {
            // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
            let _ = std::fs::remove_dir_all(&dir);
            return Err(other(format!("upload: {e}")));
        }
    }
    Ok((id, total))
}

fn head_of(file: &Path) -> Option<Vec<u8>> {
    use std::io::Read as _;
    let mut head = vec![0u8; HEADER];
    // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
    std::fs::File::open(file)
        .and_then(|mut f| f.read_exact(&mut head))
        .ok()?;
    Some(head)
}

/// An upload being transcribed. A final transcription [`arm`](Self::arm)s it: the audio is then removed when this
/// goes out of scope, however that happens (the answer, a failure, a client that went away mid-run).
pub(crate) struct Recording {
    dir: PathBuf,
    /// The last piece (`last: true`) arrived.
    pub complete: bool,
    armed: bool,
}

impl Recording {
    /// Open upload `id`. An unknown or expired one is an error text.
    pub fn open(voice_dir: &Path, id: &str, now: SystemTime) -> Result<Self, String> {
        sweep(voice_dir, now);
        let dir = dir_of(voice_dir, id).map_err(|_| "unknown upload".to_string())?;
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
        if !dir.join("data").is_file() {
            return Err("unknown upload (it expired or was never opened)".into());
        }
        Ok(Self {
            complete: dir.join("last").exists(),
            dir,
            armed: false,
        })
    }

    /// The PCM so far (after the header, a whole number of samples). With `window_s` only the last that many
    /// seconds. Empty when less than a header arrived.
    pub fn pcm(&self, window_s: Option<usize>) -> std::io::Result<Vec<u8>> {
        // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
        let all = std::fs::read(self.dir.join("data"))?;
        let mut pcm = all.get(HEADER..).unwrap_or(&[]);
        if pcm.len() % 2 == 1 {
            pcm = &pcm[..pcm.len() - 1];
        }
        if let Some(s) = window_s {
            let keep = s * BYTES_PER_SECOND;
            if pcm.len() > keep {
                pcm = &pcm[pcm.len() - keep..];
            }
        }
        Ok(pcm.to_vec())
    }

    /// From now on the audio is deleted when this is dropped.
    pub fn arm(&mut self) {
        self.armed = true;
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        if self.armed {
            // tree-access: hostd-owned (the crew store, scratch or runtime), never a crewmate's tree
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

/// What a recorder that is still writing sends first: the canonical header with both sizes `0xFFFFFFFF`.
#[cfg(test)]
pub(crate) fn live_header() -> Vec<u8> {
    let mut h = wav(&[]);
    h[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    h[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(b: &[u8]) -> String {
        B64.encode(b)
    }

    fn put(dir: &Path, upload: &str, bytes: &[u8], last: bool) -> Result<(String, u64), Refused> {
        // In pieces of at most CHUNK, as a client sends them; the first refusal is the answer.
        let mut id = upload.to_string();
        let mut total = 0;
        let pieces: Vec<&[u8]> = if bytes.is_empty() {
            vec![&[][..]]
        } else {
            bytes.chunks(CHUNK).collect()
        };
        for (i, piece) in pieces.iter().enumerate() {
            let r = put_piece(
                dir,
                &id,
                &b64(piece),
                last && i + 1 == pieces.len(),
                SystemTime::now(),
            )?;
            (id, total) = r;
        }
        Ok((id, total))
    }

    fn files(dir: &Path) -> usize {
        live(dir).len()
    }

    // The recorder never fixes the header's sizes, so they are read as what they are: not a length.
    #[test]
    fn a_header_with_unknown_sizes_is_the_declared_format() {
        assert!(header_ok(&live_header()));
        assert!(header_ok(&wav(&[0; 64])));
    }

    #[test]
    fn a_header_of_another_format_is_refused() {
        for (at, byte) in [
            (22usize, 2u8),
            (24, 0x44),
            (34, 8),
            (20, 3),
            (0, b'X'),
            (36, b'x'),
        ] {
            let mut h = live_header();
            h[at] = byte;
            assert!(!header_ok(&h), "byte {at} = {byte}");
        }
        assert!(!header_ok(&live_header()[..43]), "a header cut short");
    }

    // The length of a recording is what arrived, not what the header says: a header claiming 4 GiB is a
    // 0xFFFFFFFF, and a header claiming nothing is a recording that still has audio.
    #[test]
    fn the_length_is_the_bytes_received_not_the_header() {
        let tmp = tempfile::tempdir().unwrap();
        let mut piece = live_header();
        piece.extend(std::iter::repeat_n(7u8, 64_000)); // 2 s
        let (id, total) = put(tmp.path(), "", &piece, true).unwrap();
        assert_eq!(total, 64_044);
        let rec = Recording::open(tmp.path(), &id, SystemTime::now()).unwrap();
        assert!(rec.complete);
        let pcm = rec.pcm(None).unwrap();
        assert_eq!(pcm.len(), 64_000);
        assert_eq!(duration_ms(pcm.len()), 2000);
    }

    // The file the engines read has the real sizes: a header with 0xFFFFFFFF would make them read 4 GiB.
    #[test]
    fn the_wav_handed_to_an_engine_has_its_real_sizes() {
        let w = wav(&[1, 2, 3, 4]);
        assert!(header_ok(&w));
        assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()), 4);
        assert_eq!(u32::from_le_bytes(w[4..8].try_into().unwrap()), 40);
        assert_eq!(&w[44..], &[1, 2, 3, 4]);
    }

    // Edge of the cap: exactly 300 s is taken, one sample over is refused, and what was there stays.
    #[test]
    fn the_cap_is_three_hundred_seconds_of_audio_exactly() {
        let tmp = tempfile::tempdir().unwrap();
        let mut first = live_header();
        first.extend(vec![0u8; MAX_PCM - 2]);
        let (id, _) = put(tmp.path(), "", &first, false).unwrap();
        // 2 more bytes = exactly 300 s: taken.
        assert!(put(tmp.path(), &id, &[0, 0], false).is_ok());
        // One sample more: refused as too long, whole.
        assert_eq!(put(tmp.path(), &id, &[0, 0], false), Err(Refused::TooLong));
        // Nothing was discarded: the 300 s are still there to be transcribed.
        let rec = Recording::open(tmp.path(), &id, SystemTime::now()).unwrap();
        assert_eq!(rec.pcm(None).unwrap().len(), MAX_PCM);
    }

    #[test]
    fn a_piece_that_straddles_the_cap_is_refused_whole() {
        let tmp = tempfile::tempdir().unwrap();
        let mut first = live_header();
        first.extend(vec![0u8; MAX_PCM - 100]);
        let (id, total) = put(tmp.path(), "", &first, false).unwrap();
        assert_eq!(
            put(tmp.path(), &id, &[0u8; 200], false),
            Err(Refused::TooLong)
        );
        let rec = Recording::open(tmp.path(), &id, SystemTime::now()).unwrap();
        assert_eq!(
            rec.pcm(None).unwrap().len() as u64 + HEADER as u64,
            total,
            "not even the part that fitted"
        );
    }

    // Not the declared format: refused when the header has arrived, and nothing is left on disk.
    #[test]
    fn audio_that_is_not_the_declared_format_is_refused_and_removed() {
        let tmp = tempfile::tempdir().unwrap();
        let mut stereo = live_header();
        stereo[22] = 2;
        stereo.extend([0u8; 100]);
        assert_eq!(put(tmp.path(), "", &stereo, false), Err(Refused::BadAudio));
        assert_eq!(files(tmp.path()), 0, "a refused upload leaves nothing");
        // An mp3, an empty body, a script.
        for junk in [
            &b"ID3\x04\0\0\0\0\0\0"[..],
            b"",
            b"#!/bin/sh\nrm -rf /\n#########################",
        ] {
            let r = put(tmp.path(), "", junk, true);
            assert_eq!(r, Err(Refused::BadAudio), "{junk:?}");
            assert_eq!(files(tmp.path()), 0);
        }
    }

    // A header split over two pieces is still a header.
    #[test]
    fn a_header_may_arrive_in_two_pieces() {
        let tmp = tempfile::tempdir().unwrap();
        let h = live_header();
        let (id, _) = put(tmp.path(), "", &h[..10], false).unwrap();
        assert!(put(tmp.path(), &id, &h[10..], false).is_ok());
        let mut bad = live_header();
        bad[0] = b'X';
        let (id, _) = put(tmp.path(), "", &bad[..10], false).unwrap();
        assert_eq!(
            put(tmp.path(), &id, &bad[10..], false),
            Err(Refused::BadAudio)
        );
    }

    // The id is issued by hostd; a client cannot name a path or an upload that is not there.
    #[test]
    fn an_upload_id_is_never_a_path() {
        let tmp = tempfile::tempdir().unwrap();
        for hostile in [
            "../../etc",
            "a/b",
            "..",
            "abc",
            &"g".repeat(32),
            &"a".repeat(31),
        ] {
            let r = put(tmp.path(), hostile, &live_header(), false);
            assert!(matches!(r, Err(Refused::Other(_))), "{hostile}: {r:?}");
        }
        let missing = "a".repeat(32);
        assert!(matches!(
            put(tmp.path(), &missing, &live_header(), false),
            Err(Refused::Other(_))
        ));
        assert!(Recording::open(tmp.path(), "../x", SystemTime::now()).is_err());
    }

    #[test]
    fn the_server_holds_four_unfinished_dictations() {
        let tmp = tempfile::tempdir().unwrap();
        for _ in 0..MAX_LIVE_UPLOADS {
            put(tmp.path(), "", &live_header(), false).unwrap();
        }
        let r = put(tmp.path(), "", &live_header(), false);
        assert!(matches!(r, Err(Refused::Other(e)) if e.contains("too many")));
    }

    #[test]
    fn a_piece_is_at_most_one_chunk() {
        let tmp = tempfile::tempdir().unwrap();
        let big = vec![0u8; CHUNK + 1000];
        let r = put_piece(tmp.path(), "", &b64(&big), false, SystemTime::now());
        assert!(matches!(r, Err(Refused::Other(e)) if e.contains("512 KiB")));
        assert!(put_piece(tmp.path(), "", "***", false, SystemTime::now()).is_err());
    }

    // The ten-minute TTL: a dictation nobody finished is gone at the next put, and by the sweep at start.
    #[test]
    fn an_abandoned_upload_is_removed_after_ten_minutes() {
        let tmp = tempfile::tempdir().unwrap();
        let (id, _) = put(tmp.path(), "", &live_header(), false).unwrap();
        let now = SystemTime::now();
        sweep(tmp.path(), now + UPLOAD_TTL - Duration::from_secs(5));
        assert_eq!(files(tmp.path()), 1, "still inside the TTL");
        sweep(tmp.path(), now + UPLOAD_TTL + Duration::from_secs(5));
        assert_eq!(files(tmp.path()), 0, "past the TTL the audio is gone");
        assert!(Recording::open(tmp.path(), &id, SystemTime::now()).is_err());
    }

    #[test]
    fn the_start_sweeps_scratch_a_crash_left() {
        let tmp = tempfile::tempdir().unwrap();
        let scratch = tmp.path().join("tmp/abc");
        std::fs::create_dir_all(&scratch).unwrap();
        std::fs::write(scratch.join("in.wav"), b"audio").unwrap();
        sweep_at_start(tmp.path());
        assert!(!tmp.path().join("tmp").exists());
    }

    // A partial listens to the end of the recording only.
    #[test]
    fn a_partial_window_is_the_last_thirty_seconds() {
        let tmp = tempfile::tempdir().unwrap();
        let mut piece = live_header();
        // 40 s: the first 10 s are 1s, the last 30 s are 2s.
        piece.extend(vec![1u8; 10 * BYTES_PER_SECOND]);
        piece.extend(vec![2u8; 30 * BYTES_PER_SECOND]);
        let (id, _) = put(tmp.path(), "", &piece, false).unwrap();
        let rec = Recording::open(tmp.path(), &id, SystemTime::now()).unwrap();
        assert!(!rec.complete);
        let w = rec.pcm(Some(PARTIAL_WINDOW_S)).unwrap();
        assert_eq!(w.len(), 30 * BYTES_PER_SECOND);
        assert!(w.iter().all(|b| *b == 2));
        // A short recording is returned whole, and a half sample never reaches an engine.
        let (id2, _) = put(
            tmp.path(),
            "",
            &[live_header(), vec![9u8; 7]].concat(),
            false,
        )
        .unwrap();
        let rec = Recording::open(tmp.path(), &id2, SystemTime::now()).unwrap();
        assert_eq!(rec.pcm(Some(PARTIAL_WINDOW_S)).unwrap().len(), 6);
    }

    // An armed recording deletes the audio when it goes out of scope; an unarmed one (a partial) keeps it.
    #[test]
    fn an_armed_recording_deletes_the_audio_when_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        let (id, _) = put(tmp.path(), "", &live_header(), true).unwrap();
        drop(Recording::open(tmp.path(), &id, SystemTime::now()).unwrap());
        assert_eq!(files(tmp.path()), 1, "a partial leaves the upload open");
        let mut rec = Recording::open(tmp.path(), &id, SystemTime::now()).unwrap();
        rec.arm();
        drop(rec);
        assert_eq!(files(tmp.path()), 0);
    }
}
