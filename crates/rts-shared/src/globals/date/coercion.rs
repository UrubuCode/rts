//! OPAQUE-entry ToPrimitive coercion — the engine's generic `to_string` /
//! `ToNumber` reach these for a heap OBJECT word whose entry is not a keyed
//! object/array. Dispatch is by ENTRY KIND (a `Date` via `rtse_class_of`, a
//! `RegExp` via `Entry::Regex`) — NO class name in the engine. Kept here (out of
//! the `#[rtse::class]` impl) since it spans two classes and forwards to the
//! macro-generated `toString`/`valueOf` externs.

use rts_engine::heap::handles::{Entry, alloc_entry, rtse_class_of, with_entry};

use super::instance::{__rtsm_global_date_to_string, __rtsm_global_date_value_of};

/// ToString of an OPAQUE heap-entry (non-Vec/non-keyed): a `Date` → its
/// `toString()`, a `RegExp` → `/src/flags`. `0` = "don't know" (caller uses
/// "[object Object]").
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_OPAQUE_TO_STRING(handle: u64) -> u64 {
    if rtse_class_of(handle) == Some("Date") {
        return __rtsm_global_date_to_string(handle);
    }
    let regex_text = with_entry(handle, |e| match e {
        Some(Entry::Regex(rx)) => {
            let src = rx.engine.source();
            let src = if src.is_empty() {
                "(?:)".to_string()
            } else {
                src
            };
            Some(format!("/{}/{}", src, rx.flags))
        }
        _ => None,
    });
    match regex_text {
        Some(text) => alloc_entry(Entry::String(text.into_bytes())),
        None => 0,
    }
}

/// Does this OPAQUE entry expose a NUMERIC ToPrimitive value (a `valueOf`
/// returning a number)? `1` = a `Date` (its time value); `0` = otherwise (a
/// `RegExp`/anything else coerces to a number only through its string). The
/// engine probes this before the string fallback so `+date` / `date < date` use
/// the timestamp while `+/re/` still goes via the string (→ NaN).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_OPAQUE_HAS_NUMBER(handle: u64) -> i64 {
    if rtse_class_of(handle) == Some("Date") {
        1
    } else {
        0
    }
}

/// The NUMERIC ToPrimitive value of an OPAQUE entry that has one: for a `Date`,
/// its time value in ms (`NaN` for an Invalid Date). Only meaningful when the
/// probe returned `1`; other kinds return `NaN`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_OPAQUE_TO_NUMBER(handle: u64) -> f64 {
    if rtse_class_of(handle) == Some("Date") {
        __rtsm_global_date_value_of(handle)
    } else {
        f64::NAN
    }
}
