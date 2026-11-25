//! Wall-clock text for files, the wire and the screen. One place knows the date-time crate; callers pass a
//! `SystemTime` and get the exact text older builds wrote, so a file written by one build reads in the next.

use std::time::SystemTime;

use jiff::tz::{AmbiguousOffset, TimeZone};
use jiff::{Timestamp, civil};

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
    fn of(ts: Timestamp, tz: &TimeZone) -> Self {
        let z = ts.to_zoned_datetime(tz.clone());
        let (d, t) = (z.date(), z.time());
        Self {
            year: i32::from(d.year()),
            month: d.month().unsigned_abs().into(),
            day: d.day().unsigned_abs().into(),
            hour: t.hour().unsigned_abs().into(),
            minute: t.minute().unsigned_abs().into(),
            second: t.second().unsigned_abs().into(),
            nanos: t.subsec_nanosecond().unsigned_abs(),
            offset_secs: z.offset().seconds(),
        }
    }

    fn utc(ts: Timestamp) -> Self {
        Self::of(ts, &TimeZone::UTC)
    }

    fn local(at: SystemTime) -> Self {
        Self::of(instant(at), &TimeZone::system())
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

    /// The fraction as RFC 3339 is written here: none for a whole second, else 3, 6 or 9 digits, the fewest that
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

/// The IANA zone called `name`, from the database bundled in the binary (so it never depends on the machine's
/// tzdata); `None` for a name it does not know. The letter case of `name` is not significant.
pub fn zone(name: &str) -> Option<TimeZone> {
    let (canonical, tzif) = jiff_tzdb::get(name)?;
    TimeZone::tzif(canonical, tzif).ok()
}

/// The instant of `at`; the epoch for a time no calendar holds (past year 9999), never a panic.
fn instant(at: SystemTime) -> Timestamp {
    Timestamp::try_from(at).unwrap_or(Timestamp::UNIX_EPOCH)
}

/// `2026-11-17T10:00:05.123456+00:00`: UTC, an explicit `+00:00` (never `Z`), 0, 3, 6 or 9 fraction digits.
pub fn utc_rfc3339(at: SystemTime) -> String {
    let c = Civil::utc(instant(at));
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

/// `2026-11-17T10:00:05.123456Z`: UTC with `Z`, 0, 3, 6 or 9 fraction digits (what a stored `Timestamp` reads
/// as). The text [`utc_z`] writes and [`utc_z_serde`] reads.
pub fn utc_z(ts: Timestamp) -> String {
    let c = Civil::utc(ts);
    format!("{}T{}{}Z", c.date(), c.hms(), c.auto_fraction())
}

/// `serde(with = "atlas_core::time::utc_z_serde")` for a `jiff::Timestamp` field: written as [`utc_z`], read from
/// any RFC 3339 spelling (`Z`, `+00:00`, another offset).
pub mod utc_z_serde {
    use jiff::Timestamp;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub fn serialize<S: Serializer>(ts: &Timestamp, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::utc_z(*ts))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Timestamp, D::Error> {
        let text = String::deserialize(d)?;
        text.parse()
            .map_err(|e| D::Error::custom(format!("{text:?}: {e}")))
    }
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
    let c = Civil::utc(instant(at));
    format!(
        "{:04}{:02}{:02}{:02}{:02}{:02}",
        c.year, c.month, c.day, c.hour, c.minute, c.second
    )
}

/// The instant a [`utc_compact`] text names; `None` when `s` is not a valid 14-digit UTC date-time.
pub fn parse_utc_compact(s: &str) -> Option<SystemTime> {
    if s.len() != 14 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let dt = civil::DateTime::strptime("%Y%m%d%H%M%S", s).ok()?;
    SystemTime::try_from(dt.to_zoned_datetime(TimeZone::UTC).ok()?.timestamp()).ok()
}

/// `2026-11-17`: the UTC day of `at`.
pub fn utc_date(at: SystemTime) -> String {
    Civil::utc(instant(at)).date()
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
    let Ok(ts) = Timestamp::from_millisecond(ms as i64) else {
        return 0;
    };
    let t = ts.to_zoned_datetime(TimeZone::system()).time();
    u32::try_from(i32::from(t.hour()) * 3600 + i32::from(t.minute()) * 60 + i32::from(t.second()))
        .unwrap_or(0)
}

/// Epoch millis of `secs` after local midnight of the day holding `now_ms`. `None` for a clock time that does
/// not exist that day (inside a DST gap) or past the end of the day; the first of two when it happens twice.
pub fn local_day_second_ms(now_ms: u64, secs: u32) -> Option<u64> {
    let tz = TimeZone::system();
    let day = Timestamp::from_millisecond(now_ms as i64)
        .ok()?
        .to_zoned_datetime(tz.clone())
        .date();
    if secs >= 86_400 {
        return None;
    }
    let clock = civil::time(
        (secs / 3600) as i8,
        (secs / 60 % 60) as i8,
        (secs % 60) as i8,
        0,
    );
    let ambiguous = tz.to_ambiguous_zoned_datetime(day.to_datetime(clock));
    if matches!(ambiguous.offset(), AmbiguousOffset::Gap { .. }) {
        return None;
    }
    let at = ambiguous.earlier().ok()?;
    u64::try_from(at.timestamp().as_millisecond()).ok()
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
    // `SystemTime` ticks in 100 ns on Windows, so the nanosecond rows are multiples of 100.
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
            (123_456_700, "2026-11-17T10:00:05.123456700+00:00"),
            (100, "2026-11-17T10:00:05.000000100+00:00"),
            (999_999_900, "2026-11-17T10:00:05.999999900+00:00"),
        ] {
            assert_eq!(utc_rfc3339(at(s, nanos)), want, "{nanos} ns");
        }
        assert_eq!(utc_rfc3339(at(0, 0)), "1970-01-01T00:00:00+00:00");
        assert_eq!(
            utc_rfc3339(at(951_782_400 + 86_399, 0)),
            "2000-02-29T23:59:59+00:00"
        );
        assert_eq!(
            utc_rfc3339(at(253_402_128_000 + 79_199, 0)),
            "9999-12-30T21:59:59+00:00"
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

    #[test]
    fn parse_utc_compact_reads_what_utc_compact_writes() {
        for t in [at(0, 0), at(951_782_400 + 86_399, 0), at(1_794_909_605, 0)] {
            assert_eq!(parse_utc_compact(&utc_compact(t)), Some(t));
        }
        for bad in [
            "",
            "2026",
            "2026111710000",
            "202611171000050",
            "20261317100005",
            "2026111710000x",
            "20260230100005",
        ] {
            assert_eq!(parse_utc_compact(bad), None, "{bad:?}");
        }
    }

    // Local time follows the machine's zone: `TZ` on unix. The zone is read once per process (jiff caches it
    // for five minutes), so each test owns one zone and the suite runs under nextest, one process per
    // test; a second zone in the same process is refused instead of passing on the first one's clock.
    #[cfg(unix)]
    fn set_zone(tz: &str) {
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
        set_zone(tz);
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
        set_zone("Europe/Berlin");
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
        set_zone("Europe/Berlin");
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
        // chrono resolved the first second of a gap (02:00:00) to the instant the clock jumps; every second of
        // the gap is now "does not exist", as the doc of `resolve` always said.
        assert_eq!(local_day_second_ms(spring, 2 * 3600), None);
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
        // Fall back: 02:30 happens twice (00:30Z, then 01:30Z): the first is the answer. chrono's `earliest()`
        // answered the second.
        let fall = ms(1_792_929_600);
        assert_eq!(
            local_day_second_ms(fall, 2 * 3600 + 1800),
            Some(ms(1_792_888_200))
        );
        assert_eq!(
            local_day_second_ms(fall, 3 * 3600),
            Some(ms(1_792_888_200 + 5400))
        );
    }

    #[cfg(unix)]
    #[test]
    fn sao_paulo_midnight_gap_of_2018() {
        set_zone("America/Sao_Paulo");
        let ms = |secs: u64| secs * 1000;
        // 2018-11-04 00:00 did not exist (clocks went to 01:00).
        let day = ms(1_541_332_800);
        assert_eq!(local_day_second_ms(day, 0), None);
        assert_eq!(local_day_second_ms(day, 1), None);
        assert_eq!(local_day_second_ms(day, 1800), None);
        assert_eq!(local_day_second_ms(day, 3600), Some(ms(1_541_300_400)));
        assert_eq!(local_secs_of_day(ms(1_541_300_400)), 3600);
    }

    #[cfg(unix)]
    #[test]
    fn utc_seconds_of_day_and_local_midnight() {
        set_zone("UTC");
        let ms = |secs: u64| secs * 1000;
        assert_eq!(local_secs_of_day(ms(1_794_909_605)), 10 * 3600 + 5);
        assert_eq!(
            local_day_second_ms(ms(1_794_909_605), 0),
            Some(ms(1_794_873_600))
        );
    }

    // `installed_at` of the skills manifest: written with `Z`, read from every RFC 3339 spelling.
    #[test]
    fn utc_z_is_z_with_the_shortest_of_0_3_6_9_fraction_digits() {
        let ts = |s, n| Timestamp::new(s, n).unwrap();
        assert_eq!(utc_z(ts(1_794_909_605, 0)), "2026-11-17T10:00:05Z");
        assert_eq!(
            utc_z(ts(1_794_909_605, 120_000_000)),
            "2026-11-17T10:00:05.120Z"
        );
        assert_eq!(
            utc_z(ts(1_794_909_605, 123_400_000)),
            "2026-11-17T10:00:05.123400Z"
        );
        assert_eq!(
            utc_z(ts(1_794_909_605, 123_456_780)),
            "2026-11-17T10:00:05.123456780Z"
        );
    }

    #[test]
    fn a_timestamp_field_reads_any_offset_and_rejects_text_without_one() {
        #[derive(serde::Serialize, serde::Deserialize)]
        struct T {
            #[serde(with = "super::utc_z_serde")]
            at: Timestamp,
        }
        let read = |s: &str| serde_json::from_str::<T>(&format!(r#"{{"at":"{s}"}}"#));
        let want = "2026-11-17T10:00:05.500Z";
        for spelling in [
            "2026-11-17T10:00:05.5Z",
            "2026-11-17T10:00:05.500+00:00",
            "2026-11-17T07:00:05.5-03:00",
            "2026-11-17T15:30:05.5+05:30",
        ] {
            let t = read(spelling).unwrap_or_else(|e| panic!("{spelling}: {e}"));
            assert_eq!(serde_json::to_value(&t).unwrap()["at"], want, "{spelling}");
        }
        for bad in ["", "now", "2026-11-17", "2026-11-17T10:00:05", "1794909605"] {
            assert!(read(bad).is_err(), "{bad:?}");
        }
    }

    // Incident (CI, #540): Ubuntu 24.04 ships no `US/Pacific`, `EST5EDT`, `Zulu` or `CET` without `tzdata-legacy`; a
    // zone read from the machine failed there while chrono-tz, which bundles its own, knew them all.
    #[test]
    fn a_named_zone_comes_from_the_bundled_database_legacy_names_included() {
        for name in [
            "US/Pacific",
            "EST5EDT",
            "Zulu",
            "CET",
            "Brazil/East",
            "Europe/Berlin",
            "UTC",
            "Etc/GMT+3",
        ] {
            assert!(zone(name).is_some(), "{name}");
        }
        for name in ["", "Mars/Olympus", "../etc/passwd", "Europe/", " UTC"] {
            assert!(zone(name).is_none(), "{name:?}");
        }
        // Not just present: the rules are right (Berlin leaves CET for CEST on 2026-03-29 at 01:00Z).
        let berlin = zone("Europe/Berlin").unwrap();
        let offset = |secs| {
            Timestamp::from_second(secs)
                .unwrap()
                .to_zoned_datetime(berlin.clone())
                .offset()
                .seconds()
        };
        assert_eq!((offset(1_774_744_200), offset(1_774_746_000)), (3600, 7200));
    }
}
