//! `usage.jsonl` and its rotation (#295).
//!
//! The live file is cut off into `usage-<YYYY-MM-DD>.jsonl.gz` (the UTC day of its newest record) once it
//! passes its cap, instead of being trimmed to its newest half, so a total never loses the records older
//! than the cut. Two rotations on the same day add a gzip member to the same file (a multi-member gzip is
//! one gzip), so a name is never overwritten. At most `keep` rotated files are kept; the oldest go.
//! A reader sums the rotated files, oldest first, then the live one.

use std::io::{self, Read, Write};
use std::path::Path;

use crate::paths;

use super::jsonl;

/// The live file.
pub(super) const LIVE: &str = "usage.jsonl";

const PREFIX: &str = "usage-";
const SUFFIX: &str = ".jsonl.gz";

/// Whether `name` is a rotated usage file: `usage-YYYY-MM-DD.jsonl.gz` with a real calendar date. The only
/// way a name reaches a path from an archive, so it is strict.
pub(super) fn is_rotated_name(name: &str) -> bool {
    name.strip_prefix(PREFIX)
        .and_then(|n| n.strip_suffix(SUFFIX))
        .is_some_and(|d| d.len() == 10 && chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok())
}

/// `usage-<YYYY-MM-DD>.jsonl.gz`, the UTC day of `ts` (unix seconds; an out-of-range `ts` is day 0, 1970-01-01).
pub(super) fn rotated_name(ts: u64) -> String {
    let day = chrono::DateTime::from_timestamp(i64::try_from(ts).unwrap_or(0), 0)
        .unwrap_or_default()
        .format("%Y-%m-%d");
    format!("{PREFIX}{day}{SUFFIX}")
}

/// The rotated files of the crewmate dir `dir`, oldest first (the date sorts as text).
pub(super) fn rotated_names(dir: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut names: Vec<String> = rd
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| is_rotated_name(n))
        .collect();
    names.sort();
    names
}

/// The lines of the rotated files, then of the live file. A rotated file that cannot be read costs its own
/// records (up to where it breaks), not the others.
pub(super) fn read_lines(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for name in rotated_names(dir) {
        let Ok(f) = std::fs::File::open(dir.join(&name)) else {
            continue;
        };
        let mut bytes = Vec::new();
        // A damaged gzip keeps what was decoded before the damage.
        if let Err(e) = flate2::read::MultiGzDecoder::new(f).read_to_end(&mut bytes) {
            eprintln!("atlas crew: {} is damaged: {e}", dir.join(&name).display());
        }
        out.extend(jsonl::lines(io::Cursor::new(bytes)).map(|l| l.text));
    }
    out.extend(jsonl::read_lines(&dir.join(LIVE)));
    out
}

/// Move the live file of `dir` into the rotated file of the day of `ts` (its newest record) and delete the
/// oldest rotated files past `keep`.
pub(super) fn rotate(dir: &Path, ts: u64, keep: usize) -> io::Result<()> {
    let live = dir.join(LIVE);
    let data = std::fs::read(&live)?;
    let target = dir.join(rotated_name(ts));

    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&data)?;
    let member = gz.finish()?;
    // Same day again: the new member follows the old ones, atomically (one write of the whole file).
    let mut all = std::fs::read(&target).unwrap_or_default();
    all.extend_from_slice(&member);
    paths::write_private(&target, &all)?;
    // The gzip is durable before the live file goes: a crash in between duplicates records on the next
    // rotation of the same day at worst, never loses them.
    std::fs::remove_file(&live)?;

    let names = rotated_names(dir);
    for old in names.iter().take(names.len().saturating_sub(keep)) {
        let _ = std::fs::remove_file(dir.join(old));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Contract (chrono → jiff migration, #540): rotated usage files are named by the UTC day of the newest record,
    // and a total sums every file that name matches, so the name must never move for the same instant.
    #[test]
    fn a_rotated_file_is_named_by_the_utc_day_of_its_newest_record() {
        let day = |ts| rotated_name(ts);
        assert_eq!(day(0), "usage-1970-01-01.jsonl.gz");
        assert_eq!(day(86_399), "usage-1970-01-01.jsonl.gz");
        assert_eq!(day(86_400), "usage-1970-01-02.jsonl.gz");
        assert_eq!(day(951_782_400), "usage-2000-02-29.jsonl.gz", "a leap day");
        assert_eq!(day(1_700_000_000), "usage-2023-11-14.jsonl.gz");
        assert_eq!(day(1_794_873_599), "usage-2026-11-16.jsonl.gz");
        assert_eq!(day(1_794_873_600), "usage-2026-11-17.jsonl.gz");
        assert_eq!(day(253_402_300_799), "usage-9999-12-31.jsonl.gz");
        // A record time that no calendar holds is day 0, never a panic and never a missing name.
        assert_eq!(day(u64::MAX), "usage-1970-01-01.jsonl.gz");
        assert_eq!(day(i64::MAX as u64), "usage-1970-01-01.jsonl.gz");
    }

    #[test]
    fn only_a_real_calendar_date_is_a_rotated_name() {
        for ok in [
            "usage-1970-01-01.jsonl.gz",
            "usage-2026-11-17.jsonl.gz",
            "usage-2028-02-29.jsonl.gz",
            "usage-2000-02-29.jsonl.gz",
            "usage-9999-12-31.jsonl.gz",
        ] {
            assert!(is_rotated_name(ok), "{ok}");
        }
        for bad in [
            "usage-2026-02-29.jsonl.gz",
            "usage-1900-02-29.jsonl.gz",
            "usage-2026-13-01.jsonl.gz",
            "usage-2026-00-10.jsonl.gz",
            "usage-2026-11-00.jsonl.gz",
            "usage-2026-11-31.jsonl.gz",
            "usage-2026-1-17.jsonl.gz",
            "usage-2026-11-1.jsonl.gz",
            "usage-2026-11-17.jsonl",
            "usage-2026-11-17.jsonl.gz.tmp",
            "usage-.jsonl.gz",
            "usage-../../x.jsonl.gz",
            "usage-2026-11-17 .jsonl.gz",
            "usage-2026/11/17.jsonl.gz",
            "usage-2026-11-1a.jsonl.gz",
            "usage-２０２６-11-17.jsonl.gz",
            "usage.jsonl",
            "audit-2026-11-17.jsonl.gz",
        ] {
            assert!(!is_rotated_name(bad), "{bad}");
        }
    }
}
