//! JS-semantics coercion ABI over ambiguous i64 (handle-or-raw) operands:
//! `+`, `typeof`, `ToString`, `ToNumber`, truthiness, `.length`, `===`.

use super::snapshot::{EntrySnap, element_to_string, snapshot_entry, snapshot_to_bytes};
use crate::heap::handles::{Entry, alloc_entry, with_entry};
use crate::numfmt::format_js_number;

/// (#proto-method) Smart coercion for a template literal: if `value` (i64) is
/// a valid handle for `Entry::String`, returns the handle itself. Otherwise
/// formats it as an integer (`STRING_FROM_I64`). Used for `${expr}` when
/// `expr` came from `var_member_call` whose return has a dynamic type.
///
/// Rendering by Entry kind:
/// - String: passthrough of the handle
/// - Vec<i64>: JS-style `1,2,3` (no spaces, comma-separated —
///   `Array.prototype.toString` semantics)
/// - Map: `[object Object]` (JS semantics for objects with no custom
///   toString)
/// - Others (Buffer, sockets, etc.): `[object <Kind>]`
/// - Invalid handle / non-handle: treats the value as a raw i64
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_TPL_COERCE_AUTO(value: i64) -> u64 {
    // (cross-runtime sentinels) Array-literal sentinels become strings:
    //   MIN     = false, MIN+1 = true, MIN+2 = undefined, MIN+3 = null,
    //   MIN+4   = sparse hole (renders empty in join, but as "undefined"
    //   when coerced in isolation, e.g. a for-of var).
    if value == i64::MIN { return alloc_entry(Entry::String(b"false".to_vec())); }
    if value == i64::MIN + 1 { return alloc_entry(Entry::String(b"true".to_vec())); }
    if value == i64::MIN + 2 || value == i64::MIN + 4 {
        return alloc_entry(Entry::String(b"undefined".to_vec()));
    }
    if value == i64::MIN + 3 {
        return alloc_entry(Entry::String(b"null".to_vec()));
    }
    // (#573) value=0 with type Handle represents `null` in RTS (and also the
    // invalid-handle sentinel). In JS, console.log(null) -> "null",
    // consistent with Array.prototype.join and String(null).
    if value == 0 {
        return alloc_entry(Entry::String(b"null".to_vec()));
    }
    coerce_auto_inner(value)
}

fn coerce_auto_inner(value: i64) -> u64 {
    let h = value as u64;
    // (narrow-storage) boxed primitive float -> the number's string (String()/
    // template/console coercion). Before snapshotting, so it doesn't fall
    // into "[object Object]".
    if let Some(s) = with_entry(h, |e| match e {
        Some(Entry::FloatPrim(f)) => Some(format_js_number(*f)),
        _ => None,
    }) {
        return alloc_entry(Entry::String(s.into_bytes()));
    }
    let snap = snapshot_entry(h);
    coerce_auto_inner_with_snap(value, snap)
}

fn coerce_auto_inner_with_snap(value: i64, snap: EntrySnap) -> u64 {
    let _h = value as u64;
    match snap {
        // (#1041) Always clones when the handle is already a string. Codegen
        // frees the returned handle treating it as "fresh"; without the
        // clone, the original handle (which may be referenced elsewhere,
        // e.g. a Vec/Map slot) would be freed, causing the UAF observed in
        // `Object.fromEntries(Object.entries(obj).map(...))`.
        EntrySnap::Str(bytes) => alloc_entry(Entry::String(bytes)),
        EntrySnap::None => {
            // Not a valid handle — interpret as a raw i64 OR f64 bits.
            // (cross-runtime #49) `add.bind(null, 5)` called returns an i64
            // that is f64 bits (INVOKE_AUTO does f64_to_i64). If the value is
            // in the typical f64-bits range (but would be below
            // MIN_SAFE_INTEGER or above MAX_SAFE_INTEGER as an integer),
            // interpret it as f64 bits.
            // Range: f64 bits with a reasonable magnitude (1e-300 to 1e300)
            // have an IEEE-754 exponent between roughly 0x000 and 0x7FE, so
            // bit 63 (sign) + bits 62..52 (exponent) vary widely.
            // Conservative heuristic: if |value| > 2^53 (MAX_SAFE_INTEGER)
            // and f64::from_bits(u64) produces a finite, non-NaN, reasonable
            // number, use the f64.
            const MAX_SAFE: i64 = (1i64 << 53) - 1; // 2^53 - 1
            if value > MAX_SAFE || value < -MAX_SAFE {
                let f = f64::from_bits(value as u64);
                if f.is_finite() && !f.is_nan() && f.abs() < 1e16 && f != 0.0 {
                    // Likely f64 bits from a bind/invoke result.
                    return alloc_entry(Entry::String(format_js_number(f).into_bytes()));
                }
            }
            alloc_entry(Entry::String(value.to_string().into_bytes()))
        }
        other => {
            let bytes = snapshot_to_bytes(&other);
            alloc_entry(Entry::String(bytes))
        }
    }
}

/// (cross-runtime #335/#1056) Variant of TPL_COERCE_AUTO that prefers
/// rendering `value=0` as "0" rather than "null". Used in contexts where the
/// value comes from a getter/computation/member access — a case where
/// returning a numeric 0 is more common than a literal null. The `null` case
/// in concat is still covered via the `i64::MIN+3` sentinel.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_TPL_COERCE_NUM_BIAS(value: i64) -> u64 {
    if value == i64::MIN { return alloc_entry(Entry::String(b"false".to_vec())); }
    if value == i64::MIN + 1 { return alloc_entry(Entry::String(b"true".to_vec())); }
    if value == i64::MIN + 2 || value == i64::MIN + 4 {
        return alloc_entry(Entry::String(b"undefined".to_vec()));
    }
    if value == i64::MIN + 3 {
        return alloc_entry(Entry::String(b"null".to_vec()));
    }
    // value=0 with no valid snapshot becomes "0" (instead of "null").
    let h = value as u64;
    let snap = snapshot_entry(h);
    if matches!(snap, EntrySnap::None) && value == 0 {
        return alloc_entry(Entry::String(b"0".to_vec()));
    }
    coerce_auto_inner_with_snap(value, snap)
}

/// Variant of `__RTS_FN_RT_TPL_COERCE_AUTO` for Vec slots where `0` is a
/// literal number (not an invalid handle / null). Identical to AUTO except
/// that value=0 returns "0" (not "null").
///
/// Used by codegen in the `arr[i]` fallback when obj_handle is a Vec.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_TPL_COERCE_VEC_SLOT(value: i64) -> u64 {
    if value == i64::MIN { return alloc_entry(Entry::String(b"false".to_vec())); }
    if value == i64::MIN + 1 { return alloc_entry(Entry::String(b"true".to_vec())); }
    if value == i64::MIN + 2 || value == i64::MIN + 4 {
        return alloc_entry(Entry::String(b"undefined".to_vec()));
    }
    if value == i64::MIN + 3 {
        return alloc_entry(Entry::String(b"null".to_vec()));
    }
    // Vec slot: value=0 is the literal `0`, not null.
    let h = value as u64;
    let snap = snapshot_entry(h);
    match snap {
        // (#1041) Same as AUTO: clones instead of passthrough to avoid a
        // double-free when codegen frees the returned handle as "fresh".
        EntrySnap::Str(bytes) => alloc_entry(Entry::String(bytes)),
        EntrySnap::None => {
            const MAX_SAFE: i64 = (1i64 << 53) - 1;
            if value > MAX_SAFE || value < -MAX_SAFE {
                let f = f64::from_bits(value as u64);
                if f.is_finite() && !f.is_nan() && f.abs() < 1e16 && f != 0.0 {
                    return alloc_entry(Entry::String(format_js_number(f).into_bytes()));
                }
            }
            alloc_entry(Entry::String(value.to_string().into_bytes()))
        }
        other => {
            let bytes = snapshot_to_bytes(&other);
            alloc_entry(Entry::String(bytes))
        }
    }
}

/// JS-spec truthiness for an ambiguous Handle:
/// - 0/null/undefined -> falsy (0)
/// - Entry::String with empty bytes -> falsy (0)
/// - Empty Entry::Vec/Map -> truthy (objects are always truthy in JS)
/// - Other valid handles -> truthy (1)
/// - Invalid handle (not in the table) and value != 0 -> truthy
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_TRUTHY(value: i64) -> i64 {
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

/// (cross-runtime #1069) JS ToNumber abstract operation. Used for unary `+`
/// when the operand is an ambiguous handle/sentinel/bool.
/// - false=MIN -> 0; true=MIN+1 -> 1
/// - undefined=MIN+2 -> NaN
/// - null=MIN+3 -> 0
/// - value==0 -> 0
/// - Entry::String handle: parseFloat-like (trim + numeric tail). Empty/
///   whitespace -> 0; unparseable -> NaN; "0x10" hex -> 16.
/// - Entry::Vec handle len==0 -> 0; len==1 -> ToNumber(slot[0]); >1 -> NaN
/// - Entry::Map/other handle -> NaN
/// - raw non-handle i64: passthrough as f64.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_TO_NUMBER(value: i64) -> f64 {
    if value == 0 || value == i64::MIN || value == i64::MIN + 3 {
        return 0.0;
    }
    if value == i64::MIN + 1 {
        return 1.0;
    }
    if value == i64::MIN + 2 || value == i64::MIN + 4 {
        return f64::NAN;
    }
    if value > 0 {
        let h = value as u64;
        let result = with_entry(h, |entry| match entry {
            Some(Entry::String(b)) => {
                let s = std::str::from_utf8(b).unwrap_or("").trim();
                if s.is_empty() {
                    return Some(0.0);
                }
                // Hex/oct/bin prefixes (JS spec for unary +).
                if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                    return Some(i64::from_str_radix(hex, 16).map(|n| n as f64).unwrap_or(f64::NAN));
                }
                if let Some(oct) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
                    return Some(i64::from_str_radix(oct, 8).map(|n| n as f64).unwrap_or(f64::NAN));
                }
                if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
                    return Some(i64::from_str_radix(bin, 2).map(|n| n as f64).unwrap_or(f64::NAN));
                }
                Some(s.parse::<f64>().unwrap_or(f64::NAN))
            }
            Some(Entry::Vec(v)) => match v.len() {
                0 => Some(0.0),
                1 => Some(__RTS_FN_RT_TO_NUMBER(v[0])),
                _ => Some(f64::NAN),
            },
            Some(_) => Some(f64::NAN),
            None => None,
        });
        if let Some(r) = result {
            return r;
        }
    }
    // Fallback: treat as a raw i64 (plain integer).
    value as f64
}

/// (cross-runtime) `a + b` when BOTH operands are ambiguous i64 (from
/// vec_get/map_get/member-call with no static type) — decides at runtime: if
/// either side is a String handle, concat (JS: string + x = concat);
/// otherwise numeric sum (interpreting each as a raw i64). Fixes `arr[0] +
/// arr[1]` on a string array [which used to sum the handles] without
/// regressing `nums[0] + nums[1]` [which keeps summing].
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_ADD_AUTO(lhs: i64, rhs: i64) -> i64 {
    let ls = snapshot_entry(lhs as u64);
    let rs = snapshot_entry(rhs as u64);
    let lhs_is_str = matches!(ls, EntrySnap::Str(_));
    let rhs_is_str = matches!(rs, EntrySnap::Str(_));
    if lhs_is_str || rhs_is_str {
        // At least one side is a string -> concat (JS semantics). The
        // non-string side is stringified: a valid handle via
        // snapshot_to_bytes; a raw i64 via element_to_string [which tries a
        // lookup and falls back to integer].
        let mut out = match &ls {
            EntrySnap::Str(b) => b.clone(),
            EntrySnap::None => element_to_string(lhs).into_bytes(),
            other => snapshot_to_bytes(other),
        };
        let rbytes = match &rs {
            EntrySnap::Str(b) => b.clone(),
            EntrySnap::None => element_to_string(rhs).into_bytes(),
            other => snapshot_to_bytes(other),
        };
        out.extend_from_slice(&rbytes);
        return alloc_entry(Entry::String(out)) as i64;
    }
    // Neither is a string -> numeric sum of raw i64 (wrapping, same as the
    // iadd this helper replaces).
    lhs.wrapping_add(rhs)
}

/// `.length` universal for ambiguous handles (any/var_member_call): detects
/// Entry::Vec/Map/String and returns the size; returns -1 when the handle is
/// invalid or its type has no length. Used by codegen when the `.length`
/// receiver has a dynamic type (e.g. `JSON.parse(s).length`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_UNIVERSAL_LENGTH(handle: u64) -> i64 {
    if handle == 0 {
        return -1;
    }
    with_entry(handle, |entry| match entry {
        Some(Entry::Vec(slots)) => slots.len() as i64,
        Some(Entry::Map(map)) => map.len() as i64,
        Some(Entry::String(b)) => b.len() as i64,
        // (cross-runtime #88) Uint8Array / Buffer `.length` = byte count.
        Some(Entry::Buffer(b)) => b.len() as i64,
        Some(Entry::Json(v)) => match v.as_ref() {
            serde_json::Value::Array(a) => a.len() as i64,
            serde_json::Value::Object(o) => o.len() as i64,
            serde_json::Value::String(s) => s.len() as i64,
            _ => -1,
        },
        _ => -1,
    })
}

/// `typeof <handle-valued expression>` — returns a string handle with the
/// appropriate JS type for the handle's content. Covers the cases codegen
/// cannot resolve statically (var Symbol, member access that may be a
/// symbol, etc.).
///
/// Mapping:
/// - Entry::Symbol -> "symbol"
/// - Entry::Function -> "function"
/// - Entry::String -> "string"
/// - Entry::Vec / Entry::Map / others -> "object"
/// - Invalid handle -> "string" (preserves historical semantics)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_TYPEOF_HANDLE(handle: u64) -> u64 {
    // (cross-runtime #110) JS sentinels:
    //   MIN     = false  -> "boolean"
    //   MIN+1   = true   -> "boolean"
    //   MIN+2   = undef  -> "undefined"
    //   MIN+3   = null   -> "object"
    //   MIN+4   = hole   -> "undefined"
    let val_i64 = handle as i64;
    if val_i64 == i64::MIN || val_i64 == i64::MIN + 1 {
        return alloc_entry(Entry::String(b"boolean".to_vec()));
    }
    if val_i64 == i64::MIN + 2 || val_i64 == i64::MIN + 4 {
        return alloc_entry(Entry::String(b"undefined".to_vec()));
    }
    if val_i64 == i64::MIN + 3 {
        return alloc_entry(Entry::String(b"object".to_vec()));
    }
    let kind = with_entry(handle, |e| match e {
        Some(Entry::Symbol { .. }) => "symbol",
        Some(Entry::Function(_)) => "function",
        Some(Entry::String(_)) => "string",
        Some(Entry::BooleanBox(_)) => "object",
        Some(Entry::StringBox(_)) => "object",
        Some(Entry::NumberBox(_)) => "object",
        // (narrow-storage) boxed primitive float = primitive number.
        Some(Entry::FloatPrim(_)) => "number",
        Some(Entry::Vec(_)) | Some(Entry::Map(_)) | Some(Entry::Buffer(_))
        | Some(Entry::Json(_))
        | Some(Entry::PromiseAsync(_)) => "object",
        Some(_) => "object",
        None => {
            // (narrow-storage slice 2) Invalid handle = raw numeric value
            // (an int/float storage slot in an arr/map). Previously only
            // `<2^48` became "number" and `>=2^48` became "string" — but f64
            // BITS (e.g. 1.5 = 4.6e18 > 2^48) land here, so `typeof
            // m.get(k)` for a float gave "string". Now any non-handle value
            // is "number" (covers both large ints and f64 bits). A stale
            // handle is UB regardless.
            "number"
        }
    });
    alloc_entry(Entry::String(kind.as_bytes().to_vec()))
}

/// (cross-runtime #359) `typeof obj[key]` fallback. The codegen lowers the
/// member access to `value`; when it resolves to nothing (absent own
/// property — `0` / undefined / null / hole) AND `key` names a method every
/// object inherits from `Object.prototype`, JS reports `"function"`.
/// Otherwise defers to the normal `TYPEOF_HANDLE(value)` so arrays / present
/// keys keep correct results (`typeof arr[0]` -> "number").
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_TYPEOF_MEMBER_FALLBACK(
    value: i64,
    key_ptr: *const u8,
    key_len: i64,
) -> u64 {
    let absent = value == 0
        || value == i64::MIN + 2
        || value == i64::MIN + 3
        || value == i64::MIN + 4;
    if absent && key_len > 0 && !key_ptr.is_null() {
        let key = unsafe { std::slice::from_raw_parts(key_ptr, key_len as usize) };
        let is_proto = matches!(
            key,
            b"constructor"
                | b"toString"
                | b"valueOf"
                | b"hasOwnProperty"
                | b"isPrototypeOf"
                | b"propertyIsEnumerable"
                | b"toLocaleString"
        );
        if is_proto {
            return alloc_entry(Entry::String(b"function".to_vec()));
        }
    }
    __RTS_FN_RT_TYPEOF_HANDLE(value as u64)
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_TO_STRING_HANDLE(handle: u64) -> u64 {
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

/// Strict equality (===) between two i64 when at least one side is ambiguous
/// (a result of vec_get/map_get with no declared type). Each side is
/// interpreted dynamically:
/// - a valid handle (Entry::String/Vec/Map/...) -> a JS object
/// - an i64 outside the handle range -> a JS number
///
/// JS strict-equality rules:
/// - Same type + same value -> 1
/// - Different types -> 0
/// - Two String handles equal in content -> 1
/// - Two handles to the same object (Vec/Map) -> 1 (identity)
/// - Number vs handle -> 0
///
/// Used when codegen detects one side in `var_member_call_values` and the
/// comparison would otherwise short-circuit by static kind.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_STRICT_EQ_AMBIG(a: i64, b: i64) -> i64 {
    // Same bit pattern -> always equal (covers identical handles and equal
    // numbers).
    if a == b {
        return 1;
    }
    let snap_a = if a == 0 { EntrySnap::None } else { snapshot_entry(a as u64) };
    let snap_b = if b == 0 { EntrySnap::None } else { snapshot_entry(b as u64) };
    match (&snap_a, &snap_b) {
        (EntrySnap::Str(sa), EntrySnap::Str(sb)) => {
            if sa == sb { 1 } else { 0 }
        }
        // Different composite types, or one side a handle and the other a
        // number: 0.
        _ => 0,
    }
}
