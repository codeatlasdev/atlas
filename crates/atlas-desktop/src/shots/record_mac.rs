//! macOS side of recording: executes the recording [`Effect`]s of the capture session with ScreenCaptureKit, runs
//! the HUD ticker, joins the segments at the end and turns a clip into a GIF on request.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use super::clock::{self, Clock};
use super::geom::Rect;
use super::gif;
use super::history::Entry;
use super::keys_mac::Keys;
use super::naming::Kind;
use super::prefs::VideoQuality;
use super::recopts::{MicAccess, estimate_bytes, hud_height, video_config};
use super::record::{GifEstimate, RecordState, gif_times, segment_path};
use super::sck::StreamOpts;
use super::session::{Effect, Event};
use super::surface::{self, Surface};
use super::{Shots, pixels, runner, sck, video};
use crate::capture::sck::{Recorder, Source};

static RUN: AtomicU64 = AtomicU64::new(1);

/// A recording in progress.
pub struct Live {
    run: u64,
    dir: PathBuf,
    display: u32,
    rect: Option<Rect>,
    gif: bool,
    clock: Clock,
    started: Instant,
    /// Finished segments, in order.
    done: Vec<PathBuf>,
    /// The segment being written (`None` while paused).
    current: Option<Recorder>,
    /// Where the running segment is written.
    current_path: Option<PathBuf>,
    countdown_from: Option<Instant>,
    /// The stream options decided when the first segment started; every resumed segment keeps them.
    stream: StreamOpts,
    /// The quality preset of this recording, pinned at its start.
    quality: VideoQuality,
    /// Modelled bits per second of the preset, for the size estimate on the HUD.
    bitrate: u64,
    /// Sentences about options that were asked for and skipped, shown on the HUD.
    notes: Vec<String>,
    /// The keystroke pill, while the option is on and was granted.
    keys: Option<Keys>,
}

// SAFETY: `Recorder` is `Send` (its SCK objects are thread-safe) and everything else is plain data; `Live` is only
// touched under the `Shots.live` mutex.
unsafe impl Send for Live {}

impl Live {
    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
}

fn emit(app: &AppHandle, st: RecordState) {
    *app.state::<Shots>().record.lock() = st.clone();
    let _ = app.emit("shots://record", st);
}

/// Run one recording effect (the stills and the overlay are `runner`'s).
pub fn run(app: &AppHandle, fx: Effect) -> Result<(), String> {
    match fx {
        Effect::StartCountdown => start_countdown(app),
        Effect::StartRecording { display, rect, gif } => start_recording(app, display, rect, gif),
        Effect::PauseRecording => pause(app),
        Effect::ResumeRecording => resume(app),
        Effect::StopRecording => stop(app),
        Effect::DiscardRecording => {
            discard(app);
            Ok(())
        }
        Effect::HideHud => {
            hide_hud(app);
            Ok(())
        }
        // The overlay and stills are handled by `runner::run` before this is reached.
        _ => Ok(()),
    }
}

// ───────────────────────────── HUD ─────────────────────────────

fn show_hud(app: &AppHandle, display: Option<(Rect, Option<Rect>)>) -> Result<(), String> {
    let win = surface::build(app, Surface::Hud, 0, "")?;
    // the trim step widens the HUD; a new recording starts from the pill again
    let (sw, sh) = Surface::Hud.size();
    let _ = win.set_size(tauri::LogicalSize::new(sw, sh));
    if let Some((d, region)) = display {
        let (x, y) = surface::hud_origin(d, Surface::Hud.size(), region);
        let _ = win.set_position(tauri::LogicalPosition::new(x, y));
    }
    surface::show(app, &win);
    // The HUD takes keys (Esc, Enter, P) while it is up, like the overlay: it is a non-activating panel, so the
    // app being recorded stays frontmost.
    let a = app.clone();
    let _ = app.run_on_main_thread(move || {
        let w = a.get_window(&Surface::Hud.label(0));
        crate::dock_mac::overlay_keys(w.as_ref());
    });
    Ok(())
}

fn hide_hud(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(&Surface::Hud.label(0)) {
        let _ = w.hide();
    }
    let _ = app.run_on_main_thread(|| crate::dock_mac::overlay_keys(None));
    let taken = app.state::<Shots>().live.lock().take();
    if let Some(mut l) = taken {
        // the keystroke monitor must be removed, not just dropped
        stop_extras(app, &mut l);
    }
    emit(app, RecordState::idle());
}

// ───────────────────────────── start ─────────────────────────────

fn start_countdown(app: &AppHandle) -> Result<(), String> {
    if !sck::permission_granted() {
        sck::request_permission();
        return Err(permission_text());
    }
    let shots = app.state::<Shots>();
    let (display, rect, gif) = shots
        .pending_record
        .lock()
        .take()
        .ok_or("there is nothing to record")?;
    let content = sck::Content::fetch()?;
    let d = content
        .display(display)
        .ok_or("that display is gone")?
        .clone();
    let dir = shots.prefs.lock().dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    *shots.live.lock() = Some(Live {
        run: RUN.fetch_add(1, Ordering::Relaxed),
        dir,
        display,
        rect,
        gif,
        clock: Clock::default(),
        started: Instant::now(),
        done: Vec::new(),
        current: None,
        current_path: None,
        countdown_from: Some(Instant::now()),
        stream: StreamOpts::default(),
        quality: VideoQuality::High,
        bitrate: 0,
        notes: Vec::new(),
        keys: None,
    });
    show_hud(app, Some((d.frame, rect)))?;
    emit(app, RecordState::at("countdown", 0, gif));
    spawn_ticker(app.clone());
    Ok(())
}

fn permission_text() -> String {
    "Atlas needs the Screen Recording permission to capture the screen. Grant it in System Settings > Privacy & Security > Screen Recording, then restart Atlas.".into()
}

/// One thread per recording: it drives the countdown, publishes the elapsed time to the HUD and enforces the limit.
/// It ends when the session leaves the recording states.
fn spawn_ticker(app: AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(200));
            let shots = app.state::<Shots>();
            let mut live = shots.live.lock();
            let Some(l) = live.as_mut() else { return };
            if let Some(from) = l.countdown_from {
                let ms = clock::countdown_ms(from.elapsed());
                let gif = l.gif;
                if from.elapsed() >= clock::COUNTDOWN {
                    l.countdown_from = None;
                    drop(live);
                    super::feed(&app, Event::CountdownDone);
                } else {
                    drop(live);
                    emit(&app, RecordState::at("countdown", ms, gif));
                }
                continue;
            }
            let now = l.now_ms();
            let (gif, running, at_limit) = (l.gif, l.clock.is_running(), l.clock.at_limit(now));
            let ms = l.clock.elapsed(now);
            let recording = l.current.is_some() || !running;
            let (bitrate, notes) = (l.bitrate, l.notes.clone());
            drop(live);
            if at_limit {
                super::feed(&app, Event::Stop);
            } else if recording {
                emit(
                    &app,
                    hud_state(
                        if running { "recording" } else { "paused" },
                        ms,
                        gif,
                        bitrate,
                        &notes,
                    ),
                );
            }
        }
    });
}

/// The ScreenCaptureKit source of a recording: the display minus Atlas's own surfaces, cropped to `rect`, at the
/// pixel size and frame rate of the quality preset, with the pointer, click ring and sound the options asked for.
/// Also returns the modelled bitrate of the preset (for the size estimate).
fn source(
    app: &AppHandle,
    display: u32,
    rect: Option<Rect>,
    quality: VideoQuality,
    opts: StreamOpts,
) -> Result<(Source, u64), String> {
    let content = sck::Content::fetch()?;
    let own = runner::own_window_ids(app, &content);
    let d = content.display(display).ok_or("that display is gone")?;
    // Even pixel sides (video encoders reject odd ones), clipped to the display, display-relative.
    let region = rect.unwrap_or(d.frame);
    let (px, py, pw, ph) = super::geom::to_pixels(region, d.frame, d.scale)
        .ok_or("That region is outside the display.")?;
    let local = rect.map(|_| {
        Rect::new(
            f64::from(px) / d.scale,
            f64::from(py) / d.scale,
            f64::from(pw) / d.scale,
            f64::from(ph) / d.scale,
        )
    });
    // The preset decides the output pixels and the frame rate; ScreenCaptureKit scales the region to them.
    let vc = video_config(quality, (pw, ph), 30);
    let px = (vc.width as usize, vc.height as usize);
    let (filter, config) =
        sck::recording_filter(&content, display, &own, local, px, vc.fps as i32, opts)?;
    Ok((
        Source {
            filter,
            config,
            pixels: px,
            mp4: true,
        },
        vc.bitrate,
    ))
}

/// The microphone as the system reports it for this process.
fn mic_access() -> MicAccess {
    // SAFETY: plain class method with the audio media type constant.
    let status = unsafe {
        let Some(audio) = objc2_av_foundation::AVMediaTypeAudio else {
            return MicAccess::Refused;
        };
        objc2_av_foundation::AVCaptureDevice::authorizationStatusForMediaType(audio)
    };
    match status {
        objc2_av_foundation::AVAuthorizationStatus::NotDetermined => MicAccess::NotAsked,
        objc2_av_foundation::AVAuthorizationStatus::Authorized => MicAccess::Granted,
        _ => MicAccess::Refused,
    }
}

/// Ask for the microphone (the system prompt, once) and wait for the answer. Called only when a recording starts
/// with the microphone on, never at launch.
fn request_mic() -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    let block = block2::RcBlock::new(move |ok: objc2::runtime::Bool| {
        let _ = tx.send(ok.as_bool());
    });
    // SAFETY: plain class method; the block is copied by AVFoundation and called once on an arbitrary queue.
    unsafe {
        let Some(audio) = objc2_av_foundation::AVMediaTypeAudio else {
            return false;
        };
        objc2_av_foundation::AVCaptureDevice::requestAccessForMediaType_completionHandler(
            audio, &block,
        );
    }
    rx.recv_timeout(Duration::from_secs(120)).unwrap_or(false)
}

/// The macOS major version.
fn os_major() -> i64 {
    objc2_foundation::NSProcessInfo::processInfo()
        .operatingSystemVersion()
        .majorVersion as i64
}

/// What this Mac can record besides the picture, for Settings (`shots_record_capabilities`).
pub fn capabilities() -> super::recopts::Capabilities {
    super::recopts::capabilities(os_major(), mic_access())
}

/// Decide the options of a new recording from the prefs: the stream flags and the sentences for whatever was asked
/// for and skipped. Asks for the microphone and Input Monitoring here (first use), never earlier. A GIF has no
/// sound, so it neither records nor asks for it.
fn resolve_options(app: &AppHandle, gif: bool) -> (StreamOpts, Vec<String>) {
    let prefs = app.state::<Shots>().prefs.lock().clone();
    let mut notes = Vec::new();
    let mut opts = StreamOpts {
        cursor: prefs.highlight_cursor,
        clicks: prefs.recording.clicks,
        ..StreamOpts::default()
    };
    if !gif && (prefs.recording.system_audio || prefs.recording.microphone) {
        if os_major() < 15 {
            notes.push(super::recopts::AUDIO_SKIPPED.to_string());
        } else {
            opts.system_audio = prefs.recording.system_audio;
            if prefs.recording.microphone {
                let granted = match mic_access() {
                    MicAccess::Granted => true,
                    MicAccess::NotAsked => request_mic(),
                    MicAccess::Refused => false,
                };
                opts.microphone = granted;
                if !granted {
                    notes.push(super::recopts::MIC_SKIPPED.to_string());
                }
            }
        }
    }
    (opts, notes)
}

/// The HUD state of a running recording: the elapsed time, the size estimate of an mp4 and the notes.
fn hud_state(
    phase: &'static str,
    ms: u64,
    gif: bool,
    bitrate: u64,
    notes: &[String],
) -> RecordState {
    let est = (!gif).then(|| estimate_bytes(bitrate, ms as f64 / 1000.0));
    RecordState::at(phase, ms, gif)
        .with_estimate(est)
        .with_notes(notes)
}

fn start_recording(
    app: &AppHandle,
    display: u32,
    rect: Option<Rect>,
    gif: bool,
) -> Result<(), String> {
    let shots = app.state::<Shots>();
    let (opts, mut notes) = resolve_options(app, gif);
    let (quality, want_keys) = {
        let p = shots.prefs.lock();
        (p.recording.quality, p.recording.keystrokes)
    };
    // The keystroke pill: asked (Input Monitoring) only now that a recording with it on starts.
    let mut keys = None;
    if want_keys {
        if !super::keys_mac::granted() {
            super::keys_mac::request();
        }
        let content = sck::Content::fetch()?;
        let d = content.display(display).ok_or("that display is gone")?;
        match super::keys_mac::start(app, rect.unwrap_or(d.frame), d.frame) {
            Ok(k) => keys = Some(k),
            Err(note) => notes.push(note),
        }
    }
    let started = source(app, display, rect, quality, opts);
    let (src, bitrate) = match started {
        Ok(s) => s,
        Err(e) => {
            if let Some(k) = keys {
                k.stop(app);
            }
            return Err(e);
        }
    };
    let (dir, run, k) = {
        let live = shots.live.lock();
        let Some(l) = live.as_ref() else {
            drop(live);
            if let Some(k) = keys {
                k.stop(app);
            }
            return Err("the recording was cancelled".into());
        };
        (l.dir.clone(), l.run, l.done.len())
    };
    let path = segment_path(&dir, std::process::id(), run, k);
    let rec = match Recorder::start_source(src, &path) {
        Ok(r) => r,
        Err(e) => {
            if let Some(k) = keys {
                k.stop(app);
            }
            return Err(e);
        }
    };
    let mut live = shots.live.lock();
    let Some(l) = live.as_mut() else {
        // cancelled while the stream was starting
        drop(live);
        let _ = rec.stop();
        let _ = std::fs::remove_file(&path);
        if let Some(k) = keys {
            k.stop(app);
        }
        return Ok(());
    };
    // The options of the first segment are the ones every later segment keeps (a resume never asks again, and a
    // preset changed mid-recording must not give the segments different sizes: the join would fail).
    l.stream = opts;
    l.quality = quality;
    l.bitrate = bitrate;
    l.notes = notes.clone();
    l.keys = keys;
    l.current = Some(rec);
    l.current_path = Some(path);
    l.started = Instant::now();
    l.clock = Clock::start(0);
    drop(live);
    fit_hud(app, HudFit::Pill(&notes));
    emit(app, hud_state("recording", 0, gif, bitrate, &notes));
    Ok(())
}

// ───────────────────────────── pause / resume / stop ─────────────────────────────

/// End the running segment: stop the stream, keep the finished file in `done`.
fn end_segment(l: &mut Live) -> Result<(), String> {
    if let (Some(rec), Some(path)) = (l.current.take(), l.current_path.take()) {
        rec.stop()?;
        l.done.push(path);
    }
    Ok(())
}

fn pause(app: &AppHandle) -> Result<(), String> {
    let shots = app.state::<Shots>();
    let mut live = shots.live.lock();
    let l = live.as_mut().ok_or("nothing is recording")?;
    let now = l.now_ms();
    l.clock.pause(now);
    end_segment(l)?;
    let (ms, gif) = (l.clock.elapsed(now), l.gif);
    drop(live);
    emit(app, RecordState::at("paused", ms, gif));
    Ok(())
}

fn resume(app: &AppHandle) -> Result<(), String> {
    let shots = app.state::<Shots>();
    let (display, rect, gif, dir, run, k, opts, quality) = {
        let live = shots.live.lock();
        let l = live.as_ref().ok_or("nothing is recording")?;
        (
            l.display,
            l.rect,
            l.gif,
            l.dir.clone(),
            l.run,
            l.done.len(),
            l.stream,
            l.quality,
        )
    };
    let (src, _) = source(app, display, rect, quality, opts)?;
    let path = segment_path(&dir, std::process::id(), run, k);
    let rec = Recorder::start_source(src, &path)?;
    let mut live = shots.live.lock();
    let l = live.as_mut().ok_or("the recording was cancelled")?;
    l.current = Some(rec);
    l.current_path = Some(path);
    // the clock restarts its running segment on the caller's clock: `now_ms` keeps counting wall time since the
    // recording began, and the paused stretch is excluded by the clock itself.
    let now = l.now_ms();
    l.clock.resume(now);
    let ms = l.clock.elapsed(now);
    let (notes, bitrate) = (l.notes.clone(), l.bitrate);
    drop(live);
    emit(app, hud_state("recording", ms, gif, bitrate, &notes));
    Ok(())
}

fn discard(app: &AppHandle) {
    let shots = app.state::<Shots>();
    let taken = shots.live.lock().take();
    if let Some(mut l) = taken {
        stop_extras(app, &mut l);
        let _ = end_segment(&mut l);
        for p in l.done.iter().chain(l.current_path.iter()) {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Switch off what ran next to the recording: the keystroke pill.
fn stop_extras(app: &AppHandle, l: &mut Live) {
    if let Some(k) = l.keys.take() {
        k.stop(app);
    }
}

/// A recording that is joined and waiting for the person to trim it (or not) and save.
struct Waiting {
    /// The joined clip, a hidden file in the capture folder.
    path: PathBuf,
    secs: f64,
    gif: bool,
    dir: PathBuf,
}

/// The one recording in the trim step (a session has one recording at a time).
static WAITING: parking_lot::Mutex<Option<Waiting>> = parking_lot::Mutex::new(None);

/// How many poster frames the trim timeline shows.
const POSTERS: usize = 8;
/// Width of a poster frame, pixels (the timeline draws them ~50 points wide at 2x).
const POSTER_SIDE: u32 = 120;

fn waiting_path(dir: &Path, run: u64) -> PathBuf {
    dir.join(format!(
        ".atlas-rec-{}-{run}-joined.mp4",
        std::process::id()
    ))
}

/// Stop: end the last segment, join the segments and hand the clip to the trim step. The recording is not saved
/// yet: the person decides in the HUD (Save keeps all of it or the trimmed part). The session stays in its saving
/// state until [`save_cmd`] or [`discard_cmd`] ends it.
fn stop(app: &AppHandle) -> Result<(), String> {
    let shots = app.state::<Shots>();
    let (mut l, gif) = {
        let mut live = shots.live.lock();
        let mut l = live.take().ok_or("nothing is recording")?;
        let now = l.now_ms();
        l.clock.pause(now);
        let gif = l.gif;
        (l, gif)
    };
    stop_extras(app, &mut l);
    let notes = l.notes.clone();
    emit(
        app,
        RecordState::at("saving", l.clock.elapsed(l.now_ms()), gif).with_notes(&notes),
    );
    let joined = (|| -> Result<(PathBuf, video::Info), String> {
        end_segment(&mut l)?;
        let out = waiting_path(&l.dir, l.run);
        video::join(&l.done, &out)?;
        for p in &l.done {
            let _ = std::fs::remove_file(p);
        }
        let info = video::info(&out)?;
        Ok((out, info))
    })();
    // The HUD keeps the keyboard through the trim step (Enter saves, the handles take arrows); `finish_save` and
    // `discard_cmd` give it back. A failed join has no trim step, so it gives it back here.
    if joined.is_err() {
        let _ = app.run_on_main_thread(|| crate::dock_mac::overlay_keys(None));
    }
    match joined {
        Ok((path, info)) => {
            let posters = super::record::poster_times(info.secs, POSTERS)
                .into_iter()
                .filter_map(|t| {
                    let f = video::frame_at(&path, t, POSTER_SIDE).ok()?;
                    pixels::data_url(&f.data, f.size.0, f.size.1).ok()
                })
                .collect::<Vec<_>>();
            *WAITING.lock() = Some(Waiting {
                path,
                secs: info.secs,
                gif,
                dir: l.dir.clone(),
            });
            fit_hud(app, HudFit::Trim(&notes));
            emit(
                app,
                RecordState::trimming(info.secs, posters, gif).with_notes(&notes),
            );
            Ok(())
        }
        Err(e) => {
            for p in l.done.iter().chain(l.current_path.iter()) {
                let _ = std::fs::remove_file(p);
            }
            emit(app, RecordState::error(e.clone(), gif));
            super::feed(app, Event::Failed);
            Err(e)
        }
    }
}

/// How big the HUD window is.
#[derive(Clone, Copy)]
enum HudFit<'a> {
    /// The 120 px pill, taller when it carries notes.
    Pill(&'a [String]),
    /// The trim panel: wide for the timeline, taller for its notes.
    Trim(&'a [String]),
}

/// The HUD is a 120 px pill; the trim step needs a timeline, so it is widened for it and brought back after the
/// save (and `show_hud` resets it for the next recording). Notes (skipped options) make it taller so they are read
/// in full. The horizontal centre stays where it was.
fn fit_hud(app: &AppHandle, fit: HudFit) {
    let Some(w) = app.get_webview_window(&Surface::Hud.label(0)) else {
        return;
    };
    let (sw, sh) = Surface::Hud.size();
    let (nw, nh) = match fit {
        HudFit::Pill(notes) => (sw, hud_height(sh, notes)),
        HudFit::Trim(notes) => (TRIM_SIZE.0, hud_height(TRIM_SIZE.1, notes)),
    };
    let (Ok(p), Ok(sf), Ok(cur)) = (w.outer_position(), w.scale_factor(), w.outer_size()) else {
        let _ = w.set_size(tauri::LogicalSize::new(nw, nh));
        return;
    };
    let cx = f64::from(p.x) / sf + f64::from(cur.width) / sf / 2.0;
    let _ = w.set_size(tauri::LogicalSize::new(nw, nh));
    let _ = w.set_position(tauri::LogicalPosition::new(
        cx - nw / 2.0,
        f64::from(p.y) / sf,
    ));
}

/// The HUD window while trimming: wide enough for the timeline.
const TRIM_SIZE: (f64, f64) = (460.0, 210.0);

/// Save (HUD): keep `start..end` seconds of the waiting clip, or all of it when both are absent, then write the file
/// (or the GIF), and tell the HUD and the card. Runs on a worker thread: the HUD is not blocked while it works.
pub fn save_cmd(app: &AppHandle, start: Option<f64>, end: Option<f64>) -> Result<(), String> {
    let w = WAITING
        .lock()
        .take()
        .ok_or("There is no recording waiting to be saved.")?;
    // A clip too short to trim (or no range) is saved whole.
    let range = match (start, end) {
        (Some(s), Some(e)) => super::recopts::clamp_trim(w.secs, s, e),
        _ => None,
    };
    let app = app.clone();
    let gif = w.gif;
    emit(
        &app,
        RecordState::at("saving", (w.secs * 1000.0).round() as u64, gif),
    );
    std::thread::spawn(move || {
        let result = write_out(&app, &w, range);
        let _ = std::fs::remove_file(&w.path);
        finish_save(&app, result, gif);
    });
    Ok(())
}

/// The part of a waiting clip that is kept, written to its final file: trimmed (no re-encode) when a range cuts
/// something, a rename otherwise; then the GIF when the recording is one, with progress to the HUD.
fn write_out(app: &AppHandle, w: &Waiting, range: Option<(f64, f64)>) -> Result<Entry, String> {
    let shots = app.state::<Shots>();
    let prefs = shots.prefs.lock().clone();
    let out = runner::save_path(&w.dir, Kind::Recording);
    let (kept_secs, cut) = match range {
        Some(r) if !super::recopts::is_whole(w.secs, r) => (r.1 - r.0, Some(r)),
        _ => (w.secs, None),
    };
    match cut {
        Some((s, e)) => video::trim(&w.path, &out, s, e)?,
        None => std::fs::rename(&w.path, &out)
            .or_else(|_| std::fs::copy(&w.path, &out).map(|_| ()))
            .map_err(|e| format!("{}: {e}", out.display()))?,
    }
    let info = video::info(&out)?;
    let secs = if info.secs > 0.0 {
        info.secs
    } else {
        kept_secs
    };
    if w.gif {
        let gif_path = runner::save_path(&w.dir, Kind::Gif);
        let preset = gif::preset("balanced").ok_or("no GIF preset")?;
        let a = app.clone();
        let res = export_gif(&out, &gif_path, preset, |done, total| {
            emit(
                &a,
                RecordState::at("saving", (secs * 1000.0).round() as u64, true)
                    .with_progress(done, total),
            );
        });
        let _ = std::fs::remove_file(&out);
        let (gw, gh) = res?;
        runner::finish(app, &prefs, gif_path, Kind::Gif, (gw, gh), Some(secs))
    } else {
        runner::finish(app, &prefs, out, Kind::Recording, info.size, Some(secs))
    }
}

/// The HUD no longer needs the keyboard (the clip is saved, discarded or failed).
fn release_hud_keys(app: &AppHandle) {
    let _ = app.run_on_main_thread(|| crate::dock_mac::overlay_keys(None));
}

fn finish_save(app: &AppHandle, result: Result<Entry, String>, gif: bool) {
    release_hud_keys(app);
    match result {
        Ok(entry) => {
            fit_hud(app, HudFit::Pill(&[]));
            emit(app, RecordState::done(entry, gif));
            super::feed(app, Event::Saved);
        }
        Err(e) => {
            // an encode the person stopped is not a failure: the HUD closes quietly
            let cancelled = e == gif::CANCELLED;
            if !cancelled {
                emit(app, RecordState::error(e, gif));
            }
            super::feed(app, Event::Failed);
        }
    }
}

/// Discard (HUD) the recording waiting in the trim step.
pub fn discard_cmd(app: &AppHandle) {
    if let Some(w) = WAITING.lock().take() {
        let _ = std::fs::remove_file(&w.path);
        release_hud_keys(app);
        // `Failed` is the event that frees a saving session and hides the HUD
        super::feed(app, Event::Failed);
    }
}

// ───────────────────────────── commands (HUD) ─────────────────────────────

pub fn pause_cmd(app: &AppHandle) {
    super::feed(app, Event::Pause);
}

pub fn resume_cmd(app: &AppHandle) {
    super::feed(app, Event::Resume);
}

pub fn stop_cmd(app: &AppHandle) {
    super::feed(app, Event::Stop);
}

/// Esc / Cancel: while a GIF is being encoded it stops the encode (the session cannot cancel a save). A recording
/// waiting in the trim step is never thrown away by this (Esc must not lose a finished clip): only the explicit
/// Discard does. Otherwise it is the session's own cancel.
pub fn cancel_cmd(app: &AppHandle) {
    if gif::request_cancel() || WAITING.lock().is_some() {
        return;
    }
    super::feed(app, Event::Cancel);
}

// ───────────────────────────── GIF ─────────────────────────────

/// A 440 px poster of a clip (its first frame) for the quick card.
pub fn poster(path: &Path) -> Option<String> {
    let f = video::frame_at(path, 0.0, 440).ok()?;
    pixels::data_url(&f.data, f.size.0, f.size.1).ok()
}

/// Encode `video` as a GIF at `out` with `preset`; `progress(done, total)` after each frame. Returns the GIF size.
/// The encode stops within one frame when [`gif::request_cancel`] is called (Esc), and then leaves no file.
pub fn export_gif(
    video_path: &Path,
    out: &Path,
    preset: gif::Preset,
    progress: impl FnMut(usize, usize),
) -> Result<(u32, u32), String> {
    let info = video::info(video_path)?;
    let plan = gif::plan(info.size, info.fps, preset);
    let times = gif_times(info.secs, plan.fps);
    let encoding = gif::Encoding::begin();
    let reader = video::FrameReader::open(video_path, plan.width.max(plan.height));
    let frames = gif::collect_frames(
        &times,
        encoding.flag(),
        |t| {
            let f = reader.at(t)?;
            let data = if f.size == (plan.width, plan.height) {
                f.data
            } else {
                gif::downscale(&f.data, f.size, (plan.width, plan.height))
            };
            Ok(gif::Frame { rgba: data })
        },
        progress,
    )
    .map_err(gif::Stopped::into_message)?;
    let delays = gif::delays(frames.len(), plan.fps);
    let file = std::fs::File::create(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let written = gif::encode(
        std::io::BufWriter::new(file),
        (plan.width, plan.height),
        &frames,
        &delays,
    );
    if let Err(e) = written {
        let _ = std::fs::remove_file(out);
        return Err(e);
    }
    Ok((plan.width, plan.height))
}

fn known_video(state: &Shots, path: &str) -> Result<(PathBuf, Entry), String> {
    let p = PathBuf::from(path);
    let e = super::quick::known(state, &p)?;
    if e.kind != Kind::Recording {
        return Err("Only recordings can become a GIF.".into());
    }
    Ok((p, e))
}

pub fn estimate(state: &Shots, path: &str, preset: &str) -> Result<GifEstimate, String> {
    let (p, _) = known_video(state, path)?;
    let preset = gif::preset(preset).ok_or("Unknown GIF quality.")?;
    let info = video::info(&p)?;
    let plan = gif::plan(info.size, info.fps, preset);
    Ok(GifEstimate {
        width: plan.width,
        height: plan.height,
        fps: plan.fps,
        bytes: gif::estimate_bytes(plan, info.secs),
    })
}

pub async fn export(app: AppHandle, path: String, preset: String) -> Result<Entry, String> {
    let (p, entry) = {
        let state = app.state::<Shots>();
        known_video(&state, &path)?
    };
    let preset = gif::preset(&preset).ok_or("Unknown GIF quality.")?;
    let a = app.clone();
    tokio::task::spawn_blocking(move || {
        let state = a.state::<Shots>();
        let prefs = state.prefs.lock().clone();
        let out = runner::save_path(p.parent().unwrap_or(Path::new(".")), Kind::Gif);
        let (w, h) = export_gif(&p, &out, preset, |done, total| {
            let _ = a.emit(
                "shots://gif-progress",
                serde_json::json!({ "done": done, "total": total }),
            );
        })?;
        let bytes = std::fs::metadata(&out).map_err(|e| e.to_string())?.len();
        let e = Entry {
            path: out,
            kind: Kind::Gif,
            at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as u64),
            width: w,
            height: h,
            bytes,
            secs: entry.secs,
        };
        state.history.lock().push(e.clone(), prefs.history as usize);
        state.save_history();
        Ok(e)
    })
    .await
    .map_err(|e| e.to_string())?
}
