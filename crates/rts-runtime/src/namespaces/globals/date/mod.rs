//! `Date` global class. Migrado ao modelo `#[rts_class]` (stage 2c) via membros
//! `external`: os externs `__RTS_FN_GL_DATE_*` ficam em `instance.rs` intactos e
//! os estáticos (`now`/`parse`/`UTC`) reusam `__RTS_FN_NS_DATE_*` do namespace
//! `date`; o macro deriva apenas o `CLASS_SPEC`. setUTC* e toGMTString são
//! aliases que compartilham os símbolos dos set*/toUTCString (override `symbol=`).

pub mod instance;

#[allow(unused_imports)]
use rts_engine::abi::ty::{Handle, Str, F64, I64};
use rts_macro::rts_class;

/// Built-in Date class. Stores UTC timestamp as ms since Unix epoch.
#[rts_class(Date, prefix = "DATE", spec = "CLASS_SPEC")]
impl DateClass {
    // ── Static methods ──────────────────────────────────────────────────────
    /// Returns the current time as milliseconds since Unix epoch (UTC).
    #[rts_fn(
        external,
        name = "now",
        symbol = "__RTS_FN_NS_DATE_NOW_MS",
        ts = "now(): number"
    )]
    pub fn now() -> I64 {
        unreachable!()
    }
    /// Parses an ISO 8601 string to ms since epoch. Returns NaN on failure (JS spec).
    #[rts_fn(
        external,
        name = "parse",
        symbol = "__RTS_FN_NS_DATE_PARSE_F64",
        ts = "parse(dateString: string): number",
        pure
    )]
    pub fn parse(_date_string: Str) -> F64 {
        unreachable!()
    }
    /// Date.UTC(year, month, day?, hour?, min?, sec?, ms?) — ms since epoch.
    #[rts_fn(
        external,
        name = "UTC",
        symbol = "__RTS_FN_NS_DATE_FROM_PARTS",
        ts = "UTC(year: number, month: number, day?: number, hour?: number, min?: number, sec?: number, ms?: number): number",
        pure
    )]
    pub fn utc(_y: I64, _mo: I64, _d: I64, _h: I64, _mi: I64, _s: I64, _ms: I64) -> I64 {
        unreachable!()
    }

    // ── Constructors (distinguished by arity / arg type) ────────────────────
    /// Creates a Date representing the current instant.
    #[rts_ctor(external, symbol = "__RTS_FN_GL_DATE_NEW_NOW", ts = "new Date(): Date")]
    pub fn new_now() -> Handle {
        unreachable!()
    }
    /// Creates a Date from milliseconds since Unix epoch.
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_DATE_NEW_FROM_MS",
        ts = "new Date(value: number): Date"
    )]
    pub fn new_from_ms(_ms: I64) -> Handle {
        unreachable!()
    }
    /// Creates a Date from an ISO 8601 string.
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_DATE_NEW_FROM_ISO",
        ts = "new Date(dateString: string): Date"
    )]
    pub fn new_from_iso(_iso: Str) -> Handle {
        unreachable!()
    }
    /// Creates a Date from year/month/day/hour/min/sec/ms fields (month is 0-indexed).
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_DATE_NEW_FROM_FIELDS",
        ts = "new Date(year: number, month: number, day?: number, hour?: number, min?: number, sec?: number, ms?: number): Date"
    )]
    pub fn new_from_fields(
        _y: F64,
        _mo: F64,
        _d: F64,
        _h: F64,
        _mi: F64,
        _s: F64,
        _ms: F64,
    ) -> Handle {
        unreachable!()
    }

    // ── Instance methods ────────────────────────────────────────────────────
    /// Returns ms since epoch.
    #[rts_method(external, name = "getTime", ts = "getTime(): number", pure)]
    pub fn get_time(_h: Handle) -> I64 {
        unreachable!()
    }
    /// Same as getTime().
    #[rts_method(external, name = "valueOf", ts = "valueOf(): number", pure)]
    pub fn value_of(_h: Handle) -> I64 {
        unreachable!()
    }
    /// Full year (UTC).
    #[rts_method(external, name = "getFullYear", ts = "getFullYear(): number", pure)]
    pub fn get_full_year(_h: Handle) -> I64 {
        unreachable!()
    }
    /// Month 0-indexed (UTC). Jan=0, Dec=11.
    #[rts_method(external, name = "getMonth", ts = "getMonth(): number", pure)]
    pub fn get_month(_h: Handle) -> I64 {
        unreachable!()
    }
    /// Day of month 1-31 (UTC).
    #[rts_method(external, name = "getDate", ts = "getDate(): number", pure)]
    pub fn get_date(_h: Handle) -> I64 {
        unreachable!()
    }
    /// Day of week 0-6 (UTC). Sunday=0.
    #[rts_method(external, name = "getDay", ts = "getDay(): number", pure)]
    pub fn get_day(_h: Handle) -> I64 {
        unreachable!()
    }
    /// Hour 0-23 (UTC).
    #[rts_method(external, name = "getHours", ts = "getHours(): number", pure)]
    pub fn get_hours(_h: Handle) -> I64 {
        unreachable!()
    }
    /// Minute 0-59 (UTC).
    #[rts_method(external, name = "getMinutes", ts = "getMinutes(): number", pure)]
    pub fn get_minutes(_h: Handle) -> I64 {
        unreachable!()
    }
    /// Second 0-59 (UTC).
    #[rts_method(external, name = "getSeconds", ts = "getSeconds(): number", pure)]
    pub fn get_seconds(_h: Handle) -> I64 {
        unreachable!()
    }
    /// Millisecond 0-999 (UTC).
    #[rts_method(
        external,
        name = "getMilliseconds",
        ts = "getMilliseconds(): number",
        pure
    )]
    pub fn get_milliseconds(_h: Handle) -> I64 {
        unreachable!()
    }
    /// ISO 8601 UTC string.
    #[rts_method(external, name = "toISOString", ts = "toISOString(): string", pure)]
    pub fn to_iso_string(_h: Handle) -> Handle {
        unreachable!()
    }
    /// String representation (ISO 8601 UTC).
    #[rts_method(external, name = "toString", ts = "toString(): string", pure)]
    pub fn to_string(_h: Handle) -> Handle {
        unreachable!()
    }
    /// Locale-agnostic date string (same as toISOString in v0).
    #[rts_method(
        external,
        name = "toLocaleDateString",
        ts = "toLocaleDateString(): string",
        pure
    )]
    pub fn to_locale_date_string(_h: Handle) -> Handle {
        unreachable!()
    }
    /// Full year in UTC.
    #[rts_method(
        external,
        name = "getUTCFullYear",
        ts = "getUTCFullYear(): number",
        pure
    )]
    pub fn get_utc_full_year(_h: Handle) -> I64 {
        unreachable!()
    }
    /// Month 0-11 in UTC.
    #[rts_method(external, name = "getUTCMonth", ts = "getUTCMonth(): number", pure)]
    pub fn get_utc_month(_h: Handle) -> I64 {
        unreachable!()
    }
    /// Day of month 1-31 in UTC.
    #[rts_method(external, name = "getUTCDate", ts = "getUTCDate(): number", pure)]
    pub fn get_utc_date(_h: Handle) -> I64 {
        unreachable!()
    }
    /// Day of week 0-6 in UTC (Sunday=0).
    #[rts_method(external, name = "getUTCDay", ts = "getUTCDay(): number", pure)]
    pub fn get_utc_day(_h: Handle) -> I64 {
        unreachable!()
    }
    /// Hour 0-23 in UTC.
    #[rts_method(external, name = "getUTCHours", ts = "getUTCHours(): number", pure)]
    pub fn get_utc_hours(_h: Handle) -> I64 {
        unreachable!()
    }
    /// Minute 0-59 in UTC.
    #[rts_method(external, name = "getUTCMinutes", ts = "getUTCMinutes(): number", pure)]
    pub fn get_utc_minutes(_h: Handle) -> I64 {
        unreachable!()
    }
    /// Second 0-59 in UTC.
    #[rts_method(external, name = "getUTCSeconds", ts = "getUTCSeconds(): number", pure)]
    pub fn get_utc_seconds(_h: Handle) -> I64 {
        unreachable!()
    }
    /// Millisecond 0-999 in UTC.
    #[rts_method(
        external,
        name = "getUTCMilliseconds",
        ts = "getUTCMilliseconds(): number",
        pure
    )]
    pub fn get_utc_milliseconds(_h: Handle) -> I64 {
        unreachable!()
    }
    /// Minutos entre UTC e local. RTS sempre 0.
    #[rts_method(
        external,
        name = "getTimezoneOffset",
        ts = "getTimezoneOffset(): number",
        pure
    )]
    pub fn get_timezone_offset(_h: Handle) -> I64 {
        unreachable!()
    }
    /// UTC string (alias de toISOString em v0).
    #[rts_method(external, name = "toUTCString", ts = "toUTCString(): string", pure)]
    pub fn to_utc_string(_h: Handle) -> Handle {
        unreachable!()
    }
    /// Deprecated alias de toUTCString.
    #[rts_method(
        external,
        name = "toGMTString",
        symbol = "__RTS_FN_GL_DATE_TO_UTC_STRING",
        ts = "toGMTString(): string",
        pure
    )]
    pub fn to_gmt_string(_h: Handle) -> Handle {
        unreachable!()
    }
    /// Date portion (YYYY-MM-DD) em UTC.
    #[rts_method(external, name = "toDateString", ts = "toDateString(): string", pure)]
    pub fn to_date_string(_h: Handle) -> Handle {
        unreachable!()
    }
    // ── Setters ──────────────────────────────────────────────────────────────
    /// Substitui o ano. Retorna ms novos.
    #[rts_method(
        external,
        name = "setFullYear",
        ts = "setFullYear(year: number): number"
    )]
    pub fn set_full_year(_h: Handle, _v: I64) -> I64 {
        unreachable!()
    }
    /// Substitui o mes (0-11). Retorna ms novos.
    #[rts_method(external, name = "setMonth", ts = "setMonth(month: number): number")]
    pub fn set_month(_h: Handle, _v: I64) -> I64 {
        unreachable!()
    }
    /// Substitui o dia do mes (1-31). Retorna ms novos.
    #[rts_method(external, name = "setDate", ts = "setDate(day: number): number")]
    pub fn set_date(_h: Handle, _v: I64) -> I64 {
        unreachable!()
    }
    /// Substitui hour (0-23).
    #[rts_method(external, name = "setHours", ts = "setHours(hour: number): number")]
    pub fn set_hours(_h: Handle, _v: I64) -> I64 {
        unreachable!()
    }
    /// Substitui min (0-59).
    #[rts_method(external, name = "setMinutes", ts = "setMinutes(min: number): number")]
    pub fn set_minutes(_h: Handle, _v: I64) -> I64 {
        unreachable!()
    }
    /// Substitui sec (0-59).
    #[rts_method(external, name = "setSeconds", ts = "setSeconds(sec: number): number")]
    pub fn set_seconds(_h: Handle, _v: I64) -> I64 {
        unreachable!()
    }
    /// Substitui ms (0-999).
    #[rts_method(
        external,
        name = "setMilliseconds",
        ts = "setMilliseconds(ms: number): number"
    )]
    pub fn set_milliseconds(_h: Handle, _v: I64) -> I64 {
        unreachable!()
    }
    /// Substitui timestamp completo (ms desde epoch).
    #[rts_method(external, name = "setTime", ts = "setTime(ms: number): number")]
    pub fn set_time(_h: Handle, _v: I64) -> I64 {
        unreachable!()
    }
    /// Substitui o ano (UTC). Retorna ms novos.
    #[rts_method(
        external,
        name = "setUTCFullYear",
        symbol = "__RTS_FN_GL_DATE_SET_FULL_YEAR",
        ts = "setUTCFullYear(year: number): number"
    )]
    pub fn set_utc_full_year(_h: Handle, _v: I64) -> I64 {
        unreachable!()
    }
    /// Substitui o mes (0-11, UTC).
    #[rts_method(
        external,
        name = "setUTCMonth",
        symbol = "__RTS_FN_GL_DATE_SET_MONTH",
        ts = "setUTCMonth(month: number): number"
    )]
    pub fn set_utc_month(_h: Handle, _v: I64) -> I64 {
        unreachable!()
    }
    /// Substitui o dia do mes (1-31, UTC).
    #[rts_method(
        external,
        name = "setUTCDate",
        symbol = "__RTS_FN_GL_DATE_SET_DATE",
        ts = "setUTCDate(day: number): number"
    )]
    pub fn set_utc_date(_h: Handle, _v: I64) -> I64 {
        unreachable!()
    }
    /// Substitui hour (0-23, UTC).
    #[rts_method(
        external,
        name = "setUTCHours",
        symbol = "__RTS_FN_GL_DATE_SET_HOURS",
        ts = "setUTCHours(hour: number): number"
    )]
    pub fn set_utc_hours(_h: Handle, _v: I64) -> I64 {
        unreachable!()
    }
    /// Substitui min (0-59, UTC).
    #[rts_method(
        external,
        name = "setUTCMinutes",
        symbol = "__RTS_FN_GL_DATE_SET_MINUTES",
        ts = "setUTCMinutes(min: number): number"
    )]
    pub fn set_utc_minutes(_h: Handle, _v: I64) -> I64 {
        unreachable!()
    }
    /// Substitui sec (0-59, UTC).
    #[rts_method(
        external,
        name = "setUTCSeconds",
        symbol = "__RTS_FN_GL_DATE_SET_SECONDS",
        ts = "setUTCSeconds(sec: number): number"
    )]
    pub fn set_utc_seconds(_h: Handle, _v: I64) -> I64 {
        unreachable!()
    }
    /// Substitui ms (0-999, UTC).
    #[rts_method(
        external,
        name = "setUTCMilliseconds",
        symbol = "__RTS_FN_GL_DATE_SET_MILLISECONDS",
        ts = "setUTCMilliseconds(ms: number): number"
    )]
    pub fn set_utc_milliseconds(_h: Handle, _v: I64) -> I64 {
        unreachable!()
    }
    // ── Conversion extras ────────────────────────────────────────────────────
    /// JSON serialization — alias de toISOString.
    #[rts_method(external, name = "toJSON", ts = "toJSON(): string", pure)]
    pub fn to_json(_h: Handle) -> Handle {
        unreachable!()
    }
    /// Locale string (sem locale support em v0).
    #[rts_method(
        external,
        name = "toLocaleString",
        ts = "toLocaleString(): string",
        pure
    )]
    pub fn to_locale_string(_h: Handle) -> Handle {
        unreachable!()
    }
    /// Time portion (HH:MM:SS.mmmZ) — alias de toTimeString.
    #[rts_method(
        external,
        name = "toLocaleTimeString",
        ts = "toLocaleTimeString(): string",
        pure
    )]
    pub fn to_locale_time_string(_h: Handle) -> Handle {
        unreachable!()
    }
    /// Time portion (HH:MM:SS.mmmZ) do ISO.
    #[rts_method(external, name = "toTimeString", ts = "toTimeString(): string", pure)]
    pub fn to_time_string(_h: Handle) -> Handle {
        unreachable!()
    }
}
