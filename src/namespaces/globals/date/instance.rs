//! `Date` global class — constructor and instance method implementations.
//!
//! Each `Date` instance is stored as `Entry::DateMs(i64)` in the HandleTable,
//! where the i64 is milliseconds since Unix epoch (UTC).

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry};

// ── Helper ────────────────────────────────────────────────────────────────────

fn get_ms(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::DateMs(ms)) => *ms,
        _ => 0,
    })
}

// ── Constructors ──────────────────────────────────────────────────────────────

/// `new Date()` — current Unix timestamp in milliseconds.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_NEW_NOW() -> u64 {
    let ms = crate::namespaces::date::ops::__RTS_FN_NS_DATE_NOW_MS();
    alloc_entry(Entry::DateMs(ms))
}

/// `new Date(ms)` — from explicit milliseconds since epoch.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_NEW_FROM_MS(ms: i64) -> u64 {
    alloc_entry(Entry::DateMs(ms))
}

/// `new Date(iso_str)` — from ISO 8601 string (`ptr`/`len` ABI).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_NEW_FROM_ISO(ptr: i64, len: i64) -> u64 {
    let ms = crate::namespaces::date::ops::__RTS_FN_NS_DATE_FROM_ISO(ptr as u64, len);
    alloc_entry(Entry::DateMs(ms))
}

// ── Instance methods ──────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_TIME(handle: u64) -> i64 {
    get_ms(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_VALUE_OF(handle: u64) -> i64 {
    get_ms(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_FULL_YEAR(handle: u64) -> i64 {
    crate::namespaces::date::ops::__RTS_FN_NS_DATE_YEAR(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_MONTH(handle: u64) -> i64 {
    crate::namespaces::date::ops::__RTS_FN_NS_DATE_MONTH(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_DATE(handle: u64) -> i64 {
    crate::namespaces::date::ops::__RTS_FN_NS_DATE_DAY(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_HOURS(handle: u64) -> i64 {
    crate::namespaces::date::ops::__RTS_FN_NS_DATE_HOUR(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_MINUTES(handle: u64) -> i64 {
    crate::namespaces::date::ops::__RTS_FN_NS_DATE_MINUTE(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_SECONDS(handle: u64) -> i64 {
    crate::namespaces::date::ops::__RTS_FN_NS_DATE_SECOND(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_MILLISECONDS(handle: u64) -> i64 {
    crate::namespaces::date::ops::__RTS_FN_NS_DATE_MILLISECOND(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_DAY(handle: u64) -> i64 {
    crate::namespaces::date::ops::__RTS_FN_NS_DATE_WEEKDAY(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_ISO_STRING(handle: u64) -> u64 {
    crate::namespaces::date::ops::__RTS_FN_NS_DATE_TO_ISO(get_ms(handle))
}

/// `Date.prototype.toString()` — formato JS spec:
/// "Day Mon DD YYYY HH:MM:SS GMT+0000 (Coordinated Universal Time)".
/// RTS sempre UTC, entao tz fixo.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_STRING(handle: u64) -> u64 {
    use crate::namespaces::date::ops::*;
    let ms = with_entry(handle, |e| match e {
        Some(Entry::DateMs(v)) => *v,
        _ => 0,
    });
    let year = __RTS_FN_NS_DATE_YEAR(ms);
    let month = __RTS_FN_NS_DATE_MONTH(ms);
    let day = __RTS_FN_NS_DATE_DAY(ms);
    let hour = __RTS_FN_NS_DATE_HOUR(ms);
    let minute = __RTS_FN_NS_DATE_MINUTE(ms);
    let second = __RTS_FN_NS_DATE_SECOND(ms);
    let dow = __RTS_FN_NS_DATE_WEEKDAY(ms);
    let day_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let mon_names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun",
        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let s = format!(
        "{} {} {:02} {:04} {:02}:{:02}:{:02} GMT+0000 (Coordinated Universal Time)",
        day_names[dow.clamp(0, 6) as usize],
        mon_names[(month.clamp(1, 12) - 1) as usize],
        day,
        year,
        hour,
        minute,
        second,
    );
    alloc_entry(Entry::String(s.into_bytes()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_LOCALE_DATE_STRING(handle: u64) -> u64 {
    __RTS_FN_GL_DATE_TO_ISO_STRING(handle)
}

// (#220) UTC getters — RTS armazena ms em UTC, entao getUTCX = getX.
// Aliases sao providos por completude. Quando houver suporte a tz local,
// getX vira local e getUTCX continua UTC.

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_UTC_FULL_YEAR(handle: u64) -> i64 {
    __RTS_FN_GL_DATE_GET_FULL_YEAR(handle)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_UTC_MONTH(handle: u64) -> i64 {
    __RTS_FN_GL_DATE_GET_MONTH(handle)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_UTC_DATE(handle: u64) -> i64 {
    __RTS_FN_GL_DATE_GET_DATE(handle)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_UTC_DAY(handle: u64) -> i64 {
    __RTS_FN_GL_DATE_GET_DAY(handle)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_UTC_HOURS(handle: u64) -> i64 {
    __RTS_FN_GL_DATE_GET_HOURS(handle)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_UTC_MINUTES(handle: u64) -> i64 {
    __RTS_FN_GL_DATE_GET_MINUTES(handle)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_UTC_SECONDS(handle: u64) -> i64 {
    __RTS_FN_GL_DATE_GET_SECONDS(handle)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_UTC_MILLISECONDS(handle: u64) -> i64 {
    __RTS_FN_GL_DATE_GET_MILLISECONDS(handle)
}

/// (#220) `getTimezoneOffset()` — diferenca minutos UTC vs local.
/// RTS sempre UTC = 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_TIMEZONE_OFFSET(_handle: u64) -> i64 {
    0
}

/// (#552) `toUTCString()` — formato RFC 1123: "Day, DD Mon YYYY HH:MM:SS GMT".
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_UTC_STRING(handle: u64) -> u64 {
    use crate::namespaces::date::ops::*;
    let ms = with_entry(handle, |e| match e {
        Some(Entry::DateMs(v)) => *v,
        _ => 0,
    });
    let year = __RTS_FN_NS_DATE_YEAR(ms);
    let month = __RTS_FN_NS_DATE_MONTH(ms);
    let day = __RTS_FN_NS_DATE_DAY(ms);
    let hour = __RTS_FN_NS_DATE_HOUR(ms);
    let minute = __RTS_FN_NS_DATE_MINUTE(ms);
    let second = __RTS_FN_NS_DATE_SECOND(ms);
    let dow = __RTS_FN_NS_DATE_WEEKDAY(ms);
    let day_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let mon_names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun",
        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let s = format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        day_names[dow.clamp(0, 6) as usize],
        day,
        mon_names[(month.clamp(1, 12) - 1) as usize],
        year,
        hour,
        minute,
        second,
    );
    alloc_entry(Entry::String(s.into_bytes()))
}

/// (#220) `toDateString()` — pega ate o 'T' do ISO.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_DATE_STRING(handle: u64) -> u64 {
    let iso_h = __RTS_FN_GL_DATE_TO_ISO_STRING(handle);
    let bytes: Vec<u8> = with_entry(iso_h, |e| match e {
        Some(Entry::String(b)) => b.clone(),
        _ => Vec::new(),
    });
    let take = bytes.iter().position(|&b| b == b'T').unwrap_or(bytes.len());
    alloc_entry(Entry::String(bytes[..take].to_vec()))
}

// (#220) Date setters — mutate Entry::DateMs in-place.
// Cada setter le os parts atuais, substitui o slot, recompoe ms,
// escreve de volta no slot. Retorna novos ms (JS spec).

use crate::namespaces::gc::handles::with_entry_mut;

fn ms_to_parts(ms: i64) -> (i64, i64, i64, i64, i64, i64, i64) {
    use crate::namespaces::date::ops::*;
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

fn set_ms(handle: u64, new_ms: i64) {
    with_entry_mut(handle, |e| {
        if let Some(Entry::DateMs(slot)) = e {
            *slot = new_ms;
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_FULL_YEAR(handle: u64, year: i64) -> i64 {
    use crate::namespaces::date::ops::__RTS_FN_NS_DATE_FROM_PARTS;
    let (_, mo, d, h, mi, s, ms) = ms_to_parts(get_ms(handle));
    let new_ms = __RTS_FN_NS_DATE_FROM_PARTS(year, mo, d, h, mi, s, ms);
    set_ms(handle, new_ms);
    new_ms
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_MONTH(handle: u64, month: i64) -> i64 {
    use crate::namespaces::date::ops::__RTS_FN_NS_DATE_FROM_PARTS;
    let (y, _, d, h, mi, s, ms) = ms_to_parts(get_ms(handle));
    let new_ms = __RTS_FN_NS_DATE_FROM_PARTS(y, month, d, h, mi, s, ms);
    set_ms(handle, new_ms);
    new_ms
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_DATE(handle: u64, day: i64) -> i64 {
    use crate::namespaces::date::ops::__RTS_FN_NS_DATE_FROM_PARTS;
    let (y, mo, _, h, mi, s, ms) = ms_to_parts(get_ms(handle));
    let new_ms = __RTS_FN_NS_DATE_FROM_PARTS(y, mo, day, h, mi, s, ms);
    set_ms(handle, new_ms);
    new_ms
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_HOURS(handle: u64, hour: i64) -> i64 {
    use crate::namespaces::date::ops::__RTS_FN_NS_DATE_FROM_PARTS;
    let (y, mo, d, _, mi, s, ms) = ms_to_parts(get_ms(handle));
    let new_ms = __RTS_FN_NS_DATE_FROM_PARTS(y, mo, d, hour, mi, s, ms);
    set_ms(handle, new_ms);
    new_ms
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_MINUTES(handle: u64, min: i64) -> i64 {
    use crate::namespaces::date::ops::__RTS_FN_NS_DATE_FROM_PARTS;
    let (y, mo, d, h, _, s, ms) = ms_to_parts(get_ms(handle));
    let new_ms = __RTS_FN_NS_DATE_FROM_PARTS(y, mo, d, h, min, s, ms);
    set_ms(handle, new_ms);
    new_ms
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_SECONDS(handle: u64, sec: i64) -> i64 {
    use crate::namespaces::date::ops::__RTS_FN_NS_DATE_FROM_PARTS;
    let (y, mo, d, h, mi, _, ms) = ms_to_parts(get_ms(handle));
    let new_ms = __RTS_FN_NS_DATE_FROM_PARTS(y, mo, d, h, mi, sec, ms);
    set_ms(handle, new_ms);
    new_ms
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_MILLISECONDS(handle: u64, ms_in: i64) -> i64 {
    use crate::namespaces::date::ops::__RTS_FN_NS_DATE_FROM_PARTS;
    let (y, mo, d, h, mi, s, _) = ms_to_parts(get_ms(handle));
    let new_ms = __RTS_FN_NS_DATE_FROM_PARTS(y, mo, d, h, mi, s, ms_in);
    set_ms(handle, new_ms);
    new_ms
}

/// `setTime(ms)` — substitui timestamp completo. Retorna ms.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_TIME(handle: u64, ms_in: i64) -> i64 {
    set_ms(handle, ms_in);
    ms_in
}

/// `toJSON()` — alias de toISOString (JS spec retorna ISO).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_JSON(handle: u64) -> u64 {
    __RTS_FN_GL_DATE_TO_ISO_STRING(handle)
}

/// `toLocaleString()` — alias de toISOString (sem locale support).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_LOCALE_STRING(handle: u64) -> u64 {
    __RTS_FN_GL_DATE_TO_ISO_STRING(handle)
}

/// `toLocaleTimeString()` — pega depois do 'T' do ISO.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_LOCALE_TIME_STRING(handle: u64) -> u64 {
    __RTS_FN_GL_DATE_TO_TIME_STRING(handle)
}

/// `toTimeString()` — pega depois do 'T' do ISO.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_TIME_STRING(handle: u64) -> u64 {
    let iso_h = __RTS_FN_GL_DATE_TO_ISO_STRING(handle);
    let bytes: Vec<u8> = with_entry(iso_h, |e| match e {
        Some(Entry::String(b)) => b.clone(),
        _ => Vec::new(),
    });
    if let Some(t_pos) = bytes.iter().position(|&b| b == b'T') {
        alloc_entry(Entry::String(bytes[t_pos + 1..].to_vec()))
    } else {
        alloc_entry(Entry::String(bytes))
    }
}
