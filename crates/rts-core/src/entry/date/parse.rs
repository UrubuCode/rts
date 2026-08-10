//! Reading a date back out of text.
//!
//! One parser, used by both `Date.parse` and `new Date(string)`, because two of
//! them come to disagree about a missing `Z` — and that pair is one the language
//! guarantees answers the same number.

use super::civil::{MS_PER_DAY, clip, days_from_civil};

/// ISO-8601 as the language's `Date.parse` accepts it: `YYYY-MM-DD`, optionally
/// with a time, optionally with `Z` or a `±HH:MM` offset.
///
/// What is dropped: every non-ISO form engines accept out of habit
/// (`"Jan 1 2020"`, RFC 2822, the American `M/D/Y`). They are unspecified, each
/// engine spells them differently, and a partial imitation is a parser programs
/// would come to depend on and then find a gap in. `NaN` is the language's own
/// answer for a string it does not recognise, so the refusal is expressible.
///
/// A date-time with no offset is UTC here rather than local, where the language
/// says local — which costs nothing and is the module documentation's point:
/// there is no local time in this runtime to differ from.
pub(super) fn parse_iso(text: &str) -> f64 {
    let text = text.trim();
    let bytes = text.as_bytes();
    let number = |from: usize, len: usize| -> Option<i64> {
        let slice = text.get(from..from + len)?;
        match slice.bytes().all(|byte| byte.is_ascii_digit()) {
            true => slice.parse().ok(),
            false => None,
        }
    };

    let (sign, start) = match bytes.first() {
        Some(b'-') => (-1, 1),
        Some(b'+') => (1, 1),
        _ => (1, 0),
    };
    // An expanded year is six digits behind a sign, a plain one is four. The
    // sign is what selects the width, which is why it is read first rather than
    // folded into the number.
    let width = match start {
        0 => 4,
        _ => 6,
    };
    let month = match bytes.get(start + width) {
        Some(b'-') => number(start + width + 1, 2),
        // A year alone is a valid date, and it means the first of January.
        _ => Some(1),
    };
    let day = match bytes.get(start + width + 3) {
        Some(b'-') => number(start + width + 4, 2),
        _ => Some(1),
    };
    let (Some(year), Some(month), Some(day)) = (number(start, width), month, day) else {
        return f64::NAN;
    };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return f64::NAN;
    }

    let mut at = start + width;
    for _ in 0..2 {
        if bytes.get(at) == Some(&b'-') {
            at += 3;
        }
    }
    let mut milliseconds = 0i64;
    let mut offset = 0i64;
    if matches!(bytes.get(at), Some(b'T' | b't')) {
        let (Some(hour), Some(minute)) = (number(at + 1, 2), number(at + 4, 2)) else {
            return f64::NAN;
        };
        if bytes.get(at + 3) != Some(&b':') || hour > 24 || minute > 59 {
            return f64::NAN;
        }
        at += 6;
        let mut second = 0;
        if bytes.get(at) == Some(&b':') {
            let Some(read) = number(at + 1, 2) else {
                return f64::NAN;
            };
            second = read;
            at += 3;
        }
        let mut fraction = 0;
        if bytes.get(at) == Some(&b'.') {
            let Some(read) = number(at + 1, 3) else {
                return f64::NAN;
            };
            fraction = read;
            at += 4;
            // Anything past three digits is sub-millisecond and the time value
            // has no room for it, so it is dropped rather than rounded — the
            // same direction `clip` truncates, which keeps the two consistent.
            while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                at += 1;
            }
        }
        milliseconds = hour * 3_600_000 + minute * 60_000 + second * 1_000 + fraction;
        offset = match bytes.get(at) {
            Some(b'Z' | b'z') => {
                at += 1;
                0
            }
            Some(byte @ (b'+' | b'-')) => {
                let direction = match byte {
                    b'-' => -1,
                    _ => 1,
                };
                let (Some(hours), Some(minutes)) = (number(at + 1, 2), number(at + 4, 2)) else {
                    return f64::NAN;
                };
                at += 6;
                direction * (hours * 60 + minutes) * 60_000
            }
            _ => 0,
        };
    }
    // Trailing text is a refusal rather than something to ignore: a parser that
    // stopped early would read `"2020-01-01 lunch"` as a date, and a program
    // holding one of those never learns its data is wrong.
    if at != bytes.len() {
        return f64::NAN;
    }

    // The offset is *subtracted*, because it says how far the written clock is
    // ahead of UTC — the sign that is wrong in both directions if guessed.
    let days = days_from_civil(sign * year, month, day);
    clip((days as f64) * MS_PER_DAY + (milliseconds - offset) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The forms accepted, and the epoch as the number both agree on.
    #[test]
    fn the_accepted_forms_read_the_same_instant() {
        assert_eq!(parse_iso("1970-01-01"), 0.0);
        assert_eq!(parse_iso("1970-01-01T00:00:00.000Z"), 0.0);
        assert_eq!(parse_iso("2020-01-01T01:00:00+01:00"), parse_iso("2020-01-01T00:00:00Z"));
        assert_eq!(parse_iso("2020-01-01T00:00:00-01:00"), parse_iso("2020-01-01T01:00:00Z"));
    }

    /// A date before the epoch is a negative time value.
    #[test]
    fn a_date_before_the_epoch_is_negative() {
        assert!(parse_iso("1969-07-20T20:17:40.000Z") < 0.0);
    }

    /// What is refused, and refused as `NaN` rather than as a nearby date.
    #[test]
    fn junk_is_not_a_date() {
        assert!(parse_iso("Jan 1 2020").is_nan());
        assert!(parse_iso("2020-13-01").is_nan());
        assert!(parse_iso("2020-01-01T00:00:00Zjunk").is_nan());
        assert!(parse_iso("2020-01-0").is_nan());
        assert!(parse_iso("").is_nan());
    }
}
