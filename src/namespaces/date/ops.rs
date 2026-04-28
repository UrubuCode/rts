//! Implementacao das primitivas de Date.
//!
//! Algoritmo civil_from_days/days_from_civil de Howard Hinnant
//! (https://howardhinnant.github.io/date_algorithms.html) — handles
//! 1970-9999 com leap years corretos.

use std::time::{SystemTime, UNIX_EPOCH};

use super::super::gc::handles::{alloc_entry, Entry};

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
/// month e 0-indexed (Jan=0). Retorna None pra valores absurdos.
fn unpack(ts_ms: i64) -> (i64, i64, i64, i64, i64, i64, i64) {
    // Usa euclidean division pra suportar timestamps negativos (pre-1970).
    let days = ts_ms.div_euclid(MS_PER_DAY);
    let ms_in_day = ts_ms.rem_euclid(MS_PER_DAY);

    let h = ms_in_day / MS_PER_HOUR;
    let m = (ms_in_day % MS_PER_HOUR) / MS_PER_MIN;
    let s = (ms_in_day % MS_PER_MIN) / MS_PER_SEC;
    let ms = ms_in_day % MS_PER_SEC;

    // Hinnant civil_from_days: input eh days since 1970-01-01.
    // Shift pra epoch 0000-03-01 (offset 719468 dias).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146_096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m_civil = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m_civil <= 2 { y + 1 } else { y };

    (year, m_civil - 1, d, h, m, s, ms)
}

/// Inversa: (year, month0, day, hour, min, sec, ms) → ms-since-epoch.
fn pack(year: i64, month0: i64, day: i64, hour: i64, min: i64, sec: i64, ms: i64) -> i64 {
    let m_civil = month0 + 1;
    let y = if m_civil <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m_civil > 2 { m_civil - 3 } else { m_civil + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;

    days * MS_PER_DAY + hour * MS_PER_HOUR + min * MS_PER_MIN + sec * MS_PER_SEC + ms
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_NOW_MS() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_FROM_PARTS(
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    min: i64,
    sec: i64,
    ms: i64,
) -> i64 {
    pack(year, month, day, hour, min, sec, ms)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_FROM_ISO(ptr: u64, len: i64) -> i64 {
    let Some(bytes) = slice_from(ptr, len) else {
        return i64::MIN;
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return i64::MIN;
    };
    parse_iso(text).unwrap_or(i64::MIN)
}

/// Parse ISO 8601 — aceita:
/// - YYYY-MM-DD (assume 00:00:00.000Z)
/// - YYYY-MM-DDTHH:MM:SS (assume .000Z)
/// - YYYY-MM-DDTHH:MM:SS.mmm
/// - YYYY-MM-DDTHH:MM:SS.mmmZ
/// Retorna None em formato invalido. Sempre interpreta como UTC.
fn parse_iso(s: &str) -> Option<i64> {
    let bytes = s.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    fn read_int(buf: &[u8]) -> Option<i64> {
        if buf.is_empty() {
            return None;
        }
        if !buf.iter().all(|b| b.is_ascii_digit()) {
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
            // Lê 1-3 digitos de ms.
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

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_YEAR(ts: i64) -> i64 {
    unpack(ts).0
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_MONTH(ts: i64) -> i64 {
    unpack(ts).1
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_DAY(ts: i64) -> i64 {
    unpack(ts).2
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_WEEKDAY(ts: i64) -> i64 {
    // 1970-01-01 foi quinta-feira (4). Sunday=0 na semantica JS.
    let days = ts.div_euclid(MS_PER_DAY);
    (((days % 7) + 4) % 7 + 7) % 7
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_HOUR(ts: i64) -> i64 {
    unpack(ts).3
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_MINUTE(ts: i64) -> i64 {
    unpack(ts).4
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_SECOND(ts: i64) -> i64 {
    unpack(ts).5
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_MILLISECOND(ts: i64) -> i64 {
    unpack(ts).6
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DATE_TO_ISO(ts: i64) -> u64 {
    let (y, mo, d, h, mi, s, ms) = unpack(ts);
    let formatted = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        y,
        mo + 1,
        d,
        h,
        mi,
        s,
        ms
    );
    alloc_entry(Entry::String(formatted.into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpack_epoch() {
        let (y, mo, d, h, mi, s, ms) = unpack(0);
        assert_eq!((y, mo, d, h, mi, s, ms), (1970, 0, 1, 0, 0, 0, 0));
    }

    #[test]
    fn pack_unpack_roundtrip() {
        for (y, mo, d, h, mi, s, ms) in [
            (1970, 0, 1, 0, 0, 0, 0),
            (2024, 0, 15, 12, 30, 45, 123),
            (2000, 1, 29, 23, 59, 59, 999), // leap day
            (1999, 11, 31, 0, 0, 0, 0),
            (2100, 1, 28, 0, 0, 0, 0), // 2100 nao eh leap year
        ] {
            let ts = pack(y, mo, d, h, mi, s, ms);
            let back = unpack(ts);
            assert_eq!(back, (y, mo, d, h, mi, s, ms));
        }
    }

    #[test]
    fn parse_iso_date_only() {
        let ts = parse_iso("2024-01-15").unwrap();
        let (y, mo, d, h, mi, s, ms) = unpack(ts);
        assert_eq!((y, mo, d, h, mi, s, ms), (2024, 0, 15, 0, 0, 0, 0));
    }

    #[test]
    fn parse_iso_full() {
        let ts = parse_iso("2024-01-15T12:30:45.500Z").unwrap();
        let (y, mo, d, h, mi, s, ms) = unpack(ts);
        assert_eq!((y, mo, d, h, mi, s, ms), (2024, 0, 15, 12, 30, 45, 500));
    }

    #[test]
    fn parse_iso_invalid_returns_none() {
        assert!(parse_iso("not a date").is_none());
        assert!(parse_iso("2024-13-01").is_none()); // month out of range
    }

    #[test]
    fn weekday_known_dates() {
        // 1970-01-01 = Thursday = 4
        assert_eq!(__RTS_FN_NS_DATE_WEEKDAY(0), 4);
        // 2000-01-01 = Saturday = 6
        let ts = pack(2000, 0, 1, 0, 0, 0, 0);
        assert_eq!(__RTS_FN_NS_DATE_WEEKDAY(ts), 6);
        // 2024-04-28 = Sunday = 0
        let ts = pack(2024, 3, 28, 0, 0, 0, 0);
        assert_eq!(__RTS_FN_NS_DATE_WEEKDAY(ts), 0);
    }
}
