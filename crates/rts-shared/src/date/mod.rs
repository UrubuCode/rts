//! `date` namespace — primitives for the JS Date API.
//!
//! Cada Date e' um i64 com ms desde Unix epoch (UTC) — sem handles, sem
//! alocacao. Conversoes calendario usam o algoritmo Howard Hinnant
//! (civil_from_days), portado pra evitar dependencia em chrono.
//!
//! `date_unpack` e' pub (consumido por globals::date). `DATE_PARSE_F64`
//! (Date.parse) NAO e' membro do namespace — fica como extern abaixo.
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).

use std::time::{SystemTime, UNIX_EPOCH};

use rts_engine::abi::ty::{Handle, I64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{Entry, alloc_entry};

const MS_PER_SEC: i64 = 1000;
const MS_PER_MIN: i64 = 60 * MS_PER_SEC;
const MS_PER_HOUR: i64 = 60 * MS_PER_MIN;
const MS_PER_DAY: i64 = 24 * MS_PER_HOUR;

fn slice_from(ptr: u64, len: i64) -> Option<&'static [u8]> {
    if ptr == 0 || len < 0 {
        return None;
    }
    Some(unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) })
}

/// Converte ms-since-epoch em (year, month0, day, hour, min, sec, ms).
pub fn date_unpack(ts_ms: i64) -> (i64, i64, i64, i64, i64, i64, i64) {
    unpack(ts_ms)
}

fn unpack(ts_ms: i64) -> (i64, i64, i64, i64, i64, i64, i64) {
    let days = ts_ms.div_euclid(MS_PER_DAY);
    let ms_in_day = ts_ms.rem_euclid(MS_PER_DAY);

    let h = ms_in_day / MS_PER_HOUR;
    let m = (ms_in_day % MS_PER_HOUR) / MS_PER_MIN;
    let s = (ms_in_day % MS_PER_MIN) / MS_PER_SEC;
    let ms = ms_in_day % MS_PER_SEC;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m_civil = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m_civil <= 2 { y + 1 } else { y };

    (year, m_civil - 1, d, h, m, s, ms)
}

fn pack(year: i64, month0: i64, day: i64, hour: i64, min: i64, sec: i64, ms: i64) -> i64 {
    // Normalize an out-of-range month (JS setMonth(12) rolls the year): the
    // civil-days formula below is only valid for months 1..12.
    let year = year + month0.div_euclid(12);
    let month0 = month0.rem_euclid(12);
    let m_civil = month0 + 1;
    let y = if m_civil <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy =
        (153 * (if m_civil > 2 {
            m_civil - 3
        } else {
            m_civil + 9
        }) + 2)
            / 5
            + day
            - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;

    days * MS_PER_DAY + hour * MS_PER_HOUR + min * MS_PER_MIN + sec * MS_PER_SEC + ms
}

/// Parse ISO 8601 (UTC). None em formato invalido.
fn parse_iso(s: &str) -> Option<i64> {
    let bytes = s.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    fn read_int(buf: &[u8]) -> Option<i64> {
        if buf.is_empty() || !buf.iter().all(|b| b.is_ascii_digit()) {
            return None;
        }
        std::str::from_utf8(buf).ok()?.parse::<i64>().ok()
    }
    let y = read_int(&bytes[0..4])?;
    if bytes[4] != b'-' {
        return None;
    }
    let mo = read_int(&bytes[5..7])?;
    if bytes[7] != b'-' {
        return None;
    }
    let d = read_int(&bytes[8..10])?;
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }

    let mut h = 0i64;
    let mut mi = 0i64;
    let mut se = 0i64;
    let mut ms = 0i64;

    if bytes.len() >= 19 && (bytes[10] == b'T' || bytes[10] == b' ') {
        h = read_int(&bytes[11..13])?;
        if bytes[13] != b':' {
            return None;
        }
        mi = read_int(&bytes[14..16])?;
        if bytes[16] != b':' {
            return None;
        }
        se = read_int(&bytes[17..19])?;

        if bytes.len() > 19 && bytes[19] == b'.' {
            let mut end = 20;
            while end < bytes.len() && bytes[end].is_ascii_digit() && end < 23 {
                end += 1;
            }
            if end > 20 {
                let raw = read_int(&bytes[20..end])?;
                let pad = 3 - (end - 20);
                ms = raw * 10i64.pow(pad as u32);
            }
        }
    }

    Some(pack(y, mo - 1, d, h, mi, se, ms))
}

/// `Date.parse(s)` — JS spec: ms desde epoch ou NaN. NOT a namespace member
/// (the constructor `new Date(string)` uses the i64+sentinel `from_iso`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_PARSE_F64(ptr: u64, len: i64) -> f64 {
    let Some(bytes) = slice_from(ptr, len) else {
        return f64::NAN;
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return f64::NAN;
    };
    match parse_iso(text) {
        Some(ms) => ms as f64,
        None => f64::NAN,
    }
}

/// Now, in ms since the Unix epoch (UTC).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_NOW_MS() -> I64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Parse an ISO 8601 string to ms. Returns i64::MIN sentinel on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_FROM_ISO(text_ptr: *const u8, text_len: i64) -> I64 {
    let text = match unsafe { rts_engine::abi::str_abi::from_abi(text_ptr, text_len) } {
        Some(s) => s,
        None => return i64::MIN,
    };
    parse_iso(text).unwrap_or(i64::MIN)
}

/// Build ms from calendar parts. Two-digit years (0..99) map to 1900+y.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_FROM_PARTS(
    year: I64,
    month: I64,
    day: I64,
    hour: I64,
    min: I64,
    sec: I64,
    ms: I64,
) -> I64 {
    let year = if (0..=99).contains(&year) {
        year + 1900
    } else {
        year
    };
    pack(year, month, day, hour, min, sec, ms)
}

/// Year (UTC).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_YEAR(ts: I64) -> I64 {
    unpack(ts).0
}

/// Month, 0-indexed (UTC).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_MONTH(ts: I64) -> I64 {
    unpack(ts).1
}

/// Day of month (UTC).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_DAY(ts: I64) -> I64 {
    unpack(ts).2
}

/// Weekday, Sunday=0 (UTC).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_WEEKDAY(ts: I64) -> I64 {
    // 1970-01-01 was Thursday (4); Sunday=0 in JS semantics.
    let days = ts.div_euclid(MS_PER_DAY);
    (((days % 7) + 4) % 7 + 7) % 7
}

/// Hour (UTC).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_HOUR(ts: I64) -> I64 {
    unpack(ts).3
}

/// Minute (UTC).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_MINUTE(ts: I64) -> I64 {
    unpack(ts).4
}

/// Second (UTC).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_SECOND(ts: I64) -> I64 {
    unpack(ts).5
}

/// Millisecond (UTC).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_MILLISECOND(ts: I64) -> I64 {
    unpack(ts).6
}

/// ISO 8601 string (UTC). Returns a GC string handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_TO_ISO(ts: I64) -> Handle {
    let (y, mo, d, h, mi, s, ms) = unpack(ts);
    let formatted = format!("{y:04}-{:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{ms:03}Z", mo + 1);
    alloc_entry(Entry::String(formatted.into_bytes()))
}

/// Função `date.f(args)`.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        intrinsic: None,
    }
}

/// Registra a namespace `date` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("date")
        .doc("Date primitives — a Date is an i64 (ms since the Unix epoch, UTC).")
        .member(func(
            "now_ms",
            "__RTS_FN_NS_DATE_NOW_MS",
            Sig::new(Vec::new(), AbiType::I64),
            "now_ms(): number",
            "Now, in ms since the Unix epoch (UTC).",
            __RTS_FN_NS_DATE_NOW_MS as *const u8,
        ))
        .member(func(
            "from_iso",
            "__RTS_FN_NS_DATE_FROM_ISO",
            Sig::new(vec![AbiType::StrPtr], AbiType::I64),
            "from_iso(text: string): number",
            "Parse an ISO 8601 string to ms. Returns i64::MIN sentinel on error.",
            __RTS_FN_NS_DATE_FROM_ISO as *const u8,
        ))
        .member(func(
            "from_parts",
            "__RTS_FN_NS_DATE_FROM_PARTS",
            Sig::new(
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
            ),
            "from_parts(y: number, mo: number, d: number, h: number, mi: number, s: number, ms: number): number",
            "Build ms from calendar parts. Two-digit years (0..99) map to 1900+y.",
            __RTS_FN_NS_DATE_FROM_PARTS as *const u8,
        ))
        .member(func(
            "year",
            "__RTS_FN_NS_DATE_YEAR",
            Sig::new(vec![AbiType::I64], AbiType::I64),
            "year(ts: number): number",
            "Year (UTC).",
            __RTS_FN_NS_DATE_YEAR as *const u8,
        ))
        .member(func(
            "month",
            "__RTS_FN_NS_DATE_MONTH",
            Sig::new(vec![AbiType::I64], AbiType::I64),
            "month(ts: number): number",
            "Month, 0-indexed (UTC).",
            __RTS_FN_NS_DATE_MONTH as *const u8,
        ))
        .member(func(
            "day",
            "__RTS_FN_NS_DATE_DAY",
            Sig::new(vec![AbiType::I64], AbiType::I64),
            "day(ts: number): number",
            "Day of month (UTC).",
            __RTS_FN_NS_DATE_DAY as *const u8,
        ))
        .member(func(
            "weekday",
            "__RTS_FN_NS_DATE_WEEKDAY",
            Sig::new(vec![AbiType::I64], AbiType::I64),
            "weekday(ts: number): number",
            "Weekday, Sunday=0 (UTC).",
            __RTS_FN_NS_DATE_WEEKDAY as *const u8,
        ))
        .member(func(
            "hour",
            "__RTS_FN_NS_DATE_HOUR",
            Sig::new(vec![AbiType::I64], AbiType::I64),
            "hour(ts: number): number",
            "Hour (UTC).",
            __RTS_FN_NS_DATE_HOUR as *const u8,
        ))
        .member(func(
            "minute",
            "__RTS_FN_NS_DATE_MINUTE",
            Sig::new(vec![AbiType::I64], AbiType::I64),
            "minute(ts: number): number",
            "Minute (UTC).",
            __RTS_FN_NS_DATE_MINUTE as *const u8,
        ))
        .member(func(
            "second",
            "__RTS_FN_NS_DATE_SECOND",
            Sig::new(vec![AbiType::I64], AbiType::I64),
            "second(ts: number): number",
            "Second (UTC).",
            __RTS_FN_NS_DATE_SECOND as *const u8,
        ))
        .member(func(
            "millisecond",
            "__RTS_FN_NS_DATE_MILLISECOND",
            Sig::new(vec![AbiType::I64], AbiType::I64),
            "millisecond(ts: number): number",
            "Millisecond (UTC).",
            __RTS_FN_NS_DATE_MILLISECOND as *const u8,
        ))
        .member(func(
            "to_iso",
            "__RTS_FN_NS_DATE_TO_ISO",
            Sig::new(vec![AbiType::I64], AbiType::Handle),
            "to_iso(ts: number): string",
            "ISO 8601 string (UTC). Returns a GC string handle.",
            __RTS_FN_NS_DATE_TO_ISO as *const u8,
        ))
        .done();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpack_epoch() {
        assert_eq!(unpack(0), (1970, 0, 1, 0, 0, 0, 0));
    }

    #[test]
    fn pack_unpack_roundtrip() {
        for parts in [
            (1970, 0, 1, 0, 0, 0, 0),
            (2024, 0, 15, 12, 30, 45, 123),
            (2000, 1, 29, 23, 59, 59, 999),
            (1999, 11, 31, 0, 0, 0, 0),
            (2100, 1, 28, 0, 0, 0, 0),
        ] {
            let (y, mo, d, h, mi, s, ms) = parts;
            let ts = pack(y, mo, d, h, mi, s, ms);
            assert_eq!(unpack(ts), parts);
        }
    }

    #[test]
    fn parse_iso_full() {
        let ts = parse_iso("2024-01-15T12:30:45.500Z").unwrap();
        assert_eq!(unpack(ts), (2024, 0, 15, 12, 30, 45, 500));
    }

    #[test]
    fn parse_iso_invalid_returns_none() {
        assert!(parse_iso("not a date").is_none());
        assert!(parse_iso("2024-13-01").is_none());
    }

    #[test]
    fn weekday_known_dates() {
        assert_eq!(__RTS_FN_NS_DATE_WEEKDAY(0), 4);
        assert_eq!(__RTS_FN_NS_DATE_WEEKDAY(pack(2000, 0, 1, 0, 0, 0, 0)), 6);
        assert_eq!(__RTS_FN_NS_DATE_WEEKDAY(pack(2024, 3, 28, 0, 0, 0, 0)), 0);
    }
}
