//! Codegen-owned ITERATION-source trampolines (P5.10) — for-of / for-in.
//!
//! The loop lowering ([`crate::front::run::stmt`]) desugars every iterating loop
//! to ONE shared index walk over a real `Entry::Vec` of boxed PolyValue WORDS (the
//! engine's array representation): `for (i in 0..VEC_LEN) { x = VEC_GET(arr, i); …
//! }`. An ARRAY iterable already IS such a Vec, so it feeds the walk directly. The
//! two non-array iterables this increment supports — a STRING (for-of) and an
//! OBJECT (for-in) — are first MATERIALIZED into such a Vec by these trampolines,
//! so the walk is representation-identical for all three:
//!
//! - [`__rtsadp_str_chars`] — a string's code points as a fresh array of one-char
//!   string PolyValue words (JS `for (const c of str)` iterates code points).
//! - [`__rtsadp_obj_keys`] — a keyed object's OWN enumerable keys as a fresh array
//!   of string PolyValue words (JS `for (const k in obj)` iterates key strings),
//!   recovered from the object's slot-0 global shape-id (the SAME registry the
//!   inspect / dynamic-property paths read, so the key set never diverges).
//!
//! Convention (matching the rest of the `__rtsadp_*` surface): the source crosses
//! as a raw `u64` PolyValue word; the result is a fresh `Entry::Vec` boxed as a
//! `TAG_OBJECT` array PolyValue word.

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use crate::shape::global_shape_keys;

use super::inspect::looks_like_object;
use super::{PolyValue, abi_adapter};

/// Box a fresh real Vec handle as a `TAG_OBJECT` array PolyValue word (the engine's
/// array representation), matching [`super::globalops`]'s array-producing helpers.
fn box_vec_as_array(vec_handle: u64) -> u64 {
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(vec_handle)).raw()
}

/// `__rtsadp_str_chars(str_word)` — the code points of a string as a fresh array
/// whose elements are one-char string PolyValue words (real pool). JS `for...of`
/// over a string yields code points (not UTF-16 units), so we iterate Rust `chars`
/// (Unicode scalar values). A non-string source yields an EMPTY array — the
/// lowering only routes a proven string here, so this is just the inner safety net.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_str_chars(str_word: u64) -> u64 {
    let v = PolyValue::from_raw(str_word);
    let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    if v.is_string() {
        let s = abi_adapter::resolve_poly(v);
        for ch in s.chars() {
            let word = abi_adapter::intern_poly(&ch.to_string()).raw() as i64;
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, word);
        }
    }
    box_vec_as_array(vec)
}

/// `__rtsadp_to_iter_array(word)` — coerce an UNPROVEN for-of source to an array of
/// element words to walk: an ARRAY rides its own handle (returned verbatim); a
/// STRING is materialized to its code-point char array (`str_chars`); anything else
/// yields an EMPTY array (JS would throw "not iterable" — we have no throw channel
/// in for-of, and the lowering only routes here for values that are plausibly
/// iterable, i.e. NOT a known class instance, so an empty walk is the honest
/// no-throw fallback). This is what lets `for (const ch of s)` (string PARAM) and
/// `for (const x of row)` (nested-array for-of binding) iterate without a static
/// proof of the source's kind.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_to_iter_array(word: u64) -> u64 {
    let v = PolyValue::from_raw(word);
    if v.is_object() && !looks_like_object(v) {
        // Already an array (Vec-backed, NOT a shaped object): walk it directly.
        return word;
    }
    if v.is_string() {
        return __rtsadp_str_chars(word);
    }
    // A level-B typed-array VIEW: materialize the elements (read through the
    // shared buffer) into a fresh array.
    if let Some((bh, bytes, signed, float)) = super::taops::view_parts(word) {
        let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
        let n = super::taops::view_len(bh, bytes);
        for i in 0..n {
            let w = super::taops::view_get(bh, bytes, signed, float, i);
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, w as i64);
        }
        return box_vec_as_array(out);
    }
    // A `.ts` stdlib SET instance reached dynamically (a nested Set element, a
    // param): its elements live in the private `#items` array slot. A MAP yields
    // `[k, v]` pairs from `#keys`/`#vals`. Detected by SHAPE (the engine-owned
    // private slot names), never by class name.
    if v.is_object() {
        let items = super::objops::__rtsadp_obj_get(word, abi_adapter::intern_poly("#items").raw());
        let iv = PolyValue::from_raw(items);
        if iv.is_object() && !looks_like_object(iv) {
            return items;
        }
        let ks = super::objops::__rtsadp_obj_get(word, abi_adapter::intern_poly("#keys").raw());
        let vs = super::objops::__rtsadp_obj_get(word, abi_adapter::intern_poly("#vals").raw());
        let (kv, vv) = (PolyValue::from_raw(ks), PolyValue::from_raw(vs));
        if kv.is_object() && !looks_like_object(kv) && vv.is_object() && !looks_like_object(vv) {
            let kh = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(kv.as_handle());
            let vh = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(vv.as_handle());
            let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(kh).max(0);
            let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
            for i in 0..len {
                let pair = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
                let k = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(kh, i);
                let vl = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(vh, i);
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(pair, k);
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(pair, vl);
                let pair_word = PolyValue::from_object_handle(
                    rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(pair),
                )
                .raw() as i64;
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, pair_word);
            }
            return box_vec_as_array(out);
        }
    }
    box_vec_as_array(rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW())
}

/// `__rtsadp_obj_keys(obj_word)` — the OWN enumerable keys of a keyed object as a
/// fresh array of string PolyValue words, in insertion order (the shape's ordered
/// key list). Recovered from the object's slot-0 global shape-id via the SAME
/// process-global registry the inspect / dynamic-property trampolines read. A
/// non-object source (or one without a live shape header) yields an EMPTY array —
/// the lowering only routes a proven keyed object here.
/// JS own-property ENUMERATION order: ARRAY-INDEX keys (a canonical non-negative
/// integer string `< 2^32-1`, no leading zero) come FIRST in ascending NUMERIC
/// order, then every other key in insertion order. `{ "10": …, "3": …, "b": …,
/// "1": … }` enumerates `1, 3, 10, b`. Used for `Object.keys`/`getOwnPropertyNames`/
/// `for-in` — NOT for the storage-order `object_keys_vec` (slot manipulation).
pub(crate) fn reorder_enum_keys(keys: Vec<String>) -> Vec<String> {
    let as_index = |s: &str| -> Option<u32> {
        if s == "0" {
            return Some(0);
        }
        if s.is_empty() || s.as_bytes()[0] == b'0' || !s.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        s.parse::<u32>().ok().filter(|&n| n != u32::MAX)
    };
    let mut idx: Vec<(u32, String)> = Vec::new();
    let mut rest: Vec<String> = Vec::new();
    for k in keys {
        match as_index(&k) {
            Some(n) => idx.push((n, k)),
            None => rest.push(k),
        }
    }
    idx.sort_by_key(|(n, _)| *n);
    idx.into_iter().map(|(_, k)| k).chain(rest).collect()
}

/// `Reflect.ownKeys(target)` — the trap's list VERBATIM (the ECMA `ownKeys`
/// reflector does NOT run the per-key enumerability filter `Object.keys` does).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_own_keys_raw(obj_word: u64) -> u64 {
    if let Some((target, handler)) = super::objops::proxy_parts(obj_word) {
        let trap_key = abi_adapter::intern_poly("ownKeys").raw();
        let trap = super::objops::__rtsadp_obj_get(handler, trap_key);
        if PolyValue::from_raw(trap).is_function() {
            let undef = PolyValue::undefined().raw();
            return super::funcops::__rtsadp_fn_invoke(trap, target, undef, undef, undef, 0);
        }
        return __rtsadp_own_keys_raw(target);
    }
    obj_keys_impl(obj_word, false)
}

#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_keys(obj_word: u64) -> u64 {
    // PROXY (#218): the `ownKeys` trap lists the keys; JS's `Object.keys` then
    // runs [[GetOwnProperty]] PER KEY (trap/forward) and keeps only ENUMERABLE
    // ones — a trap key absent from the target (and with no getOwnDesc trap)
    // yields `undefined` → filtered (Bun/Node return `[]` for that shape). No
    // trap → forward to the target.
    if let Some((target, handler)) = super::objops::proxy_parts(obj_word) {
        let trap_key = abi_adapter::intern_poly("ownKeys").raw();
        let trap = super::objops::__rtsadp_obj_get(handler, trap_key);
        if PolyValue::from_raw(trap).is_function() {
            let undef = PolyValue::undefined().raw();
            let keys_word =
                super::funcops::__rtsadp_fn_invoke(trap, target, undef, undef, undef, 0);
            let keys = PolyValue::from_raw(keys_word);
            if !keys.is_object() {
                return keys_word;
            }
            let kh = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(keys.as_handle());
            let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(kh).max(0);
            let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
            let enum_key = abi_adapter::intern_poly("enumerable").raw();
            for i in 0..len {
                let k = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(kh, i) as u64;
                let desc = super::objops::__rtsadp_obj_get_own_property_descriptor(obj_word, k);
                let dv = PolyValue::from_raw(desc);
                let keep = dv.is_object()
                    && PolyValue::from_raw(super::objops::__rtsadp_obj_get(desc, enum_key))
                        .is_truthy();
                if keep {
                    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, k as i64);
                }
            }
            return PolyValue::from_object_handle(
                rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(out),
            )
            .raw();
        }
        return __rtsadp_obj_keys(target);
    }
    obj_keys_impl(obj_word, true)
}

/// The shared key-enumeration core. `enumerable_only` = `Object.keys`/for-in
/// semantics (skip `defineProperty(.., {enumerable:false})` properties);
/// `false` = `Reflect.ownKeys`/`getOwnPropertyNames` (every own string key).
fn obj_keys_impl(obj_word: u64, enumerable_only: bool) -> u64 {
    let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let obj = PolyValue::from_raw(obj_word);
    if obj.is_object() && looks_like_object(obj) {
        let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(obj.as_handle());
        let slot0 = PolyValue::from_raw(rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, 0) as u64);
        if let Some(keys) = slot0
            .is_int32()
            .then(|| global_shape_keys(slot0.as_i32() as u32))
            .flatten()
        {
            let mut listed: Vec<String> = Vec::new();
            for k in reorder_enum_keys(keys) {
                // Symbol-keyed entries (`@@sym:<handle>` canonical repr, #798) are
                // NOT string-enumerable: `Object.keys`/for-in/`JSON.stringify`
                // all skip them (JS spec). `Reflect.ownKeys` uses the raw path.
                if k.starts_with("@@sym:") {
                    continue;
                }
                // A `defineProperty(.., {enumerable:false})` property is skipped
                // by Object.keys/values/entries/for-in (JS spec).
                if enumerable_only && !super::objops::prop_enumerable(obj_word, &k) {
                    continue;
                }
                // ACCESSOR slots (`__get_<k>`/`__set_<k>` — materialized literal
                // getters / defineProperty accessors): enumerate as the PROPERTY
                // name (a getter IS an enumerable own property in JS), deduped.
                let name = k
                    .strip_prefix("__get_")
                    .or_else(|| k.strip_prefix("__set_"))
                    .unwrap_or(&k)
                    .to_string();
                if listed.iter().any(|x| x == &name) {
                    continue;
                }
                listed.push(name.clone());
                let word = abi_adapter::intern_poly(&name).raw() as i64;
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, word);
            }
        }
    } else if obj.is_object() {
        // ARRAY receiver: `Object.keys([a, b, c])` is the STRING indices
        // `["0", "1", "2"]` (JS treats an array as an object whose own enumerable
        // keys are its indices). The element count is the backing Vec's length.
        let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(obj.as_handle());
        let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(handle).max(0);
        for i in 0..len {
            // A HOLE index (sparse elision / `delete arr[i]`) is NOT an own key.
            let elem = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, i) as u64;
            if PolyValue::from_raw(elem).is_hole() {
                continue;
            }
            let word = abi_adapter::intern_poly(&i.to_string()).raw() as i64;
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, word);
        }
    } else if obj.is_string() {
        // STRING receiver: `for (const i in "abc")` enumerates the CODE-UNIT
        // indices `"0".."len-1"` (JS string exotic object). Everything else
        // (numbers, bools, null/undefined) has no enumerable keys → `[]`,
        // which makes a for-in over it iterate zero times (JS semantics).
        let len = abi_adapter::resolve_poly(obj).encode_utf16().count();
        for i in 0..len {
            let word = abi_adapter::intern_poly(&i.to_string()).raw() as i64;
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, word);
        }
    }
    box_vec_as_array(vec)
}

/// `Object.getOwnPropertySymbols(o)` — the SYMBOL-keyed own entries, decoded
/// from their canonical `@@sym:<handle>` storage keys (#798) back to symbol
/// words, in shape order. Non-keyed receivers → `[]`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_own_symbols(obj_word: u64) -> u64 {
    let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let obj = PolyValue::from_raw(obj_word);
    if obj.is_object() && looks_like_object(obj) {
        let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(obj.as_handle());
        let slot0 = PolyValue::from_raw(rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, 0) as u64);
        if let Some(keys) = slot0
            .is_int32()
            .then(|| global_shape_keys(slot0.as_i32() as u32))
            .flatten()
        {
            for k in keys {
                if let Some(h) = k.strip_prefix("@@sym:").and_then(|s| s.parse::<u64>().ok()) {
                    let word = PolyValue::from_object_handle(
                        rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(h),
                    )
                    .raw() as i64;
                    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, word);
                }
            }
        }
    }
    box_vec_as_array(vec)
}

/// `Object.getOwnPropertyNames(x)` — like [`__rtsadp_obj_keys`] but includes the
/// NON-enumerable own properties JS exposes here: for an ARRAY, the trailing
/// `"length"` (`getOwnPropertyNames([a,b,c])` is `["0","1","2","length"]`). For a
/// keyed object it equals `Object.keys` (the corpus has no non-enumerable own
/// props on plain objects). Reuses `__rtsadp_obj_keys` then appends `"length"`
/// when the receiver is an array (object, not a keyed shape).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_own_names(obj_word: u64) -> u64 {
    let obj = PolyValue::from_raw(obj_word);
    // A STRING BOX (`new String("hi")` — a keyed object whose primitive lives in
    // the `__prim` slot): JS exposes the code-unit indices + `length` as own
    // property names (#789). Shape-detected (the `__prim` slot holding a string),
    // never by class name.
    if obj.is_object() && looks_like_object(obj) {
        let prim = super::objops::__rtsadp_obj_get(
            obj_word,
            abi_adapter::intern_poly("__prim").raw(),
        );
        if PolyValue::from_raw(prim).is_string() {
            let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
            let len = abi_adapter::resolve_poly(PolyValue::from_raw(prim))
                .encode_utf16()
                .count();
            for i in 0..len {
                let word = abi_adapter::intern_poly(&i.to_string()).raw() as i64;
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, word);
            }
            let word = abi_adapter::intern_poly("length").raw() as i64;
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, word);
            return box_vec_as_array(vec);
        }
    }
    // A raw STRING primitive: indices + "length" too.
    if obj.is_string() {
        let keys_arr = __rtsadp_obj_keys(obj_word);
        let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(
            PolyValue::from_raw(keys_arr).as_handle(),
        );
        let word = abi_adapter::intern_poly("length").raw() as i64;
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(handle, word);
        return keys_arr;
    }
    // Keyed OBJECT: the OWN names include non-enumerable properties
    // (`defineProperty(.., {enumerable:false})`) — the enumerable_only=false
    // pass, unlike `Object.keys`.
    if obj.is_object() && looks_like_object(obj) {
        return obj_keys_impl(obj_word, false);
    }
    let keys_arr = __rtsadp_obj_keys(obj_word);
    if obj.is_object() && !looks_like_object(obj) {
        // ARRAY: append the own non-enumerable "length".
        let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(
            PolyValue::from_raw(keys_arr).as_handle(),
        );
        let word = abi_adapter::intern_poly("length").raw() as i64;
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(handle, word);
    }
    keys_arr
}

// ===========================================================================
// Tagged-template `strings.raw` (String.raw / custom tags): the desugar builds
// the COOKED strings array and the RAW strings array, pairs them here, and the
// dynamic `.raw` property read on the cooked array resolves via this table.
// Keyed by the cooked array's WORD (stable — the payload is its heap slot).
// ===========================================================================

fn tsa_raw_table() -> &'static std::sync::Mutex<std::collections::HashMap<u64, u64>> {
    static T: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u64, u64>>> =
        std::sync::OnceLock::new();
    T.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// `engine.tsa_raw(cooked, raw)` — record `cooked.raw = raw` and return the
/// cooked array word (the tag call's first arg).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_tsa_raw(cooked_word: u64, raw_word: u64) -> u64 {
    if let Ok(mut t) = tsa_raw_table().lock() {
        t.insert(cooked_word, raw_word);
    }
    cooked_word
}

/// The paired RAW strings array of a tagged-template cooked array, if any.
pub(crate) fn tsa_raw_of(cooked_word: u64) -> Option<u64> {
    tsa_raw_table().lock().ok()?.get(&cooked_word).copied()
}

/// `String.raw(callSite, ...subs)` — the FUNCTION-call form (the tag form is
/// desugared at compile time): read `callSite.raw` (array or array-like with
/// `length`), interleave its ToString'd segments with the substitutions.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_string_raw(callsite_word: u64, subs_word: u64) -> u64 {
    use rts_runtime::namespaces::gc::handles as rt_handles;
    let raw_key = abi_adapter::intern_poly("raw").raw();
    let raw = super::objops::__rtsadp_obj_get(callsite_word, raw_key);
    let len_w = super::dyndispatch::__rtsadp_dyn_length(raw);
    let len = {
        let v = PolyValue::from_raw(len_w);
        if v.is_int32() {
            v.as_i32() as i64
        } else if v.is_double() {
            v.as_f64() as i64
        } else {
            0
        }
    };
    let subs: Vec<i64> = {
        let v = PolyValue::from_raw(subs_word);
        if v.is_object() {
            let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
            rt_handles::with_entry(h, |e| match e {
                Some(rt_handles::Entry::Vec(items)) => items.as_ref().clone(),
                _ => Vec::new(),
            })
        } else {
            Vec::new()
        }
    };
    let mut out = String::new();
    for i in 0..len {
        let idx_w = PolyValue::from_i32(i as i32).raw();
        let seg = super::dyndispatch::__rtsadp_idx_get(raw, idx_w);
        let seg_s = super::genops::__rtsadp_to_string(seg);
        out.push_str(&abi_adapter::resolve_poly(PolyValue::from_raw(seg_s)));
        if i + 1 < len {
            if let Some(&s) = subs.get(i as usize) {
                let ss = super::genops::__rtsadp_to_string(s as u64);
                out.push_str(&abi_adapter::resolve_poly(PolyValue::from_raw(ss)));
            }
        }
    }
    abi_adapter::intern_poly(&out).raw()
}
