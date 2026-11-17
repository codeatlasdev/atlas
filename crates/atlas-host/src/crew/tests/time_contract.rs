//! Contract of the schedule semantics, pinned before the chrono → jiff / croner migration (#540): which cron
//! expressions and time zone names are accepted, and the instant each one fires next, DST included. A crewmate's
//! automations were written against these answers; the migration must not move one of them.
//!
//! Instants are written as RFC 3339 UTC text and converted by the small integer functions below, so these tests
//! name no date-time crate.

use crate::crew::schedule::{validate_cron, validate_tz};
use crate::crew::store::TriggerKind;
use crate::crew::triggers::{compute_next_fire, should_fire_cron};

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `2026-03-29T01:30:00Z` → unix seconds.
fn at(s: &str) -> u64 {
    let n = |r: std::ops::Range<usize>| s[r].parse::<i64>().unwrap();
    assert!(s.len() == 20 && s.ends_with('Z'), "{s}");
    let days = days_from_civil(n(0..4), n(5..7), n(8..10));
    u64::try_from(days * 86_400 + n(11..13) * 3600 + n(14..16) * 60 + n(17..19)).unwrap()
}

/// Unix seconds → `2026-03-29T01:30:00Z`.
fn iso(secs: u64) -> String {
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}

fn next(cron: &str, tz: Option<&str>, since: &str) -> Option<String> {
    let kind = TriggerKind::Schedule { cron: cron.into() };
    compute_next_fire(&kind, Some(at(since)), tz, 0).map(iso)
}

#[test]
fn cron_expressions_are_accepted_and_rejected_as_before() {
    let accepted = [
        "* * * * *",
        "*/5 * * * *",
        "0 9 * * MON-FRI",
        "0 0 1 JAN *",
        "0 0 * * 7",
        "0 0 * * 0",
        "0 0 L * *",
        "0 0 * * 5L",
        "0 0 * * 5#2",
        "0 0 15W * *",
        "0 0 31 2 *",
        // The `start/step` spelling of older builds.
        "0/15 * * * *",
        "5/5 * * * *",
        "@yearly",
        "@annually",
        "@monthly",
        "@weekly",
        "@daily",
        "@hourly",
        "  0   9 * * *  ",
        "0 9 * * *\n",
    ];
    let rejected = [
        "",
        "not a cron",
        "* * * *",
        "60 * * * *",
        "* 24 * * *",
        "* * 0 * *",
        "* * 32 * *",
        "* * * 13 *",
        "* * * * 8",
        "@midnight",
        "@reboot",
        "@every 5m",
        "0 0 * * FUNDAY",
        // No seconds field.
        "*/10 * * * * *",
        "* * * * * * *",
    ];
    let mut wrong = Vec::new();
    for e in accepted {
        if validate_cron(e).is_err() {
            wrong.push(format!("must be accepted: {e:?}"));
        }
    }
    for e in rejected {
        if validate_cron(e).is_ok() {
            wrong.push(format!("must be rejected: {e:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn the_error_of_a_bad_expression_names_it() {
    let e = validate_cron("not a cron").unwrap_err();
    assert!(e.starts_with("invalid cron 'not a cron': "), "{e}");
}

#[test]
fn time_zone_names_are_accepted_and_rejected_as_before() {
    let accepted = [
        "UTC",
        "Etc/UTC",
        "GMT",
        "Zulu",
        "Europe/Berlin",
        "America/Sao_Paulo",
        "America/Argentina/Buenos_Aires",
        "Asia/Kolkata",
        "Asia/Kathmandu",
        "Australia/Lord_Howe",
        "Pacific/Chatham",
        "US/Pacific",
        "EST5EDT",
        "CET",
        "Etc/GMT+3",
        "Brazil/East",
        "europe/berlin",
        "utc",
    ];
    let rejected = [
        "",
        " ",
        "Mars/Olympus",
        "Europe/Atlantis",
        "+03:00",
        "UTC+3",
        "BRT",
        "America/Sao Paulo",
        "../etc/passwd",
        "Europe/",
        "/Europe/Berlin",
        // Names are not trimmed (chrono-tz also refused other letter cases; jiff reads them).
        " UTC",
    ];
    let mut wrong = Vec::new();
    for z in accepted {
        if validate_tz(z).is_err() {
            wrong.push(format!("must be accepted: {z:?}"));
        }
    }
    for z in rejected {
        if validate_tz(z).is_ok() {
            wrong.push(format!("must be rejected: {z:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    assert_eq!(
        validate_tz("Mars/Olympus").unwrap_err(),
        "unknown timezone: Mars/Olympus"
    );
}

#[test]
fn the_next_fire_in_utc_and_in_a_zone() {
    let table: &[(&str, Option<&str>, &str, Option<&str>)] = &[
        (
            "* * * * *",
            None,
            "2026-11-17T10:00:30Z",
            Some("2026-11-17T10:01:00Z"),
        ),
        (
            "* * * * *",
            None,
            "2026-11-17T10:00:00Z",
            Some("2026-11-17T10:01:00Z"),
        ),
        (
            "*/15 * * * *",
            None,
            "2026-11-17T10:07:00Z",
            Some("2026-11-17T10:15:00Z"),
        ),
        (
            "0 * * * *",
            None,
            "2026-11-17T12:00:00Z",
            Some("2026-11-17T13:00:00Z"),
        ),
        (
            "@daily",
            None,
            "2026-11-17T10:00:00Z",
            Some("2026-11-18T00:00:00Z"),
        ),
        (
            "@daily",
            Some("America/Sao_Paulo"),
            "2026-11-17T10:00:00Z",
            Some("2026-11-18T03:00:00Z"),
        ),
        (
            "@daily",
            Some("Asia/Kolkata"),
            "2026-11-17T10:00:00Z",
            Some("2026-11-17T18:30:00Z"),
        ),
        (
            "@hourly",
            Some("Asia/Kathmandu"),
            "2026-11-17T10:00:00Z",
            Some("2026-11-17T10:15:00Z"),
        ),
        (
            "@weekly",
            None,
            "2026-11-17T10:00:00Z",
            Some("2026-11-22T00:00:00Z"),
        ),
        (
            "@monthly",
            Some("Europe/Berlin"),
            "2026-11-17T10:00:00Z",
            Some("2026-11-30T23:00:00Z"),
        ),
        (
            "@yearly",
            Some("Europe/Berlin"),
            "2026-11-17T10:00:00Z",
            Some("2026-12-31T23:00:00Z"),
        ),
        (
            "0 9 * * MON-FRI",
            Some("America/Sao_Paulo"),
            "2026-11-20T13:00:00Z",
            Some("2026-11-23T12:00:00Z"),
        ),
        (
            "0 9 * * MON-FRI",
            Some("America/Sao_Paulo"),
            "2026-11-20T12:00:00Z",
            Some("2026-11-23T12:00:00Z"),
        ),
        (
            "0 9 * * MON-FRI",
            Some("Europe/Berlin"),
            "2026-11-20T08:00:00Z",
            Some("2026-11-23T08:00:00Z"),
        ),
        (
            "30 8 * * *",
            Some("Australia/Lord_Howe"),
            "2026-11-17T10:00:00Z",
            Some("2026-11-17T21:30:00Z"),
        ),
        (
            "0 0 * * 7",
            None,
            "2026-11-17T10:00:00Z",
            Some("2026-11-22T00:00:00Z"),
        ),
        (
            "0 0 * * 0",
            None,
            "2026-11-17T10:00:00Z",
            Some("2026-11-22T00:00:00Z"),
        ),
        // Day of month and day of week together.
        (
            "0 0 13 * FRI",
            None,
            "2026-11-01T00:00:00Z",
            Some("2026-11-06T00:00:00Z"),
        ),
        (
            "0 0 1,15 * MON",
            None,
            "2026-11-01T00:00:00Z",
            Some("2026-11-02T00:00:00Z"),
        ),
        // The `L`, `#` and `W` extensions.
        (
            "0 0 L * *",
            None,
            "2026-11-17T10:00:00Z",
            Some("2026-11-30T00:00:00Z"),
        ),
        (
            "0 0 L * *",
            None,
            "2028-02-10T00:00:00Z",
            Some("2028-02-29T00:00:00Z"),
        ),
        (
            "0 0 * * 5L",
            None,
            "2026-11-01T00:00:00Z",
            Some("2026-11-27T00:00:00Z"),
        ),
        (
            "0 0 * * 5#2",
            None,
            "2026-11-01T00:00:00Z",
            Some("2026-11-13T00:00:00Z"),
        ),
        (
            "0 0 15W * *",
            None,
            "2026-11-01T00:00:00Z",
            Some("2026-11-16T00:00:00Z"),
        ),
        // Leap day, and a date that does not exist.
        (
            "0 0 29 2 *",
            None,
            "2026-03-01T00:00:00Z",
            Some("2028-02-29T00:00:00Z"),
        ),
        (
            "0 0 29 2 *",
            None,
            "2028-02-29T00:00:00Z",
            Some("2032-02-29T00:00:00Z"),
        ),
        ("0 0 31 2 *", None, "2026-03-01T00:00:00Z", None),
        ("0 0 31 4 *", None, "2026-03-01T00:00:00Z", None),
        // `since` is exclusive: standing on a fire time moves to the next one.
        (
            "0 * * * *",
            None,
            "2026-11-17T12:00:00Z",
            Some("2026-11-17T13:00:00Z"),
        ),
        // Year ends and far dates.
        (
            "0 0 1 1 *",
            None,
            "2026-12-31T23:59:59Z",
            Some("2027-01-01T00:00:00Z"),
        ),
        (
            "0 0 1 1 *",
            Some("America/Sao_Paulo"),
            "2027-01-01T02:59:59Z",
            Some("2027-01-01T03:00:00Z"),
        ),
        (
            "0 0 * * *",
            None,
            "2099-12-31T23:59:59Z",
            Some("2100-01-01T00:00:00Z"),
        ),
    ];
    let mut wrong = Vec::new();
    for (cron, tz, since, want) in table {
        let got = next(cron, *tz, since);
        if got.as_deref() != *want {
            wrong.push(format!(
                "{cron:?} {tz:?} since {since}: want {want:?}, got {got:?}"
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn dst_spring_forward_and_fall_back() {
    let table: &[(&str, &str, &str, Option<&str>)] = &[
        // Berlin: 2026-03-29 02:00 CET jumps to 03:00 CEST (01:00Z); 02:30 does not exist that day.
        (
            "30 2 * * *",
            "Europe/Berlin",
            "2026-03-28T12:00:00Z",
            Some("2026-03-29T01:00:00Z"),
        ),
        (
            "30 2 * * *",
            "Europe/Berlin",
            "2026-03-29T00:00:00Z",
            Some("2026-03-29T01:00:00Z"),
        ),
        (
            "0 3 * * *",
            "Europe/Berlin",
            "2026-03-28T12:00:00Z",
            Some("2026-03-29T01:00:00Z"),
        ),
        (
            "30 1 * * *",
            "Europe/Berlin",
            "2026-03-28T12:00:00Z",
            Some("2026-03-29T00:30:00Z"),
        ),
        (
            "0 2 * * *",
            "Europe/Berlin",
            "2026-03-28T12:00:00Z",
            Some("2026-03-29T01:00:00Z"),
        ),
        (
            "0 * * * *",
            "Europe/Berlin",
            "2026-03-29T00:30:00Z",
            Some("2026-03-29T01:00:00Z"),
        ),
        (
            "*/30 * * * *",
            "Europe/Berlin",
            "2026-03-29T00:20:00Z",
            Some("2026-03-29T00:30:00Z"),
        ),
        // Berlin: 2026-10-25 03:00 CEST falls back to 02:00 CET (01:00Z); 02:30 happens twice.
        (
            "30 2 * * *",
            "Europe/Berlin",
            "2026-10-24T12:00:00Z",
            Some("2026-10-25T00:30:00Z"),
        ),
        (
            "30 2 * * *",
            "Europe/Berlin",
            "2026-10-25T00:30:00Z",
            Some("2026-10-26T01:30:00Z"),
        ),
        (
            "30 2 * * *",
            "Europe/Berlin",
            "2026-10-25T01:30:00Z",
            Some("2026-10-26T01:30:00Z"),
        ),
        (
            "0 * * * *",
            "Europe/Berlin",
            "2026-10-25T00:30:00Z",
            // The repeated hour: croner 2.2 answered 02:00Z (skipping 02:00 CET); an hourly job now runs every
            // real hour, 00:00Z, 01:00Z, 02:00Z.
            Some("2026-10-25T01:00:00Z"),
        ),
        (
            "0 * * * *",
            "Europe/Berlin",
            "2026-10-25T01:30:00Z",
            Some("2026-10-25T02:00:00Z"),
        ),
        (
            "*/30 * * * *",
            "Europe/Berlin",
            "2026-10-25T00:20:00Z",
            Some("2026-10-25T00:30:00Z"),
        ),
        (
            "0 3 * * *",
            "Europe/Berlin",
            "2026-10-24T12:00:00Z",
            Some("2026-10-25T02:00:00Z"),
        ),
        // New York: 2026-03-08 02:00 EST jumps to 03:00 EDT (07:00Z).
        (
            "30 2 * * *",
            "America/New_York",
            "2026-03-07T12:00:00Z",
            Some("2026-03-08T07:00:00Z"),
        ),
        (
            "0 0 * * *",
            "America/New_York",
            "2026-03-07T12:00:00Z",
            Some("2026-03-08T05:00:00Z"),
        ),
        // New York: 2026-11-01 02:00 EDT falls back to 01:00 EST (06:00Z); 01:30 happens twice.
        (
            "30 1 * * *",
            "America/New_York",
            "2026-10-31T12:00:00Z",
            Some("2026-11-01T05:30:00Z"),
        ),
        (
            "30 1 * * *",
            "America/New_York",
            "2026-11-01T05:30:00Z",
            Some("2026-11-02T06:30:00Z"),
        ),
        // Lord Howe changes by 30 minutes: 2026-10-04 02:00 +10:30 jumps to 02:30 +11.
        (
            "15 2 * * *",
            "Australia/Lord_Howe",
            "2026-10-03T12:00:00Z",
            Some("2026-10-03T15:30:00Z"),
        ),
        (
            "45 2 * * *",
            "Australia/Lord_Howe",
            "2026-10-03T12:00:00Z",
            Some("2026-10-03T15:45:00Z"),
        ),
        // Sao Paulo has had no DST since 2019: a fixed -03:00.
        (
            "0 0 * * *",
            "America/Sao_Paulo",
            "2026-10-31T12:00:00Z",
            Some("2026-11-01T03:00:00Z"),
        ),
        // A day without midnight (Sao Paulo 2018-11-04 00:00 -> 01:00).
        (
            "0 0 * * *",
            "America/Sao_Paulo",
            "2018-11-03T12:00:00Z",
            Some("2018-11-04T03:00:00Z"),
        ),
        (
            "30 0 * * *",
            "America/Sao_Paulo",
            "2018-11-03T12:00:00Z",
            Some("2018-11-04T03:00:00Z"),
        ),
    ];
    let mut wrong = Vec::new();
    for (cron, tz, since, want) in table {
        let got = next(cron, Some(tz), since);
        if got.as_deref() != *want {
            wrong.push(format!(
                "{cron:?} {tz} since {since}: want {want:?}, got {got:?}"
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn an_unknown_zone_or_a_bad_cron_changes_nothing_that_fires() {
    // An unknown zone falls back to UTC (hostd validates it on write; a file edited by hand must not crash).
    assert_eq!(
        next("0 9 * * *", Some("Mars/Olympus"), "2026-11-17T10:00:00Z"),
        next("0 9 * * *", None, "2026-11-17T10:00:00Z")
    );
    assert_eq!(next("not a cron", None, "2026-11-17T10:00:00Z"), None);
    assert!(!should_fire_cron("not a cron", None, None, 4_000_000_000));
    // An interval has no cron: the next fire is `last + every`.
    let k = TriggerKind::Interval { every_secs: 120 };
    assert_eq!(compute_next_fire(&k, Some(1000), None, 5000), Some(1120));
    assert_eq!(compute_next_fire(&k, None, None, 5000), Some(5120));
    // No `since` at all is "now" for the next fire and the epoch for the due check.
    let s = TriggerKind::Schedule {
        cron: "0 0 * * *".into(),
    };
    assert_eq!(
        compute_next_fire(&s, None, None, at("2026-11-17T10:00:00Z")).map(iso),
        Some("2026-11-18T00:00:00Z".to_string())
    );
    // An absurd `since` (beyond what a calendar holds) still answers, never panics.
    let _ = compute_next_fire(&s, Some(u64::MAX), None, 0);
    let _ = should_fire_cron("0 0 * * *", Some(u64::MAX), Some("Europe/Berlin"), 0);
}

#[test]
fn due_means_the_next_fire_is_at_or_before_now() {
    let last = at("2026-11-17T10:00:00Z");
    // "0 * * * *": the next fire is 11:00:00Z.
    let due = |now: &str| should_fire_cron("0 * * * *", Some(last), None, at(now));
    assert!(!due("2026-11-17T10:59:59Z"));
    assert!(due("2026-11-17T11:00:00Z"), "at the second it is due");
    assert!(due("2026-11-17T13:00:00Z"));
    // In a zone: 09:00 in Sao Paulo is 12:00Z.
    let due_sp = |now: &str| {
        should_fire_cron(
            "0 9 * * *",
            Some(at("2026-11-17T12:00:00Z")),
            Some("America/Sao_Paulo"),
            at(now),
        )
    };
    assert!(!due_sp("2026-11-18T11:59:59Z"));
    assert!(due_sp("2026-11-18T12:00:00Z"));
}

// croner 2.2 (chrono) answered these wrongly around a clock change: a time before `since` (a trigger created in
// that hour fired on every 30 s tick until it passed), no answer at all, the repeated hour skipped for a job that
// runs every hour or minute, `1W` crossing into the previous month. croner 4 (jiff) answers as below; the rows
// that matter are checked against what a person reading the pattern would say.
#[test]
fn the_clock_changes_croner_2_got_wrong_are_answered_right() {
    let table: &[(&str, Option<&str>, &str, &str)] = &[
        // Before `since`, once an hour a year: 02:30 happened at 00:30Z and again at 01:30Z, the job ran at the first.
        (
            "30 2 * * *",
            Some("Europe/Berlin"),
            "2026-10-25T01:00:00Z",
            "2026-10-26T01:30:00Z",
        ),
        (
            "30 1 * * *",
            Some("Europe/London"),
            "2026-10-25T01:00:00Z",
            "2026-10-26T01:30:00Z",
        ),
        (
            "59 1 * * *",
            Some("America/New_York"),
            "2026-11-01T06:29:59Z",
            "2026-11-02T06:59:00Z",
        ),
        // No answer, in a zone that is in the gap's hour.
        (
            "0 * * * *",
            Some("Antarctica/Troll"),
            "2026-03-29T00:30:00Z",
            "2026-03-29T01:00:00Z",
        ),
        // An every-minute job runs through the repeated hour.
        (
            "* * * * *",
            Some("Europe/Berlin"),
            "2026-10-25T00:59:58Z",
            "2026-10-25T01:00:00Z",
        ),
        (
            "*/30 * * * *",
            Some("America/New_York"),
            "2027-11-07T05:58:59Z",
            "2027-11-07T06:00:00Z",
        ),
        // `:30` in a gap hour does not exist; the next real :30 is an hour later.
        (
            "30 * * * *",
            Some("Europe/Berlin"),
            "2026-03-29T00:30:00Z",
            "2026-03-29T01:30:00Z",
        ),
        // A list or a step that lands in a gap skips that slot (single hours and ranges still run at the gap's end).
        (
            "0 */2 * * *",
            Some("America/New_York"),
            "2027-03-14T06:59:01Z",
            "2027-03-14T08:00:00Z",
        ),
        (
            "0 0,12 * * *",
            Some("America/Santiago"),
            "2026-09-06T01:59:59Z",
            "2026-09-06T15:00:00Z",
        ),
        (
            "0 0 * * *",
            Some("America/Santiago"),
            "2026-09-06T01:59:59Z",
            "2026-09-06T04:00:00Z",
        ),
        // `1W` is the weekday nearest the 1st inside the month: Saturday Dec 1st 2018 is Monday the 3rd.
        (
            "0 0 1W * *",
            None,
            "2018-11-04T02:59:59Z",
            "2018-12-03T00:00:00Z",
        ),
    ];
    let mut wrong = Vec::new();
    for (cron, tz, since, want) in table {
        let got = next(cron, *tz, since);
        if got.as_deref() != Some(*want) {
            wrong.push(format!(
                "{cron:?} {tz:?} since {since}: want {want}, got {got:?}"
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

// The invariant behind the table above: the next fire is never at or before `since`, so a job that just ran
// cannot be due again on the next tick (the same table, every zone and cron of the clock-change tests).
#[test]
fn the_next_fire_is_always_after_since() {
    let zones = [
        "Europe/Berlin",
        "Europe/London",
        "America/New_York",
        "Australia/Lord_Howe",
        "America/Santiago",
        "Antarctica/Troll",
    ];
    let crons = [
        "* * * * *",
        "*/7 * * * *",
        "0 * * * *",
        "30 * * * *",
        "30 2 * * *",
        "30 1 * * *",
        "0 0 * * *",
        "0 */2 * * *",
        "@daily",
        "@hourly",
    ];
    let anchors = [
        "2026-03-29T00:30:00Z",
        "2026-10-25T00:30:00Z",
        "2026-03-08T06:30:00Z",
        "2026-11-01T05:30:00Z",
        "2026-10-03T15:00:00Z",
        "2026-09-06T03:30:00Z",
    ];
    let mut wrong = Vec::new();
    for z in zones {
        for c in crons {
            for a in anchors {
                for off in [0u64, 1, 59, 1799, 1800, 3599, 3600, 5400, 7199, 86_399] {
                    let since = at(a) + off;
                    let kind = TriggerKind::Schedule { cron: c.into() };
                    if let Some(n) = compute_next_fire(&kind, Some(since), Some(z), 0)
                        && n <= since
                    {
                        wrong.push(format!("{c:?} {z} since {}: next {}", iso(since), iso(n)));
                    }
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

// croner 2.2 read `?` as `*` in every field; croner 4 allows it only in the day fields. Automations saved with
// a `?` in the minute, hour or month field keep their schedule: the expected seconds below are what croner 2.2
// answered for each of them (since 1_790_000_000), and they equal the `*` spelling's.
#[test]
fn a_question_mark_in_the_minute_hour_or_month_is_the_star_croner_2_read() {
    let since = 1_790_000_000;
    let next = |cron: &str, tz: Option<&str>| {
        compute_next_fire(
            &TriggerKind::Schedule { cron: cron.into() },
            Some(since),
            tz,
            0,
        )
    };
    for (cron, want) in [
        ("? * * * *", 1_790_000_040),
        ("? ?/2 * * *", 1_790_000_040),
        ("?/5 * * * *", 1_790_000_100),
        ("0 ? * * *", 1_790_002_800),
        ("0 0 * ? *", 1_790_035_200),
        ("0 0 * ?/3 *", 1_790_812_800),
        ("? ? ? ? ?", 1_790_000_040),
        ("0 0 ? * ?", 1_790_035_200),
    ] {
        assert!(validate_cron(cron).is_ok(), "{cron}");
        assert_eq!(next(cron, None), Some(want), "{cron}");
    }
    assert_eq!(
        next("0 0 * * ?", Some("America/Sao_Paulo")),
        Some(1_790_046_000)
    );
    // The spellings that never parsed in either stay errors, and so does a sign croner 2 read as a digit.
    for bad in [
        "?-5 * * * *",
        "?,5 * * * *",
        "5-? * * * *",
        "?? * * * *",
        "+5 * * * *",
    ] {
        assert!(validate_cron(bad).is_err(), "{bad}");
    }
}
