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

/// `MakeFullYear` — two digits mean the twentieth century, so `new Date(99, 0)`
/// is 1999. A rule the language keeps for compatibility, and this keeps for the
/// same reason.
///
/// A function of its own rather than a step inside [`from_parts`], because the
/// specification applies it in exactly two places — the constructor and
/// `Date.UTC` — and *not* in the setters. Folding it into the shared arithmetic
/// would make `d.setFullYear(50)` mean 1950, which no engine does and which
/// nothing in this file could then spell.
pub(super) fn full_year(year: f64) -> f64 {
    match (0.0..=99.0).contains(&year.trunc()) {
        true => 1900.0 + year.trunc(),
        false => year,
    }
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
    format!(
        "{}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        iso_year(parts.year),
        parts.month + 1,
        parts.day,
        parts.hour,
        parts.minute,
        parts.second,
        parts.milli
    )
}

/// The year field of an ISO-8601 date, in whichever of its two widths applies.
///
/// Four digits inside `0000..=9999` and the **expanded** `±YYYYYY` outside it,
/// which is what a time value at either end of the range needs: `new Date(8.64e15)`
/// is the year 275 760 and `Date.UTC(-1, 0, 1)` is the year -1, and neither fits
/// in four digits. It printed them plainly — `275760-09-13` and `-001-01-01` —
/// with a comment calling that "the narrower wrong answer", and it was wrong in
/// the way that matters most for this format: neither string parses back.
/// `Date.parse` here already reads the expanded form, because the sign is what
/// selects the six-digit width, so this is the half that was missing.
///
/// The sign is mandatory in the expanded form and forbidden in the plain one,
/// which is the specification's rule and not a stylistic choice — `+012345` and
/// `012345` are not the same year to a reader who knows only one of the widths.
fn iso_year(year: i64) -> String {
    match (0..=9999).contains(&year) {
        true => format!("{year:04}"),
        false => format!("{}{:06}", if year < 0 { '-' } else { '+' }, year.abs()),
    }
}

/// The names Node's `toUTCString`/`toDateString`/`toString` write, in the
/// three-letter form the format shares between weekday and month.
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// A time value from every field a setter can touch, civil year through
/// millisecond.
///
/// The ONE construction in this file, and every caller reaches it: the
/// constructor, `Date.UTC` and all seven setters. It was two — a `from_year_month`
/// beside this — and the pair is what let the constructor stop at a month while
/// the setters carried all seven fields.
///
/// A month outside `0..12` or a day outside the month rolls forward or back
/// rather than being refused, because that is what every setter this crate has
/// already committed to does, and a constructor disagreeing would be the one
/// place `new Date(y, 12)` and `d.setMonth(12)` meant different things.
pub(super) fn from_parts(
    year: f64,
    month: f64,
    day: f64,
    hour: f64,
    minute: f64,
    second: f64,
    milli: f64,
) -> f64 {
    if ![year, month, day, hour, minute, second, milli]
        .iter()
        .all(|field| field.is_finite())
    {
        return f64::NAN;
    }
    let months = year.trunc() as i64 * 12 + month.trunc() as i64;
    let days = days_from_civil(months.div_euclid(12), months.rem_euclid(12) + 1, day.trunc() as i64);
    let time_within_day = hour.trunc() * 3_600_000.0
        + minute.trunc() * 60_000.0
        + second.trunc() * 1_000.0
        + milli.trunc();
    clip(days as f64 * MS_PER_DAY + time_within_day)
}

/// `"Thu Jan 01 1970"` — the date-only half of [`utc_string`]'s fields, in the
/// order `toDateString` prints them.
pub(super) fn date_string(ms: f64) -> String {
    let Some(parts) = parts_of(ms) else {
        return "Invalid Date".to_owned();
    };
    format!(
        "{} {} {:02} {:04}",
        WEEKDAYS[parts.weekday as usize], MONTHS[parts.month as usize], parts.day, parts.year
    )
}

/// `"00:00:00.000Z"` — the time-only half, in the form `toTimeString` answers
/// here. See the module documentation for why there is no timezone name.
pub(super) fn time_string(ms: f64) -> String {
    let Some(parts) = parts_of(ms) else {
        return "Invalid Date".to_owned();
    };
    format!("{:02}:{:02}:{:02}.{:03}Z", parts.hour, parts.minute, parts.second, parts.milli)
}

/// `"00:00:00"` — `toLocaleTimeString` with no locale database behind it, so
/// this is the 24-hour form rather than a guess at one operating system's
/// default. See [`locale_string`] for why this shape and not `am`/`pm`.
pub(super) fn locale_time_string(ms: f64) -> String {
    let Some(parts) = parts_of(ms) else {
        return "Invalid Date".to_owned();
    };
    format!("{:02}:{:02}:{:02}", parts.hour, parts.minute, parts.second)
}

/// `"01/01/1970, 00:00:00"` — `toLocaleString` with no locale database.
///
/// `DD/MM/YYYY` rather than ISO order: this crate has no locale to consult, so
/// there is no "correct" order to pick, and this is the one already pinned by
/// `date_to_json.test.ts` (#220, #1202) — chosen there to match Bun/Node under
/// a Windows host's default locale, which is what this workspace runs on.
pub(super) fn locale_string(ms: f64) -> String {
    let Some(parts) = parts_of(ms) else {
        return "Invalid Date".to_owned();
    };
    format!(
        "{:02}/{:02}/{:04}, {:02}:{:02}:{:02}",
        parts.month + 1,
        parts.day,
        parts.year,
        parts.hour,
        parts.minute,
        parts.second
    )
}

/// `"Thu, 01 Jan 1970 00:00:00 GMT"` — the RFC 7231 form `toUTCString` answers.
///
/// Always `GMT`, never an offset: [`super`]'s module documentation is why —
/// this runtime's local time *is* UTC, so the field is naming that rather than
/// claiming a conversion happened. `"Invalid Date"` for a time value with no
/// parts, matching [`iso_text`].
pub(super) fn utc_string(ms: f64) -> String {
    let Some(parts) = parts_of(ms) else {
        return "Invalid Date".to_owned();
    };
    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        WEEKDAYS[parts.weekday as usize],
        parts.day,
        MONTHS[(parts.month) as usize],
        parts.year,
        parts.hour,
        parts.minute,
        parts.second
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

    /// The epoch in the RFC 7231 form, the one every reader recognises.
    #[test]
    fn the_epoch_prints_as_a_thursday_in_gmt() {
        assert_eq!(utc_string(0.0), "Thu, 01 Jan 1970 00:00:00 GMT");
    }

    /// A month outside the year rolls it, in both directions.
    #[test]
    fn a_month_outside_the_year_rolls_it() {
        let midnight = |year, month| from_parts(year, month, 1.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(iso_text(midnight(2020.0, 12.0)), "2021-01-01T00:00:00.000Z");
        assert_eq!(iso_text(midnight(2020.0, -1.0)), "2019-12-01T00:00:00.000Z");
        // Two digits are the twentieth century — but only where the constructor
        // and `Date.UTC` ask for it, which is why this is a separate step.
        assert_eq!(iso_text(midnight(full_year(99.0), 0.0)), "1999-01-01T00:00:00.000Z");
        assert_eq!(full_year(2020.0), 2020.0);
    }

    /// A year that does not fit in four digits is written in the expanded form,
    /// with the sign that form requires.
    ///
    /// The pair that matters is the ends of the range: both `new Date(8.64e15)`
    /// and `new Date(-8.64e15)` are legal dates, and both printed something no
    /// parser reads back — `275760-09-13` had no `+` and `-1` came out as `-001`.
    #[test]
    fn a_year_outside_four_digits_is_expanded_and_signed() {
        assert_eq!(iso_text(MAX_TIME), "+275760-09-13T00:00:00.000Z");
        assert_eq!(iso_text(-MAX_TIME), "-271821-04-20T00:00:00.000Z");
        let year_one_bc = from_parts(-1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(iso_text(year_one_bc), "-000001-01-01T00:00:00.000Z");
        // And a year that DOES fit keeps the plain four digits, with no sign —
        // the two widths are not interchangeable to a reader.
        assert_eq!(iso_text(0.0), "1970-01-01T00:00:00.000Z");
    }

    /// Every field of the constructor's seven reaches the time value.
    #[test]
    fn seven_fields_all_arrive() {
        let ms = from_parts(2024.0, 0.0, 15.0, 10.0, 30.0, 45.0, 123.0);
        assert_eq!(iso_text(ms), "2024-01-15T10:30:45.123Z");
    }
}
