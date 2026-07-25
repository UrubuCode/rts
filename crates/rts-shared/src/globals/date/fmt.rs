//! `Date` string-formatting helpers — pure `i64 ms → String` functions ported
//! verbatim from the previous hand-written externs. Kept here (out of the
//! `#[rtse::class]` impl block) so `instance.rs`'s string methods are 1-liners
//! and the impl block stays within the file-size ceiling. Every helper computes
//! its calendar parts through the `date` namespace math backend
//! (`crate::date::__RTS_FN_NS_DATE_*`) exactly as before — no behavior change.

use super::instance::INVALID_MS;

const DAY_NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MON_NAMES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The ISO 8601 UTC string. Reads the `date` namespace's canonical ISO extern
/// (a string handle) back into an owned `String`.
pub(crate) fn iso(ms: i64) -> String {
    let h = crate::date::__RTS_FN_NS_DATE_TO_ISO(ms);
    rts_engine::heap::handles::read_string_handle(h).unwrap_or_default()
}

/// `Date.prototype.toString()` — JS spec:
/// "Day Mon DD YYYY HH:MM:SS GMT+0000 (Coordinated Universal Time)".
/// RTS always UTC, so the tz is fixed. An Invalid Date yields "Invalid Date".
pub(crate) fn to_string(ms: i64) -> String {
    use crate::date::*;
    if ms == INVALID_MS {
        return "Invalid Date".to_string();
    }
    let (year, month, day) = (
        __RTS_FN_NS_DATE_YEAR(ms),
        __RTS_FN_NS_DATE_MONTH(ms),
        __RTS_FN_NS_DATE_DAY(ms),
    );
    let (hour, minute, second) = (
        __RTS_FN_NS_DATE_HOUR(ms),
        __RTS_FN_NS_DATE_MINUTE(ms),
        __RTS_FN_NS_DATE_SECOND(ms),
    );
    let dow = __RTS_FN_NS_DATE_WEEKDAY(ms);
    format!(
        "{} {} {:02} {:04} {:02}:{:02}:{:02} GMT+0000 (Coordinated Universal Time)",
        DAY_NAMES[dow.clamp(0, 6) as usize],
        // month is 0-based (getMonth() returns 0..11).
        MON_NAMES[month.clamp(0, 11) as usize],
        day,
        year,
        hour,
        minute,
        second,
    )
}

/// `toUTCString()` — RFC 1123: "Day, DD Mon YYYY HH:MM:SS GMT".
pub(crate) fn utc(ms: i64) -> String {
    use crate::date::*;
    let (year, month, day) = (
        __RTS_FN_NS_DATE_YEAR(ms),
        __RTS_FN_NS_DATE_MONTH(ms),
        __RTS_FN_NS_DATE_DAY(ms),
    );
    let (hour, minute, second) = (
        __RTS_FN_NS_DATE_HOUR(ms),
        __RTS_FN_NS_DATE_MINUTE(ms),
        __RTS_FN_NS_DATE_SECOND(ms),
    );
    let dow = __RTS_FN_NS_DATE_WEEKDAY(ms);
    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        DAY_NAMES[dow.clamp(0, 6) as usize],
        day,
        MON_NAMES[month.clamp(0, 11) as usize],
        year,
        hour,
        minute,
        second,
    )
}

/// `toDateString()` — JS spec `Sat Jun 15 2024`.
pub(crate) fn date_string(ms: i64) -> String {
    use crate::date::*;
    let (year, month, day) = (
        __RTS_FN_NS_DATE_YEAR(ms),
        __RTS_FN_NS_DATE_MONTH(ms),
        __RTS_FN_NS_DATE_DAY(ms),
    );
    let dow = __RTS_FN_NS_DATE_WEEKDAY(ms);
    format!(
        "{} {} {:02} {:04}",
        DAY_NAMES[dow.clamp(0, 6) as usize],
        MON_NAMES[month.clamp(0, 11) as usize],
        day,
        year,
    )
}

/// `toTimeString()` / `toLocaleTimeString()` share this: the ISO string's tail
/// after the `T`.
pub(crate) fn time_string(ms: i64) -> String {
    let s = iso(ms);
    match s.split_once('T') {
        Some((_, rest)) => rest.to_string(),
        None => s,
    }
}

/// `toLocaleDateString()` — default `DD/MM/YYYY` (pt-BR-style, no Intl).
pub(crate) fn locale_date(ms: i64) -> String {
    use crate::date::*;
    let year = __RTS_FN_NS_DATE_YEAR(ms);
    // getMonth() is 0..11; locale strings are 1-based (01..12).
    let month = __RTS_FN_NS_DATE_MONTH(ms) + 1;
    let day = __RTS_FN_NS_DATE_DAY(ms);
    format!("{day:02}/{month:02}/{year:04}")
}

/// `toLocaleString()` — `DD/MM/YYYY, HH:MM:SS`.
pub(crate) fn locale(ms: i64) -> String {
    use crate::date::*;
    let year = __RTS_FN_NS_DATE_YEAR(ms);
    let month = __RTS_FN_NS_DATE_MONTH(ms) + 1;
    let day = __RTS_FN_NS_DATE_DAY(ms);
    let (hour, minute, second) = (
        __RTS_FN_NS_DATE_HOUR(ms),
        __RTS_FN_NS_DATE_MINUTE(ms),
        __RTS_FN_NS_DATE_SECOND(ms),
    );
    format!("{day:02}/{month:02}/{year:04}, {hour:02}:{minute:02}:{second:02}")
}

/// `toLocaleTimeString()` — `HH:MM:SS` (no ms, no tz).
pub(crate) fn locale_time(ms: i64) -> String {
    use crate::date::*;
    let (hour, minute, second) = (
        __RTS_FN_NS_DATE_HOUR(ms),
        __RTS_FN_NS_DATE_MINUTE(ms),
        __RTS_FN_NS_DATE_SECOND(ms),
    );
    format!("{hour:02}:{minute:02}:{second:02}")
}
