//! The audio of one dictation as a WAV file in memory: 16 kHz, mono, signed 16-bit little-endian (`wav_pcm_s16le`,
//! the format of `crew_voice_status.audio` on the wire), at most [`MAX_SECONDS`] of sound.
//!
//! The file is read while it is still being written (the page uploads pieces for the live preview), so the header
//! says "size unknown" (`0xFFFFFFFF`, what a streaming encoder writes) until [`Wav::finish`] patches the real sizes.
//! A receiver sizes the audio by the bytes it got after the 44-byte header, never by the header's fields.

/// Samples per second of the dictation format.
pub const RATE: u32 = 16_000;
/// The hard cap of one dictation, in seconds (`crew_voice_status.max_seconds`).
pub const MAX_SECONDS: usize = 300;
/// Bytes of the canonical PCM WAV header.
pub const HEADER: usize = 44;
/// Samples (and so frames: it is mono) that fit under the cap.
pub const MAX_SAMPLES: usize = RATE as usize * MAX_SECONDS;
/// The size field of a file whose length is not known yet.
const UNKNOWN: u32 = u32::MAX;

/// A growing WAV file.
#[derive(Debug)]
pub struct Wav {
    bytes: Vec<u8>,
}

impl Wav {
    /// An empty file: the header with unknown sizes, no sound.
    pub fn new() -> Self {
        let mut bytes = Vec::with_capacity(HEADER + 64 * 1024);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&UNKNOWN.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&1u16.to_le_bytes()); // mono
        bytes.extend_from_slice(&RATE.to_le_bytes());
        bytes.extend_from_slice(&(RATE * 2).to_le_bytes()); // bytes per second
        bytes.extend_from_slice(&2u16.to_le_bytes()); // bytes per frame
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&UNKNOWN.to_le_bytes());
        debug_assert_eq!(bytes.len(), HEADER);
        Self { bytes }
    }

    /// Add `samples`. Past the cap nothing more is taken: the return value is how many were, and `samples.len()`
    /// minus that is what the cap left out.
    pub fn push(&mut self, samples: &[i16]) -> usize {
        let take = samples.len().min(MAX_SAMPLES - self.len());
        self.bytes.reserve(take * 2);
        for s in &samples[..take] {
            self.bytes.extend_from_slice(&s.to_le_bytes());
        }
        take
    }

    /// Samples held.
    pub fn len(&self) -> usize {
        (self.bytes.len() - HEADER) / 2
    }

    /// No room left: the cap is reached.
    pub fn is_full(&self) -> bool {
        self.len() >= MAX_SAMPLES
    }

    /// Seconds of sound held.
    pub fn seconds(&self) -> f64 {
        self.len() as f64 / RATE as f64
    }

    /// Write the real sizes into the header. The file stays readable and growable: a later [`Wav::push`] makes the
    /// sizes stale, so call it when recording is over.
    pub fn finish(&mut self) {
        let data = (self.bytes.len() - HEADER) as u32;
        self.bytes[4..8].copy_from_slice(&(36 + data).to_le_bytes());
        self.bytes[40..44].copy_from_slice(&data.to_le_bytes());
    }

    /// Total bytes of the file (header included).
    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }

    /// Up to `max` bytes of the file from `offset`; empty at or past the end.
    pub fn slice(&self, offset: usize, max: usize) -> &[u8] {
        let from = offset.min(self.bytes.len());
        let to = from.saturating_add(max).min(self.bytes.len());
        &self.bytes[from..to]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_at(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
    }
    fn u16_at(b: &[u8], at: usize) -> u16 {
        u16::from_le_bytes(b[at..at + 2].try_into().unwrap())
    }

    #[test]
    fn a_finished_file_is_canonical_16k_mono_s16le_pcm() {
        let mut w = Wav::new();
        w.push(&[0, 1, -1, i16::MAX, i16::MIN]);
        w.finish();
        let b = w.slice(0, usize::MAX);
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(u32_at(b, 4) as usize, b.len() - 8, "RIFF size");
        assert_eq!(&b[8..16], b"WAVEfmt ");
        assert_eq!(u32_at(b, 16), 16, "fmt chunk size");
        assert_eq!(u16_at(b, 20), 1, "PCM");
        assert_eq!(u16_at(b, 22), 1, "mono");
        assert_eq!(u32_at(b, 24), 16_000, "sample rate");
        assert_eq!(u32_at(b, 28), 32_000, "bytes per second");
        assert_eq!(u16_at(b, 32), 2, "bytes per frame");
        assert_eq!(u16_at(b, 34), 16, "bits per sample");
        assert_eq!(&b[36..40], b"data");
        assert_eq!(u32_at(b, 40), 10, "data size");
        assert_eq!(b.len(), HEADER + 10);
        // Little endian, in the order pushed.
        assert_eq!(
            &b[HEADER..],
            &[0, 0, 1, 0, 0xFF, 0xFF, 0xFF, 0x7F, 0x00, 0x80]
        );
    }

    #[test]
    fn while_recording_the_header_says_unknown_size_and_the_sound_still_follows_it() {
        let mut w = Wav::new();
        w.push(&[7, 8]);
        let b = w.slice(0, usize::MAX);
        assert_eq!(u32_at(b, 4), u32::MAX);
        assert_eq!(u32_at(b, 40), u32::MAX);
        assert_eq!(b.len(), HEADER + 4);
    }

    #[test]
    fn the_cap_takes_exactly_300_seconds_and_names_what_it_left_out() {
        let mut w = Wav::new();
        let minute = vec![1i16; RATE as usize * 60];
        for _ in 0..4 {
            assert_eq!(w.push(&minute), minute.len());
        }
        assert!(!w.is_full());
        assert_eq!(w.seconds(), 240.0);
        // 61 more seconds: the 60 that fit are taken, the last one is not.
        let over = vec![2i16; RATE as usize * 61];
        assert_eq!(w.push(&over), RATE as usize * 60);
        assert!(w.is_full());
        assert_eq!(w.len(), MAX_SAMPLES);
        assert_eq!(w.push(&[3]), 0, "full: nothing more is taken");
    }

    #[test]
    fn a_file_stopped_at_the_cap_keeps_every_sample_and_its_size_matches_the_contract() {
        let mut w = Wav::new();
        // A ramp: every sample is distinguishable, so a dropped or duplicated one shows.
        let all: Vec<i16> = (0..MAX_SAMPLES + 5_000)
            .map(|i| (i % 30_000) as i16)
            .collect();
        let taken = w.push(&all);
        assert_eq!(taken, MAX_SAMPLES);
        w.finish();
        let b = w.slice(0, usize::MAX);
        assert_eq!(
            b.len(),
            HEADER + 300 * 32_000,
            "300 s of s16le mono at 16 kHz"
        );
        assert_eq!(u32_at(b, 40) as usize, 300 * 32_000);
        for (i, want) in all[..MAX_SAMPLES].iter().enumerate().step_by(997) {
            let at = HEADER + i * 2;
            assert_eq!(i16::from_le_bytes([b[at], b[at + 1]]), *want, "sample {i}");
        }
        let last = HEADER + (MAX_SAMPLES - 1) * 2;
        assert_eq!(
            i16::from_le_bytes([b[last], b[last + 1]]),
            all[MAX_SAMPLES - 1],
            "the last sample before the cap is kept"
        );
    }

    #[test]
    fn slices_cover_the_file_exactly_with_no_overlap_or_gap() {
        let mut w = Wav::new();
        w.push(&(0..1000).map(|i| i as i16).collect::<Vec<_>>());
        w.finish();
        let whole = w.slice(0, usize::MAX).to_vec();
        let mut got = Vec::new();
        let mut at = 0;
        loop {
            let piece = w.slice(at, 333);
            if piece.is_empty() {
                break;
            }
            got.extend_from_slice(piece);
            at += piece.len();
        }
        assert_eq!(got, whole);
        assert!(w.slice(w.byte_len(), 10).is_empty());
        assert!(
            w.slice(w.byte_len() + 99, 10).is_empty(),
            "past the end is empty, never a panic"
        );
        assert!(
            w.slice(5, usize::MAX).len() == whole.len() - 5,
            "a huge max does not overflow"
        );
    }
}
