//! From what the microphone delivers (any channel count and rate, interleaved f32) to what the dictation file holds
//! (16 kHz mono s16le), plus the level of the sound for the meter. Pure: no device, no clock; time is counted in
//! samples, so a test feeds seconds of audio in microseconds.

use super::wav::RATE;

/// Streaming converter: mixes the channels down (their mean) and resamples by averaging over each output sample's
/// span of input (a box filter: enough low-pass for speech at 44.1/48 kHz, and exact for integer ratios).
#[derive(Debug)]
pub struct Mono16k {
    /// Input samples per output sample.
    step: f64,
    /// Input weight still missing from the output sample being built.
    need: f64,
    acc: f64,
}

impl Mono16k {
    /// A converter for a device that runs at `rate` Hz.
    pub fn new(rate: u32) -> Self {
        let step = f64::from(rate.max(1)) / f64::from(RATE);
        Self {
            step,
            need: step,
            acc: 0.0,
        }
    }

    /// Convert interleaved `frames` of `channels` and append the 16 kHz mono samples to `out`.
    pub fn push(&mut self, frames: &[f32], channels: usize, out: &mut Vec<i16>) {
        let channels = channels.max(1);
        for frame in frames.chunks_exact(channels) {
            let x = f64::from(frame.iter().sum::<f32>()) / channels as f64;
            let mut w = 1.0;
            while w >= self.need {
                self.acc += x * self.need;
                out.push(to_i16(self.acc / self.step));
                w -= self.need;
                self.need = self.step;
                self.acc = 0.0;
            }
            self.acc += x * w;
            self.need -= w;
        }
    }
}

/// A sample in -1..1 as s16 (clipped, rounded).
fn to_i16(x: f64) -> i16 {
    (x.clamp(-1.0, 1.0) * f64::from(i16::MAX)).round() as i16
}

/// The loudness of the sound for the meter, one value per [`Meter::BLOCK`] samples: the block's RMS on a dB scale
/// from silence (0) to full scale (1), the way a level meter reads, not the raw amplitude (speech would hardly move
/// a linear bar).
#[derive(Debug, Default)]
pub struct Meter {
    sum: f64,
    n: usize,
}

impl Meter {
    /// 50 ms of 16 kHz audio: 20 readings a second.
    pub const BLOCK: usize = RATE as usize / 20;
    /// The quietest level shown, in dBFS: below it the meter reads 0.
    const FLOOR_DB: f64 = -60.0;

    /// Take `samples`; returns the reading of every block they completed, in order.
    pub fn push(&mut self, samples: &[i16]) -> Vec<f32> {
        let mut out = Vec::new();
        for s in samples {
            let v = f64::from(*s) / f64::from(i16::MAX);
            self.sum += v * v;
            self.n += 1;
            if self.n == Self::BLOCK {
                out.push(level(self.sum / self.n as f64));
                self.sum = 0.0;
                self.n = 0;
            }
        }
        out
    }
}

/// 0..1 for a mean square of samples in -1..1.
fn level(mean_square: f64) -> f32 {
    if mean_square <= 0.0 {
        return 0.0;
    }
    let db = 10.0 * mean_square.log10();
    ((db - Meter::FLOOR_DB) / -Meter::FLOOR_DB).clamp(0.0, 1.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert(rate: u32, channels: usize, frames: &[f32]) -> Vec<i16> {
        let mut out = Vec::new();
        Mono16k::new(rate).push(frames, channels, &mut out);
        out
    }

    #[test]
    fn a_16k_mono_source_passes_through_unchanged() {
        let src: Vec<f32> = [0.0, 0.5, -0.5, 1.0, -1.0].to_vec();
        let got = convert(16_000, 1, &src);
        assert_eq!(got, vec![0, 16384, -16384, 32767, -32767]);
    }

    #[test]
    fn a_48k_source_is_averaged_down_by_three_and_the_length_is_exact() {
        // One second in, one second out.
        let src = vec![0.25f32; 48_000];
        let got = convert(48_000, 1, &src);
        assert_eq!(got.len(), 16_000);
        assert!(
            got.iter().all(|s| *s == 8192),
            "a constant stays constant: {:?}",
            &got[..4]
        );
        // Three different samples become their mean.
        let got = convert(48_000, 1, &[0.0, 0.3, 0.6]);
        assert_eq!(got, vec![(0.3f64 * 32767.0).round() as i16]);
    }

    #[test]
    fn a_44100_source_keeps_the_duration_within_one_sample_across_pushes() {
        // Pushed in odd-sized pieces: the fractional position must carry over, or each piece drifts.
        let mut c = Mono16k::new(44_100);
        let mut out = Vec::new();
        let src = vec![0.1f32; 44_100];
        for piece in src.chunks(441 * 7 + 3) {
            c.push(piece, 1, &mut out);
        }
        assert!(
            (out.len() as i64 - 16_000).abs() <= 1,
            "one second of 44.1 kHz is 16000 samples, got {}",
            out.len()
        );
    }

    #[test]
    fn stereo_is_the_mean_of_the_channels_and_a_trailing_half_frame_is_ignored() {
        // L=1.0 R=0.0 -> 0.5; the last lone sample is not a frame.
        let got = convert(16_000, 2, &[1.0, 0.0, 0.0, 1.0, 0.7]);
        assert_eq!(got, vec![16384, 16384]);
    }

    #[test]
    fn samples_beyond_full_scale_clip_instead_of_wrapping() {
        let got = convert(16_000, 1, &[3.0, -3.0]);
        assert_eq!(got, vec![32767, -32767]);
    }

    #[test]
    fn a_device_slower_than_16k_still_fills_the_timeline() {
        let got = convert(8_000, 1, &vec![0.5f32; 8_000]);
        assert!((got.len() as i64 - 16_000).abs() <= 1, "{}", got.len());
        assert!(got.iter().all(|s| *s == 16384));
    }

    #[test]
    fn the_meter_reads_one_value_per_50_ms_on_a_db_scale() {
        let mut m = Meter::default();
        // Silence reads 0, full scale reads 1, -30 dBFS (RMS 0.0316) reads half.
        assert_eq!(m.push(&vec![0i16; Meter::BLOCK]), vec![0.0]);
        let full = m.push(&vec![i16::MAX; Meter::BLOCK]);
        assert!((full[0] - 1.0).abs() < 1e-3, "{full:?}");
        let half = m.push(&vec![(0.0316 * 32767.0) as i16; Meter::BLOCK]);
        assert!((half[0] - 0.5).abs() < 0.02, "{half:?}");
        // Two and a half blocks: two readings now, the rest waits for the next push.
        let r = m.push(&vec![1000i16; Meter::BLOCK * 2 + Meter::BLOCK / 2]);
        assert_eq!(r.len(), 2);
        let r = m.push(&vec![1000i16; Meter::BLOCK / 2]);
        assert_eq!(r.len(), 1, "the half block completed with the next push");
    }
}
