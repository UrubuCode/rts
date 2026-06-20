//! `Date` global class. Migrado do `#[rts_class]` (macro) pro modelo builder
//! hand-written do `rts-engine` (rumo à remoção da `rts-macro`). Todos os
//! membros são `external`: os externs `__RTS_FN_GL_DATE_*` ficam em `instance.rs`
//! intactos e os estáticos (`now`/`parse`/`UTC`) reusam `__RTS_FN_NS_DATE_*` do
//! namespace `date`; nenhum extern é emitido aqui (todos fn_ptr = null). setUTC*
//! e toGMTString são aliases que compartilham os símbolos dos set*/toUTCString.

pub mod instance;

use rts_engine::{AbiType, DefaultArg, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Membro de classe global (helper hand-written, espelha `leak_class` da macro).
#[allow(clippy::too_many_arguments)]
fn m(
    name: &str,
    kind: MemberKind,
    sig: Sig,
    symbol: &str,
    ts: &str,
    doc: &str,
    pure: bool,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(std::ptr::null()),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        intrinsic: None,
    }
}

/// Como [`m`], mas marca o membro `MemberFlags::UNSOUND`: resolve no Registry mas
/// NÃO é seguro de lowerar (a honesty floor vira DADO no spec). Usado nos
/// formatadores timezone-divergentes (`toString`/`toLocale*`/`toDateString`/
/// `toUTCString`/`toGMTString`/`toTimeString`) e em todos os mutadores `setX` —
/// o codegen dá bail genérico via a flag, sem predicado hardcoded por classe.
#[allow(clippy::too_many_arguments)]
fn m_unsound(
    name: &str,
    kind: MemberKind,
    sig: Sig,
    symbol: &str,
    ts: &str,
    doc: &str,
    pure: bool,
) -> Member {
    Member {
        flags: MemberFlags::UNSOUND,
        ..m(name, kind, sig, symbol, ts, doc, pure)
    }
}

/// Registra a classe global `Date` no motor (hand-written, sem macro).
/// Stores UTC timestamp as ms since Unix epoch.
pub fn register_class_spec(e: &mut Engine) {
    e.class("Date")
        .doc("Built-in Date class. Stores UTC timestamp as ms since Unix epoch.")
        // ── Static methods ──────────────────────────────────────────────────
        .member(m(
            "now",
            MemberKind::Function,
            Sig::new(Vec::new(), AbiType::I64),
            "__RTS_FN_NS_DATE_NOW_MS",
            "now(): number",
            "Returns the current time as milliseconds since Unix epoch (UTC).",
            false,
        ))
        .member(m(
            "parse",
            MemberKind::Function,
            Sig::new(vec![AbiType::StrPtr], AbiType::F64),
            "__RTS_FN_NS_DATE_PARSE_F64",
            "parse(dateString: string): number",
            "Parses an ISO 8601 string to ms since epoch. Returns NaN on failure (JS spec).",
            true,
        ))
        .member(m(
            "UTC",
            MemberKind::Function,
            // 7×I64 -> I64, mas só year+month são requeridos. Os defaults JS do
            // tail (day=1, hour/min/sec/ms=0) viram DADO no spec: o emissor
            // genérico de estáticos (registryclass) padda o tail omitido a partir
            // daqui, em vez de hardcodar um `pad_utc_defaults` no codegen.
            Sig::with_defaults(
                vec![
                    AbiType::I64,
                    AbiType::I64,
                    AbiType::I64,
                    AbiType::I64,
                    AbiType::I64,
                    AbiType::I64,
                    AbiType::I64,
                ],
                AbiType::I64,
                vec![
                    DefaultArg::Required, // year
                    DefaultArg::Required, // month
                    DefaultArg::Int(1),   // day
                    DefaultArg::Int(0),   // hour
                    DefaultArg::Int(0),   // min
                    DefaultArg::Int(0),   // sec
                    DefaultArg::Int(0),   // ms
                ],
            ),
            "__RTS_FN_NS_DATE_FROM_PARTS",
            "UTC(year: number, month: number, day?: number, hour?: number, min?: number, sec?: number, ms?: number): number",
            "Date.UTC(year, month, day?, hour?, min?, sec?, ms?) — ms since epoch.",
            true,
        ))
        // ── Constructors (distinguished by arity / arg type) ─────────────────
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(Vec::new(), AbiType::Handle),
            "__RTS_FN_GL_DATE_NEW_NOW",
            "new Date(): Date",
            "Creates a Date representing the current instant.",
            false,
        ))
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::I64], AbiType::Handle),
            "__RTS_FN_GL_DATE_NEW_FROM_MS",
            "new Date(value: number): Date",
            "Creates a Date from milliseconds since Unix epoch.",
            false,
        ))
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_DATE_NEW_FROM_ISO",
            "new Date(dateString: string): Date",
            "Creates a Date from an ISO 8601 string.",
            false,
        ))
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(
                vec![
                    AbiType::F64,
                    AbiType::F64,
                    AbiType::F64,
                    AbiType::F64,
                    AbiType::F64,
                    AbiType::F64,
                    AbiType::F64,
                ],
                AbiType::Handle,
            ),
            "__RTS_FN_GL_DATE_NEW_FROM_FIELDS",
            "new Date(year: number, month: number, day?: number, hour?: number, min?: number, sec?: number, ms?: number): Date",
            "Creates a Date from year/month/day/hour/min/sec/ms fields (month is 0-indexed).",
            false,
        ))
        // ── Instance methods ─────────────────────────────────────────────────
        .member(m(
            "getTime",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_GET_TIME",
            "getTime(): number",
            "Returns ms since epoch.",
            true,
        ))
        .member(m(
            "valueOf",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_VALUE_OF",
            "valueOf(): number",
            "Same as getTime().",
            true,
        ))
        .member(m(
            "getFullYear",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_GET_FULL_YEAR",
            "getFullYear(): number",
            "Full year (UTC).",
            true,
        ))
        .member(m(
            "getMonth",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_GET_MONTH",
            "getMonth(): number",
            "Month 0-indexed (UTC). Jan=0, Dec=11.",
            true,
        ))
        .member(m(
            "getDate",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_GET_DATE",
            "getDate(): number",
            "Day of month 1-31 (UTC).",
            true,
        ))
        .member(m(
            "getDay",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_GET_DAY",
            "getDay(): number",
            "Day of week 0-6 (UTC). Sunday=0.",
            true,
        ))
        .member(m(
            "getHours",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_GET_HOURS",
            "getHours(): number",
            "Hour 0-23 (UTC).",
            true,
        ))
        .member(m(
            "getMinutes",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_GET_MINUTES",
            "getMinutes(): number",
            "Minute 0-59 (UTC).",
            true,
        ))
        .member(m(
            "getSeconds",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_GET_SECONDS",
            "getSeconds(): number",
            "Second 0-59 (UTC).",
            true,
        ))
        .member(m(
            "getMilliseconds",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_GET_MILLISECONDS",
            "getMilliseconds(): number",
            "Millisecond 0-999 (UTC).",
            true,
        ))
        .member(m(
            "toISOString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_DATE_TO_ISO_STRING",
            "toISOString(): string",
            "ISO 8601 UTC string.",
            true,
        ))
        .member(m_unsound(
            "toString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_DATE_TO_STRING",
            "toString(): string",
            "String representation (ISO 8601 UTC).",
            true,
        ))
        .member(m_unsound(
            "toLocaleDateString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_DATE_TO_LOCALE_DATE_STRING",
            "toLocaleDateString(): string",
            "Locale-agnostic date string (same as toISOString in v0).",
            true,
        ))
        .member(m(
            "getUTCFullYear",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_GET_UTC_FULL_YEAR",
            "getUTCFullYear(): number",
            "Full year in UTC.",
            true,
        ))
        .member(m(
            "getUTCMonth",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_GET_UTC_MONTH",
            "getUTCMonth(): number",
            "Month 0-11 in UTC.",
            true,
        ))
        .member(m(
            "getUTCDate",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_GET_UTC_DATE",
            "getUTCDate(): number",
            "Day of month 1-31 in UTC.",
            true,
        ))
        .member(m(
            "getUTCDay",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_GET_UTC_DAY",
            "getUTCDay(): number",
            "Day of week 0-6 in UTC (Sunday=0).",
            true,
        ))
        .member(m(
            "getUTCHours",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_GET_UTC_HOURS",
            "getUTCHours(): number",
            "Hour 0-23 in UTC.",
            true,
        ))
        .member(m(
            "getUTCMinutes",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_GET_UTC_MINUTES",
            "getUTCMinutes(): number",
            "Minute 0-59 in UTC.",
            true,
        ))
        .member(m(
            "getUTCSeconds",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_GET_UTC_SECONDS",
            "getUTCSeconds(): number",
            "Second 0-59 in UTC.",
            true,
        ))
        .member(m(
            "getUTCMilliseconds",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_GET_UTC_MILLISECONDS",
            "getUTCMilliseconds(): number",
            "Millisecond 0-999 in UTC.",
            true,
        ))
        .member(m(
            "getTimezoneOffset",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATE_GET_TIMEZONE_OFFSET",
            "getTimezoneOffset(): number",
            "Minutos entre UTC e local. RTS sempre 0.",
            true,
        ))
        .member(m_unsound(
            "toUTCString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_DATE_TO_UTC_STRING",
            "toUTCString(): string",
            "UTC string (alias de toISOString em v0).",
            true,
        ))
        .member(m_unsound(
            "toGMTString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_DATE_TO_UTC_STRING",
            "toGMTString(): string",
            "Deprecated alias de toUTCString.",
            true,
        ))
        .member(m_unsound(
            "toDateString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_DATE_TO_DATE_STRING",
            "toDateString(): string",
            "Date portion (YYYY-MM-DD) em UTC.",
            true,
        ))
        // ── Setters ──────────────────────────────────────────────────────────
        .member(m_unsound(
            "setFullYear",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_DATE_SET_FULL_YEAR",
            "setFullYear(year: number): number",
            "Substitui o ano. Retorna ms novos.",
            false,
        ))
        .member(m_unsound(
            "setMonth",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_DATE_SET_MONTH",
            "setMonth(month: number): number",
            "Substitui o mes (0-11). Retorna ms novos.",
            false,
        ))
        .member(m_unsound(
            "setDate",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_DATE_SET_DATE",
            "setDate(day: number): number",
            "Substitui o dia do mes (1-31). Retorna ms novos.",
            false,
        ))
        .member(m_unsound(
            "setHours",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_DATE_SET_HOURS",
            "setHours(hour: number): number",
            "Substitui hour (0-23).",
            false,
        ))
        .member(m_unsound(
            "setMinutes",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_DATE_SET_MINUTES",
            "setMinutes(min: number): number",
            "Substitui min (0-59).",
            false,
        ))
        .member(m_unsound(
            "setSeconds",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_DATE_SET_SECONDS",
            "setSeconds(sec: number): number",
            "Substitui sec (0-59).",
            false,
        ))
        .member(m_unsound(
            "setMilliseconds",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_DATE_SET_MILLISECONDS",
            "setMilliseconds(ms: number): number",
            "Substitui ms (0-999).",
            false,
        ))
        .member(m_unsound(
            "setTime",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_DATE_SET_TIME",
            "setTime(ms: number): number",
            "Substitui timestamp completo (ms desde epoch).",
            false,
        ))
        .member(m_unsound(
            "setUTCFullYear",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_DATE_SET_FULL_YEAR",
            "setUTCFullYear(year: number): number",
            "Substitui o ano (UTC). Retorna ms novos.",
            false,
        ))
        .member(m_unsound(
            "setUTCMonth",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_DATE_SET_MONTH",
            "setUTCMonth(month: number): number",
            "Substitui o mes (0-11, UTC).",
            false,
        ))
        .member(m_unsound(
            "setUTCDate",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_DATE_SET_DATE",
            "setUTCDate(day: number): number",
            "Substitui o dia do mes (1-31, UTC).",
            false,
        ))
        .member(m_unsound(
            "setUTCHours",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_DATE_SET_HOURS",
            "setUTCHours(hour: number): number",
            "Substitui hour (0-23, UTC).",
            false,
        ))
        .member(m_unsound(
            "setUTCMinutes",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_DATE_SET_MINUTES",
            "setUTCMinutes(min: number): number",
            "Substitui min (0-59, UTC).",
            false,
        ))
        .member(m_unsound(
            "setUTCSeconds",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_DATE_SET_SECONDS",
            "setUTCSeconds(sec: number): number",
            "Substitui sec (0-59, UTC).",
            false,
        ))
        .member(m_unsound(
            "setUTCMilliseconds",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_DATE_SET_MILLISECONDS",
            "setUTCMilliseconds(ms: number): number",
            "Substitui ms (0-999, UTC).",
            false,
        ))
        // ── Conversion extras ────────────────────────────────────────────────
        .member(m(
            "toJSON",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_DATE_TO_JSON",
            "toJSON(): string",
            "JSON serialization — alias de toISOString.",
            true,
        ))
        .member(m_unsound(
            "toLocaleString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_DATE_TO_LOCALE_STRING",
            "toLocaleString(): string",
            "Locale string (sem locale support em v0).",
            true,
        ))
        .member(m_unsound(
            "toLocaleTimeString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_DATE_TO_LOCALE_TIME_STRING",
            "toLocaleTimeString(): string",
            "Time portion (HH:MM:SS.mmmZ) — alias de toTimeString.",
            true,
        ))
        .member(m_unsound(
            "toTimeString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_DATE_TO_TIME_STRING",
            "toTimeString(): string",
            "Time portion (HH:MM:SS.mmmZ) do ISO.",
            true,
        ))
        .instanceof_predicate("__RTS_FN_NS_GC_IS_DATE")
        .done();
}
