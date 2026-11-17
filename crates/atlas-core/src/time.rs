//! Wall-clock text for files, the wire and the screen. One place knows the date-time crate; callers pass a
//! `SystemTime` and get the exact text older builds wrote, so a file written by one build reads in the next.

use std::time::SystemTime;

use chrono::{DateTime, Datelike, Local, NaiveTime, Offset, TimeZone, Timelike, Utc};

/// A calendar reading of an instant: the date, the clock, and the offset from UTC in force.
struct Civil {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    nanos: u32,
    offset_secs: i32,
}

impl Civil {
    fn of<Tz: TimeZone>(dt: DateTime<Tz>) -> Self {
        Self {
            year: dt.year(),
            month: dt.month(),
            day: dt.day(),
            hour: dt.hour(),
            minute: dt.minute(),
            second: dt.second(),
            nanos: dt.nanosecond() % 1_000_000_000,
            offset_secs: dt.offset().fix().local_minus_utc(),
        }
    }

    fn utc(at: SystemTime) -> Self {
        Self::of(DateTime::<Utc>::from(at))
    }

    fn local(at: SystemTime) -> Self {
        Self::of(DateTime::<Local>::from(at))
    }

    fn date(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    fn hms(&self) -> String {
        format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }

    /// `+03:00`, `-03:00`, `+00:00`.
    fn offset(&self) -> String {
        let (sign, abs) = if self.offset_secs < 0 {
            ('-', -self.offset_secs)
        } else {
            ('+', self.offset_secs)
        };
        format!("{sign}{:02}:{:02}", abs / 3600, abs / 60 % 60)
    }

    /// The fraction as RFC 3339 writes it here: none for a whole second, else 3, 6 or 9 digits, the fewest that
    /// hold the value (`.120`, `.123400`, `.123456780`).
    fn auto_fraction(&self) -> String {
        match self.nanos {
            0 => String::new(),
            n if n % 1_000_000 == 0 => format!(".{:03}", n / 1_000_000),
            n if n % 1000 == 0 => format!(".{:06}", n / 1000),
            n => format!(".{n:09}"),
        }
    }
}

/// `2026-11-17T10:00:05.123456+00:00`: UTC, an explicit `+00:00` (never `Z`), 0, 3, 6 or 9 fraction digits.
pub fn utc_rfc3339(at: SystemTime) -> String {
    let c = Civil::utc(at);
    format!(
        "{}T{}{}{}",
        c.date(),
        c.hms(),
        c.auto_fraction(),
        c.offset()
    )
}

/// [`utc_rfc3339`] of now.
pub fn utc_rfc3339_now() -> String {
    utc_rfc3339(SystemTime::now())
}

/// `2026-11-17T10:00:05.123-03:00`: the machine's local time, milliseconds, a numeric offset.
pub fn local_rfc3339_millis(at: SystemTime) -> String {
    let c = Civil::local(at);
    format!(
        "{}T{}.{:03}{}",
        c.date(),
        c.hms(),
        c.nanos / 1_000_000,
        c.offset()
    )
}

/// `20261117100005`: UTC, for ids that sort by time.
pub fn utc_compact(at: SystemTime) -> String {
    let c = Civil::utc(at);
    format!(
        "{:04}{:02}{:02}{:02}{:02}{:02}",
        c.year, c.month, c.day, c.hour, c.minute, c.second
    )
}

/// [`utc_compact`] of now.
pub fn utc_compact_now() -> String {
    utc_compact(SystemTime::now())
}

/// `10:00:05` on the machine's clock.
pub fn local_hms(at: SystemTime) -> String {
    Civil::local(at).hms()
}

/// `10:00:05.123` on the machine's clock.
pub fn local_hms_millis(at: SystemTime) -> String {
    let c = Civil::local(at);
    format!("{}.{:03}", c.hms(), c.nanos / 1_000_000)
}

/// `2026-11-17 10:00` on the machine's clock.
pub fn local_date_hm(at: SystemTime) -> String {
    let c = Civil::local(at);
    format!("{} {:02}:{:02}", c.date(), c.hour, c.minute)
}

/// `2026-11-17 10:00:05` on the machine's clock.
pub fn local_date_hms(at: SystemTime) -> String {
    let c = Civil::local(at);
    format!("{} {}", c.date(), c.hms())
}

/// `(year, month, day, hour, minute, second)` on the machine's clock.
pub fn local_parts(at: SystemTime) -> (i32, u32, u32, u32, u32, u32) {
    let c = Civil::local(at);
    (c.year, c.month, c.day, c.hour, c.minute, c.second)
}

/// Seconds since local midnight (wall clock: a DST day has 23 or 25 hours) at epoch-millis `ms`.
pub fn local_secs_of_day(ms: u64) -> u32 {
    Local
        .timestamp_millis_opt(ms as i64)
        .single()
        .map_or(0, |d| d.num_seconds_from_midnight())
}

/// Epoch millis of `secs` after local midnight of the day holding `now_ms`. `None` for a clock time that does
/// not exist that day (inside a DST gap; the gap's first second is the change itself and resolves) or past
/// the end of the day; the first of two when it happens twice.
pub fn local_day_second_ms(now_ms: u64, secs: u32) -> Option<u64> {
    let day = Local
        .timestamp_millis_opt(now_ms as i64)
        .single()?
        .date_naive();
    let t = NaiveTime::from_num_seconds_from_midnight_opt(secs, 0)?;
    let at = Local.from_local_datetime(&day.and_time(t)).earliest()?;
    u64::try_from(at.timestamp_millis()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(secs: u64, nanos: u32) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::new(secs, nanos)
    }

    // Contract (chrono → jiff migration, #540): these strings reach disk (project and worktree registries'
    // `added`, deploy ids) and are read back by the next build; the bytes must not change.
    #[test]
    fn utc_rfc3339_keeps_plus_zero_and_the_shortest_of_0_3_6_9_fraction_digits() {
        let s = 1_794_909_605; // 2026-11-17T10:00:05Z
        for (nanos, want) in [
            (0, "2026-11-17T10:00:05+00:00"),
            (120_000_000, "2026-11-17T10:00:05.120+00:00"),
            (1_000_000, "2026-11-17T10:00:05.001+00:00"),
            (123_400_000, "2026-11-17T10:00:05.123400+00:00"),
            (123_456_000, "2026-11-17T10:00:05.123456+00:00"),
            (1_000, "2026-11-17T10:00:05.000001+00:00"),
            (123_456_780, "2026-11-17T10:00:05.123456780+00:00"),
            (1, "2026-11-17T10:00:05.000000001+00:00"),
            (999_999_999, "2026-11-17T10:00:05.999999999+00:00"),
        ] {
            assert_eq!(utc_rfc3339(at(s, nanos)), want, "{nanos} ns");
        }
        assert_eq!(utc_rfc3339(at(0, 0)), "1970-01-01T00:00:00+00:00");
        assert_eq!(
            utc_rfc3339(at(951_782_400 + 86_399, 0)),
            "2000-02-29T23:59:59+00:00"
        );
        assert_eq!(
            utc_rfc3339(at(253_402_300_799, 0)),
            "9999-12-31T23:59:59+00:00"
        );
        // The clock now: this century, `+00:00`.
        let now = utc_rfc3339_now();
        assert!(now.starts_with("20") && now.ends_with("+00:00"), "{now}");
    }

    #[test]
    fn utc_compact_is_fourteen_digits_in_utc() {
        assert_eq!(
            utc_compact(at(1_794_909_605, 999_999_999)),
            "20261117100005"
        );
        assert_eq!(utc_compact(at(0, 0)), "19700101000000");
        assert_eq!(utc_compact(at(951_782_400, 0)), "20000229000000");
        let now = utc_compact_now();
        assert!(
            now.len() == 14 && now.bytes().all(|b| b.is_ascii_digit()),
            "{now}"
        );
    }

    // Local time follows the machine's zone: `TZ` on unix. The zone is read once per process (chrono caches it
    // for a second, jiff for good), so each test owns one zone and the suite runs under nextest, one process per
    // test; a second zone in the same process is refused instead of passing on the first one's clock.
    #[cfg(unix)]
    fn zone(tz: &str) {
        static SET: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let first = SET.get_or_init(|| {
            // SAFETY: first thing the test does, before any local-time read; the process runs this one test.
            unsafe { std::env::set_var("TZ", tz) };
            tz.to_string()
        });
        assert_eq!(first, tz, "one zone per process: run with `cargo nextest`");
    }

    #[cfg(unix)]
    fn local_text_in(tz: &str, rfc: &str, hms: &str, date_hm: &str) {
        zone(tz);
        let t = at(1_794_909_605, 123_456_789); // 2026-11-17T10:00:05.123456789Z
        assert_eq!(local_rfc3339_millis(t), rfc);
        assert_eq!(local_hms(t), hms);
        assert_eq!(local_hms_millis(t), format!("{hms}.123"));
        assert_eq!(local_date_hm(t), date_hm);
        assert_eq!(local_date_hms(t), format!("{} {hms}", &date_hm[..10]));
        let (y, mo, d, h, mi, s) = local_parts(t);
        assert_eq!(
            format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}"),
            local_date_hms(t)
        );
    }

    #[cfg(unix)]
    #[test]
    fn local_text_in_utc() {
        local_text_in(
            "UTC",
            "2026-11-17T10:00:05.123+00:00",
            "10:00:05",
            "2026-11-17 10:00",
        );
    }

    #[cfg(unix)]
    #[test]
    fn local_text_in_sao_paulo_has_a_negative_offset() {
        local_text_in(
            "America/Sao_Paulo",
            "2026-11-17T07:00:05.123-03:00",
            "07:00:05",
            "2026-11-17 07:00",
        );
        assert_eq!(
            local_rfc3339_millis(at(0, 0)),
            "1969-12-31T21:00:00.000-03:00"
        );
    }

    #[cfg(unix)]
    #[test]
    fn local_text_in_kolkata_has_a_half_hour_offset() {
        local_text_in(
            "Asia/Kolkata",
            "2026-11-17T15:30:05.123+05:30",
            "15:30:05",
            "2026-11-17 15:30",
        );
    }

    #[cfg(unix)]
    #[test]
    fn local_text_in_kathmandu_has_a_45_minute_offset() {
        local_text_in(
            "Asia/Kathmandu",
            "2026-11-17T15:45:05.123+05:45",
            "15:45:05",
            "2026-11-17 15:45",
        );
    }

    #[cfg(unix)]
    #[test]
    fn local_text_in_kiritimati_is_a_day_ahead_at_plus_14() {
        local_text_in(
            "Pacific/Kiritimati",
            "2026-11-18T00:00:05.123+14:00",
            "00:00:05",
            "2026-11-18 00:00",
        );
    }

    #[cfg(unix)]
    #[test]
    fn local_text_in_berlin_follows_the_dst_changes() {
        zone("Europe/Berlin");
        // 2026-03-29: 00:30Z is 01:30 CET (+01:00), 01:00Z is 03:00 CEST (+02:00).
        assert_eq!(
            local_rfc3339_millis(at(1_774_744_200, 0)),
            "2026-03-29T01:30:00.000+01:00"
        );
        assert_eq!(
            local_rfc3339_millis(at(1_774_746_000, 0)),
            "2026-03-29T03:00:00.000+02:00"
        );
        // 2026-10-25 falls back: 00:30Z is 02:30 CEST, 01:30Z is 02:30 CET.
        assert_eq!(local_hms(at(1_792_888_200, 0)), "02:30:00");
        assert_eq!(local_hms(at(1_792_888_200 + 3600, 0)), "02:30:00");
        assert_eq!(
            local_rfc3339_millis(at(1_792_888_200, 0)),
            "2026-10-25T02:30:00.000+02:00"
        );
        assert_eq!(
            local_rfc3339_millis(at(1_792_888_200 + 3600, 0)),
            "2026-10-25T02:30:00.000+01:00"
        );
    }

    #[cfg(unix)]
    #[test]
    fn berlin_seconds_of_day_are_wall_clock_and_a_gap_has_no_second() {
        zone("Europe/Berlin");
        let ms = |secs: u64| secs * 1000;
        // 2026-03-29T01:00Z is 03:00 CEST: wall clock, not elapsed time (which would be 02:00).
        assert_eq!(local_secs_of_day(ms(1_774_746_000)), 3 * 3600);
        assert_eq!(local_secs_of_day(ms(1_774_744_200)), 3600 + 1800);
        // Spring forward: the day is the one holding 12:00Z (14:00 CEST).
        let spring = ms(1_774_785_600);
        assert_eq!(local_day_second_ms(spring, 0), Some(ms(1_774_738_800))); // 00:00 CET = 23:00Z before
        assert_eq!(
            local_day_second_ms(spring, 2 * 3600 + 1800),
            None,
            "02:30 does not exist"
        );
        // The first second of a gap is the change itself: it resolves to the instant the clock jumps.
        assert_eq!(
            local_day_second_ms(spring, 2 * 3600),
            Some(ms(1_774_746_000))
        );
        assert_eq!(local_day_second_ms(spring, 2 * 3600 + 1), None);
        assert_eq!(
            local_day_second_ms(spring, 3 * 3600),
            Some(ms(1_774_746_000))
        );
        assert_eq!(local_day_second_ms(spring, 86_399), Some(ms(1_774_821_599)));
        assert_eq!(
            local_day_second_ms(spring, 86_400),
            None,
            "past the end of the day"
        );
        // Fall back: 02:30 happens twice (00:30Z, then 01:30Z). chrono's `earliest()` answers the second.
        let fall = ms(1_792_929_600);
        assert_eq!(
            local_day_second_ms(fall, 2 * 3600 + 1800),
            Some(ms(1_792_888_200 + 3600))
        );
        assert_eq!(
            local_day_second_ms(fall, 3 * 3600),
            Some(ms(1_792_888_200 + 5400))
        );
    }

    #[cfg(unix)]
    #[test]
    fn sao_paulo_midnight_gap_of_2018() {
        zone("America/Sao_Paulo");
        let ms = |secs: u64| secs * 1000;
        // 2018-11-04 00:00 did not exist (clocks went to 01:00).
        let day = ms(1_541_332_800);
        assert_eq!(local_day_second_ms(day, 0), Some(ms(1_541_300_400)));
        assert_eq!(local_day_second_ms(day, 1), None);
        assert_eq!(local_day_second_ms(day, 1800), None);
        assert_eq!(local_day_second_ms(day, 3600), Some(ms(1_541_300_400)));
        assert_eq!(local_secs_of_day(ms(1_541_300_400)), 3600);
    }

    #[cfg(unix)]
    #[test]
    fn utc_seconds_of_day_and_local_midnight() {
        zone("UTC");
        let ms = |secs: u64| secs * 1000;
        assert_eq!(local_secs_of_day(ms(1_794_909_605)), 10 * 3600 + 5);
        assert_eq!(
            local_day_second_ms(ms(1_794_909_605), 0),
            Some(ms(1_794_873_600))
        );
    }
}
