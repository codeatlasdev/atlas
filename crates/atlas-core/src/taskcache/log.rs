//! The captured output of a cached run: chunks tagged with the stream they came from, stored as
//! one blob (`[stream: u8][len: u32 LE][bytes]…`) and replayed to the same streams on a hit.

use std::io::{self, Write};

/// What a run printed is kept up to this many bytes (the newest ones): a build log of hundreds of
/// MiB must not decide the size of the cache.
pub const MAX_LOG_BYTES: usize = 8 << 20;
/// How much of the end of the log `last_result` returns.
pub const TAIL_BYTES: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub stream: Stream,
    pub bytes: Vec<u8>,
}

/// Keep the newest chunks within [`MAX_LOG_BYTES`] (cutting the oldest chunk's head if needed).
pub fn clamp(chunks: &mut Vec<Chunk>) {
    let mut total: usize = chunks.iter().map(|c| c.bytes.len()).sum();
    while total > MAX_LOG_BYTES {
        let over = total - MAX_LOG_BYTES;
        let first = &mut chunks[0];
        if first.bytes.len() <= over {
            total -= first.bytes.len();
            chunks.remove(0);
        } else {
            first.bytes.drain(..over);
            break;
        }
    }
}

pub fn encode(chunks: &[Chunk]) -> Vec<u8> {
    let mut out = Vec::with_capacity(chunks.iter().map(|c| c.bytes.len() + 5).sum());
    for c in chunks {
        out.push(match c.stream {
            Stream::Stdout => 1,
            Stream::Stderr => 2,
        });
        out.extend_from_slice(&(c.bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&c.bytes);
    }
    out
}

/// `None` when the blob is not a well-formed log (a truncated or foreign file).
pub fn decode(mut b: &[u8]) -> Option<Vec<Chunk>> {
    let mut out = Vec::new();
    while !b.is_empty() {
        let (head, rest) = b.split_at_checked(5)?;
        let len = u32::from_le_bytes(head[1..5].try_into().ok()?) as usize;
        let (bytes, rest) = rest.split_at_checked(len)?;
        let stream = match head[0] {
            1 => Stream::Stdout,
            2 => Stream::Stderr,
            _ => return None,
        };
        out.push(Chunk {
            stream,
            bytes: bytes.to_vec(),
        });
        b = rest;
    }
    Some(out)
}

/// Write each chunk to the stream it came from.
pub fn replay(chunks: &[Chunk], out: &mut impl Write, err: &mut impl Write) -> io::Result<()> {
    for c in chunks {
        match c.stream {
            Stream::Stdout => out.write_all(&c.bytes)?,
            Stream::Stderr => err.write_all(&c.bytes)?,
        }
    }
    out.flush()?;
    err.flush()
}

/// The last [`TAIL_BYTES`] of the log as text (both streams, in order), cut at a character edge.
pub fn tail(chunks: &[Chunk]) -> String {
    let all: Vec<u8> = chunks
        .iter()
        .flat_map(|c| c.bytes.iter().copied())
        .collect();
    let from = all.len().saturating_sub(TAIL_BYTES);
    String::from_utf8_lossy(&all[from..])
        .trim_start_matches('\u{FFFD}')
        .to_string()
}
