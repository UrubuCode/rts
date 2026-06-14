//! Codegen-owned Map / Set instance trampolines (P5.3) — PolyValue-native.
//!
//! `new Map()` / `new Set()` are RUNTIME (Registry) classes — the engine does NOT
//! name them (PRIMORDIAL-vs-Registry doctrine). Like every other `__rtsadp_*`
//! surface ([`super::arrayops`] / [`super::globalops`]) these trampolines bridge
//! the engine's [`PolyValue`] value model to the REAL runtime `MAP_*`/`SET_*`
//! symbols (`rts-shared collections::map`), so:
//!
//! - a Map/Set instance is a `TAG_OBJECT` [`PolyValue`] over the REAL Map handle;
//! - a Map VALUE is a raw PolyValue WORD stored in the map's `i64` value slot
//!   (exactly the array convention — the runtime treats the slot as opaque `i64`);
//! - a Map KEY (and a Set ELEMENT) is marshaled from a PolyValue to the runtime's
//!   string-key ABI: a string key uses its content; a number/bool key uses its
//!   ToString; an object/function key BAILS at the lowering (the runtime keys by
//!   string content / handle identity, which would silently diverge from JS
//!   SameValueZero for object keys — refuse, never guess).
//!
//! Keys/values cross as raw `u64` PolyValue words; the trampoline does the
//! string-key marshaling and the box/unbox. The lowering ([`crate::front::run::globalclass`])
//! resolves these via the class-metadata table — ONE generic path keyed by class
//! name, no hardcoded switchboard in the engine.

use rts_runtime::namespaces::collections::map as rt_map;
use rts_runtime::namespaces::gc::handles as rt_handles;

use super::abi_adapter;
use super::{genops, PolyValue};

/// Box a real Map/Set runtime handle as a `TAG_OBJECT` PolyValue word.
fn box_map(handle: u64) -> u64 {
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(handle)).raw()
}

/// The real Map/Set runtime handle behind a `TAG_OBJECT` instance word.
fn unbox_map(word: u64) -> u64 {
    rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(PolyValue::from_raw(word).as_handle())
}

/// JS string-key for a PolyValue key word: a string stays its content; everything
/// else (number/bool/null/undefined) goes through the engine's `ToString` (so
/// `m.set(1, …)` keys on `"1"`, matching the runtime's decimal convention). An
/// object/function key is refused at the lowering, so it never reaches here.
fn key_string(key_word: u64) -> String {
    let pv = PolyValue::from_raw(key_word);
    if pv.is_string() {
        abi_adapter::resolve_poly(pv)
    } else {
        let s_word = genops::__rtsadp_to_string(key_word);
        abi_adapter::resolve_poly(PolyValue::from_raw(s_word))
    }
}

// ===========================================================================
// Map.
// ===========================================================================

/// `new Map()` — a fresh empty Map instance word (marked Map-kind so `instanceof
/// Map` / the unified has/delete take the Map branch).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_map_new() -> u64 {
    let h = rt_map::__RTS_FN_NS_COLLECTIONS_MAP_NEW();
    rt_map::__RTS_FN_NS_COLLECTIONS_MARK_AS_MAP(h);
    box_map(h)
}

/// `m.set(key, value)` — store the raw PolyValue `value_word` under the string key.
/// Returns the SAME map instance word (JS `Map.set` returns the map for chaining).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_map_set(map_word: u64, key_word: u64, value_word: u64) -> u64 {
    let h = unbox_map(map_word);
    let key = key_string(key_word);
    let bytes = key.as_bytes();
    rt_map::__RTS_FN_NS_COLLECTIONS_MAP_SET(
        h,
        bytes.as_ptr(),
        bytes.len() as i64,
        value_word as i64,
    );
    map_word
}

/// `m.get(key)` — the raw PolyValue value word, or `undefined` when absent. Uses
/// the runtime's `_AUTO` getter (returns the `i64::MIN+2` undefined sentinel for a
/// missing key) so a genuinely-stored `undefined`/`0` value is not confused with
/// "absent".
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_map_get(map_word: u64, key_word: u64) -> u64 {
    let h = unbox_map(map_word);
    let key = key_string(key_word);
    let bytes = key.as_bytes();
    let v = rt_map::__RTS_FN_NS_COLLECTIONS_MAP_GET_AUTO(h, bytes.as_ptr(), bytes.len() as i64);
    if v == i64::MIN + 2 {
        PolyValue::undefined().raw()
    } else {
        v as u64
    }
}

/// `m.has(key)` — a PolyValue bool word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_map_has(map_word: u64, key_word: u64) -> u64 {
    let h = unbox_map(map_word);
    let key = key_string(key_word);
    let bytes = key.as_bytes();
    let yes = rt_map::__RTS_FN_NS_COLLECTIONS_MAP_HAS(h, bytes.as_ptr(), bytes.len() as i64) != 0;
    PolyValue::bool(yes).raw()
}

/// `m.delete(key)` — a PolyValue bool word (true iff removed).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_map_delete(map_word: u64, key_word: u64) -> u64 {
    let h = unbox_map(map_word);
    let key = key_string(key_word);
    let bytes = key.as_bytes();
    let removed =
        rt_map::__RTS_FN_NS_COLLECTIONS_MAP_DELETE(h, bytes.as_ptr(), bytes.len() as i64) != 0;
    PolyValue::bool(removed).raw()
}

/// `m.clear()` — remove all entries; returns `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_map_clear(map_word: u64) -> u64 {
    rt_map::__RTS_FN_NS_COLLECTIONS_MAP_CLEAR(unbox_map(map_word));
    PolyValue::undefined().raw()
}

/// `m.size` — the entry count as a tagged int PolyValue word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_map_size(map_word: u64) -> u64 {
    let n = rt_map::__RTS_FN_NS_COLLECTIONS_MAP_LEN(unbox_map(map_word)).max(0);
    genops::number_result(n as f64).raw()
}

// ===========================================================================
// Set — implemented over the same Map machinery (element = key; the runtime's
// `SET_*` symbols key by `set_stable_key`, which preserves identity for objects
// and dedups primitives by value). The handle is MARKED as a Set so the unified
// has/delete take the Set branch.
// ===========================================================================

/// `new Set()` — a fresh empty Set instance word (a Map handle marked Set-kind).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_set_new() -> u64 {
    let h = rt_map::__RTS_FN_NS_COLLECTIONS_MAP_NEW();
    rt_map::__RTS_FN_NS_COLLECTIONS_MARK_AS_SET(h);
    box_map(h)
}

/// The runtime "element raw" for a Set member PolyValue: a STRING element passes
/// its REAL string handle (so the runtime's `set_stable_key` dedups by CONTENT —
/// two `intern_poly("a")` may be distinct handles but share content); any other
/// element passes its raw word (numbers/bools dedup by their decimal/word form;
/// objects dedup by handle identity). Note a number's PolyValue word also dedups
/// by value (the bit-identical word for two equal numbers).
fn set_elem_raw(elem_word: u64) -> i64 {
    let pv = PolyValue::from_raw(elem_word);
    if pv.is_string() {
        abi_adapter::real_handle_of(pv) as i64
    } else {
        elem_word as i64
    }
}

/// `s.add(elem)` — insert the element (dedup by the runtime's stable key). Returns
/// the SAME set instance word (JS `Set.add` chains).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_set_add(set_word: u64, elem_word: u64) -> u64 {
    rt_map::__RTS_FN_NS_COLLECTIONS_SET_ADD(unbox_map(set_word), set_elem_raw(elem_word));
    set_word
}

/// `s.has(elem)` — a PolyValue bool word (identity-/content-aware via the set key).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_set_has(set_word: u64, elem_word: u64) -> u64 {
    let h = unbox_map(set_word);
    // The unified Set/Map `has` keys by `set_stable_key(elem_raw)` on the Set
    // branch; the (key_ptr,key_len) pair is unused there but must be valid.
    let yes =
        rt_map::__RTS_FN_NS_COLLECTIONS_SET_OR_MAP_HAS(h, [].as_ptr(), 0, set_elem_raw(elem_word))
            != 0;
    PolyValue::bool(yes).raw()
}

/// `s.delete(elem)` — a PolyValue bool word (true iff removed).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_set_delete(set_word: u64, elem_word: u64) -> u64 {
    let h = unbox_map(set_word);
    let removed = rt_map::__RTS_FN_NS_COLLECTIONS_SET_OR_MAP_DELETE(
        h,
        [].as_ptr(),
        0,
        set_elem_raw(elem_word),
    ) != 0;
    PolyValue::bool(removed).raw()
}

/// `s.clear()` — remove all elements; returns `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_set_clear(set_word: u64) -> u64 {
    rt_map::__RTS_FN_NS_COLLECTIONS_MAP_CLEAR(unbox_map(set_word));
    PolyValue::undefined().raw()
}

/// `s.size` — the element count as a tagged int PolyValue word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_set_size(set_word: u64) -> u64 {
    let n = rt_map::__RTS_FN_NS_COLLECTIONS_MAP_LEN(unbox_map(set_word)).max(0);
    genops::number_result(n as f64).raw()
}
