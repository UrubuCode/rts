//! `Date` global class — constructor and instance method implementations.
//!
//! Each `Date` instance is stored as `Entry::DateMs(i64)` in the HandleTable,
//! where the i64 is milliseconds since Unix epoch (UTC).

use rts_engine::heap::handles::{Entry, alloc_entry, with_entry};

// ── Helper ────────────────────────────────────────────────────────────────────

fn get_ms(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::DateMs(ms)) => *ms,
        _ => 0,
    })
}

/// Sentinel do estado INVALID DATE no slot `DateMs` (JS: um time value NaN).
/// `i64::MIN` nunca é um timestamp válido (o range JS é ±8.64e15 ms) — e é o
/// mesmo valor que `DATE_FROM_ISO` já devolve num parse falho.
const INVALID_MS: i64 = i64::MIN;

/// O range de time values válidos da spec (±8.64e15 ms). Fora dele → Invalid.
fn clamp_time_value(ms: f64) -> i64 {
    const MAX: f64 = 8.64e15;
    if ms.is_finite() && ms.abs() <= MAX {
        ms as i64
    } else {
        INVALID_MS
    }
}

/// Um componente de setter: o sentinel de OMITIDO (`i64::MIN as f64`, o
/// `DefaultArg::Int(i64::MIN)` coerced) mantém o campo atual; NaN INVALIDA
/// (None); um número usa seu truncamento.
fn keep_f(v: f64, cur: i64) -> Option<i64> {
    if v == i64::MIN as f64 {
        Some(cur)
    } else if v.is_nan() {
        None
    } else {
        Some(v as i64)
    }
}

/// Os campos-base de um setter: os do instante atual, ou — num Invalid Date —
/// os de `t = +0` (spec 21.4.4.*: um setter completo REVIVE a data).
fn setter_base_parts(handle: u64) -> (i64, i64, i64, i64, i64, i64, i64) {
    let cur = get_ms(handle);
    if cur == INVALID_MS {
        (1970, 0, 1, 0, 0, 0, 0)
    } else {
        ms_to_parts(cur)
    }
}

/// Fecha um setter: `None` em qualquer componente → estado INVALID + NaN;
/// senão grava e devolve os ms novos como f64.
fn finish_setter(handle: u64, parts: Option<(i64, i64, i64, i64, i64, i64, i64)>) -> f64 {
    use crate::date::__RTS_FN_NS_DATE_FROM_PARTS;
    match parts {
        Some((y, mo, d, h, mi, s, ms)) => {
            let new_ms = __RTS_FN_NS_DATE_FROM_PARTS(y, mo, d, h, mi, s, ms);
            set_ms(handle, new_ms);
            new_ms as f64
        }
        None => {
            set_ms(handle, INVALID_MS);
            f64::NAN
        }
    }
}

// ── Constructors ──────────────────────────────────────────────────────────────

/// `new Date()` — current Unix timestamp in milliseconds.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_NEW_NOW() -> u64 {
    let ms = crate::date::__RTS_FN_NS_DATE_NOW_MS();
    alloc_entry(Entry::DateMs(ms))
}

/// `new Date(ms)` — from explicit milliseconds since epoch.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_NEW_FROM_MS(ms: f64) -> u64 {
    // NaN / fora do range da spec → INVALID DATE.
    alloc_entry(Entry::DateMs(clamp_time_value(ms)))
}

/// `new Date(iso_str)` — from ISO 8601 string (`ptr`/`len` ABI).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_NEW_FROM_ISO(ptr: i64, len: i64) -> u64 {
    let ms = crate::date::__RTS_FN_NS_DATE_FROM_ISO(ptr as *const u8, len);
    alloc_entry(Entry::DateMs(ms))
}

/// `new Date(year, month, day?, hour?, min?, sec?, ms?)` — JS spec
/// constructor com componentes individuais. Month e' 0-indexed (Janeiro=0).
/// Componentes ausentes (NaN) recebem default JS: day=1, demais=0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_NEW_FROM_FIELDS(
    year: f64,
    month: f64,
    day: f64,
    hour: f64,
    minute: f64,
    second: f64,
    ms: f64,
) -> u64 {
    let y = year as i64;
    let mo = month as i64;
    let d = if day.is_nan() { 1 } else { day as i64 };
    let h = if hour.is_nan() { 0 } else { hour as i64 };
    let mi = if minute.is_nan() { 0 } else { minute as i64 };
    let s = if second.is_nan() { 0 } else { second as i64 };
    let frac_ms = if ms.is_nan() { 0 } else { ms as i64 };
    let total_ms_pretend_utc =
        crate::date::__RTS_FN_NS_DATE_FROM_PARTS(y, mo, d, h, mi, s, frac_ms);
    // (cross-runtime #172) `new Date(year, mon, day, ...)` em JS spec
    // interpreta os componentes como LOCAL time. pack() retorna ms como
    // se fosse UTC, entao convertemos: utc_ms = local_components_ms -
    // local_offset_at(date). Ex: 2024-01-01 00:00 local em UTC-3 vira
    // 2024-01-01 03:00 UTC, ou seja +3h.
    let local_offset_secs = local_offset_seconds_at(total_ms_pretend_utc);
    let total_ms = total_ms_pretend_utc - (local_offset_secs as i64) * 1000;
    alloc_entry(Entry::DateMs(total_ms))
}

// ── Instance methods ──────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_TIME(handle: u64) -> f64 {
    let ms = get_ms(handle);
    if ms == INVALID_MS {
        f64::NAN
    } else {
        ms as f64
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_VALUE_OF(handle: u64) -> f64 {
    __RTS_FN_GL_DATE_GET_TIME(handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_FULL_YEAR(handle: u64) -> i64 {
    crate::date::__RTS_FN_NS_DATE_YEAR(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_MONTH(handle: u64) -> i64 {
    crate::date::__RTS_FN_NS_DATE_MONTH(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_DATE(handle: u64) -> i64 {
    crate::date::__RTS_FN_NS_DATE_DAY(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_HOURS(handle: u64) -> i64 {
    crate::date::__RTS_FN_NS_DATE_HOUR(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_MINUTES(handle: u64) -> i64 {
    crate::date::__RTS_FN_NS_DATE_MINUTE(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_SECONDS(handle: u64) -> i64 {
    crate::date::__RTS_FN_NS_DATE_SECOND(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_MILLISECONDS(handle: u64) -> i64 {
    crate::date::__RTS_FN_NS_DATE_MILLISECOND(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_DAY(handle: u64) -> i64 {
    crate::date::__RTS_FN_NS_DATE_WEEKDAY(get_ms(handle))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_ISO_STRING(handle: u64) -> u64 {
    crate::date::__RTS_FN_NS_DATE_TO_ISO(get_ms(handle))
}

/// `Date.prototype.toString()` — formato JS spec:
/// "Day Mon DD YYYY HH:MM:SS GMT+0000 (Coordinated Universal Time)".
/// RTS sempre UTC, entao tz fixo.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_STRING(handle: u64) -> u64 {
    use crate::date::*;
    let ms = with_entry(handle, |e| match e {
        Some(Entry::DateMs(v)) => *v,
        _ => 0,
    });
    if ms == INVALID_MS {
        return alloc_entry(Entry::String(b"Invalid Date".to_vec()));
    }
    let year = __RTS_FN_NS_DATE_YEAR(ms);
    let month = __RTS_FN_NS_DATE_MONTH(ms);
    let day = __RTS_FN_NS_DATE_DAY(ms);
    let hour = __RTS_FN_NS_DATE_HOUR(ms);
    let minute = __RTS_FN_NS_DATE_MINUTE(ms);
    let second = __RTS_FN_NS_DATE_SECOND(ms);
    let dow = __RTS_FN_NS_DATE_WEEKDAY(ms);
    let day_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let mon_names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let s = format!(
        "{} {} {:02} {:04} {:02}:{:02}:{:02} GMT+0000 (Coordinated Universal Time)",
        day_names[dow.clamp(0, 6) as usize],
        // (PR #1204) month e' 0-based (JS spec: getMonth() retorna 0..11).
        // Antes era `(month.clamp(1, 12) - 1)` assumindo 1-based, errando
        // o nome do mes: June (5) virava May (4).
        mon_names[month.clamp(0, 11) as usize],
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
    // Default locale: pt-BR-style `DD/MM/YYYY` (mesmo formato emitido
    // por Bun em locale padrao do Windows). Sem Intl real, retorna o
    // formato mais comum em vez do ISO completo. JS spec deixa em aberto
    // o formato exato — qualquer impl deve gerar string parseable e
    // estavel pra o user.
    use crate::date::*;
    let ms = with_entry(handle, |e| match e {
        Some(Entry::DateMs(v)) => *v,
        _ => 0,
    });
    let year = __RTS_FN_NS_DATE_YEAR(ms);
    // getMonth() retorna 0..11; locale strings sao 1-based (01..12).
    let month = __RTS_FN_NS_DATE_MONTH(ms) + 1;
    let day = __RTS_FN_NS_DATE_DAY(ms);
    let s = format!("{:02}/{:02}/{:04}", day, month, year);
    alloc_entry(Entry::String(s.into_bytes()))
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

/// (#220 / cross-runtime #172) `getTimezoneOffset()` — diferenca minutos
/// (local - UTC) NEGADA segundo JS spec. Ex: UTC-3 retorna +180.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_GET_TIMEZONE_OFFSET(handle: u64) -> i64 {
    let ms = with_entry(handle, |e| match e {
        Some(Entry::DateMs(v)) => *v,
        _ => 0,
    });
    let secs = local_offset_seconds_at(ms);
    // JS retorna (local - UTC) negado em minutos.
    -(secs as i64) / 60
}

/// Retorna offset local em segundos para um timestamp ms-since-epoch.
/// Best-effort: usa `time::UtcOffset::current_local_offset` (que pode
/// falhar em ambientes multi-thread). Fallback retorna 0 (UTC).
pub(crate) fn local_offset_seconds_at(_ms: i64) -> i32 {
    // Note: ignoramos `ms` (sem DST history precisa) e usamos o offset
    // atual do sistema. Para a maior parte das fixtures que testam
    // `new Date(2024, 0, 1)` em ambiente moderno isso eh suficiente.
    match time::UtcOffset::current_local_offset() {
        Ok(o) => o.whole_seconds(),
        Err(_) => 0,
    }
}

/// (#552) `toUTCString()` — formato RFC 1123: "Day, DD Mon YYYY HH:MM:SS GMT".
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_UTC_STRING(handle: u64) -> u64 {
    use crate::date::*;
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
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let s = format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        day_names[dow.clamp(0, 6) as usize],
        day,
        // (PR #1204) month e' 0-based (JS spec).
        mon_names[month.clamp(0, 11) as usize],
        year,
        hour,
        minute,
        second,
    );
    alloc_entry(Entry::String(s.into_bytes()))
}

/// (#220) `toDateString()` — JS spec format `Sat Jun 15 2024`.
/// Antes RTS retornava `YYYY-MM-DD` (ISO date), divergindo de Bun/Node
/// que usam o formato `<weekday> <month> <day> <year>` definido pela spec.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_DATE_STRING(handle: u64) -> u64 {
    use crate::date::*;
    let ms = with_entry(handle, |e| match e {
        Some(Entry::DateMs(v)) => *v,
        _ => 0,
    });
    let year = __RTS_FN_NS_DATE_YEAR(ms);
    let month = __RTS_FN_NS_DATE_MONTH(ms);
    let day = __RTS_FN_NS_DATE_DAY(ms);
    let dow = __RTS_FN_NS_DATE_WEEKDAY(ms);
    let day_names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let mon_names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let s = format!(
        "{} {} {:02} {:04}",
        day_names[dow.clamp(0, 6) as usize],
        mon_names[month.clamp(0, 11) as usize],
        day,
        year,
    );
    alloc_entry(Entry::String(s.into_bytes()))
}

// (#220) Date setters — mutate Entry::DateMs in-place.
// Cada setter le os parts atuais, substitui o slot, recompoe ms,
// escreve de volta no slot. Retorna novos ms (JS spec).

use rts_engine::heap::handles::with_entry_mut;

fn ms_to_parts(ms: i64) -> (i64, i64, i64, i64, i64, i64, i64) {
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

fn set_ms(handle: u64, new_ms: i64) {
    with_entry_mut(handle, |e| {
        if let Some(Entry::DateMs(slot)) = e {
            *slot = new_ms;
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_FULL_YEAR(
    handle: u64,
    year: f64,
    month: f64,
    day: f64,
) -> f64 {
    let (_, mo, d, h, mi, s, ms) = setter_base_parts(handle);
    let parts = (|| {
        Some((
            keep_f(year, 0)?,
            keep_f(month, mo)?,
            keep_f(day, d)?,
            h,
            mi,
            s,
            ms,
        ))
    })();
    finish_setter(handle, parts)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_MONTH(handle: u64, month: f64, day: f64) -> f64 {
    let (y, _, d, h, mi, s, ms) = setter_base_parts(handle);
    let parts = (|| Some((y, keep_f(month, 0)?, keep_f(day, d)?, h, mi, s, ms)))();
    finish_setter(handle, parts)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_DATE(handle: u64, day: f64) -> f64 {
    let (y, mo, _, h, mi, s, ms) = setter_base_parts(handle);
    let parts = (|| Some((y, mo, keep_f(day, 0)?, h, mi, s, ms)))();
    finish_setter(handle, parts)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_HOURS(
    handle: u64,
    hour: f64,
    min: f64,
    sec: f64,
    msec: f64,
) -> f64 {
    let (y, mo, d, _, mi, s, ms) = setter_base_parts(handle);
    let parts = (|| {
        Some((
            y,
            mo,
            d,
            keep_f(hour, 0)?,
            keep_f(min, mi)?,
            keep_f(sec, s)?,
            keep_f(msec, ms)?,
        ))
    })();
    finish_setter(handle, parts)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_MINUTES(handle: u64, min: f64, sec: f64, msec: f64) -> f64 {
    let (y, mo, d, h, _, s, ms) = setter_base_parts(handle);
    let parts = (|| {
        Some((
            y,
            mo,
            d,
            h,
            keep_f(min, 0)?,
            keep_f(sec, s)?,
            keep_f(msec, ms)?,
        ))
    })();
    finish_setter(handle, parts)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_SECONDS(handle: u64, sec: f64, msec: f64) -> f64 {
    let (y, mo, d, h, mi, _, ms) = setter_base_parts(handle);
    let parts = (|| Some((y, mo, d, h, mi, keep_f(sec, 0)?, keep_f(msec, ms)?)))();
    finish_setter(handle, parts)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_MILLISECONDS(handle: u64, ms_in: f64) -> f64 {
    let (y, mo, d, h, mi, s, _) = setter_base_parts(handle);
    let parts = (|| Some((y, mo, d, h, mi, s, keep_f(ms_in, 0)?)))();
    finish_setter(handle, parts)
}

/// `setTime(ms)` — substitui timestamp completo. Retorna ms.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_SET_TIME(handle: u64, ms_in: f64) -> f64 {
    let v = clamp_time_value(ms_in);
    set_ms(handle, v);
    if v == INVALID_MS { f64::NAN } else { v as f64 }
}

/// `toJSON()` — alias de toISOString (JS spec retorna ISO).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_JSON(handle: u64) -> u64 {
    __RTS_FN_GL_DATE_TO_ISO_STRING(handle)
}

/// `toLocaleString()` — formato `DD/MM/YYYY, HH:MM:SS` (locale pt-BR
/// default no Windows, mesmo formato emitido por Bun). Sem Intl real,
/// retorna o formato mais comum em vez do ISO completo.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_LOCALE_STRING(handle: u64) -> u64 {
    use crate::date::*;
    let ms = with_entry(handle, |e| match e {
        Some(Entry::DateMs(v)) => *v,
        _ => 0,
    });
    let year = __RTS_FN_NS_DATE_YEAR(ms);
    let month = __RTS_FN_NS_DATE_MONTH(ms) + 1;
    let day = __RTS_FN_NS_DATE_DAY(ms);
    let hour = __RTS_FN_NS_DATE_HOUR(ms);
    let minute = __RTS_FN_NS_DATE_MINUTE(ms);
    let second = __RTS_FN_NS_DATE_SECOND(ms);
    let s = format!(
        "{:02}/{:02}/{:04}, {:02}:{:02}:{:02}",
        day, month, year, hour, minute, second
    );
    alloc_entry(Entry::String(s.into_bytes()))
}

/// `toLocaleTimeString()` — pega depois do 'T' do ISO.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_DATE_TO_LOCALE_TIME_STRING(handle: u64) -> u64 {
    // Locale time string: `HH:MM:SS` (sem ms nem timezone).
    use crate::date::*;
    let ms = with_entry(handle, |e| match e {
        Some(Entry::DateMs(v)) => *v,
        _ => 0,
    });
    let hour = __RTS_FN_NS_DATE_HOUR(ms);
    let minute = __RTS_FN_NS_DATE_MINUTE(ms);
    let second = __RTS_FN_NS_DATE_SECOND(ms);
    let s = format!("{:02}:{:02}:{:02}", hour, minute, second);
    alloc_entry(Entry::String(s.into_bytes()))
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

/// ToString de um heap-entry OPACO (não-Vec/não-Map) — a autoridade fica NESTA
/// camada (rts-shared), onde as classes não-primordiais moram: o genérico
/// `to_string` do motor chama isto para um OBJECT word cujo entry não é um
/// array/objeto keyed; `0` = "não sei" (o caller usa "[object Object]").
/// Dispatch por ENTRY KIND — nenhum nome de classe no motor.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_OPAQUE_TO_STRING(handle: u64) -> u64 {
    let kind = with_entry(handle, |e| match e {
        Some(Entry::DateMs(_)) => 1u8,
        Some(Entry::Regex(_)) => 2,
        _ => 0,
    });
    match kind {
        1 => __RTS_FN_GL_DATE_TO_STRING(handle),
        2 => {
            let text = with_entry(handle, |e| match e {
                Some(Entry::Regex(rx)) => {
                    let src = rx.engine.source();
                    let src = if src.is_empty() {
                        "(?:)".to_string()
                    } else {
                        src
                    };
                    format!("/{}/{}", src, rx.flags)
                }
                _ => String::new(),
            });
            alloc_entry(Entry::String(text.into_bytes()))
        }
        _ => 0,
    }
}

/// Does this OPAQUE heap-entry expose a NUMERIC ToPrimitive value (a `valueOf` that
/// returns a number)? `1` = yes (a `Date` — its `valueOf` is the time value), `0` =
/// no (a `RegExp` / anything else coerces to a number only through its string). The
/// engine's generic `ToNumber` probes this BEFORE the string-based fallback so
/// `+date` / `date < date` use the timestamp, while `+/re/` still goes via the
/// string (→ `NaN`). Dispatch by ENTRY KIND — no class name in the engine.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_OPAQUE_HAS_NUMBER(handle: u64) -> i64 {
    with_entry(handle, |e| match e {
        Some(Entry::DateMs(_)) => 1i64,
        _ => 0,
    })
}

/// The NUMERIC ToPrimitive value of an OPAQUE heap-entry that has one (see
/// `__RTS_FN_RT_OPAQUE_HAS_NUMBER`). For a `Date` it is the time value in ms (`NaN`
/// for an Invalid Date, matching `+new Date(NaN)`). Only meaningful when the probe
/// returned `1`; other kinds return `NaN`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_OPAQUE_TO_NUMBER(handle: u64) -> f64 {
    let kind = with_entry(handle, |e| match e {
        Some(Entry::DateMs(_)) => 1u8,
        _ => 0,
    });
    match kind {
        1 => __RTS_FN_GL_DATE_VALUE_OF(handle),
        _ => f64::NAN,
    }
}
