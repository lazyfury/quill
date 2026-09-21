//! Clock helpers: the Shanghai wall-clock stamp, a duration countdown and an
//! RFC 3339 parser.
//!
//! Pure `std`. The tool's stamps are all UTC+8 and the reset stamps are RFC 3339,
//! so a fixed offset and a small parser are enough — no date crate and no
//! timezone database.

use std::time::Duration;

/// Shanghai keeps UTC+8 all year (no DST since 1991), so a fixed offset is exact
/// and the tool needs no timezone database.
pub const SHANGHAI_OFFSET_SECONDS: i64 = 8 * 3_600;

/// Epoch seconds now.
pub fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// `HH:MM:SS UTC+8` wall-clock stamp for the "refreshed at" line.
pub fn timestamp() -> String {
    timestamp_at(now_epoch())
}

/// [`timestamp`] for a given epoch-second value, so the day boundaries stay
/// testable without a clock.
pub fn timestamp_at(epoch_seconds: i64) -> String {
    let seconds_of_day = (epoch_seconds + SHANGHAI_OFFSET_SECONDS).rem_euclid(86_400);
    format!(
        "{:02}:{:02}:{:02} UTC+8",
        seconds_of_day / 3_600,
        (seconds_of_day % 3_600) / 60,
        seconds_of_day % 60
    )
}

/// A countdown as `MM:SS`, or `H:MM:SS` past an hour.
///
/// Rounded up, so a countdown never reads `00:00` while a second is still left:
/// with truncation the final value would be on screen for two seconds.
pub fn clock(remaining: Duration) -> String {
    let seconds = remaining.as_secs() + u64::from(remaining.subsec_millis() > 0);
    let (hours, minutes, seconds) = (seconds / 3_600, (seconds % 3_600) / 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

/// Parses the subset of RFC 3339 an API reset stamp uses into epoch seconds.
///
/// Accepts `YYYY-MM-DD[Tt ]HH:MM:SS`, optional `.fff`, and either `Z`/`z` or a
/// `±HH:MM` / `±HHMM` offset; a missing suffix is read as UTC. Anything else is
/// `None` rather than a guess, so a bad stamp leaves the cell empty.
pub fn parse_rfc3339_utc(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 19 {
        return None;
    }

    let year = digits(bytes, 0, 4)?;
    expect(bytes, 4, b'-')?;
    let month = digits(bytes, 5, 2)?;
    expect(bytes, 7, b'-')?;
    let day = digits(bytes, 8, 2)?;
    match bytes[10] {
        b'T' | b't' | b' ' => {}
        _ => return None,
    }
    let hour = digits(bytes, 11, 2)?;
    expect(bytes, 13, b':')?;
    let minute = digits(bytes, 14, 2)?;
    expect(bytes, 16, b':')?;
    let second = digits(bytes, 17, 2)?;

    let mut index = 19;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == start {
            return None;
        }
    }

    let offset = match bytes.get(index) {
        None => 0,
        Some(b'Z' | b'z') => {
            index += 1;
            0
        }
        Some(sign @ (b'+' | b'-')) => {
            let negative = *sign == b'-';
            index += 1;
            let hours = digits(bytes, index, 2)?;
            index += 2;
            if bytes.get(index) == Some(&b':') {
                index += 1;
            }
            let minutes = digits(bytes, index, 2)?;
            index += 2;
            let seconds = hours * 3_600 + minutes * 60;
            if negative {
                -seconds
            } else {
                seconds
            }
        }
        _ => return None,
    };
    if index != bytes.len() {
        return None;
    }

    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }

    Some(days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second - offset)
}

/// Reads exactly `count` ASCII digits at `at`.
fn digits(bytes: &[u8], at: usize, count: usize) -> Option<i64> {
    let slice = bytes.get(at..at + count)?;
    if !slice.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let mut value = 0i64;
    for &byte in slice {
        value = value * 10 + i64::from(byte - b'0');
    }
    Some(value)
}

fn expect(bytes: &[u8], at: usize, byte: u8) -> Option<()> {
    (bytes.get(at) == Some(&byte)).then_some(())
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm), so the
/// reset countdown needs no date crate.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stamp is a fixed UTC+8 wall clock, and it wraps at Shanghai midnight
    /// (16:00 UTC) instead of going negative for pre-epoch values.
    #[test]
    fn the_stamp_is_shanghai_wall_clock() {
        let stamp = timestamp();
        assert_eq!(stamp.len(), "00:00:00 UTC+8".len(), "{stamp}");
        assert!(stamp.ends_with(" UTC+8"), "{stamp}");

        // Epoch 0 is 1970-01-01T00:00:00Z -> 08:00:00 in Shanghai.
        assert_eq!(timestamp_at(0), "08:00:00 UTC+8");
        assert_eq!(timestamp_at(3_600), "09:00:00 UTC+8");
        assert_eq!(timestamp_at(16 * 3_600), "00:00:00 UTC+8");
        assert_eq!(timestamp_at(86_400 - 1), "07:59:59 UTC+8");
        assert_eq!(timestamp_at(-3_600), "07:00:00 UTC+8");
    }

    #[test]
    fn the_countdown_reads_out_mm_ss() {
        let cases = [
            (Duration::ZERO, "00:00"),
            (Duration::from_millis(1), "00:01"),
            (Duration::from_secs(5), "00:05"),
            (Duration::from_secs(59), "00:59"),
            (Duration::from_secs(60), "01:00"),
            (Duration::from_secs(300), "05:00"),
            (Duration::from_secs(3_599), "59:59"),
            (Duration::from_secs(3_600), "1:00:00"),
            (Duration::from_secs(3_661), "1:01:01"),
            // Rounded up: 4.0s is `00:04` for one second, not two.
            (Duration::from_millis(4_000), "00:04"),
            (Duration::from_millis(4_001), "00:05"),
        ];
        for (remaining, expected) in cases {
            assert_eq!(clock(remaining), expected, "for {remaining:?}");
        }
    }

    /// The RFC 3339 shapes an API reset stamp takes: the epoch, fractional
    /// seconds, a space separator, numeric offsets and a leap day.
    #[test]
    fn parses_rfc3339_forms() {
        assert_eq!(parse_rfc3339_utc("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339_utc("1970-01-01T08:00:00+08:00"), Some(0));

        let utc = parse_rfc3339_utc("2026-09-21T12:00:00Z").unwrap();
        assert_eq!(
            parse_rfc3339_utc("2026-09-21T12:00:00.123Z"),
            Some(utc),
            "the fraction is dropped"
        );
        assert_eq!(
            parse_rfc3339_utc("2026-09-21 12:00:00Z"),
            Some(utc),
            "a space separates date and time too"
        );
        assert_eq!(
            parse_rfc3339_utc("2026-09-21T20:00:00+08:00"),
            Some(utc),
            "same instant, different zone"
        );
        assert_eq!(parse_rfc3339_utc("2026-09-21T04:00:00-08:00"), Some(utc));

        // 2024-02-29 is a leap day, and the day after is exactly 24h later.
        let leap = parse_rfc3339_utc("2024-02-29T00:00:00Z");
        assert_eq!(leap, Some(1_709_164_800));
        assert_eq!(
            parse_rfc3339_utc("2024-03-01T00:00:00Z").unwrap() - leap.unwrap(),
            86_400
        );
    }

    #[test]
    fn malformed_stamps_are_rejected_instead_of_guessed() {
        for bad in [
            "",
            "2026-09-21",
            "2026-09-21T12:00",
            "2026-13-01T00:00:00Z",
            "2026-09-32T00:00:00Z",
            "2026-09-21T25:00:00Z",
            "2026-09-21T12:00:00Zextra",
            "garbage2026-09-21T12:00:00Z",
        ] {
            assert_eq!(parse_rfc3339_utc(bad), None, "for {bad:?}");
        }
    }
}
