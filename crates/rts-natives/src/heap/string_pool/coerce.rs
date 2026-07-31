//! JS-semantics coercion ABI over an ambiguous i64 (handle-or-raw) operand:
//! truthiness and `ToString`.
//!
//! What used to live here — `TPL_COERCE_AUTO`/`_NUM_BIAS`/`_VEC_SLOT`,
//! `ADD_AUTO`, `TO_NUMBER`, `UNIVERSAL_LENGTH`, `TYPEOF_HANDLE`,
//! `TYPEOF_MEMBER_FALLBACK`, `STRICT_EQ_AMBIG` — belonged to the DELETED
//! engine's overloaded-i64 value model (the `i64::MIN + n` sentinel family).
//! The live engine carries the tag in the value (`PolyValue`), so those
//! decisions are made by the tag, not by probing the HandleTable, and nothing
//! called them any more.

use super::snapshot::{EntrySnap, snapshot_entry, snapshot_to_bytes};
use crate::heap::handles::{Entry, alloc_entry, with_entry};
use crate::numfmt::format_js_number;

/// JS-spec truthiness for an ambiguous Handle:
/// - 0/null/undefined -> falsy (0)
/// - Entry::String with empty bytes -> falsy (0)
/// - Empty Entry::Vec/Map -> truthy (objects are always truthy in JS)
/// - Other valid handles -> truthy (1)
/// - Invalid handle (not in the table) and value != 0 -> truthy
#[rtse::abi("__RTS_FN_RT_TRUTHY")]
pub fn __RTS_FN_RT_TRUTHY(value: i64) -> i64 {
    if value == 0 {
        return 0;
    }
    // Bool sentinel in a Vec/Map slot (codegen packs false as i64::MIN, true
    // as i64::MIN+1). Treated as a JS bool.
    if value == i64::MIN {
        return 0;
    }
    if value == i64::MIN + 1 {
        return 1;
    }
    // (cross-runtime #223) undefined/null/hole sentinels are falsy in JS,
    // equivalent to RTS's 0 for truthy-check purposes.
    if value == i64::MIN + 2 || value == i64::MIN + 3 || value == i64::MIN + 4 {
        return 0;
    }
    let h = value as u64;
    let snap = with_entry(h, |entry| match entry {
        Some(Entry::String(b)) => {
            // `undefined` (a string-literal handle "undefined") is falsy.
            // So is the empty string.
            if b.is_empty() || b.as_slice() == b"undefined" {
                Some(false)
            } else {
                Some(true)
            }
        }
        Some(_) => Some(true),
        None => None,
    });
    match snap {
        Some(true) => 1,
        Some(false) => 0,
        None => 1,
    }
}

/// `<handle>.toString()` runtime dispatch based on the Entry's type. Covers
/// cases codegen cannot dispatch statically:
/// - Entry::Symbol -> "Symbol(desc)" / "Symbol()"
/// - Entry::Function -> "function name() { [native code] }"
/// - Entry::String -> passthrough of the handle
/// - Entry::Vec -> "1,2,3" (Array.prototype.toString)
/// - Entry::Map -> "[object Object]"
/// - Others -> "[object Kind]"
///
/// An invalid handle returns "" (never crashes).
#[rtse::abi("__RTS_FN_RT_TO_STRING_HANDLE")]
pub fn __RTS_FN_RT_TO_STRING_HANDLE(handle: u64) -> u64 {
    let snap = snapshot_entry(handle);
    match snap {
        EntrySnap::Str(_) => handle, // passthrough
        _ => {
            // The snapshot doesn't cover Symbol/Function explicitly — do a
            // second, direct lookup to detect them.
            let special = with_entry(handle, |e| match e {
                Some(Entry::Symbol { description }) => match description {
                    Some(d) => Some(format!("Symbol({})", d)),
                    None => Some("Symbol()".to_string()),
                },
                Some(Entry::Function(d)) => {
                    let name = if d.name.is_empty() { "anonymous" } else { &*d.name };
                    Some(format!("function {}() {{ [native code] }}", name))
                }
                // (narrow-storage) boxed primitive float -> the number's string.
                Some(Entry::FloatPrim(f)) => Some(format_js_number(*f)),
                _ => None,
            });
            match special {
                Some(s) => alloc_entry(Entry::String(s.into_bytes())),
                None => {
                    let bytes = snapshot_to_bytes(&snap);
                    alloc_entry(Entry::String(bytes))
                }
            }
        }
    }
}
