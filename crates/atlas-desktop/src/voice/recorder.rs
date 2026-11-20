//! The dictation recorder: what is allowed when, who owns the audio, and how a recording ends. The microphone is a
//! [`Source`] behind a trait, so the whole state machine (a second start refused, the cap, a stop from anywhere,
//! a lost device) is tested with a fake that pushes frames, no sound card needed.
//!
//! The audio lives in memory only (never a file on disk) and is dropped at the next start, on `cancel`, and with
//! the process.

use std::sync::Arc;

use parking_lot::Mutex;
use serde::Serialize;

use super::pipe::{Meter, Mono16k};
use super::wav::Wav;

/// Why a recording ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// The person (or the composer) stopped it.
    Requested,
    /// 300 s of sound: everything up to the cap is kept and transcribed.
    Cap,
    /// The window went to the background: the microphone is released, what was said is kept.
    Background,
    /// The device stopped delivering (unplugged, taken over by another app).
    DeviceLost,
}

/// Why a recording could not start. The page translates the code; `detail` is for the log and the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum StartError {
    /// A recording is already running.
    Busy,
    /// The system refuses this app the microphone.
    Denied,
    /// No input device.
    NoDevice,
    /// The platform cannot show the permission prompt here (an agent instance).
    Unavailable { detail: String },
    /// The device or the stream could not be opened.
    Failed { detail: String },
}

/// What the recorder tells the shell while it runs.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A meter reading, 0..1 (one every 50 ms of sound).
    Level(f32),
    /// The recording ended by itself (cap or lost device): the shell stops it and tells the page.
    Ended(Reason),
}

/// The sink a microphone feeds. Cheap to clone; every clone feeds the same recording.
#[derive(Clone)]
pub struct Feed {
    inner: Arc<Mutex<Inner>>,
    emit: Arc<dyn Fn(Event) + Send + Sync>,
}

struct Inner {
    wav: Wav,
    conv: Option<Mono16k>,
    meter: Meter,
    /// Set once the cap (or a lost device) ended the recording: later frames are ignored.
    ended: bool,
    scratch: Vec<i16>,
}

impl Feed {
    /// Frames from the device: interleaved `channels` at `rate` Hz. Called on the audio thread.
    pub fn push(&self, frames: &[f32], channels: u16, rate: u32) {
        let (readings, capped) = {
            let mut g = self.inner.lock();
            if g.ended {
                return;
            }
            let g = &mut *g;
            let conv = g.conv.get_or_insert_with(|| Mono16k::new(rate));
            g.scratch.clear();
            conv.push(frames, usize::from(channels), &mut g.scratch);
            let taken = g.wav.push(&g.scratch);
            let readings = g.meter.push(&g.scratch[..taken]);
            let capped = g.wav.is_full();
            g.ended |= capped;
            (readings, capped)
        };
        for l in readings {
            (self.emit)(Event::Level(l));
        }
        if capped {
            (self.emit)(Event::Ended(Reason::Cap));
        }
    }

    /// The device failed: the recording ends with what it has.
    pub fn lost(&self) {
        let first = {
            let mut g = self.inner.lock();
            !std::mem::replace(&mut g.ended, true)
        };
        if first {
            (self.emit)(Event::Ended(Reason::DeviceLost));
        }
    }
}

/// A microphone. `open` starts delivering to `feed`; `close` releases the device and returns once it is released.
pub trait Source: Send {
    fn open(&mut self, feed: Feed) -> Result<(), StartError>;
    fn close(&mut self);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Live,
    /// Over; the audio is still readable until the next start or a cancel.
    Done(Reason),
}

/// What the page learns when a recording is over.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Take {
    pub reason: Reason,
    /// Bytes of the WAV file (header included): what `read` will deliver in total.
    pub bytes: usize,
    pub seconds: f64,
}

/// The recorder: one recording at a time.
pub struct Recorder<S> {
    source: S,
    phase: Phase,
    feed: Option<Feed>,
    emit: Arc<dyn Fn(Event) + Send + Sync>,
}

impl<S: Source> Recorder<S> {
    pub fn new(source: S, emit: impl Fn(Event) + Send + Sync + 'static) -> Self {
        Self {
            source,
            phase: Phase::Idle,
            feed: None,
            emit: Arc::new(emit),
        }
    }

    /// Whether the microphone is open.
    pub fn is_live(&self) -> bool {
        self.phase == Phase::Live
    }

    /// Start a recording. The previous one (if it was left unread) is dropped. Refused while one is live.
    pub fn start(&mut self) -> Result<(), StartError> {
        if self.is_live() {
            return Err(StartError::Busy);
        }
        self.feed = None;
        let feed = Feed {
            inner: Arc::new(Mutex::new(Inner {
                wav: Wav::new(),
                conv: None,
                meter: Meter::default(),
                ended: false,
                scratch: Vec::new(),
            })),
            emit: self.emit.clone(),
        };
        self.source.open(feed.clone())?;
        self.feed = Some(feed);
        self.phase = Phase::Live;
        Ok(())
    }

    /// End the recording and release the microphone. `None` when none is live (a stop that raced the cap or the
    /// background release). The audio stays readable.
    pub fn stop(&mut self, reason: Reason) -> Option<Take> {
        if !self.is_live() {
            return None;
        }
        self.source.close();
        self.phase = Phase::Done(reason);
        let mut g = self.feed.as_ref()?.inner.lock();
        g.ended = true;
        g.wav.finish();
        Some(Take {
            reason,
            bytes: g.wav.byte_len(),
            seconds: g.wav.seconds(),
        })
    }

    /// Release the microphone and throw the audio away.
    pub fn cancel(&mut self) {
        if self.is_live() {
            self.source.close();
        }
        self.phase = Phase::Idle;
        self.feed = None;
    }

    /// Up to `max` bytes of the WAV from `offset`, and the file's total so far. Readable while live (the header
    /// then says unknown size) and after the stop. Nothing recorded: an empty piece.
    pub fn read(&self, offset: usize, max: usize) -> Piece {
        match &self.feed {
            Some(f) => {
                let g = f.inner.lock();
                Piece {
                    data: g.wav.slice(offset, max).to_vec(),
                    total: g.wav.byte_len(),
                    live: self.phase == Phase::Live,
                }
            }
            None => Piece {
                data: Vec::new(),
                total: 0,
                live: false,
            },
        }
    }
}

/// A piece of the file.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub data: Vec<u8>,
    /// Bytes of the file so far.
    pub total: usize,
    /// Still recording: `total` can grow.
    pub live: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::wav::{HEADER, MAX_SAMPLES};

    /// A microphone that is a handle the test pushes frames through.
    #[derive(Clone, Default)]
    struct Fake {
        feed: Arc<Mutex<Option<Feed>>>,
        opened: Arc<Mutex<u32>>,
        closed: Arc<Mutex<u32>>,
        fail: Arc<Mutex<Option<StartError>>>,
    }
    impl Source for Fake {
        fn open(&mut self, feed: Feed) -> Result<(), StartError> {
            if let Some(e) = self.fail.lock().clone() {
                return Err(e);
            }
            *self.opened.lock() += 1;
            *self.feed.lock() = Some(feed);
            Ok(())
        }
        fn close(&mut self) {
            *self.closed.lock() += 1;
            *self.feed.lock() = None;
        }
    }
    impl Fake {
        /// `secs` seconds of a 16 kHz mono tone through the open microphone (nothing when it is closed).
        fn say(&self, secs: usize) {
            if let Some(f) = self.feed.lock().clone() {
                f.push(&vec![0.5f32; 16_000 * secs], 1, 16_000);
            }
        }
        fn is_open(&self) -> bool {
            self.feed.lock().is_some()
        }
    }

    fn rec() -> (Recorder<Fake>, Fake, Arc<Mutex<Vec<Event>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let e2 = events.clone();
        let fake = Fake::default();
        (
            Recorder::new(fake.clone(), move |e| e2.lock().push(e)),
            fake,
            events,
        )
    }

    #[test]
    fn a_second_start_while_live_is_refused_and_leaves_the_first_recording_alone() {
        let (mut r, fake, _) = rec();
        r.start().unwrap();
        fake.say(1);
        assert_eq!(r.start(), Err(StartError::Busy));
        assert_eq!(*fake.opened.lock(), 1, "the device was not opened twice");
        let t = r.stop(Reason::Requested).unwrap();
        assert_eq!(t.bytes, HEADER + 32_000, "the first second survived the refused start");
    }

    #[test]
    fn stop_releases_the_microphone_and_a_second_stop_is_nothing() {
        let (mut r, fake, _) = rec();
        r.start().unwrap();
        assert!(fake.is_open());
        let late = fake.feed.lock().clone().unwrap();
        assert!(r.stop(Reason::Requested).is_some());
        assert!(!fake.is_open(), "the device is released at stop");
        assert_eq!(r.stop(Reason::Background), None, "already over: no second take");
        assert_eq!(*fake.closed.lock(), 1);
        // A callback already in flight when the device closed is not recorded.
        late.push(&vec![0.5f32; 16_000], 1, 16_000);
        assert_eq!(r.read(0, usize::MAX).total, HEADER);
    }

    #[test]
    fn a_failed_open_leaves_the_recorder_idle_and_free_to_try_again() {
        let (mut r, fake, _) = rec();
        *fake.fail.lock() = Some(StartError::Denied);
        assert_eq!(r.start(), Err(StartError::Denied));
        assert!(!r.is_live());
        assert_eq!(r.stop(Reason::Requested), None);
        *fake.fail.lock() = None;
        r.start().unwrap();
        assert!(r.is_live());
    }

    #[test]
    fn the_cap_ends_the_recording_by_itself_and_keeps_all_300_seconds() {
        let (mut r, fake, events) = rec();
        r.start().unwrap();
        // 290 s, then 20 s more: the last 10 fit, the other 10 are beyond the cap.
        fake.say(290);
        assert!(!events.lock().iter().any(|e| matches!(e, Event::Ended(_))));
        fake.say(20);
        let ended: Vec<_> = events
            .lock()
            .iter()
            .filter(|e| matches!(e, Event::Ended(_)))
            .cloned()
            .collect();
        assert_eq!(ended, vec![Event::Ended(Reason::Cap)], "announced once");
        // The shell reacts to `Ended` with a stop: nothing is discarded.
        let t = r.stop(Reason::Cap).unwrap();
        assert_eq!(t.bytes, HEADER + MAX_SAMPLES * 2);
        assert_eq!(t.seconds, 300.0);
        let all = r.read(0, usize::MAX);
        assert_eq!(all.data.len(), t.bytes);
        assert!(!all.live);
        // Frames after the cap are ignored, not an error and not a second announcement.
        fake.say(1);
        assert_eq!(
            events
                .lock()
                .iter()
                .filter(|e| matches!(e, Event::Ended(_)))
                .count(),
            1
        );
    }

    #[test]
    fn a_lost_device_ends_the_recording_once_and_what_was_said_is_kept() {
        let (mut r, fake, events) = rec();
        r.start().unwrap();
        fake.say(2);
        let feed = fake.feed.lock().clone().unwrap();
        feed.lost();
        feed.lost();
        assert_eq!(
            events
                .lock()
                .iter()
                .filter(|e| **e == Event::Ended(Reason::DeviceLost))
                .count(),
            1
        );
        let t = r.stop(Reason::DeviceLost).unwrap();
        assert_eq!(t.seconds, 2.0);
    }

    #[test]
    fn the_audio_is_readable_while_live_and_whole_after_the_stop_with_a_valid_header() {
        let (mut r, fake, _) = rec();
        r.start().unwrap();
        fake.say(1);
        let live = r.read(0, 1000);
        assert!(live.live);
        assert_eq!(live.data.len(), 1000);
        assert_eq!(live.total, HEADER + 32_000);
        // The live header does not know the size yet.
        assert_eq!(&live.data[40..44], &u32::MAX.to_le_bytes());
        fake.say(1);
        r.stop(Reason::Requested).unwrap();
        let done = r.read(0, usize::MAX);
        assert!(!done.live);
        assert_eq!(done.total, HEADER + 64_000);
        assert_eq!(&done.data[40..44], &64_000u32.to_le_bytes(), "the real size after the stop");
    }

    #[test]
    fn cancel_drops_the_audio_and_the_next_start_begins_empty() {
        let (mut r, fake, _) = rec();
        r.start().unwrap();
        fake.say(3);
        r.cancel();
        assert!(!fake.is_open());
        assert_eq!(r.read(0, usize::MAX).total, 0, "nothing of the cancelled take is readable");
        r.start().unwrap();
        assert_eq!(r.read(0, usize::MAX).total, HEADER, "a new take starts at zero, not after the old audio");
        // Cancel with nothing live is harmless.
        r.stop(Reason::Requested).unwrap();
        r.cancel();
        r.cancel();
    }

    #[test]
    fn a_new_start_after_a_stop_drops_the_unread_previous_take() {
        let (mut r, fake, _) = rec();
        r.start().unwrap();
        fake.say(2);
        r.stop(Reason::Requested).unwrap();
        r.start().unwrap();
        fake.say(1);
        let t = r.stop(Reason::Requested).unwrap();
        assert_eq!(t.seconds, 1.0);
    }

    #[test]
    fn the_meter_emits_a_reading_per_50_ms_of_sound() {
        let (mut r, fake, events) = rec();
        r.start().unwrap();
        fake.say(1);
        let levels = events
            .lock()
            .iter()
            .filter(|e| matches!(e, Event::Level(_)))
            .count();
        assert_eq!(levels, 20);
        r.cancel();
    }
}
