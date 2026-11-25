//! The real microphone: the system's default input device through `cpal` (CoreAudio, WASAPI, ALSA), delivering
//! interleaved f32 frames to a [`Feed`].
//!
//! A `cpal` stream is not `Send`, and dropping it is what releases the device, so one thread owns it from the first
//! frame to the release: `open` starts the thread and waits until the stream plays (or says why not), `close`
//! tells it to drop the stream and waits until that is done. Nothing of the device outlives `close`.

use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Data, SampleFormat};

use super::recorder::{Feed, Source, StartError};

/// How long the device gets to start playing before the recording is refused (a sound server that hangs).
const OPEN_TIMEOUT: Duration = Duration::from_secs(10);

/// The default input device.
#[derive(Default)]
pub struct Mic {
    running: Option<Running>,
}

struct Running {
    stop: mpsc::Sender<()>,
    thread: JoinHandle<()>,
}

impl Source for Mic {
    fn open(&mut self, feed: Feed) -> Result<(), StartError> {
        if self.running.is_some() {
            return Err(StartError::Busy);
        }
        let (ready_tx, ready_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("atlas-voice".into())
            .spawn(move || match start_stream(feed) {
                Ok(stream) => {
                    let _ = ready_tx.send(Ok(()));
                    // The sender is dropped or sends: either way the person is done with the microphone.
                    let _ = stop_rx.recv();
                    drop(stream);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .map_err(|e| StartError::Failed {
                detail: format!("could not start the audio thread: {e}"),
            })?;
        match ready_rx.recv_timeout(OPEN_TIMEOUT) {
            Ok(Ok(())) => {
                self.running = Some(Running {
                    stop: stop_tx,
                    thread,
                });
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                // The device never answered: tell the thread to give the stream back whenever it does.
                drop(stop_tx);
                Err(StartError::Failed {
                    detail: "the audio device did not start in time".into(),
                })
            }
        }
    }

    fn close(&mut self) {
        if let Some(r) = self.running.take() {
            let _ = r.stop.send(());
            let _ = r.thread.join();
        }
    }
}

impl Drop for Mic {
    fn drop(&mut self) {
        self.close();
    }
}

fn start_stream(feed: Feed) -> Result<cpal::Stream, StartError> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or(StartError::NoDevice)?;
    let supported = device
        .default_input_config()
        .map_err(|e| StartError::Failed {
            detail: format!("input configuration: {e}"),
        })?;
    let format = supported.sample_format();
    let config = supported.config();
    let (channels, rate) = (config.channels, config.sample_rate);
    let lost = feed.clone();
    // Reused by every callback: the audio thread does not allocate after the first buffer.
    let mut buf: Vec<f32> = Vec::new();
    let stream = device
        .build_input_stream_raw(
            config,
            format,
            move |data, _| {
                if frames(data, &mut buf) {
                    feed.push(&buf, channels, rate);
                } else {
                    // A format this build cannot read: end the recording rather than record silence.
                    feed.lost();
                }
            },
            move |_| lost.lost(),
            None,
        )
        .map_err(|e| match e.kind() {
            cpal::ErrorKind::DeviceNotAvailable => StartError::NoDevice,
            _ => StartError::Failed {
                detail: format!("{format:?} stream: {e}"),
            },
        })?;
    stream.play().map_err(|e| StartError::Failed {
        detail: format!("play: {e}"),
    })?;
    Ok(stream)
}

/// The samples of a callback buffer as f32 in `out` (whatever the device's format is); false for a format this
/// build does not read.
fn frames(data: &Data, out: &mut Vec<f32>) -> bool {
    macro_rules! convert {
        ($t:ty) => {
            match data.as_slice::<$t>() {
                Some(s) => {
                    out.clear();
                    out.extend(s.iter().map(|v| cpal::Sample::to_sample::<f32>(*v)));
                    true
                }
                None => false,
            }
        };
    }
    match data.sample_format() {
        SampleFormat::F32 => convert!(f32),
        SampleFormat::I16 => convert!(i16),
        SampleFormat::I32 => convert!(i32),
        SampleFormat::U16 => convert!(u16),
        SampleFormat::I8 => convert!(i8),
        SampleFormat::U8 => convert!(u8),
        SampleFormat::I64 => convert!(i64),
        SampleFormat::U32 => convert!(u32),
        SampleFormat::U64 => convert!(u64),
        SampleFormat::F64 => convert!(f64),
        _ => false,
    }
}
