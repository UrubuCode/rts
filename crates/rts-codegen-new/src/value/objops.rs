//! Codegen-owned DYNAMIC property-access trampolines (P5.5).
//!
//! P3.6 / P5.4 do `obj.key` only when the object's SHAPE is statically proven in
//! the lowering ctx (an object literal / typed local), resolving the slot index at
//! COMPILE time and emitting a direct `VEC_GET(obj, 1+slot)`. When the shape is
//! known only at RUNTIME — a reassigned object local, a computed key `obj[k]`, a
//! returned-then-accessed object — the lowering cannot bake a constant slot, so it
//! routes the access through these PolyValue-aware trampolines, which read the
//! object's slot-0 GLOBAL shape-id at runtime, recover its ordered keys from the
//! process-global shape registry ([`crate::shape`]), find the key's index, and
//! `VEC_GET`/`VEC_SET` at `1 + index`.
//!
//! ## Soundness — the trampolines are conservative, never wrong
//!
//! - The receiver must be a `TAG_OBJECT` keyed object (slot-0 header is a live
//!   shape-id whose key count matches the Vec length — the same
//!   [`crate::value::inspect::looks_like_object`] discriminator the inspect path
//!   uses). An array word, a number, a string, `undefined`/`null` — anything that
//!   is NOT a keyed object — reads `undefined` and a set is a no-op. That is JS-
//!   correct for a missing property, but NOT for `"str".length` / `(5).toString`;
//!   the LOWERING is therefore responsible for routing here ONLY a receiver it has
//!   PROVEN to be a keyed object (never a Tagged/Unknown receiver that could be a
//!   primitive). The trampoline's "non-object → undefined" is the inner safety
//!   net, not the soundness boundary.
//! - A get on a key NOT in the shape reads `undefined` (a JS-correct missing-key
//!   read). A set on a key NOT in the shape is a NO-OP returning the value (adding
//!   a new key is the transition tree, a later increment): the lowering only emits
//!   a dynamic SET when it cannot prove the key is absent, and the static path
//!   bails on a provably-new key, so a silent-drop set is not reachable from a
//!   correct program — but the trampoline still cannot fault.
//!
//! ## Reuse, not divergence
//!
//! The object representation, the slot-0 shape-id header, the global registry, and
//! the `1 + slot_index` value layout are EXACTLY [`super::inspect`]'s — these
//! trampolines read the same bytes the inspect path renders, so a dynamic
//! `obj.key` and a `console.log(obj)` of the same object agree by construction.

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;
use rts_runtime::namespaces::globals::proxy as rt_proxy;

use crate::shape::global_shape_keys;

use super::inspect::looks_like_object;
use super::{PolyValue, abi_adapter};

/// PROXY (#218) — when `obj_word` wraps an `Entry::Proxy`, return its
/// `(target, handler)` reboxed as `TAG_OBJECT` PolyValue words (the stored values
/// are real GC handles; `& PAYLOAD_MASK` drops the generation to the 48-bit slot
/// the box expects). `None` for any non-proxy receiver — the cost is one extra
/// HandleTable lookup on the DYNAMIC property path (never the proven-shape fast
/// path), accepted for the trap semantics.
fn proxy_parts(obj_word: u64) -> Option<(u64, u64)> {
    let obj = PolyValue::from_raw(obj_word);
    if !obj.is_object() {
        return None;
    }
    let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(obj.as_handle());
    let (target, handler) = rt_proxy::ops::resolve_proxy(handle)?;
    // The stored target/handler are full GC handles; `POLY_FROM_HANDLE` drops the
    // generation to the 48-bit slot the `TAG_OBJECT` box expects (NOT a raw mask —
    // the handle layout is not slot-in-low-48).
    let t_slot = rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(target);
    let h_slot = rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(handler);
    let t_word = PolyValue::from_object_handle(t_slot).raw();
    let h_word = PolyValue::from_object_handle(h_slot).raw();
    Some((t_word, h_word))
}

/// Whether `obj_word` wraps an `Entry::Proxy`. A proxy is `TAG_OBJECT` but neither
/// a keyed object nor an array, so callers that discriminate by
/// [`looks_like_object`] (e.g. the index dispatcher's `is_array_word`) must check
/// this FIRST and route to [`__rtsadp_obj_get`]/[`__rtsadp_obj_set`] (the trap).
pub(crate) fn is_proxy_word(obj_word: u64) -> bool {
    proxy_parts(obj_word).is_some()
}

/// Proxy `get` trap: `handler.get(target, key)` when the handler defines a `get`
/// FUNCTION; otherwise read the property straight off the target (the default
/// behavior of a trap-less handler). The trap is invoked through the
/// `__rtsadp_fn_invoke` callback bridge with `(target, key)` — the same bridge the
/// EventEmitter listeners use.
fn proxy_get(target_word: u64, handler_word: u64, key_str_handle: u64) -> u64 {
    let get_key = abi_adapter::intern_poly("get").raw();
    let trap = __rtsadp_obj_get(handler_word, get_key);
    if PolyValue::from_raw(trap).is_function() {
        let undef = PolyValue::undefined().raw();
        super::funcops::__rtsadp_fn_invoke(trap, target_word, key_str_handle, undef, undef, 0)
    } else {
        __rtsadp_obj_get(target_word, key_str_handle)
    }
}

/// Proxy `set` trap: `handler.set(target, key, value)` when the handler defines a
/// `set` FUNCTION; otherwise write straight to the target. JS assignment evaluates
/// to the assigned `value` regardless of the trap's own return.
fn proxy_set(target_word: u64, handler_word: u64, key_str_handle: u64, val_word: u64) -> u64 {
    let set_key = abi_adapter::intern_poly("set").raw();
    let trap = __rtsadp_obj_get(handler_word, set_key);
    if PolyValue::from_raw(trap).is_function() {
        let undef = PolyValue::undefined().raw();
        super::funcops::__rtsadp_fn_invoke(trap, target_word, key_str_handle, val_word, undef, 0);
        val_word
    } else {
        __rtsadp_obj_set(target_word, key_str_handle, val_word)
    }
}

/// Resolve `(real_vec_handle, key_index)` for `obj_word`.`key`: `Some((handle, i))`
/// when `obj_word` is a keyed object whose shape contains `key` (value lives at
/// `VEC` slot `1 + i`), `None` when `obj_word` is not a keyed object or the key is
/// absent. `key_str_handle` is a real string PolyValue word (the lowering interns a
/// literal key, or ToStrings a computed key, into a string PolyValue).
fn resolve_slot(obj_word: u64, key_str_handle: u64) -> Option<(u64, i64)> {
    let obj = PolyValue::from_raw(obj_word);
    if !obj.is_object() || !looks_like_object(obj) {
        return None;
    }
    let key = key_text(key_str_handle);
    let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(obj.as_handle());
    let slot0 = PolyValue::from_raw(rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, 0) as u64);
    let keys = slot0
        .is_int32()
        .then(|| global_shape_keys(slot0.as_i32() as u32))
        .flatten()?;
    let idx = keys.iter().position(|k| *k == key)? as i64;
    Some((handle, idx))
}

/// The UTF-8 text of a key PolyValue. A string PolyValue is read from the real
/// pool; a non-string (a defensively-passed number/bool) is rendered through the
/// engine's own ToString so a numeric computed key (`o["0"]` vs `o[0]`) coerces
/// identically to JS property-key stringification.
fn key_text(key_str_handle: u64) -> String {
    let k = PolyValue::from_raw(key_str_handle);
    if k.is_string() {
        abi_adapter::resolve_poly(k)
    } else {
        // Reuse the engine ToString (numbers/bools); the result is the JS property
        // key string (`o[0]` keys on "0").
        let s_word = super::genops::__rtsadp_to_string(key_str_handle);
        abi_adapter::resolve_poly(PolyValue::from_raw(s_word))
    }
}

/// `__rtsadp_obj_get(obj_word, key_str_handle)` — read property `key` of a keyed
/// object at RUNTIME. Returns the stored PolyValue word for a present key, or
/// `undefined` when the key is absent OR `obj_word` is not a keyed object (an
/// array / primitive). The lowering routes here only proven-object receivers.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_get(obj_word: u64, key_str_handle: u64) -> u64 {
    // PROXY (#218): a proxy receiver routes through its `get` trap. Checked first —
    // a proxy is not a keyed Vec object, so `resolve_slot` would read `undefined`.
    if let Some((target, handler)) = proxy_parts(obj_word) {
        return proxy_get(target, handler, key_str_handle);
    }
    match resolve_slot(obj_word, key_str_handle) {
        Some((handle, idx)) => rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, 1 + idx) as u64,
        None => PolyValue::undefined().raw(),
    }
}

/// `__rtsadp_obj_set(obj_word, key_str_handle, val_word)` — write `val` to property
/// `key` of a keyed object at RUNTIME, returning `val` (JS assignment evaluates to
/// the assigned value). An EXISTING key overwrites its slot. A NEW key TRANSITIONS
/// the object's shape: the ordered key list grows by `key`, a fresh global shape-id
/// is interned, slot 0 is updated to it, and `val` is pushed as the new trailing
/// slot — so `obj[k] = v` for an absent `k` works generically (`JSON.parse`, any
/// dynamic property add). A non-object receiver is a no-op. (`Object` is a
/// PRIMORDIAL → property addition is engine-direct.)
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_set(obj_word: u64, key_str_handle: u64, val_word: u64) -> u64 {
    // PROXY (#218): a proxy receiver routes through its `set` trap.
    if let Some((target, handler)) = proxy_parts(obj_word) {
        return proxy_set(target, handler, key_str_handle, val_word);
    }
    if let Some((handle, idx)) = resolve_slot(obj_word, key_str_handle) {
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(handle, 1 + idx, val_word as i64);
        return val_word;
    }
    // Absent key on a keyed object → shape transition (append the key + value).
    let obj = PolyValue::from_raw(obj_word);
    if obj.is_object() && looks_like_object(obj) {
        let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(obj.as_handle());
        let mut keys = object_keys_vec(obj_word);
        keys.push(key_text(key_str_handle));
        let new_shape = crate::shape::intern_global_shape(&keys);
        let slot0 = PolyValue::from_i32(new_shape as i32).raw() as i64;
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(handle, 0, slot0);
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(handle, val_word as i64);
    }
    val_word
}

/// `Object.fromEntries(entries)` — build a keyed object from an array of `[k, v]`
/// pairs. `entries_word` is an array word; each element is itself a 2-element array
/// `[key, value]`. The object starts empty (the `{}` shape header) and each pair is
/// applied via `__rtsadp_obj_set` (shape transition per new key; a repeated key
/// overwrites). The key is ToString'd to a property key (numbers/bools coerce like
/// JS). Returns the new object word. A non-array entry (or non-array source) is
/// skipped defensively — the lowering only routes a proven-array source here.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_from_entries(entries_word: u64) -> u64 {
    // Empty keyed object (slot 0 = empty-shape id, like a `{}` literal).
    let obj_handle = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let empty_shape = crate::shape::intern_global_shape(&[]);
    let slot0 = PolyValue::from_i32(empty_shape as i32).raw() as i64;
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(obj_handle, slot0);
    let obj_word =
        PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(obj_handle)).raw();

    let entries = PolyValue::from_raw(entries_word);
    if entries.is_object() {
        let eh = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(entries.as_handle());
        let n = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(eh).max(0);
        for i in 0..n {
            let pair_word = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(eh, i) as u64;
            let pair = PolyValue::from_raw(pair_word);
            if !pair.is_object() {
                continue;
            }
            let ph = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(pair.as_handle());
            if rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(ph) < 2 {
                continue;
            }
            let k_word = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(ph, 0) as u64;
            let v_word = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(ph, 1) as u64;
            // Key → a string PolyValue (obj_set's key channel); numbers/bools ToString.
            let key_str = super::genops::__rtsadp_to_string(k_word);
            __rtsadp_obj_set(obj_word, key_str, v_word);
        }
    }
    obj_word
}

/// `__rtsadp_obj_has(obj_word, key_str_handle)` — `key in obj` for a keyed object:
/// `1` when the object has the property, `0` otherwise (incl. a non-object
/// receiver). Returns an unboxed i64 (the `Bool` ABI) for a direct `brif`/`select`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_has(obj_word: u64, key_str_handle: u64) -> i64 {
    resolve_slot(obj_word, key_str_handle).is_some() as i64
}

/// Recover a keyed object's ordered keys from its slot-0 global shape-id. Empty
/// for a non-object / unrecognized header (the safe default — `Object.keys` of a
/// non-object yields `[]`).
fn object_keys_vec(obj_word: u64) -> Vec<String> {
    let obj = PolyValue::from_raw(obj_word);
    if !obj.is_object() || !looks_like_object(obj) {
        return Vec::new();
    }
    let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(obj.as_handle());
    let slot0 = PolyValue::from_raw(rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, 0) as u64);
    slot0
        .is_int32()
        .then(|| global_shape_keys(slot0.as_i32() as u32))
        .flatten()
        .unwrap_or_default()
}

/// Box a fresh Vec handle as a `TAG_OBJECT` array PolyValue word.
fn array_word(vec: u64) -> u64 {
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(vec)).raw()
}

// `Object.keys` (dynamic) reuses the existing `iterops::__rtsadp_obj_keys` (for-in
// already builds the key array) — not redefined here.

/// `__rtsadp_obj_values(obj_word)` — `Object.values(obj)` at RUNTIME: a fresh array
/// of the object's slot VALUES (slots `1..`; slot 0 is the shape-id header).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_values(obj_word: u64) -> u64 {
    let obj = PolyValue::from_raw(obj_word);
    let keys = object_keys_vec(obj_word);
    let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    if !keys.is_empty() {
        let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(obj.as_handle());
        for i in 0..keys.len() {
            let v = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, 1 + i as i64);
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, v);
        }
    }
    array_word(out)
}

/// `__rtsadp_obj_entries(obj_word)` — `Object.entries(obj)` at RUNTIME: a fresh
/// array of `[key, value]` 2-element sub-arrays (each its own `Entry::Vec`).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_entries(obj_word: u64) -> u64 {
    let obj = PolyValue::from_raw(obj_word);
    let keys = object_keys_vec(obj_word);
    let outer = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    if !keys.is_empty() {
        let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(obj.as_handle());
        for (i, k) in keys.iter().enumerate() {
            let v = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, 1 + i as i64);
            let pair = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(pair, abi_adapter::intern_poly(k).raw() as i64);
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(pair, v);
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(outer, array_word(pair) as i64);
        }
    }
    array_word(outer)
}
