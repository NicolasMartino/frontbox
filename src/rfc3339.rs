//! The wire format's timestamp rendering.
//!
//! Decision 010 pins `client_datetime` to the bytes the source server already accepts. Decision 011
//! keeps those bytes while dropping the date library that used to produce them: the format is one
//! fixed shape, and owning it removes the last crate from this library's public compatibility
//! surface that was there for formatting alone.
//!
//! "Owning it" is only safe because it is checked rather than asserted. `chrono` remains a
//! dev-dependency and [`tests::matches_chrono_across_the_representable_range`] compares this
//! module's output against it. The guarantee decision 010 bought by delegating is therefore still
//! bought — by a test that runs on every build, rather than by a dependency edge.

/// Milliseconds in a day. Every day, because Unix time has no leap seconds.
const MILLIS_PER_DAY: i64 = 86_400_000;

/// The earliest instant the wire format can express: `0000-01-01T00:00:00.000Z`.
pub(crate) const MIN_MILLIS: i64 = -62_167_219_200_000;

/// The last instant the wire format can express: `9999-12-31T23:59:59.999Z`.
pub(crate) const MAX_MILLIS: i64 = 253_402_300_799_999;

/// Whether [`format`] can render this instant.
///
/// The bound is RFC 3339's, not an implementation limit. `date-fullyear` is `4DIGIT`, so a year
/// outside `0000..=9999` has no representation the grammar admits. chrono renders those in ISO 8601
/// expanded form — `+10000-01-01T00:00:00Z`, with a sign no RFC 3339 parser is obliged to accept —
/// which is a payload the server would refuse. Refusing to build it here turns a remote rejection
/// into a local quarantine, where the record is still readable and still recoverable (decision 006).
///
/// # Why this is public
///
/// It is part of the storage contract, not an implementation detail. A durable backend has to
/// decide whether a stored row is decodable, and this bound is one of the three things that make a
/// row corrupt. Leaving it `pub(crate)` meant an out-of-tree backend had to *reimplement* the
/// range — and two implementations of one rule is exactly the divergence the conformance suite
/// exists to prevent. Found while building `frontbox-sqlite`, which is the first backend outside
/// this crate and therefore the first thing in a position to notice.
pub fn is_representable(millis: i64) -> bool {
    (MIN_MILLIS..=MAX_MILLIS).contains(&millis)
}

/// Render epoch milliseconds as the wire format's `client_datetime`.
///
/// Returns `None` for an instant outside [`is_representable`].
///
/// The fraction is present only when it is non-zero, and is exactly three digits when it is —
/// `…:20Z` and `…:20.100Z`, never `…:20.000Z` or `…:20.1Z`. That is not a style choice: it is what
/// the existing server already receives, and the shape the oracle test holds this to.
pub(crate) fn format(millis: i64) -> Option<String> {
    if !is_representable(millis) {
        return None;
    }

    // Euclidean rather than truncating, so a pre-epoch instant borrows from the day rather than
    // rounding toward zero. `-1` is `1969-12-31T23:59:59.999Z`, one millisecond before the epoch —
    // truncating division would put it on 1970-01-01 with a negative time of day.
    let (day, millis_of_day) = (
        millis.div_euclid(MILLIS_PER_DAY),
        millis.rem_euclid(MILLIS_PER_DAY),
    );
    let (year, month, day_of_month) = civil_from_days(day);

    let hour = millis_of_day / 3_600_000;
    let minute = millis_of_day % 3_600_000 / 60_000;
    let second = millis_of_day % 60_000 / 1_000;
    let milli = millis_of_day % 1_000;

    Some(if milli == 0 {
        format!("{year:04}-{month:02}-{day_of_month:02}T{hour:02}:{minute:02}:{second:02}Z")
    } else {
        format!(
            "{year:04}-{month:02}-{day_of_month:02}T{hour:02}:{minute:02}:{second:02}.{milli:03}Z"
        )
    })
}

/// Parse a `client_datetime` back to epoch milliseconds.
///
/// # Accepted grammar
///
/// RFC 3339 `date-time`, with the concessions the specification itself names and nothing more:
///
/// - `T` or `t` as the date/time separator. A space is **not** accepted. RFC 3339 permits it only
///   "by mutual agreement" (§5.6, NOTE), and this library has no such agreement to appeal to.
/// - `Z`, `z`, or a numeric `±HH:MM` offset. An offset is required — a bare local time names no
///   instant.
/// - Any number of fractional-second digits. Everything below a millisecond is truncated, because
///   milliseconds are the only precision [`Clock`](crate::clock::Clock) produces or records store;
///   accepting the digits and dropping them is honest about that, whereas rejecting them would
///   refuse a timestamp that is merely more precise than we need.
/// - A second value of `60` is **rejected**. chrono accepts it and maps it onto the following
///   second, which silently moves the value by up to a second. Since this library generates its own
///   timestamps from an `i64` that cannot express a leap second, a `60` on the wire is far likelier
///   to be a defect than an observation, and shifting it quietly is the wrong answer either way.
///
/// This is deliberately stricter than what produced it. That direction is safe: everything
/// [`format`] emits parses, so the round trip holds.
pub(crate) fn parse(text: &str) -> Result<i64, &'static str> {
    let bytes = text.as_bytes();
    // `YYYY-MM-DDTHH:MM:SS` plus at least a one-character offset.
    if bytes.len() < 20 {
        return Err("too short for an RFC 3339 date-time");
    }
    if bytes[4] != b'-' || bytes[7] != b'-' || bytes[13] != b':' || bytes[16] != b':' {
        return Err("expected `YYYY-MM-DDThh:mm:ss`");
    }
    if !matches!(bytes[10], b'T' | b't') {
        return Err("expected `T` between the date and the time");
    }

    let year = number(&bytes[0..4])?;
    let month = number(&bytes[5..7])?;
    let day = number(&bytes[8..10])?;
    let hour = number(&bytes[11..13])?;
    let minute = number(&bytes[14..16])?;
    let second = number(&bytes[17..19])?;

    if !(1..=12).contains(&month) {
        return Err("month out of range");
    }
    if day < 1 || day > days_in_month(year, month) {
        return Err("day out of range for the month");
    }
    if hour > 23 || minute > 59 {
        return Err("time out of range");
    }
    if second > 59 {
        return Err("second out of range (leap seconds are not accepted)");
    }

    let mut rest = &bytes[19..];
    let mut milli = 0;
    if rest[0] == b'.' {
        rest = &rest[1..];
        let digits = rest.iter().take_while(|b| b.is_ascii_digit()).count();
        if digits == 0 {
            return Err("a fractional second needs at least one digit");
        }
        // Only the first three digits survive; the rest are validated and discarded.
        for (place, digit) in rest[..digits.min(3)].iter().enumerate() {
            milli += i64::from(digit - b'0') * 10_i64.pow(2 - place as u32);
        }
        rest = &rest[digits..];
    }

    let offset_minutes = match rest {
        [b'Z' | b'z'] => 0,
        [sign @ (b'+' | b'-'), rest @ ..] => {
            if rest.len() != 5 || rest[2] != b':' {
                return Err("expected a `±hh:mm` offset");
            }
            let hours = number(&rest[0..2])?;
            let minutes = number(&rest[3..5])?;
            if hours > 23 || minutes > 59 {
                return Err("offset out of range");
            }
            let magnitude = hours * 60 + minutes;
            if *sign == b'-' {
                -magnitude
            } else {
                magnitude
            }
        }
        [] => return Err("an offset is required; a local time names no instant"),
        _ => return Err("trailing characters after the instant"),
    };

    let millis = days_from_civil(year, month, day) * MILLIS_PER_DAY
        + hour * 3_600_000
        + minute * 60_000
        + second * 1_000
        + milli
        - offset_minutes * 60_000;

    // An in-range civil time can still fall outside the range once its offset is applied — the last
    // hours of 9999-12-31 at a negative offset, for instance.
    if !is_representable(millis) {
        return Err("instant outside the representable range");
    }
    Ok(millis)
}

/// Read a run of ASCII digits as a number.
fn number(bytes: &[u8]) -> Result<i64, &'static str> {
    let mut value = 0;
    for byte in bytes {
        if !byte.is_ascii_digit() {
            return Err("expected a digit");
        }
        value = value * 10 + i64::from(byte - b'0');
    }
    Ok(value)
}

/// Whether `year` has a 29-day February, by the proleptic Gregorian rule.
fn is_leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Days in `month` of `year`, which is 1-indexed.
fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

// The two calendar conversions below are Howard Hinnant's `civil_from_days` and `days_from_civil`
// (`http://howardhinnant.github.io/date_algorithms.html`), which are exact over the whole i64 range
// and shift the year to start in March so that the leap day lands at the end of a year rather than
// in the middle of one. That is why the month arithmetic looks rotated: `mp` counts March as 0.
//
// They are transcribed rather than invented, and the oracle test is what confirms the
// transcription. Neither is called anywhere except through `format` and `parse`.

/// Convert days since 1970-01-01 to a proleptic Gregorian `(year, month, day)`.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    // Shift the epoch to 0000-03-01, the start of a 400-year era.
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097; // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365; // [0, 399]
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100); // [0, 365]
    let month_prime = (5 * day_of_year + 2) / 153; // [0, 11], March = 0
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1; // [1, 31]
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };

    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Convert a proleptic Gregorian `(year, month, day)` to days since 1970-01-01.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400; // [0, 399]
    let month_prime = if month > 2 { month - 3 } else { month + 9 }; // [0, 11], March = 0
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1; // [0, 365]
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests;
