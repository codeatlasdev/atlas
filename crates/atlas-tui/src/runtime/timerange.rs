//! Time-range filter of the log view: `Last 1/5/15/60 min`, `Since restart`,
//! `Since stack start` and `Custom` clock times (today).
//!
//! `TimeRange` is what the user picked; `Window` is that choice *resolved*
//! against the buffer and the clock into plain bounds the incremental
//! `ViewIndex` can apply per line. Relative windows keep only their length —
//! the index advances their cutoff with `ViewIndex::advance_clock`, so a
//! sliding window never forces a rescan.

use chrono::{Local, NaiveTime, TimeZone, Timelike};

use super::logs::LogBuffer;

/// The presets `V` cycles through, in order (after `All`).
pub const PRESET_MINUTES: [u32; 4] = [1, 5, 15, 60];

/// What the user chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimeRange {
    #[default]
    All,
    /// The last N minutes, sliding.
    LastMinutes(u32),
    /// From the newest restart marker of the viewed service (of any service on
    /// the All tab).
    SinceRestart,
    /// From the first line the stack printed (the oldest line held).
    SinceStackStart,
    /// Clock times of today, in seconds from local midnight (`to` inclusive of
    /// its whole second); `to = None` is open-ended.
    Custom { from_s: u32, to_s: Option<u32> },
}

impl TimeRange {
    /// Next preset of the `V` cycle: all → 1m → 5m → 15m → 60m → restart →
    /// stack start → all. `Custom` re-enters the cycle at `All`.
    pub fn next(self) -> Self {
        match self {
            Self::All | Self::Custom { .. } => Self::LastMinutes(PRESET_MINUTES[0]),
            Self::LastMinutes(m) => match PRESET_MINUTES.iter().position(|&p| p == m) {
                Some(i) if i + 1 < PRESET_MINUTES.len() => Self::LastMinutes(PRESET_MINUTES[i + 1]),
                _ => Self::SinceRestart,
            },
            Self::SinceRestart => Self::SinceStackStart,
            Self::SinceStackStart => Self::All,
        }
    }

    /// Badge text (`last 5m`, `since restart`, `14:05–14:20`).
    pub fn label(self) -> String {
        match self {
            Self::All => "all time".into(),
            Self::LastMinutes(m) if m % 60 == 0 => format!("last {}h", m / 60),
            Self::LastMinutes(m) => format!("last {m}m"),
            Self::SinceRestart => "since restart".into(),
            Self::SinceStackStart => "since start".into(),
            Self::Custom { from_s, to_s: None } => format!("from {}", clock(from_s)),
            Self::Custom {
                from_s,
                to_s: Some(to_s),
            } => format!("{}–{}", clock(from_s), clock(to_s)),
        }
    }

    pub fn is_all(self) -> bool {
        self == Self::All
    }
}

/// `HH:MM` of seconds-from-midnight.
fn clock(s: u32) -> String {
    format!("{:02}:{:02}", s / 3600, s / 60 % 60)
}

/// `14:05`, `14:05:30`, `9:5` → seconds from midnight. `None` when it is not a
/// valid 24-hour clock time.
fn parse_clock(s: &str) -> Option<u32> {
    let mut parts = s.trim().split(':');
    let h: u32 = parts.next()?.trim().parse().ok()?;
    let m: u32 = parts.next()?.trim().parse().ok()?;
    let sec: u32 = match parts.next() {
        Some(p) => p.trim().parse().ok()?,
        None => 0,
    };
    if parts.next().is_some() || h > 23 || m > 59 || sec > 59 {
        return None;
    }
    Some(h * 3600 + m * 60 + sec)
}

/// Parse the custom field: `14:05` (from, open-ended) or `14:05–14:20`
/// (`-`, `–`, `—` or `..` separate; both ends inclusive). A range whose end is
/// before its start is rejected.
pub fn parse_custom(input: &str) -> Result<TimeRange, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("type a time: 14:05 or 14:05–14:20".into());
    }
    let sep = ['–', '—', '-'];
    let (a, b) = if let Some((a, b)) = input.split_once("..") {
        (a, Some(b))
    } else if let Some(i) = input.find(sep) {
        let sep_len = input[i..].chars().next().map_or(1, char::len_utf8);
        (&input[..i], Some(&input[i + sep_len..]))
    } else {
        (input, None)
    };
    let from_s = parse_clock(a).ok_or_else(|| format!("not a time: {:?}", a.trim()))?;
    let to_s = match b {
        None => None,
        Some(b) if b.trim().is_empty() => None,
        // an end without seconds means the whole minute: `14:20` ends at 14:20:59
        Some(b) => Some(
            parse_clock(b).ok_or_else(|| format!("not a time: {:?}", b.trim()))?
                + if b.matches(':').count() == 1 { 59 } else { 0 },
        ),
    };
    if to_s.is_some_and(|t| t < from_s) {
        return Err("the end is before the start".into());
    }
    Ok(TimeRange::Custom { from_s, to_s })
}

/// A `TimeRange` resolved to concrete bounds. Part of the view key: any change
/// rebuilds the index once (a new restart marker, a new day, a changed custom
/// range); a *sliding* window only stores its length and is advanced in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Window {
    /// Lines older than this sequence number are out (restart / stack start).
    pub min_seq: u64,
    /// Sliding window length; the cutoff is `now - last_ms`.
    pub last_ms: Option<u64>,
    /// Absolute lower bound, epoch millis.
    pub from_ms: Option<u64>,
    /// Absolute inclusive upper bound, epoch millis.
    pub to_ms: Option<u64>,
}

impl Window {
    pub fn is_unbounded(&self) -> bool {
        *self == Self::default()
    }

    /// Does a line at `(seq, ts)` pass, given the sliding cutoff `cutoff_ms`?
    pub fn allows(&self, seq: u64, ts: u64, cutoff_ms: u64) -> bool {
        seq >= self.min_seq
            && ts >= cutoff_ms
            && self.from_ms.is_none_or(|f| ts >= f)
            && self.to_ms.is_none_or(|t| ts <= t)
    }

    /// First instant a sliding window currently lets through.
    pub fn cutoff(&self, now_ms: u64) -> u64 {
        self.last_ms.map_or(0, |l| now_ms.saturating_sub(l))
    }
}

/// Epoch millis of `s` seconds after local midnight of the day containing
/// `now_ms`. `None` for a nonexistent local time (DST gap) — callers treat the
/// range as unresolvable and show nothing rather than guess.
fn local_second(now_ms: u64, s: u32) -> Option<u64> {
    let day = Local
        .timestamp_millis_opt(now_ms as i64)
        .single()?
        .date_naive();
    let t = NaiveTime::from_num_seconds_from_midnight_opt(s, 0)?;
    let at = Local.from_local_datetime(&day.and_time(t)).earliest()?;
    u64::try_from(at.timestamp_millis()).ok()
}

/// Resolve `range` for a view over `service` (`None` = All tab) at `now_ms`.
///
/// * `SinceRestart`: the newest restart marker of `service` (any service on
///   All); with none, the whole buffer (nothing restarted yet).
/// * `SinceStackStart`: the oldest line held.
/// * `Custom`: today's clock times; `to` covers its whole second.
pub fn resolve(
    range: TimeRange,
    buf: &LogBuffer,
    service: Option<&str>,
    now_ms: u64,
) -> Option<Window> {
    let mut w = Window::default();
    match range {
        TimeRange::All => {}
        TimeRange::LastMinutes(m) => w.last_ms = Some(u64::from(m) * 60_000),
        TimeRange::SinceRestart => {
            w.min_seq = buf.last_restart_seq(service).unwrap_or(0);
        }
        TimeRange::SinceStackStart => w.min_seq = buf.oldest_seq(None),
        TimeRange::Custom { from_s, to_s } => {
            w.from_ms = Some(local_second(now_ms, from_s)?);
            if let Some(t) = to_s {
                w.to_ms = Some(local_second(now_ms, t)? + 999);
            }
        }
    }
    Some(w)
}

/// Seconds since local midnight of `now_ms` (for the custom field's default).
pub fn seconds_of_day(now_ms: u64) -> u32 {
    Local
        .timestamp_millis_opt(now_ms as i64)
        .single()
        .map_or(0, |d| d.num_seconds_from_midnight())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_walks_every_preset_then_wraps_to_all() {
        let mut r = TimeRange::All;
        let mut seen = vec![];
        for _ in 0..7 {
            r = r.next();
            seen.push(r.label());
        }
        assert_eq!(
            seen,
            [
                "last 1m",
                "last 5m",
                "last 15m",
                "last 1h",
                "since restart",
                "since start",
                "all time"
            ]
        );
        let custom = TimeRange::Custom {
            from_s: 1,
            to_s: None,
        };
        assert_eq!(custom.next(), TimeRange::LastMinutes(1));
    }

    #[test]
    fn parses_single_and_range_clock_times() {
        assert_eq!(
            parse_custom("14:05"),
            Ok(TimeRange::Custom {
                from_s: 14 * 3600 + 300,
                to_s: None
            })
        );
        // An `HH:MM` end covers its whole minute: `14:20` ends at 14:20:59. Incident: CI ran the logview
        // `00:00-23:59` test at 23:59:53 UTC and the lines of that last minute were filtered out.
        let want = Ok(TimeRange::Custom {
            from_s: 14 * 3600 + 300,
            to_s: Some(14 * 3600 + 1200 + 59),
        });
        for s in [
            "14:05–14:20",
            "14:05-14:20",
            "14:05 — 14:20",
            "14:05..14:20",
            " 14:05 - 14:20 ",
        ] {
            assert_eq!(parse_custom(s), want, "{s}");
        }
        assert_eq!(
            parse_custom("9:5-9:30:15"),
            Ok(TimeRange::Custom {
                from_s: 9 * 3600 + 300,
                to_s: Some(9 * 3600 + 30 * 60 + 15)
            })
        );
        assert_eq!(
            parse_custom("00:00-23:59"),
            Ok(TimeRange::Custom {
                from_s: 0,
                to_s: Some(86_399)
            }),
            "the whole day, last minute included"
        );
        // Trailing separator = open end.
        assert_eq!(
            parse_custom("14:05-"),
            Ok(TimeRange::Custom {
                from_s: 14 * 3600 + 300,
                to_s: None
            })
        );
    }

    #[test]
    fn rejects_invalid_custom_input() {
        for s in [
            "",
            "   ",
            "abc",
            "25:00",
            "12:60",
            "12",
            "12:00:60",
            "1:2:3:4",
            "-14:20",
            "14:20-14:05",
            "12:xx-13:00",
        ] {
            assert!(parse_custom(s).is_err(), "{s:?} should be rejected");
        }
    }

    #[test]
    fn label_formats_custom_ranges() {
        let r = parse_custom("14:05-14:20").unwrap();
        assert_eq!(r.label(), "14:05–14:20");
        assert_eq!(parse_custom("08:00").unwrap().label(), "from 08:00");
    }

    #[test]
    fn relative_window_cutoff_slides_with_the_clock() {
        let w = Window {
            last_ms: Some(60_000),
            ..Window::default()
        };
        assert_eq!(w.cutoff(100_000), 40_000);
        assert_eq!(w.cutoff(30_000), 0, "never underflows");
        assert!(
            w.allows(0, 40_000, w.cutoff(100_000)),
            "boundary is inclusive"
        );
        assert!(!w.allows(0, 39_999, w.cutoff(100_000)));
        assert!(
            w.allows(0, 39_999, w.cutoff(99_999)),
            "later clock, later cutoff"
        );
    }

    #[test]
    fn absolute_bounds_and_seq_floor_compose() {
        let w = Window {
            min_seq: 10,
            from_ms: Some(1000),
            to_ms: Some(2000),
            last_ms: None,
        };
        assert!(w.allows(10, 1000, 0));
        assert!(w.allows(11, 2000, 0));
        assert!(!w.allows(9, 1500, 0), "before the restart marker");
        assert!(!w.allows(10, 999, 0));
        assert!(!w.allows(10, 2001, 0));
    }

    #[test]
    fn custom_resolves_to_today_and_covers_the_whole_end_second() {
        let now = Local
            .with_ymd_and_hms(2026, 3, 4, 15, 0, 0)
            .earliest()
            .unwrap()
            .timestamp_millis() as u64;
        let buf = LogBuffer::new(10);
        let r = parse_custom("14:05-14:20").unwrap();
        let w = resolve(r, &buf, None, now).unwrap();
        let at = |h, m, s| {
            Local
                .with_ymd_and_hms(2026, 3, 4, h, m, s)
                .earliest()
                .unwrap()
                .timestamp_millis() as u64
        };
        assert_eq!(w.from_ms, Some(at(14, 5, 0)));
        assert_eq!(
            w.to_ms,
            Some(at(14, 20, 59) + 999),
            "an HH:MM end covers its whole minute"
        );
        assert_eq!(seconds_of_day(now), 15 * 3600);
    }

    #[test]
    fn since_restart_uses_the_service_marker_and_defaults_to_everything() {
        use crate::runtime::logs::LogStream;
        let mut b = LogBuffer::new(100);
        b.push("a".into(), "boot".into(), LogStream::Stdout); // 0
        b.push("b".into(), "boot".into(), LogStream::Stdout); // 1
        let w = resolve(TimeRange::SinceRestart, &b, Some("a"), 0).unwrap();
        assert_eq!(w.min_seq, 0, "no restart yet: whole buffer");
        b.push_restart("a".into()); // 2
        b.push("a".into(), "after".into(), LogStream::Stdout); // 3
        b.push_restart("b".into()); // 4
        let a = resolve(TimeRange::SinceRestart, &b, Some("a"), 0).unwrap();
        let bb = resolve(TimeRange::SinceRestart, &b, Some("b"), 0).unwrap();
        let all = resolve(TimeRange::SinceRestart, &b, None, 0).unwrap();
        assert_eq!((a.min_seq, bb.min_seq, all.min_seq), (2, 4, 4));
        let start = resolve(TimeRange::SinceStackStart, &b, None, 0).unwrap();
        assert_eq!(start.min_seq, 0);
    }
}
