//! `Date` global class — authored as a normal Rust struct + `#[rtse::class]`.
//!
//! Migrated (DRAIN_MOTOR) from the hand-written `Member` table + per-method
//! `#[no_mangle] extern "C"` fns (over the bespoke `Entry::DateMs(i64)` storage)
//! to the `#[rtse::class]` authoring macro. The instance is now a normal Rust
//! struct (`Date { ms }`) stored via the generic `Entry::Rtse` — the same storage
//! every macro-authored class uses. All the real date math still runs through the
//! `date` namespace backend (`crate::date::__RTS_FN_NS_DATE_*`); nothing here
//! reimplements it.
//!
//! Macro-authored here: the four `new Date(...)` ctors (overloaded by arity and,
//! at arity 1, by arg type — `f64` ms vs `&str` ISO), the `now`/`parse`/`UTC`
//! statics, every getter (`getTime`/`valueOf`/`getFullYear`/… + the UTC variants
//! + `getTimezoneOffset`), the string methods (`toISOString`/`toString`/… /
//! `toJSON`/`toLocale*`), and the single-arg setters (`setTime`/`setDate`/
//! `setMilliseconds` + the `setUTCDate`/`setUTCMilliseconds` aliases).
//!
//! Left hand-written (appended by `super::register_class_spec` via the
//! class-reopen pattern): the MULTI-COMPONENT setters (`setFullYear(y, mo?, d?)`
//! …) — they use an `i64::MIN` "keep current" sentinel for omitted args, which is
//! the OPPOSITE of the macro's `optional=N` (undefined/NaN = "invalidate"), so the
//! macro cannot express them — plus their UTC aliases and `toGMTString`. See
//! `super::setters`.

use super::fmt;

/// A `Date` instance: milliseconds since the Unix epoch (UTC). A dedicated
/// `#[rtse::class("Date")]` struct stored via the generic `Entry::Rtse`. `ms` is
/// `pub` so the runtime-layer consumers that must read a Date's timestamp
/// directly (JSON snapshot, structuredClone, N-API, the ToPrimitive coercion
/// trio in `super::coercion`) can `with_rtse::<Date, _>(h, |d| d.ms)`.
#[rtse::class("Date")]
#[derive(Clone)]
pub struct Date {
    pub ms: i64,
}

/// Sentinel for the INVALID DATE state (JS: a NaN time value). `i64::MIN` is never
/// a valid timestamp (the JS range is ±8.64e15 ms) and is the same value
/// `DATE_FROM_ISO` returns on a failed parse.
pub(crate) const INVALID_MS: i64 = i64::MIN;

/// The spec's valid time-value range (±8.64e15 ms). Outside it → Invalid.
pub(crate) fn clamp_time_value(ms: f64) -> i64 {
    const MAX: f64 = 8.64e15;
    if ms.is_finite() && ms.abs() <= MAX {
        ms as i64
    } else {
        INVALID_MS
    }
}

/// A setter component: the OMITTED sentinel (`i64::MIN as f64`) keeps the current
/// field; `NaN` INVALIDATES (`None`); a number uses its truncation.
pub(crate) fn keep_f(v: f64, cur: i64) -> Option<i64> {
    if v == i64::MIN as f64 {
        Some(cur)
    } else if v.is_nan() {
        None
    } else {
        Some(v as i64)
    }
}

/// The calendar parts of a ms timestamp (year, month, day, hour, min, sec, ms).
pub(crate) fn ms_to_parts(ms: i64) -> (i64, i64, i64, i64, i64, i64, i64) {
    use crate::date::*;
    (
        __RTS_FN_NS_DATE_YEAR(ms),
        __RTS_FN_NS_DATE_MONTH(ms),
        __RTS_FN_NS_DATE_DAY(ms),
        __RTS_FN_NS_DATE_HOUR(ms),
        __RTS_FN_NS_DATE_MINUTE(ms),
        __RTS_FN_NS_DATE_SECOND(ms),
        __RTS_FN_NS_DATE_MILLISECOND(ms),
    )
}

/// Local offset in seconds for a ms-since-epoch timestamp. Best-effort: uses the
/// system's current offset (no DST history). `0` (UTC) on failure.
pub(crate) fn local_offset_seconds_at(_ms: i64) -> i32 {
    match time::UtcOffset::current_local_offset() {
        Ok(o) => o.whole_seconds(),
        Err(_) => 0,
    }
}

impl Date {
    /// The numeric time value (`getTime`/`valueOf`): `NaN` for an Invalid Date.
    fn time_value(&self) -> f64 {
        if self.ms == INVALID_MS {
            f64::NAN
        } else {
            self.ms as f64
        }
    }

    /// The setter base parts: those of the current instant, or — for an Invalid
    /// Date — those of `t = +0` (spec 21.4.4.*: a complete setter REVIVES it).
    fn setter_base_parts(&self) -> (i64, i64, i64, i64, i64, i64, i64) {
        if self.ms == INVALID_MS {
            (1970, 0, 1, 0, 0, 0, 0)
        } else {
            ms_to_parts(self.ms)
        }
    }

    /// Close a single-component setter: `None` → INVALID + NaN; else store the
    /// recomposed ms and return it as `f64`.
    fn finish_setter(&mut self, parts: Option<(i64, i64, i64, i64, i64, i64, i64)>) -> f64 {
        match parts {
            Some((y, mo, d, h, mi, s, ms)) => {
                let new_ms = crate::date::__RTS_FN_NS_DATE_FROM_PARTS(y, mo, d, h, mi, s, ms);
                self.ms = new_ms;
                new_ms as f64
            }
            None => {
                self.ms = INVALID_MS;
                f64::NAN
            }
        }
    }
}

#[rtse::class("Date")]
impl Date {
    // ── Constructors (overloaded by arity; arity 1 by arg type) ──────────────

    /// `new Date()` — the current instant.
    #[rtse::ctor]
    fn new_now() -> Self {
        Date {
            ms: crate::date::__RTS_FN_NS_DATE_NOW_MS(),
        }
    }

    /// `new Date(value)` — from ms since epoch. NaN / out-of-range → Invalid.
    #[rtse::ctor]
    fn new_from_ms(value: f64) -> Self {
        Date {
            ms: clamp_time_value(value),
        }
    }

    /// `new Date(dateString)` — from an ISO 8601 string.
    #[rtse::ctor]
    fn new_from_iso(s: &str) -> Self {
        Date {
            ms: crate::date::__RTS_FN_NS_DATE_FROM_ISO(s.as_ptr(), s.len() as i64),
        }
    }

    /// `new Date(year, month, day?, hour?, min?, sec?, ms?)` — component ctor
    /// (month 0-indexed). Omitted tail components (undefined → NaN) default to
    /// JS: day=1, rest=0. Components are LOCAL time (cross-runtime #172): the
    /// packed pretend-UTC ms is converted with the local offset at that date.
    #[rtse::ctor(optional = 5)]
    fn new_from_fields(
        year: f64,
        month: f64,
        day: f64,
        hour: f64,
        minute: f64,
        second: f64,
        ms: f64,
    ) -> Self {
        let y = year as i64;
        let mo = month as i64;
        let d = if day.is_nan() { 1 } else { day as i64 };
        let h = if hour.is_nan() { 0 } else { hour as i64 };
        let mi = if minute.is_nan() { 0 } else { minute as i64 };
        let s = if second.is_nan() { 0 } else { second as i64 };
        let frac_ms = if ms.is_nan() { 0 } else { ms as i64 };
        let pretend_utc = crate::date::__RTS_FN_NS_DATE_FROM_PARTS(y, mo, d, h, mi, s, frac_ms);
        let local_offset_secs = local_offset_seconds_at(pretend_utc);
        Date {
            ms: pretend_utc - (local_offset_secs as i64) * 1000,
        }
    }

    // ── Statics ──────────────────────────────────────────────────────────────

    /// `Date.now()` — current ms since epoch (UTC).
    #[rtse::statical(name = "now")]
    fn now() -> i64 {
        crate::date::__RTS_FN_NS_DATE_NOW_MS()
    }

    /// `Date.parse(s)` — ISO 8601 → ms, or NaN on failure (JS spec).
    #[rtse::statical(name = "parse")]
    fn parse(s: &str) -> f64 {
        crate::date::__RTS_FN_NS_DATE_PARSE_F64(s.as_ptr() as u64, s.len() as i64)
    }

    /// `Date.UTC(year, month, day?, hour?, min?, sec?, ms?)` — ms since epoch.
    /// Omitted tail (undefined → NaN) defaults to day=1, rest=0.
    #[rtse::statical(name = "UTC", optional = 5)]
    fn utc(year: f64, month: f64, day: f64, hour: f64, minute: f64, second: f64, ms: f64) -> i64 {
        let y = year as i64;
        let mo = month as i64;
        let d = if day.is_nan() { 1 } else { day as i64 };
        let h = if hour.is_nan() { 0 } else { hour as i64 };
        let mi = if minute.is_nan() { 0 } else { minute as i64 };
        let s = if second.is_nan() { 0 } else { second as i64 };
        let frac_ms = if ms.is_nan() { 0 } else { ms as i64 };
        crate::date::__RTS_FN_NS_DATE_FROM_PARTS(y, mo, d, h, mi, s, frac_ms)
    }

    // ── Getters (instance methods called with parens) ─────────────────────────

    /// `getTime()` — ms since epoch (NaN for an Invalid Date).
    #[rtse::method(name = "getTime")]
    fn get_time(&self) -> f64 {
        self.time_value()
    }

    /// `valueOf()` — same as `getTime()`.
    #[rtse::method(name = "valueOf")]
    fn value_of(&self) -> f64 {
        self.time_value()
    }

    /// `getFullYear()` — full year (UTC).
    #[rtse::method(name = "getFullYear")]
    fn get_full_year(&self) -> i64 {
        crate::date::__RTS_FN_NS_DATE_YEAR(self.ms)
    }

    /// `getMonth()` — month 0..11 (UTC).
    #[rtse::method(name = "getMonth")]
    fn get_month(&self) -> i64 {
        crate::date::__RTS_FN_NS_DATE_MONTH(self.ms)
    }

    /// `getDate()` — day of month 1..31 (UTC).
    #[rtse::method(name = "getDate")]
    fn get_date(&self) -> i64 {
        crate::date::__RTS_FN_NS_DATE_DAY(self.ms)
    }

    /// `getDay()` — day of week 0..6 (UTC, Sunday=0).
    #[rtse::method(name = "getDay")]
    fn get_day(&self) -> i64 {
        crate::date::__RTS_FN_NS_DATE_WEEKDAY(self.ms)
    }

    /// `getHours()` — hour 0..23 (UTC).
    #[rtse::method(name = "getHours")]
    fn get_hours(&self) -> i64 {
        crate::date::__RTS_FN_NS_DATE_HOUR(self.ms)
    }

    /// `getMinutes()` — minute 0..59 (UTC).
    #[rtse::method(name = "getMinutes")]
    fn get_minutes(&self) -> i64 {
        crate::date::__RTS_FN_NS_DATE_MINUTE(self.ms)
    }

    /// `getSeconds()` — second 0..59 (UTC).
    #[rtse::method(name = "getSeconds")]
    fn get_seconds(&self) -> i64 {
        crate::date::__RTS_FN_NS_DATE_SECOND(self.ms)
    }

    /// `getMilliseconds()` — millisecond 0..999 (UTC).
    #[rtse::method(name = "getMilliseconds")]
    fn get_milliseconds(&self) -> i64 {
        crate::date::__RTS_FN_NS_DATE_MILLISECOND(self.ms)
    }

    // RTS stores ms in UTC, so getUTCX == getX (aliases until local-tz support).

    /// `getUTCFullYear()`.
    #[rtse::method(name = "getUTCFullYear")]
    fn get_utc_full_year(&self) -> i64 {
        crate::date::__RTS_FN_NS_DATE_YEAR(self.ms)
    }

    /// `getUTCMonth()`.
    #[rtse::method(name = "getUTCMonth")]
    fn get_utc_month(&self) -> i64 {
        crate::date::__RTS_FN_NS_DATE_MONTH(self.ms)
    }

    /// `getUTCDate()`.
    #[rtse::method(name = "getUTCDate")]
    fn get_utc_date(&self) -> i64 {
        crate::date::__RTS_FN_NS_DATE_DAY(self.ms)
    }

    /// `getUTCDay()`.
    #[rtse::method(name = "getUTCDay")]
    fn get_utc_day(&self) -> i64 {
        crate::date::__RTS_FN_NS_DATE_WEEKDAY(self.ms)
    }

    /// `getUTCHours()`.
    #[rtse::method(name = "getUTCHours")]
    fn get_utc_hours(&self) -> i64 {
        crate::date::__RTS_FN_NS_DATE_HOUR(self.ms)
    }

    /// `getUTCMinutes()`.
    #[rtse::method(name = "getUTCMinutes")]
    fn get_utc_minutes(&self) -> i64 {
        crate::date::__RTS_FN_NS_DATE_MINUTE(self.ms)
    }

    /// `getUTCSeconds()`.
    #[rtse::method(name = "getUTCSeconds")]
    fn get_utc_seconds(&self) -> i64 {
        crate::date::__RTS_FN_NS_DATE_SECOND(self.ms)
    }

    /// `getUTCMilliseconds()`.
    #[rtse::method(name = "getUTCMilliseconds")]
    fn get_utc_milliseconds(&self) -> i64 {
        crate::date::__RTS_FN_NS_DATE_MILLISECOND(self.ms)
    }

    /// `getTimezoneOffset()` — (local − UTC) minutes, NEGATED (JS spec). RTS is
    /// always UTC unless the system reports a local offset.
    #[rtse::method(name = "getTimezoneOffset")]
    fn get_timezone_offset(&self) -> i64 {
        let secs = local_offset_seconds_at(self.ms);
        -(secs as i64) / 60
    }

    // ── String methods ────────────────────────────────────────────────────────

    /// `toISOString()` — ISO 8601 UTC.
    #[rtse::method(name = "toISOString")]
    fn to_iso_string(&self) -> String {
        fmt::iso(self.ms)
    }

    /// `toString()` — JS spec "Day Mon DD YYYY HH:MM:SS GMT+0000 (…)".
    #[rtse::method(name = "toString")]
    fn to_string(&self) -> String {
        fmt::to_string(self.ms)
    }

    /// `toUTCString()` — RFC 1123.
    #[rtse::method(name = "toUTCString")]
    fn to_utc_string(&self) -> String {
        fmt::utc(self.ms)
    }

    /// `toDateString()` — `Sat Jun 15 2024`.
    #[rtse::method(name = "toDateString")]
    fn to_date_string(&self) -> String {
        fmt::date_string(self.ms)
    }

    /// `toTimeString()` — the ISO string's time portion (HH:MM:SS.mmmZ).
    #[rtse::method(name = "toTimeString")]
    fn to_time_string(&self) -> String {
        fmt::time_string(self.ms)
    }

    /// `toJSON()` — alias of `toISOString()`.
    #[rtse::method(name = "toJSON")]
    fn to_json(&self) -> String {
        fmt::iso(self.ms)
    }

    /// `toLocaleDateString()` — `DD/MM/YYYY` (no Intl).
    #[rtse::method(name = "toLocaleDateString")]
    fn to_locale_date_string(&self) -> String {
        fmt::locale_date(self.ms)
    }

    /// `toLocaleString()` — `DD/MM/YYYY, HH:MM:SS` (no Intl).
    #[rtse::method(name = "toLocaleString")]
    fn to_locale_string(&self) -> String {
        fmt::locale(self.ms)
    }

    /// `toLocaleTimeString()` — `HH:MM:SS`.
    #[rtse::method(name = "toLocaleTimeString")]
    fn to_locale_time_string(&self) -> String {
        fmt::locale_time(self.ms)
    }

    // ── Single-component setters (macro-expressible) ──────────────────────────

    /// `setTime(ms)` — replace the whole timestamp. Returns the new ms.
    #[rtse::method(name = "setTime")]
    fn set_time(&mut self, ms_in: f64) -> f64 {
        let v = clamp_time_value(ms_in);
        self.ms = v;
        if v == INVALID_MS {
            f64::NAN
        } else {
            v as f64
        }
    }

    /// `setDate(day)` — replace the day of month (1..31). Returns the new ms.
    #[rtse::method(name = "setDate")]
    fn set_date(&mut self, day: f64) -> f64 {
        let (y, mo, _, h, mi, s, ms) = self.setter_base_parts();
        let parts = (|| Some((y, mo, keep_f(day, 0)?, h, mi, s, ms)))();
        self.finish_setter(parts)
    }

    /// `setMilliseconds(ms)` — replace the millisecond (0..999).
    #[rtse::method(name = "setMilliseconds")]
    fn set_milliseconds(&mut self, ms_in: f64) -> f64 {
        let (y, mo, d, h, mi, s, _) = self.setter_base_parts();
        let parts = (|| Some((y, mo, d, h, mi, s, keep_f(ms_in, 0)?)))();
        self.finish_setter(parts)
    }

    /// `setUTCDate(day)` — RTS is UTC, so identical to `setDate`.
    #[rtse::method(name = "setUTCDate")]
    fn set_utc_date(&mut self, day: f64) -> f64 {
        self.set_date(day)
    }

    /// `setUTCMilliseconds(ms)` — identical to `setMilliseconds`.
    #[rtse::method(name = "setUTCMilliseconds")]
    fn set_utc_milliseconds(&mut self, ms_in: f64) -> f64 {
        self.set_milliseconds(ms_in)
    }
}
