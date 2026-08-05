//! The civil calendar, and a time value's fields — arithmetic, and nothing else.
//!
//! Separate from the class for the reason README rule 6 gives, and the split is
//! along a real seam rather than at a line count: nothing in this file touches a
//! `Context`, a `Value` or a borrow, so every function here is testable on its
//! own and the tests at the bottom need no runtime at all. The alternative —
//! splitting the getters off from the constructor — would have put two halves of
//! one class in two files and still needed this arithmetic in a third.

/// Milliseconds in a day, as the double every conversion here divides by.
pub(super) const MS_PER_DAY: f64 = 86_400_000.0;

/// The largest magnitude a time value may have: ±100 000 000 days.
///
/// Outside it the language says the date is invalid rather than saturating, so
/// [`clip`] answers `NaN` — which is also what keeps [`civil_from_days`] away
/// from the range where an `i64` day count would stop being exact.
const MAX_TIME: f64 = 8.64e15;

/// A time value truncated to a whole millisecond, or `NaN` outside the range.
pub(super) fn clip(ms: f64) -> f64 {
    match ms.is_finite() && ms.abs() <= MAX_TIME {
        true => ms.trunc(),
        false => f64::NAN,
    }
}

/// Midnight on the first of a month, from a year and a zero-based month.
///
/// A month outside `0..12` rolls the year, which is what the language does and
/// what a range check would have got wrong: `new Date(2020, 12)` is January 2021
/// in every engine, and programs written against that behaviour rely on it.
pub(super) fn from_year_month(year: f64, month: f64) -> f64 {
    if !year.is_finite() || !month.is_finite() {
        return f64::NAN;
    }
    // Two digits mean the twentieth century — `new Date(99, 0)` is 1999. A rule
    // the language keeps for compatibility, and this keeps for the same reason.
    let full = match (0.0..=99.0).contains(&year.trunc()) {
        true => 1900.0 + year.trunc(),
        false => year.trunc(),
    };
    let months = full as i64 * 12 + month.trunc() as i64;
    // Euclidean rather than truncating, so `new Date(2020, -1)` is December 2019
    // instead of month zero of a year that did not move.
    let days = days_from_civil(months.div_euclid(12), months.rem_euclid(12) + 1, 1);
    clip(days as f64 * MS_PER_DAY)
}

/// A time value broken into the fields the getters answer.
pub(super) struct Parts {
    pub(super) year: i64,
    /// Zero-based, because that is what `getMonth` answers.
    pub(super) month: i64,
    pub(super) day: i64,
    pub(super) weekday: i64,
    pub(super) hour: i64,
    pub(super) minute: i64,
    pub(super) second: i64,
    pub(super) milli: i64,
}

/// The fields of a time value, `None` when there is no date to break apart.
pub(super) fn parts_of(ms: f64) -> Option<Parts> {
    if !ms.is_finite() {
        return None;
    }
    // Floored rather than truncated, so a negative time value borrows from the
    // day instead of counting backwards inside it: 1969 has to have a 23:00.
    let days = (ms / MS_PER_DAY).floor();
    let within = (ms - days * MS_PER_DAY) as i64;
    let days = days as i64;
    let (year, month, day) = civil_from_days(days);
    Some(Parts {
        year,
        month: month - 1,
        day,
        // The epoch was a Thursday, which is the only constant in this file that
        // is a fact about the calendar rather than about arithmetic.
        weekday: (days + 4).rem_euclid(7),
        hour: within / 3_600_000,
        minute: within / 60_000 % 60,
        second: within / 1_000 % 60,
        milli: within % 1_000,
    })
}

/// Days since the epoch, from a proleptic Gregorian year, month and day.
///
/// Howard Hinnant's `days_from_civil`, and its inverse below. What it gets right
/// and a naive `365.25` does not: every leap rule at once — four, hundred, four
/// hundred — and years before 1582 under the *proleptic* Gregorian calendar,
/// which is the one the language defines its time value over. `month` is
/// one-based here and zero-based in [`Parts`]; the conversion happens at that one
/// boundary rather than inside the algorithm, which stays stated in its own terms.
pub(super) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The year, one-based month and day a day count names.
pub(super) fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 { shifted } else { shifted - 146_096 } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 };
    (year + i64::from(month <= 2), month, day)
}

/// `YYYY-MM-DDTHH:MM:SS.sssZ`, and `"Invalid Date"` when there is no date.
pub(super) fn iso_text(ms: f64) -> String {
    let Some(parts) = parts_of(ms) else {
        return "Invalid Date".to_owned();
    };
    // Four digits is the only width the language guarantees; a year outside it
    // has an expanded form this does not write, and printing it plainly is the
    // narrower wrong answer than refusing to format the date at all.
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        parts.year,
        parts.month + 1,
        parts.day,
        parts.hour,
        parts.minute,
        parts.second,
        parts.milli
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A leap day exists where the four-hundred rule says it does.
    ///
    /// 2000 is a leap year and 1900 is not — the pair that separates the real
    /// calendar from `year % 4 == 0`, and the reason this algorithm is here
    /// rather than a division.
    #[test]
    fn a_leap_day_lands_on_the_right_day() {
        assert_eq!(civil_from_days(days_from_civil(2000, 2, 29)), (2000, 2, 29));
        // 1900 had no 29th of February, so the day count for it is the 1st of
        // March — the round trip failing to round trip, on purpose.
        assert_eq!(civil_from_days(days_from_civil(1900, 2, 29)), (1900, 3, 1));
    }

    /// A time value before the epoch is negative, and its fields still count
    /// forwards inside the day.
    ///
    /// The truncating division that looks correct answers hour 23 as hour -1
    /// here, which is the bug this pins.
    #[test]
    fn a_date_before_the_epoch_reads_forwards() {
        let ms = days_from_civil(1969, 7, 20) as f64 * MS_PER_DAY + 73_060_000.0;
        assert!(ms < 0.0);
        let parts = parts_of(ms).expect("a finite time value has parts");
        assert_eq!((parts.year, parts.month, parts.day), (1969, 6, 20));
        assert_eq!((parts.hour, parts.minute, parts.second), (20, 17, 40));
        // A Sunday.
        assert_eq!(parts.weekday, 0);
        assert_eq!(iso_text(ms), "1969-07-20T20:17:40.000Z");
    }

    /// The epoch itself, and the weekday every other answer here is measured
    /// from.
    #[test]
    fn the_epoch_is_a_thursday() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(parts_of(0.0).expect("zero is finite").weekday, 4);
    }

    /// A month outside the year rolls it, in both directions.
    #[test]
    fn a_month_outside_the_year_rolls_it() {
        assert_eq!(iso_text(from_year_month(2020.0, 12.0)), "2021-01-01T00:00:00.000Z");
        assert_eq!(iso_text(from_year_month(2020.0, -1.0)), "2019-12-01T00:00:00.000Z");
        // Two digits are the twentieth century.
        assert_eq!(iso_text(from_year_month(99.0, 0.0)), "1999-01-01T00:00:00.000Z");
    }
}
