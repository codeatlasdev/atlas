//! Screen recording (mp4) and GIF recording: the HUD state the page shows, and on macOS the runner that executes the
//! recording effects of the capture session ([`super::session`]).
//!
//! A recording is a list of **segments**: ScreenCaptureKit has no pause, so a pause ends the running segment and a
//! resume starts the next one; stopping joins them (`video::join`, no re-encode). The clock ([`super::clock`]) counts
//! only recorded time, so the HUD timer freezes while paused and the limit is on recorded time too.
//!
//! The HUD is excluded from the recording by window id (`runner::own_window_ids`), like every capture surface.

use serde::Serialize;

use super::clock::MAX_MS;
use super::history::Entry;

/// What the HUD page shows (`shots://record`, `shots_record_state`), camelCase.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordState {
    pub phase: &'static str,
    pub elapsed_ms: u64,
    pub max_ms: u64,
    pub kind: &'static str,
    pub message: Option<String>,
    pub entry: Option<Entry>,
    /// About how big the mp4 is by now, bytes (`recopts::estimate_bytes`); `None` outside recording/paused and for GIFs.
    pub estimate_bytes: Option<u64>,
    /// Sentences about options that were asked for and skipped (no permission, old macOS): shown on the HUD.
    pub notes: Vec<String>,
    /// GIF encode progress while saving a GIF recording.
    pub progress: Option<Progress>,
    /// The trim step after Stop: the clip's length and poster frames.
    pub trim: Option<TrimState>,
}

/// Frames done of total while a GIF is encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
}

/// What the trim timeline needs: the clip's length and evenly spread poster frames (data URLs).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrimState {
    pub secs: f64,
    pub posters: Vec<String>,
}

impl Default for RecordState {
    fn default() -> Self {
        Self::idle()
    }
}

impl RecordState {
    pub fn idle() -> Self {
        Self {
            phase: "idle",
            elapsed_ms: 0,
            max_ms: MAX_MS,
            kind: "recording",
            message: None,
            entry: None,
            estimate_bytes: None,
            notes: Vec::new(),
            progress: None,
            trim: None,
        }
    }

    pub fn at(phase: &'static str, elapsed_ms: u64, gif: bool) -> Self {
        Self {
            phase,
            elapsed_ms,
            kind: if gif { "gif" } else { "recording" },
            ..Self::idle()
        }
    }

    pub fn done(entry: Entry, gif: bool) -> Self {
        let ms = entry.secs.map_or(0, |s| (s * 1000.0).round() as u64);
        Self {
            entry: Some(entry),
            ..Self::at("done", ms, gif)
        }
    }

    pub fn error(message: String, gif: bool) -> Self {
        Self {
            message: Some(message),
            ..Self::at("error", 0, gif)
        }
    }

    /// The same state with the notes of options that were skipped.
    pub fn with_notes(self, notes: &[String]) -> Self {
        Self {
            notes: notes.to_vec(),
            ..self
        }
    }

    /// The same state with the running size estimate (mp4 only).
    pub fn with_estimate(self, bytes: Option<u64>) -> Self {
        Self {
            estimate_bytes: bytes,
            ..self
        }
    }

    /// Saving a GIF: the frames encoded so far.
    pub fn with_progress(self, done: usize, total: usize) -> Self {
        Self {
            progress: Some(Progress { done, total }),
            ..self
        }
    }

    /// The trim step: the joined clip is ready, the person decides what to keep.
    pub fn trimming(secs: f64, posters: Vec<String>, gif: bool) -> Self {
        Self {
            trim: Some(TrimState { secs, posters }),
            ..Self::at("trim", (secs * 1000.0).round() as u64, gif)
        }
    }
}

// ───────────────────────────── commands ─────────────────────────────
//
// Cross-platform shells: the work is macOS's (`record_mac`), every other platform says so.

/// GIF quality choices of the export sheet.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GifPresetInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub fps: u32,
    pub max_side: u32,
}

/// What a GIF export would produce (`shots_gif_estimate`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GifEstimate {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bytes: u64,
}

#[tauri::command]
pub fn shots_record_pause(app: tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    super::record_mac::pause_cmd(&app);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[tauri::command]
pub fn shots_record_resume(app: tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    super::record_mac::resume_cmd(&app);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[tauri::command]
pub fn shots_record_stop(app: tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    super::record_mac::stop_cmd(&app);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[tauri::command]
pub fn shots_record_cancel(app: tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    super::record_mac::cancel_cmd(&app);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

/// What this Mac can record besides the picture (sound sources), for the toggles in Settings.
#[tauri::command]
pub fn shots_record_capabilities() -> super::recopts::Capabilities {
    #[cfg(target_os = "macos")]
    return super::record_mac::capabilities();
    #[cfg(not(target_os = "macos"))]
    super::recopts::Capabilities {
        system_audio: false,
        microphone: false,
        reason: Some("Recording is not available on this platform yet.".into()),
    }
}

/// Save the recording waiting in the trim step: keep `start..end` seconds, or all of it when both are absent.
#[tauri::command]
pub fn shots_record_save(
    app: tauri::AppHandle,
    start: Option<f64>,
    end: Option<f64>,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return super::record_mac::save_cmd(&app, start, end);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, start, end);
        super::unavailable()
    }
}

/// Throw away the recording waiting in the trim step.
#[tauri::command]
pub fn shots_record_discard(app: tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    super::record_mac::discard_cmd(&app);
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

/// Stop the GIF encode that is running (Esc in the HUD or in the card's GIF sheet). `false`: none was running.
#[tauri::command]
pub fn shots_gif_cancel() -> bool {
    super::gif::request_cancel()
}

#[tauri::command]
pub fn shots_gif_presets() -> Vec<GifPresetInfo> {
    super::gif::PRESETS
        .iter()
        .map(|p| GifPresetInfo {
            id: p.id,
            label: p.label,
            fps: p.fps,
            max_side: p.max_side,
        })
        .collect()
}

#[tauri::command]
pub fn shots_gif_estimate(
    state: tauri::State<'_, super::Shots>,
    path: String,
    preset: String,
) -> Result<GifEstimate, String> {
    #[cfg(target_os = "macos")]
    return super::record_mac::estimate(&state, &path, &preset);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (state, path, preset);
        super::unavailable()
    }
}

#[tauri::command]
pub async fn shots_gif_export(
    app: tauri::AppHandle,
    path: String,
    preset: String,
) -> Result<Entry, String> {
    #[cfg(target_os = "macos")]
    return super::record_mac::export(app, path, preset).await;
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, path, preset);
        super::unavailable()
    }
}

/// The video file a recording is written to before it is joined: `<dir>/.atlas-rec-<pid>-<n>-seg<k>.mp4`. A hidden
/// name: a crash must not leave a half-written "Screen Recording" in the user's capture folder.
pub fn segment_path(dir: &std::path::Path, pid: u32, run: u64, k: usize) -> std::path::PathBuf {
    dir.join(format!(".atlas-rec-{pid}-{run}-seg{k}.mp4"))
}

/// `n` times (seconds) spread over a clip of `secs`, the first at 0 and the last just inside the end: the poster
/// frames of the trim timeline.
pub fn poster_times(secs: f64, n: usize) -> Vec<f64> {
    let n = n.max(1);
    if n == 1 || secs.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
        return vec![0.0];
    }
    let last = (secs - 0.05).max(0.0);
    (0..n).map(|i| last * i as f64 / (n - 1) as f64).collect()
}

/// The still frames a GIF takes from a clip of `secs` seconds at `src_fps`, as the times (seconds) to read: one
/// every `1 / plan_fps`, starting at 0, never past the end.
pub fn gif_times(secs: f64, plan_fps: u32) -> Vec<f64> {
    let fps = f64::from(plan_fps.max(1));
    let n = ((secs * fps).floor() as usize).max(1);
    (0..n)
        .map(|i| (i as f64 / fps).min((secs - 0.001).max(0.0)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The HUD page reads exactly these keys (`RecordState` in `lib/shots/types.ts`): serialize the real value.
    #[test]
    fn the_hud_state_serializes_with_the_keys_the_page_reads() {
        let v = serde_json::to_value(RecordState::at("recording", 1500, true)).unwrap();
        assert_eq!(v["phase"], "recording");
        assert_eq!(v["elapsedMs"], 1500);
        assert_eq!(v["maxMs"], MAX_MS);
        assert_eq!(v["kind"], "gif");
        assert!(v["message"].is_null() && v["entry"].is_null());
        let e = serde_json::to_value(RecordState::error("no permission".into(), false)).unwrap();
        assert_eq!(
            (e["phase"].as_str(), e["message"].as_str()),
            (Some("error"), Some("no permission"))
        );
        assert_eq!(e["kind"], "recording");
    }

    /// The HUD reads these extra keys (`HudState` in `lib/shots/record.ts`): the estimate while recording, the notes
    /// of skipped options, the GIF progress and the trim step. Serialize the real values.
    #[test]
    fn the_hud_extras_serialize_with_the_keys_the_page_reads() {
        let idle = serde_json::to_value(RecordState::at("recording", 1, false)).unwrap();
        assert!(
            idle["estimateBytes"].is_null() && idle["progress"].is_null() && idle["trim"].is_null()
        );
        assert_eq!(idle["notes"], serde_json::json!([]));

        let v = serde_json::to_value(
            RecordState::at("recording", 1000, false)
                .with_estimate(Some(1234))
                .with_notes(&["Microphone skipped".to_string()]),
        )
        .unwrap();
        assert_eq!(v["estimateBytes"], 1234);
        assert_eq!(v["notes"][0], "Microphone skipped");

        let p =
            serde_json::to_value(RecordState::at("saving", 0, true).with_progress(3, 40)).unwrap();
        assert_eq!(p["progress"]["done"], 3);
        assert_eq!(p["progress"]["total"], 40);

        let t = serde_json::to_value(RecordState::trimming(12.5, vec!["data:a".into()], false))
            .unwrap();
        assert_eq!(t["phase"], "trim");
        assert_eq!(t["trim"]["secs"], 12.5);
        assert_eq!(t["trim"]["posters"][0], "data:a");
        assert_eq!(t["elapsedMs"], 12500);
    }

    #[test]
    fn poster_times_are_spread_over_the_clip_and_end_before_its_last_instant() {
        let t = poster_times(10.0, 5);
        assert_eq!(t.len(), 5);
        assert_eq!(t[0], 0.0);
        assert!(t.windows(2).all(|w| w[0] < w[1]), "{t:?}");
        assert!(t[4] < 10.0 && t[4] > 8.0, "{t:?}");
        assert_eq!(poster_times(0.0, 5), vec![0.0]);
        assert_eq!(poster_times(3.0, 1), vec![0.0]);
    }

    /// A crash mid-recording leaves segments behind: their names must not look like a user's capture (they would
    /// show up in Finder as a broken "Screen Recording"), and two recordings never share a segment.
    /// The HUD's "Saved" card shows the length of what was saved, not 0:00 (seen in the first agent run).
    #[test]
    fn the_saved_card_carries_the_length_of_the_clip() {
        let e = Entry {
            path: "/a.mp4".into(),
            kind: super::super::naming::Kind::Recording,
            at: 1,
            width: 2,
            height: 2,
            bytes: 3,
            secs: Some(4.786),
        };
        assert_eq!(RecordState::done(e.clone(), false).elapsed_ms, 4786);
        let no_len = Entry { secs: None, ..e };
        assert_eq!(RecordState::done(no_len, false).elapsed_ms, 0);
    }

    #[test]
    fn segments_are_hidden_files_unique_per_run_and_segment() {
        let d = std::path::Path::new("/shots");
        let a = segment_path(d, 7, 1, 0);
        assert!(a.file_name().unwrap().to_str().unwrap().starts_with('.'));
        assert_ne!(a, segment_path(d, 7, 1, 1));
        assert_ne!(a, segment_path(d, 7, 2, 0));
        assert_ne!(a, segment_path(d, 8, 1, 0));
        assert_eq!(a.parent(), Some(d));
    }

    #[test]
    fn gif_frames_are_spread_at_the_plan_rate_and_stay_inside_the_clip() {
        let t = gif_times(2.0, 15);
        assert_eq!(t.len(), 30);
        assert_eq!(t[0], 0.0);
        assert!((t[1] - 1.0 / 15.0).abs() < 1e-9);
        assert!(
            t.iter().all(|x| *x < 2.0),
            "a frame at the exact end has no picture: {t:?}"
        );
        // a clip shorter than one frame still yields one frame at 0
        assert_eq!(gif_times(0.01, 15), vec![0.0]);
        assert_eq!(gif_times(0.0, 15), vec![0.0]);
    }
}
