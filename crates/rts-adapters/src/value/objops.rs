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

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

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
pub(crate) fn proxy_parts(obj_word: u64) -> Option<(u64, u64)> {
    let obj = PolyValue::from_raw(obj_word);
    if !obj.is_object() {
        return None;
    }
    let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(obj.as_handle());
    let (target, handler) = rt_proxy::ops::resolve_proxy(handle)?;
    // The stored target/handler are full GC handles; `POLY_FROM_HANDLE` drops the
    // generation to the 48-bit slot the box expects (NOT a raw mask — the handle
    // layout is not slot-in-low-48). The TARGET's tag follows its live entry: a
    // proxied FUNCTION (`new Proxy(fn, …)` — the apply/construct traps) must ride
    // `TAG_FUNCTION` or the forward path reads it as a plain object and refuses
    // to call/construct it.
    let t_slot = rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(target);
    let h_slot = rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(handler);
    let t_is_fn = rt_handles::with_entry(target, |e| {
        matches!(e, Some(rt_handles::Entry::Function(_)))
    });
    let t_word = if t_is_fn {
        PolyValue::from_function_handle(t_slot).raw()
    } else {
        PolyValue::from_object_handle(t_slot).raw()
    };
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
    } else if k.is_object() && {
        let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(k.as_handle());
        rt_handles::with_entry(h, |e| matches!(e, Some(rt_handles::Entry::Symbol { .. })))
    } {
        // A SYMBOL property key: the canonical storage repr is `@@sym:<handle>`
        // (#798) — unique per symbol, filtered from string enumeration
        // (`Object.keys`/for-in/`JSON.stringify` skip it; `getOwnPropertySymbols`
        // decodes it back).
        let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(k.as_handle());
        format!("@@sym:{h}")
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
    // A FUNCTION receiver: `.name` is the Function getter (`"anonymous"` for a
    // `new Function`); `.length` falls into the tag-dispatched length below.
    {
        let v = PolyValue::from_raw(obj_word);
        if v.is_function() && key_text(key_str_handle) == "name" {
            let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
            let name_h =
                rts_runtime::namespaces::globals::function::ops::__RTS_FN_GL_FUNCTION_NAME(h);
            return abi_adapter::poly_from_real_handle(name_h).raw();
        }
    }
    // A legacy DICTIONARY object (`Entry::Map` — e.g. `Promise.allSettled`'s
    // `{status, value}` result rows built runtime-side): read the key straight
    // from the IndexMap, boxing the raw i64 value by its live kind (int word,
    // heap handle → STR/OBJECT word, f64 bits).
    {
        let obj = PolyValue::from_raw(obj_word);
        if obj.is_object() {
            let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(obj.as_handle());
            let hit = rt_handles::with_entry(h, |e| match e {
                Some(rt_handles::Entry::Map(m)) => {
                    Some(m.get(&key_text(key_str_handle)).copied())
                }
                _ => None,
            });
            if let Some(v) = hit {
                return match v {
                    Some(x) => {
                        let w = x as u64;
                        if let Ok(i) = i32::try_from(x) {
                            PolyValue::from_i32(i).raw()
                        } else if w >= (1u64 << 48) {
                            super::genops::__rtsadp_box_handle_auto(w)
                        } else {
                            PolyValue::from_f64(x as f64).raw()
                        }
                    }
                    None => PolyValue::undefined().raw(),
                };
            }
        }
    }
    // `.length` on a NON-keyed receiver (a string / an array reached through the
    // dynamic get — `cfg?.list.length`): route the tag-dispatched length. A KEYED
    // object falls through to its own `length` slot below (`dyn_length`'s keyed
    // arm calls back here — the guard prevents the mutual recursion).
    {
        let obj = PolyValue::from_raw(obj_word);
        let is_keyed = obj.is_object() && looks_like_object(obj);
        if !is_keyed && key_text(key_str_handle) == "length" {
            return super::dyndispatch::__rtsadp_dyn_length(obj_word);
        }
        // A BUFFER receiver (`Entry::Buffer` — an ArrayBuffer / encode result):
        // a numeric key reads the byte (the Uint8Array indexing surface,
        // out-of-bounds → `undefined`); `byteLength` reads the byte count.
        if !is_keyed && obj.is_object() {
            let key = key_text(key_str_handle);
            if key == "byteLength" {
                let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(obj.as_handle());
                if let Some(len) = rt_handles::with_entry(h, |e| match e {
                    Some(rt_handles::Entry::Buffer(b)) => Some(b.len()),
                    _ => None,
                }) {
                    return PolyValue::from_i32(len as i32).raw();
                }
            }
            if let Ok(i) = key.parse::<usize>() {
                let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(obj.as_handle());
                if let Some(byte) = rt_handles::with_entry(h, |e| match e {
                    Some(rt_handles::Entry::Buffer(b)) => Some(b.get(i).copied()),
                    _ => None,
                }) {
                    return match byte {
                        Some(v) => PolyValue::from_i32(v as i32).raw(),
                        None => PolyValue::undefined().raw(),
                    };
                }
            }
        }
    }
    // SYMBOL primitive (#216): `.description` reads the stored description (a
    // string word, or `undefined` for a description-less symbol). A symbol wraps
    // `Entry::Symbol`, not a keyed Vec, so `resolve_slot` cannot see it.
    {
        let obj = PolyValue::from_raw(obj_word);
        if obj.is_object() && key_text(key_str_handle) == "description" {
            let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(obj.as_handle());
            let desc = rt_handles::with_entry(handle, |e| match e {
                Some(rt_handles::Entry::Symbol { description }) => Some(description.clone()),
                _ => None,
            });
            if let Some(d) = desc {
                return match d {
                    Some(s) => abi_adapter::intern_poly(&s).raw(),
                    None => PolyValue::undefined().raw(),
                };
            }
        }
    }
    match resolve_slot(obj_word, key_str_handle) {
        Some((handle, idx)) => rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, 1 + idx) as u64,
        // Own-slot MISS: an ACCESSOR slot (`__get_<key>` — a defineProperty /
        // literal getter) anywhere on the chain wins, invoked with `this` = the
        // ORIGINAL receiver; else walk the prototype chain for the data key.
        None => {
            let key = key_text(key_str_handle);
            if !key.starts_with("__get_") && !key.starts_with("__set_") {
                let gkey = abi_adapter::intern_poly(&format!("__get_{key}")).raw();
                let getter = lookup_chain(obj_word, gkey, 0);
                if PolyValue::from_raw(getter).is_function() {
                    let undef = PolyValue::undefined().raw();
                    return super::funcops::__rtsadp_fn_invoke_method(
                        getter, obj_word, undef, undef, undef, 0,
                    );
                }
            }
            match super::protos::proto_of(obj_word) {
                Some(proto) => __rtsadp_obj_get(proto, key_str_handle),
                None => PolyValue::undefined().raw(),
            }
        }
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
/// SHAPED-OBJECT bridge for the LOW-LEVEL `collections.map_get` surface
/// (#264): a fn-ctor writing `collections.map_set(this, k, v)` receives the
/// new-engine shape-vec instance, not an `Entry::Map`. The map namespace
/// routes a non-Map `Entry::Vec` handle here (hook installed at bootstrap);
/// the read goes through the SAME shape-aware `obj_get` any property read
/// uses, and the result is normalized to the raw-i64 map surface.
pub extern "C" fn shaped_object_get(h: u64, key_ptr: *const u8, key_len: i64) -> i64 {
    use rts_runtime::namespaces::gc::handles as rt_handles;
    let Some(key) = (unsafe { rts_engine::abi::str_abi::from_abi(key_ptr, key_len) }) else {
        return 0;
    };
    let word = PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(h)).raw();
    let kh = rts_runtime::namespaces::collector::string_pool::__RTS_FN_NS_GC_STRING_NEW(key.as_ptr(), key.len() as i64);
    let kw = PolyValue::from_str_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(kh)).raw();
    let out = PolyValue::from_raw(__rtsadp_obj_get(word, kw));
    // Normalize to the i64 surface: INT32 → the int; a heap value → its real
    // handle; a double → truncated; undefined/absent → 0.
    if out.is_int32() {
        return out.as_i32() as i64;
    }
    if let Some(hh) = rts_engine::heap::poly::poly_handle_normalize(out.raw()) {
        return hh as i64;
    }
    if out.is_double() {
        return out.as_f64() as i64;
    }
    0
}

/// SHAPED-OBJECT write bridge (see [`shaped_object_get`]): boxes the raw-i64
/// value as a number word and routes through the shape-aware `obj_set`
/// (existing-slot write or append-with-transition).
pub extern "C" fn shaped_object_set(h: u64, key_ptr: *const u8, key_len: i64, value: i64) {
    use rts_runtime::namespaces::gc::handles as rt_handles;
    let Some(key) = (unsafe { rts_engine::abi::str_abi::from_abi(key_ptr, key_len) }) else {
        return;
    };
    let word = PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(h)).raw();
    let kh = rts_runtime::namespaces::collector::string_pool::__RTS_FN_NS_GC_STRING_NEW(key.as_ptr(), key.len() as i64);
    let kw = PolyValue::from_str_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(kh)).raw();
    let vw = match i32::try_from(value) {
        Ok(i) => PolyValue::from_i32(i),
        Err(_) => PolyValue::from_f64(value as f64),
    };
    __rtsadp_obj_set(word, kw, vw.raw());
}

#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_set(obj_word: u64, key_str_handle: u64, val_word: u64) -> u64 {
    // PROXY (#218): a proxy receiver routes through its `set` trap.
    if let Some((target, handler)) = proxy_parts(obj_word) {
        return proxy_set(target, handler, key_str_handle, val_word);
    }
    // Level-B typed-array VIEW + NUMERIC key: element write through the SHARED
    // buffer (a shape transition would orphan the write).
    if let Some((bh, bytes, _s, float)) = super::taops::view_parts(obj_word) {
        if let Ok(i) = key_text(key_str_handle).parse::<i64>() {
            super::taops::view_set(bh, bytes, float, i, val_word);
            return val_word;
        }
    }
    // An ARRAY receiver + NUMERIC key (`v[i] = x` through the dynamic path —
    // the receiver was an `any` param/local): element write. `i == len`
    // appends; `i > len` fills the gap with `undefined` (no holes in the
    // model) then appends — JS growth semantics.
    {
        let obj = PolyValue::from_raw(obj_word);
        if obj.is_object() && !looks_like_object(obj) {
            if let Ok(i) = key_text(key_str_handle).parse::<i64>() {
                let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(obj.as_handle());
                let is_vec = rt_handles::with_entry(handle, |e| {
                    matches!(e, Some(rt_handles::Entry::Vec(_)))
                });
                if is_vec && i >= 0 {
                    let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(handle).max(0);
                    if i < len {
                        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(handle, i, val_word as i64);
                    } else {
                        let undef = PolyValue::undefined().raw() as i64;
                        for _ in len..i {
                            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(handle, undef);
                        }
                        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(handle, val_word as i64);
                    }
                    return val_word;
                }
            }
        }
    }
    if let Some((handle, idx)) = resolve_slot(obj_word, key_str_handle) {
        // A `writable:false` data property (set via defineProperty) blocks
        // re-assignment (sloppy mode: silent no-op, returns the value).
        if !prop_writable(obj_word, &key_text(key_str_handle)) {
            return val_word;
        }
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(handle, 1 + idx, val_word as i64);
        return val_word;
    }
    // Absent key on a keyed object → shape transition (append the key + value).
    // A non-extensible object (Object.preventExtensions/freeze) rejects new keys.
    let obj = PolyValue::from_raw(obj_word);
    if obj.is_object() && looks_like_object(obj) && !is_non_extensible(obj_word) {
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
/// A PROXY receiver routes through its `has` trap (#218) — `handler.has(target,
/// key)` when defined, ToBoolean of the trap's return; otherwise forward to the
/// target (recursion terminates: the target of a proxy-of-proxy chain is finite).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_has(obj_word: u64, key_str_handle: u64) -> i64 {
    if let Some((target, handler)) = proxy_parts(obj_word) {
        let has_key = abi_adapter::intern_poly("has").raw();
        let trap = __rtsadp_obj_get(handler, has_key);
        if PolyValue::from_raw(trap).is_function() {
            let undef = PolyValue::undefined().raw();
            let r = super::funcops::__rtsadp_fn_invoke(
                trap,
                target,
                key_str_handle,
                undef,
                undef,
                0,
            );
            return PolyValue::from_raw(r).is_truthy() as i64;
        }
        return __rtsadp_obj_has(target, key_str_handle);
    }
    if resolve_slot(obj_word, key_str_handle).is_some() {
        return 1;
    }
    // `[[HasProperty]]` walks the PROTOTYPE chain (unlike `hasOwnProperty`):
    // `"m" in child` is true for an inherited `m`. `"__proto__"` reads true for
    // any object whose [[Prototype]] is wired (the Object.prototype accessor in
    // real JS) — an Object.create(null)-style bare object stays false.
    let key = key_text(key_str_handle);
    if key == "__proto__" {
        return super::protos::proto_of(obj_word).is_some() as i64;
    }
    match super::protos::proto_of(obj_word) {
        Some(proto) => __rtsadp_obj_has(proto, key_str_handle),
        None => 0,
    }
}

/// `obj.hasOwnProperty(key)` → a BOXED bool PolyValue word (`true`/`false`
/// singleton). OWN-only: `resolve_slot` reads the receiver's own shape and does
/// NOT walk the prototype chain (unlike `obj_get`). The word return makes it usable
/// from the uniform-word dynamic-method dispatch (unlike `__rtsadp_obj_has`'s raw
/// i64).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_has_own(obj_word: u64, key_str_handle: u64) -> u64 {
    PolyValue::bool(resolve_slot(obj_word, key_str_handle).is_some()).raw()
}

// ===========================================================================
// PROPERTY DESCRIPTORS + EXTENSIBILITY (real state, NOT mocked).
//
// The shape-slot object stores only the VALUE (in its slot). The descriptor
// FLAGS (writable/enumerable/configurable) and the object's EXTENSIBILITY are
// tracked here, keyed by object word + key. A property created by ordinary
// assignment (`o.k = v`) has NO flags entry and defaults to all-true (a normal
// data property); only `Object.defineProperty`/`Reflect.defineProperty` records
// explicit flags. `obj_set` consults both: a `writable:false` property blocks
// re-assignment, and a NEW key on a non-extensible object is rejected.
// ===========================================================================

/// `(obj_word, key)` → packed flags: bit0 writable, bit1 enumerable, bit2
/// configurable. Absent ⇒ the property is a normal data property (all true).
fn desc_flags_table() -> &'static Mutex<HashMap<(u64, String), u8>> {
    static T: OnceLock<Mutex<HashMap<(u64, String), u8>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Object words marked non-extensible (`Object.preventExtensions`/`freeze`/`seal`).
fn non_extensible_table() -> &'static Mutex<HashSet<u64>> {
    static T: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Whether `obj_word.key` is writable: an explicit flags entry's bit0, or `true`
/// for a normal data property (no entry).
fn prop_writable(obj_word: u64, key: &str) -> bool {
    match desc_flags_table().lock() {
        Ok(t) => t
            .get(&(obj_word, key.to_string()))
            .map(|f| f & 1 != 0)
            .unwrap_or(true),
        Err(_) => true,
    }
}

/// Whether `obj_word.key` is enumerable: an explicit flags entry's bit1, or
/// `true` for a normal data property. `Object.keys`/`values`/`entries`/for-in
/// skip a `defineProperty(.., {enumerable:false})` property through this.
pub(crate) fn prop_enumerable(obj_word: u64, key: &str) -> bool {
    match desc_flags_table().lock() {
        Ok(t) => t
            .get(&(obj_word, key.to_string()))
            .map(|f| f & 2 != 0)
            .unwrap_or(true),
        Err(_) => true,
    }
}

/// Whether `obj_word.key` is configurable (bit2; absent ⇒ true).
fn prop_configurable(obj_word: u64, key: &str) -> bool {
    match desc_flags_table().lock() {
        Ok(t) => t
            .get(&(obj_word, key.to_string()))
            .map(|f| f & 4 != 0)
            .unwrap_or(true),
        Err(_) => true,
    }
}

fn is_non_extensible(obj_word: u64) -> bool {
    non_extensible_table()
        .lock()
        .map(|t| t.contains(&obj_word))
        .unwrap_or(false)
}

/// `Object.defineProperty(o, k, desc)` / `Reflect.defineProperty` — write the
/// VALUE and record the explicit FLAGS (packed: bit0 writable, bit1 enumerable,
/// bit2 configurable). Returns a bool word: `false` when adding a NEW key to a
/// non-extensible object (the define fails), else `true`. A redefine of an
/// existing key always applies here (configurable:false strictness is a later
/// increment — it would only throw, never produce a wrong value).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_define_prop(
    obj_word: u64,
    key_str_handle: u64,
    val_word: u64,
    flags: i64,
) -> i64 {
    // PROXY (#218 phase 3): route through the `defineProperty` trap — ToBoolean of
    // the trap's return (the trap arg is the descriptor's VALUE; the full
    // descriptor object was unpacked by the `.ts` caller). No trap → forward the
    // define to the target.
    if let Some((target, handler)) = proxy_parts(obj_word) {
        let trap_key = abi_adapter::intern_poly("defineProperty").raw();
        let trap = __rtsadp_obj_get(handler, trap_key);
        if PolyValue::from_raw(trap).is_function() {
            let undef = PolyValue::undefined().raw();
            let r = super::funcops::__rtsadp_fn_invoke(
                trap,
                target,
                key_str_handle,
                val_word,
                undef,
                0,
            );
            return PolyValue::from_raw(r).is_truthy() as i64;
        }
        return __rtsadp_define_prop(target, key_str_handle, val_word, flags);
    }
    let is_own = resolve_slot(obj_word, key_str_handle).is_some();
    if !is_own && is_non_extensible(obj_word) {
        return 0;
    }
    // Write the value via the normal slot/transition path. A `writable:false`
    // flags entry must NOT block THIS write (defineProperty sets it), so record the
    // flags AFTER, and write the slot directly here (bypassing obj_set's writable
    // guard) — define always sets its own value.
    define_write_slot(obj_word, key_str_handle, val_word);
    if let Ok(mut t) = desc_flags_table().lock() {
        t.insert((obj_word, key_text(key_str_handle)), (flags & 0x7) as u8);
    }
    1
}

/// `Object.getOwnPropertyDescriptor(obj, key)` — build the DATA descriptor object
/// `{ value, writable, enumerable, configurable }` for an OWN property, or
/// `undefined` when `key` is not an own property of `obj`. Flags come from the
/// descriptor table (a plain assignment is all-true; `defineProperty` carries its
/// own). Accessor (`get`/`set`) descriptors are a later increment.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_get_own_property_descriptor(obj_word: u64, key_word: u64) -> u64 {
    // PROXY (#218 phase 3): the `getOwnPropertyDescriptor` trap returns the
    // descriptor object verbatim; no trap → synthesize from the target.
    if let Some((target, handler)) = proxy_parts(obj_word) {
        let trap_key = abi_adapter::intern_poly("getOwnPropertyDescriptor").raw();
        let trap = __rtsadp_obj_get(handler, trap_key);
        if PolyValue::from_raw(trap).is_function() {
            let undef = PolyValue::undefined().raw();
            return super::funcops::__rtsadp_fn_invoke(trap, target, key_word, undef, undef, 0);
        }
        return __rtsadp_obj_get_own_property_descriptor(target, key_word);
    }
    let flags = __rtsadp_prop_flags(obj_word, key_word);
    if flags < 0 {
        return PolyValue::undefined().raw();
    }
    let obj_handle = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let empty_shape = crate::shape::intern_global_shape(&[]);
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(
        obj_handle,
        PolyValue::from_i32(empty_shape as i32).raw() as i64,
    );
    let desc =
        PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(obj_handle)).raw();
    let val = __rtsadp_obj_get(obj_word, key_word);
    __rtsadp_obj_set(desc, abi_adapter::intern_poly("value").raw(), val);
    let b = |bit: i64| PolyValue::bool(flags & bit != 0).raw();
    __rtsadp_obj_set(desc, abi_adapter::intern_poly("writable").raw(), b(1));
    __rtsadp_obj_set(desc, abi_adapter::intern_poly("enumerable").raw(), b(2));
    __rtsadp_obj_set(desc, abi_adapter::intern_poly("configurable").raw(), b(4));
    desc
}

/// `Object.getOwnPropertyDescriptors(obj)` — an object mapping each own key to its
/// descriptor object (`{ k: { value, writable, enumerable, configurable }, … }`).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_get_own_property_descriptors(obj_word: u64) -> u64 {
    let res_handle = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let empty_shape = crate::shape::intern_global_shape(&[]);
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(
        res_handle,
        PolyValue::from_i32(empty_shape as i32).raw() as i64,
    );
    let res =
        PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(res_handle)).raw();
    let keys = object_keys_vec(obj_word);
    for k in &keys {
        // An accessor slot pair (`__get_<name>` / `__set_<name>` — how a literal
        // getter/setter is stored): emit ONE accessor descriptor `{ get, set,
        // enumerable, configurable }` under the REAL name; never leak the
        // internal keys (#749).
        if let Some(name) = k.strip_prefix("__get_") {
            let desc = accessor_descriptor(obj_word, name);
            __rtsadp_obj_set(res, abi_adapter::intern_poly(name).raw(), desc);
            continue;
        }
        if let Some(name) = k.strip_prefix("__set_") {
            // Setter-only accessor (no paired getter — that case already emitted).
            if !keys.iter().any(|g| g == &format!("__get_{name}")) {
                let desc = accessor_descriptor(obj_word, name);
                __rtsadp_obj_set(res, abi_adapter::intern_poly(name).raw(), desc);
            }
            continue;
        }
        let key_word = abi_adapter::intern_poly(k).raw();
        let desc = __rtsadp_obj_get_own_property_descriptor(obj_word, key_word);
        __rtsadp_obj_set(res, key_word, desc);
    }
    res
}

/// Build an ACCESSOR descriptor `{ get, set, enumerable, configurable }` for the
/// property `name` of `obj_word`, reading the stored `__get_<name>` /
/// `__set_<name>` slots (absent slot → `undefined`, JS accessor-descriptor form).
fn accessor_descriptor(obj_word: u64, name: &str) -> u64 {
    let desc_handle = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let empty_shape = crate::shape::intern_global_shape(&[]);
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(
        desc_handle,
        PolyValue::from_i32(empty_shape as i32).raw() as i64,
    );
    let desc = PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(
        desc_handle,
    ))
    .raw();
    let getter = __rtsadp_obj_get(obj_word, abi_adapter::intern_poly(&format!("__get_{name}")).raw());
    let setter = __rtsadp_obj_get(obj_word, abi_adapter::intern_poly(&format!("__set_{name}")).raw());
    __rtsadp_obj_set(desc, abi_adapter::intern_poly("get").raw(), getter);
    __rtsadp_obj_set(desc, abi_adapter::intern_poly("set").raw(), setter);
    let t = PolyValue::bool(true).raw();
    __rtsadp_obj_set(desc, abi_adapter::intern_poly("enumerable").raw(), t);
    __rtsadp_obj_set(desc, abi_adapter::intern_poly("configurable").raw(), t);
    desc
}

/// `Object.defineProperty(obj, key, descriptor)` — read the DATA descriptor's
/// `value` + `writable`/`enumerable`/`configurable` flags off the descriptor
/// object and apply them via [`__rtsadp_define_prop`]. Omitted flags default to
/// `false` (JS `defineProperty` semantics, unlike a plain assignment's all-true).
/// Returns the object. (Accessor descriptors `get`/`set` are a later increment —
/// only the `value` data form is read here.)
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_define_property(obj_word: u64, key_word: u64, desc_word: u64) -> u64 {
    // A flag OMITTED from the descriptor stays TRUE (#749: only an explicit
    // `writable:false`/`enumerable:false` marks the property); an explicit value
    // reads as its ToBoolean. (Full JS omitted⇒false strictness is a later
    // increment — it would flip properties nobody asked about.)
    let flag = |k: &str| -> i64 {
        let key = abi_adapter::intern_poly(k).raw();
        if __rtsadp_obj_has(desc_word, key) == 0 {
            return 1;
        }
        let w = __rtsadp_obj_get(desc_word, key);
        super::genops::to_boolean(PolyValue::from_raw(w)) as i64
    };
    // ACCESSOR descriptor (`{ get: fn, set?: fn }`): store the accessor fns
    // under the canonical `__get_<key>` / `__set_<key>` slots (the same storage
    // an object-literal getter uses) — the dynamic `obj_get` invokes `__get_<k>`
    // on a key miss (walking the prototype chain), so a
    // `defineProperty(C.prototype, k, { get })` read works on every instance.
    let getter = __rtsadp_obj_get(desc_word, abi_adapter::intern_poly("get").raw());
    let setter = __rtsadp_obj_get(desc_word, abi_adapter::intern_poly("set").raw());
    if PolyValue::from_raw(getter).is_function() || PolyValue::from_raw(setter).is_function() {
        let key = key_text(key_word);
        if PolyValue::from_raw(getter).is_function() {
            __rtsadp_obj_set(
                obj_word,
                abi_adapter::intern_poly(&format!("__get_{key}")).raw(),
                getter,
            );
        }
        if PolyValue::from_raw(setter).is_function() {
            __rtsadp_obj_set(
                obj_word,
                abi_adapter::intern_poly(&format!("__set_{key}")).raw(),
                setter,
            );
        }
        return obj_word;
    }
    let val = __rtsadp_obj_get(desc_word, abi_adapter::intern_poly("value").raw());
    let flags = flag("writable") | (flag("enumerable") << 1) | (flag("configurable") << 2);
    __rtsadp_define_prop(obj_word, key_word, val, flags);
    obj_word
}

/// Write `obj_word.key = value` at the slot (existing) or via a shape transition
/// (new key), WITHOUT the writable/extensibility guards — used by `define_prop`
/// (which has already checked extensibility and intentionally overrides writable).
fn define_write_slot(obj_word: u64, key_str_handle: u64, val_word: u64) {
    if let Some((handle, idx)) = resolve_slot(obj_word, key_str_handle) {
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(handle, 1 + idx, val_word as i64);
        return;
    }
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
}

/// `Reflect.getOwnPropertyDescriptor` helper → packed flags (bit0 writable, bit1
/// enumerable, bit2 configurable) of an OWN property, or `-1` when `key` is not an
/// own property. A normal data property (no explicit flags entry) is `7` (all true).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_prop_flags(obj_word: u64, key_str_handle: u64) -> i64 {
    if resolve_slot(obj_word, key_str_handle).is_none() {
        return -1;
    }
    let key = key_text(key_str_handle);
    match desc_flags_table().lock() {
        Ok(t) => *t.get(&(obj_word, key)).unwrap_or(&7) as i64,
        Err(_) => 7,
    }
}

/// `o.propertyIsEnumerable(k)` → a BOXED bool word. Polymorphic over the
/// receiver TAG: an ARRAY answers true for an in-bounds numeric index (its
/// element props are enumerable; `length` and any other name are not); a KEYED
/// object answers its own-key enumerable flag (`prop_flags` bit 2, default
/// true); anything else (primitives, missing key) → false.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_prop_is_enumerable(obj_word: u64, key_word: u64) -> u64 {
    let obj = PolyValue::from_raw(obj_word);
    if obj.is_object() && !looks_like_object(obj) {
        // Array (a non-keyed Vec): in-bounds index ⇒ enumerable own element.
        if let Ok(i) = key_text(key_word).parse::<i64>() {
            let lw = super::dyndispatch::__rtsadp_dyn_length(obj_word);
            let len = super::genops::to_number(PolyValue::from_raw(lw));
            return PolyValue::bool(i >= 0 && (i as f64) < len).raw();
        }
        return PolyValue::bool(false).raw();
    }
    let flags = __rtsadp_prop_flags(obj_word, key_word);
    PolyValue::bool(flags >= 0 && (flags & 2) != 0).raw()
}

/// RAW chain lookup: the stored word for `key` on `obj_word` or any prototype
/// (bounded walk), WITHOUT invoking accessors — used to find `__get_<k>` fn
/// words. `undefined` when absent.
fn lookup_chain(obj_word: u64, key_str_handle: u64, depth: u32) -> u64 {
    if depth > 64 {
        return PolyValue::undefined().raw();
    }
    if let Some((handle, idx)) = resolve_slot(obj_word, key_str_handle) {
        return rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, 1 + idx) as u64;
    }
    match super::protos::proto_of(obj_word) {
        Some(proto) => lookup_chain(proto, key_str_handle, depth + 1),
        None => PolyValue::undefined().raw(),
    }
}

/// `Object.preventExtensions(o)` / `Reflect.preventExtensions` — mark `o`
/// non-extensible (no new keys). Returns a bool word `true`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_prevent_ext(obj_word: u64) -> i64 {
    if PolyValue::from_raw(obj_word).is_object() {
        if let Ok(mut t) = non_extensible_table().lock() {
            t.insert(obj_word);
        }
    }
    1
}

/// `Object.freeze(o)` — non-extensible + every CURRENT own key marked
/// `writable:false` (and non-configurable, bit2 clear), so an existing-key
/// re-assignment is a silent no-op (JS non-strict) via `obj_set`'s
/// `prop_writable` gate. `seal`/`preventExtensions` use
/// [`__rtsadp_prevent_ext`] instead (they keep existing keys writable).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_freeze(obj_word: u64) -> i64 {
    if PolyValue::from_raw(obj_word).is_object() {
        if let Ok(mut t) = non_extensible_table().lock() {
            t.insert(obj_word);
        }
        let keys = object_keys_vec(obj_word);
        if let Ok(mut t) = desc_flags_table().lock() {
            for k in keys {
                // bit0 writable=0, bit1 enumerable=1, bit2 configurable=0.
                t.insert((obj_word, k), 0b010);
            }
        }
    }
    1
}

/// `Object.isFrozen(o)` → 1/0: non-extensible AND every own key `writable:false`
/// + `configurable:false`. A NON-object is frozen (`1`), matching JS (primitives
/// are trivially frozen). Mirrors what [`__rtsadp_freeze`] records.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_is_frozen(obj_word: u64) -> i64 {
    if !PolyValue::from_raw(obj_word).is_object() {
        return 1;
    }
    if !is_non_extensible(obj_word) {
        return 0;
    }
    let keys = object_keys_vec(obj_word);
    match desc_flags_table().lock() {
        Ok(t) => keys
            .iter()
            .all(|k| {
                t.get(&(obj_word, k.clone()))
                    // bit0 writable, bit2 configurable — both must be OFF.
                    .map(|f| f & 0b101 == 0)
                    .unwrap_or(false)
            }) as i64,
        Err(_) => 0,
    }
}

/// `Object.seal(o)` — non-extensible + every CURRENT own key marked
/// `configurable:false` (writable/enumerable stay as-is), so `isSealed` is true
/// but existing keys remain writable (JS seal semantics; freeze also clears
/// writable). `preventExtensions` alone does NOT touch key flags.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_seal(obj_word: u64) -> i64 {
    if PolyValue::from_raw(obj_word).is_object() {
        if let Ok(mut t) = non_extensible_table().lock() {
            t.insert(obj_word);
        }
        let keys = object_keys_vec(obj_word);
        if let Ok(mut t) = desc_flags_table().lock() {
            for k in keys {
                let cur = t.get(&(obj_word, k.clone())).copied().unwrap_or(0b111);
                t.insert((obj_word, k), cur & !0b100);
            }
        }
    }
    1
}

/// `Object.isSealed(o)` → 1/0: non-extensible AND every own key
/// `configurable:false` (writable may stay true — `seal` permits updates).
/// A bare `preventExtensions` object with default-configurable keys is NOT
/// sealed (matching JS); an empty non-extensible object IS.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_is_sealed(obj_word: u64) -> i64 {
    if !PolyValue::from_raw(obj_word).is_object() {
        return 1;
    }
    if !is_non_extensible(obj_word) {
        return 0;
    }
    object_keys_vec(obj_word)
        .iter()
        .all(|k| !prop_configurable(obj_word, k)) as i64
}

/// `Object.isExtensible(o)` / `Reflect.isExtensible` → 1/0. A non-object is not
/// extensible (`0`), matching JS.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_is_extensible(obj_word: u64) -> i64 {
    (PolyValue::from_raw(obj_word).is_object() && !is_non_extensible(obj_word)) as i64
}

/// `delete obj.key` — slot removal via a shape transition (the inverse of the
/// `obj_set` key-append). Resolve the key's slot: ABSENT (or a non-object) → `1`
/// (a no-op delete evaluates to `true` in JS). PRESENT → shift the value slots
/// after it down by one over the hole, drop the now-duplicate tail slot, and set
/// slot 0 to the shape WITHOUT the key. Always returns `1` (own/configurable/absent
/// deletes are all `true`; this model has no non-configurable props). A PROXY
/// receiver routes through its `deleteProperty` trap (#218) — ToBoolean of the
/// trap's return when defined; otherwise forward the delete to the target.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_delete(obj_word: u64, key_str_handle: u64) -> i64 {
    if let Some((target, handler)) = proxy_parts(obj_word) {
        let trap_key = abi_adapter::intern_poly("deleteProperty").raw();
        let trap = __rtsadp_obj_get(handler, trap_key);
        if PolyValue::from_raw(trap).is_function() {
            let undef = PolyValue::undefined().raw();
            let r = super::funcops::__rtsadp_fn_invoke(
                trap,
                target,
                key_str_handle,
                undef,
                undef,
                0,
            );
            return PolyValue::from_raw(r).is_truthy() as i64;
        }
        return __rtsadp_obj_delete(target, key_str_handle);
    }
    let Some((handle, idx)) = resolve_slot(obj_word, key_str_handle) else {
        return 1;
    };
    // Read the keys + intern the key-less shape BEFORE mutating the Vec: the shift +
    // pop below transiently breaks the shape↔length invariant `object_keys_vec`
    // relies on, so computing the new shape after the mutation would read empty.
    let mut keys = object_keys_vec(obj_word);
    keys.remove(idx as usize);
    let new_shape = crate::shape::intern_global_shape(&keys);
    let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(handle).max(0);
    // Shift the value slots (1+idx+1 .. len) down by one, over the removed slot.
    let mut j = 1 + idx;
    while j + 1 < len {
        let next = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, j + 1);
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(handle, j, next);
        j += 1;
    }
    // Drop the now-duplicate tail slot, then commit the key-less shape header.
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_POP(handle);
    let slot0 = PolyValue::from_i32(new_shape as i32).raw() as i64;
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(handle, 0, slot0);
    1
}

/// Recover a keyed object's ordered keys from its slot-0 global shape-id. Empty
/// for a non-object / unrecognized header (the safe default — `Object.keys` of a
/// non-object yields `[]`).
/// `Object.assign(target, source)` — copy every OWN enumerable key of `source`
/// onto `target` (JS, last write wins). Adds NEW keys to `target` (shape
/// transition via `__rtsadp_obj_set`). Returns `target`. A non-object `source`
/// contributes nothing. The N-source form chains this per source.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_assign(target_word: u64, source_word: u64) -> u64 {
    for k in object_keys_vec(source_word) {
        let key_word = abi_adapter::intern_poly(&k).raw();
        let val = __rtsadp_obj_get(source_word, key_word);
        __rtsadp_obj_set(target_word, key_word, val);
    }
    target_word
}

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
    let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    // ENUMERATION order (array-index keys ascending first) — read each value BY KEY
    // (`obj_get`), not by storage slot, so the reorder and the value stay aligned.
    for k in super::iterops::reorder_enum_keys(object_keys_vec(obj_word)) {
        if !prop_enumerable(obj_word, &k) {
            continue;
        }
        let v = __rtsadp_obj_get(obj_word, abi_adapter::intern_poly(&k).raw());
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, v as i64);
    }
    array_word(out)
}

/// `__rtsadp_obj_entries(obj_word)` — `Object.entries(obj)` at RUNTIME: a fresh
/// array of `[key, value]` 2-element sub-arrays (each its own `Entry::Vec`).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_entries(obj_word: u64) -> u64 {
    let outer = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    for k in super::iterops::reorder_enum_keys(object_keys_vec(obj_word)) {
        if !prop_enumerable(obj_word, &k) {
            continue;
        }
        let key_word = abi_adapter::intern_poly(&k).raw();
        let v = __rtsadp_obj_get(obj_word, key_word);
        let pair = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(pair, key_word as i64);
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(pair, v as i64);
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(outer, array_word(pair) as i64);
    }
    array_word(outer)
}
