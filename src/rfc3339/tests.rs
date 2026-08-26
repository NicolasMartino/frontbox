//! The oracle and the grammar.
//!
//! `chrono` is a dev-dependency and appears only here. That is the whole arrangement decision 011
//! rests on: the bytes stay chrono's, checked by comparison, while the library ships without it.

use super::*;

/// What `chrono`'s `Serialize` impl produces — the exact path decision 010 pinned.
///
/// Going through `serde_json` rather than `to_rfc3339_opts` is deliberate. The wire format is
/// whatever chrono *serializes* to, so that is what has to be compared; a formatting call that
/// happened to agree today would not catch the serde impl choosing differently.
fn chrono_rendering(millis: i64) -> Option<String> {
    let instant = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(millis)?;
    match serde_json::to_value(instant).expect("chrono serializes an instant") {
        serde_json::Value::String(text) => Some(text),
        other => panic!("chrono did not serialize to a string: {other}"),
    }
}

/// Deterministic sample points. A fixed seed keeps a failure reproducible from the message alone.
fn samples(count: usize) -> impl Iterator<Item = i64> {
    let span = (MAX_MILLIS as i128) - (MIN_MILLIS as i128) + 1;
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    (0..count).map(move |_| {
        // xorshift64*, which is plenty for spreading probe points over a range.
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let draw = i128::from(state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 1);
        MIN_MILLIS + (draw % span) as i64
    })
}

/// The claim decision 011 makes: our bytes are chrono's bytes.
#[test]
fn matches_chrono_across_the_representable_range() {
    for millis in samples(200_000) {
        assert_eq!(
            format(millis).as_deref(),
            chrono_rendering(millis).as_deref(),
            "rendering diverged at {millis}"
        );
    }
}

/// Day boundaries in the years most likely to expose a calendar bug.
///
/// 1900 is a century that is *not* a leap year and 2000 is a century that is, which is the rule
/// `is_leap_year` exists to get right. 0000 and 9999 are the ends of the range, where the era
/// arithmetic in `civil_from_days` is furthest from the epoch it is shifted around.
#[test]
fn matches_chrono_at_every_day_boundary_of_representative_years() {
    for year in [0, 1, 1899, 1900, 1970, 1972, 2000, 2024, 2100, 9999] {
        let start = days_from_civil(year, 1, 1) * MILLIS_PER_DAY;
        let end = days_from_civil(year, 12, 31) * MILLIS_PER_DAY;
        for day in (start..=end).step_by(MILLIS_PER_DAY as usize) {
            for millis in [day, day + MILLIS_PER_DAY - 1] {
                assert_eq!(
                    format(millis).as_deref(),
                    chrono_rendering(millis).as_deref(),
                    "rendering diverged at {millis} (year {year})"
                );
            }
        }
    }
}

/// The fraction is the part a hand-rolled renderer is most likely to get subtly wrong.
///
/// chrono omits it entirely at zero and pads it to three digits otherwise, so `.100` and `.120`
/// keep their trailing zeros while `.000` disappears. An implementation that always printed three
/// digits would pass a round-trip test and still send bytes the server has never seen.
#[test]
fn matches_chrono_for_every_millisecond_within_a_second() {
    for base in [0, 1_700_000_000_000, -1_000, 253_402_300_799_000] {
        for milli in 0..1_000 {
            let millis = base + milli;
            assert_eq!(
                format(millis).as_deref(),
                chrono_rendering(millis).as_deref(),
                "rendering diverged at {millis}"
            );
        }
    }
}

/// The fixtures from the conformance suite and the source protocol, spelled out.
#[test]
fn renders_the_documented_shapes() {
    let cases = [
        (0, "1970-01-01T00:00:00Z"),
        (1, "1970-01-01T00:00:00.001Z"),
        (999, "1970-01-01T00:00:00.999Z"),
        (1_000, "1970-01-01T00:00:01Z"),
        (1_700_000_000_123, "2023-11-14T22:13:20.123Z"),
        (1_700_000_000_000, "2023-11-14T22:13:20Z"),
        (1_700_000_000_100, "2023-11-14T22:13:20.100Z"),
        // Pre-epoch borrows from the day rather than rounding toward zero.
        (-1, "1969-12-31T23:59:59.999Z"),
        (-1_000, "1969-12-31T23:59:59Z"),
        (MIN_MILLIS, "0000-01-01T00:00:00Z"),
        (MAX_MILLIS, "9999-12-31T23:59:59.999Z"),
    ];
    for (millis, expected) in cases {
        assert_eq!(format(millis).as_deref(), Some(expected));
    }
}

/// The range is RFC 3339's four-digit year, not chrono's.
///
/// chrono renders one millisecond past our maximum as `+10000-01-01T00:00:00Z`. That is ISO 8601
/// expanded form: legal there, absent from RFC 3339's grammar, and a payload the server would
/// refuse. Declining to build it is the behaviour case 26 turns into a quarantine.
#[test]
fn the_range_stops_where_rfc_3339_stops() {
    assert!(is_representable(MIN_MILLIS));
    assert!(is_representable(MAX_MILLIS));
    assert!(!is_representable(MIN_MILLIS - 1));
    assert!(!is_representable(MAX_MILLIS + 1));
    assert_eq!(format(MIN_MILLIS - 1), None);
    assert_eq!(format(MAX_MILLIS + 1), None);
    assert_eq!(format(i64::MIN), None);
    assert_eq!(format(i64::MAX), None);

    // What chrono would have produced there, and why we do not.
    assert_eq!(
        chrono_rendering(MAX_MILLIS + 1).as_deref(),
        Some("+10000-01-01T00:00:00Z")
    );
    assert_eq!(
        chrono_rendering(MIN_MILLIS - 1).as_deref(),
        Some("-0001-12-31T23:59:59.999Z")
    );
}

/// Everything `format` emits, `parse` reads back.
#[test]
fn every_rendering_round_trips() {
    for millis in samples(100_000).chain([MIN_MILLIS, MAX_MILLIS, 0, -1, 1]) {
        let text = format(millis).expect("in range");
        assert_eq!(parse(&text), Ok(millis), "round trip failed for {text}");
    }
}

/// The concessions RFC 3339 itself names, and the offsets that make them mean something.
#[test]
fn parses_the_accepted_grammar() {
    let cases = [
        ("2023-11-14T22:13:20.123Z", 1_700_000_000_123),
        ("2023-11-14T22:13:20Z", 1_700_000_000_000),
        // RFC 3339 §5.6 allows lowercase `t` and `z`.
        ("2023-11-14t22:13:20z", 1_700_000_000_000),
        ("2023-11-14T22:13:20+00:00", 1_700_000_000_000),
        ("2023-11-14T22:13:20-00:00", 1_700_000_000_000),
        ("2023-11-14T23:13:20+01:00", 1_700_000_000_000),
        ("2023-11-14T21:13:20-01:00", 1_700_000_000_000),
        // Sub-millisecond precision is accepted and truncated, not refused.
        ("2023-11-14T22:13:20.123456789Z", 1_700_000_000_123),
        ("2023-11-14T22:13:20.1239Z", 1_700_000_000_123),
        // Fewer than three digits are positional, not padded on the left: `.5` is 500ms.
        ("2023-11-14T22:13:20.5Z", 1_700_000_000_500),
        ("2023-11-14T22:13:20.05Z", 1_700_000_000_050),
        // A leap day the leap-century rule has to admit.
        ("2000-02-29T00:00:00Z", 951_782_400_000),
    ];
    for (text, expected) in cases {
        assert_eq!(parse(text), Ok(expected), "failed to parse {text}");
    }
}

/// What is refused, and why each one is worth refusing.
#[test]
fn refuses_what_it_cannot_represent_faithfully() {
    for text in [
        // An offset is mandatory: a local time names no instant.
        "2023-11-14T22:13:20",
        // Permitted by RFC 3339 only "by mutual agreement", which we do not have.
        "2023-11-14 22:13:20Z",
        // chrono accepts this and moves the value onto the following second.
        "2016-12-31T23:59:60Z",
        "2023-11-14T22:13:60Z",
        // Not in the RFC 3339 grammar; chrono emits it past year 9999.
        "+10000-01-01T00:00:00Z",
        // Calendar validity, including the century that is not a leap year.
        "2023-13-01T00:00:00Z",
        "2023-02-30T00:00:00Z",
        "1900-02-29T00:00:00Z",
        "2023-00-10T00:00:00Z",
        "2023-11-00T00:00:00Z",
        // Field ranges.
        "2023-11-14T24:00:00Z",
        "2023-11-14T22:60:00Z",
        "2023-11-14T22:13:20+24:00",
        // Malformed rather than out of range.
        "2023-11-14T22:13:20.Z",
        "2023-11-14T22:13:20+0100",
        "2023-11-14T22:13:20Zextra",
        "2023-11-14X22:13:20Z",
        "20231114T221320Z",
        "not-a-date-at-all",
        "",
    ] {
        assert!(parse(text).is_err(), "should have refused {text:?}");
    }
}

/// An offset can carry an in-range civil time out of range.
#[test]
fn an_offset_is_applied_before_the_range_is_checked() {
    // 9999-12-31T23:59:59Z is the last representable second; the same civil time at -01:00 is an
    // hour later in UTC, which is year 10000.
    assert!(parse("9999-12-31T23:00:00Z").is_ok());
    assert_eq!(
        parse("9999-12-31T23:00:00-02:00"),
        Err("instant outside the representable range")
    );
    assert!(parse("0000-01-01T02:00:00+01:00").is_ok());
    assert_eq!(
        parse("0000-01-01T00:00:00+01:00"),
        Err("instant outside the representable range")
    );
}

/// The two calendar conversions are inverses, which is what the rest of the module assumes.
#[test]
fn the_calendar_conversions_invert_each_other() {
    for millis in samples(50_000) {
        let day = millis.div_euclid(MILLIS_PER_DAY);
        let (year, month, day_of_month) = civil_from_days(day);
        assert_eq!(days_from_civil(year, month, day_of_month), day);
        assert!((1..=12).contains(&month), "month {month} out of range");
        assert!(
            (1..=days_in_month(year, month)).contains(&day_of_month),
            "day {day_of_month} out of range for {year}-{month}"
        );
    }
}

/// The rule that trips people up: divisible by 100 is not enough to skip a leap day.
#[test]
fn leap_years_follow_the_gregorian_rule() {
    for (year, leap) in [
        (1900, false),
        (2000, true),
        (2024, true),
        (2023, false),
        (0, true),
        (2100, false),
    ] {
        assert_eq!(is_leap_year(year), leap, "wrong leap rule for {year}");
        assert_eq!(days_in_month(year, 2), if leap { 29 } else { 28 });
    }
}
