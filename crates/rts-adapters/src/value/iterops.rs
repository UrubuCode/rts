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
    // CUSTOM `[Symbol.iterator]` — the REAL protocol; see also the LAZY
    // trampolines (`__rtsadp_iter_open/next/close`) the for-of lowering drives
    // directly (close-on-break). This eager path remains for spread /
    // `Array.from` / `new Map(iterable)`. This is ALSO how the `.ts` stdlib
    // Set/Map materialize: their `*[Symbol.iterator]` generators publish on the
    // class prototype — the engine never reads their private layout. A generator
    // method returns a lazy GenState handle (drained by the state machine); a
    // plain iterator object drives `next()` until `done`, collecting `value`s.
    if v.is_object() {
        let m = custom_iterator_method(word).unwrap_or(PolyValue::undefined().raw());
        if PolyValue::from_raw(m).is_function() {
            let undef = PolyValue::undefined().raw();
            let it = super::funcops::__rtsadp_fn_invoke_method(m, word, undef, undef, undef, 0);
            let itv = PolyValue::from_raw(it);
            if itv.is_object() {
                // A `*[Symbol.iterator]` GENERATOR returns a lazy GenState
                // handle, not a `{next}` object — drain it to a real array.
                let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(itv.as_handle());
                let is_gen = rt_handles::with_entry(h, |e| {
                    matches!(e, Some(rt_handles::Entry::GenState(_)))
                });
                if is_gen {
                    let arr_h = rts_runtime::namespaces::collector::generator::__RTS_FN_NS_GC_GEN_SM_DRAIN(h);
                    return box_vec_as_array(arr_h);
                }
                // An EAGER generator method (`*[Symbol.iterator]` desugared to a
                // `__gen_buf` buffer — the `.ts` stdlib Set/Map shape) returned
                // the materialized element ARRAY itself: walk it directly. The
                // `{next}` drive below would silently read an empty walk off it.
                if !looks_like_object(itv) {
                    return it;
                }
                let next_key = abi_adapter::intern_poly("next").raw();
                let done_key = abi_adapter::intern_poly("done").raw();
                let value_key = abi_adapter::intern_poly("value").raw();
                let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
                // Bounded drive (1M) — a runaway iterator can't hang the walk.
                for _ in 0..1_000_000 {
                    let nm = super::objops::__rtsadp_obj_get(it, next_key);
                    if !PolyValue::from_raw(nm).is_function() {
                        break;
                    }
                    let r = super::funcops::__rtsadp_fn_invoke_method(
                        nm, it, undef, undef, undef, 0,
                    );
                    if !PolyValue::from_raw(r).is_object() {
                        break;
                    }
                    let done = super::objops::__rtsadp_obj_get(r, done_key);
                    if super::genops::__rtsadp_to_boolean(done) != 0 {
                        break;
                    }
                    let val = super::objops::__rtsadp_obj_get(r, value_key);
                    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, val as i64);
                }
                return box_vec_as_array(out);
            }
        }
    }
    // Not iterable — JS throws (`for (const x of {})` is a TypeError, never a
    // silent empty walk). The pending-error unwind routes to the caller's catch.
    super::errslot::throw_js_error("TypeError", "value is not iterable");
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
    let order = enum_key_order(&keys);
    let mut keys: Vec<Option<String>> = keys.into_iter().map(Some).collect();
    order
        .into_iter()
        .map(|i| keys[i].take().expect("permutation visits each index once"))
        .collect()
}

/// The PERMUTATION that puts `keys` in the canonical own-property enumeration
/// order (the rule above), as ORIGINAL indices. This is the form the front's
/// static `Object.keys/values/entries` emission consumes: a value still lives
/// at the slot of its original (insertion) index, so the emitter needs the
/// index mapping, not just the reordered names.
pub fn enum_key_order(keys: &[String]) -> Vec<usize> {
    let as_index = |s: &str| -> Option<u32> {
        if s == "0" {
            return Some(0);
        }
        if s.is_empty() || s.as_bytes()[0] == b'0' || !s.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        s.parse::<u32>().ok().filter(|&n| n != u32::MAX)
    };
    let mut idx: Vec<(u32, usize)> = Vec::new();
    let mut rest: Vec<usize> = Vec::new();
    for (i, k) in keys.iter().enumerate() {
        match as_index(k) {
            Some(n) => idx.push((n, i)),
            None => rest.push(i),
        }
    }
    idx.sort_by_key(|(n, _)| *n);
    idx.into_iter().map(|(_, i)| i).chain(rest).collect()
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
            let res = super::funcops::__rtsadp_fn_invoke(trap, target, undef, undef, undef, 0);
            // Proxy INVARIANT: the trap result must include every
            // NON-CONFIGURABLE own key of the target — omitting one is a
            // TypeError (spec [[OwnPropertyKeys]] validation).
            let listed: Vec<String> = {
                let rv = PolyValue::from_raw(res);
                if rv.is_object() {
                    let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(rv.as_handle());
                    let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(h).max(0);
                    (0..len)
                        .map(|i| {
                            abi_adapter::resolve_poly(PolyValue::from_raw(
                                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(h, i) as u64,
                            ))
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            };
            let tk = __rtsadp_own_keys_raw(target);
            let th = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(
                PolyValue::from_raw(tk).as_handle(),
            );
            let tlen = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(th).max(0);
            for i in 0..tlen {
                let k = abi_adapter::resolve_poly(PolyValue::from_raw(
                    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(th, i) as u64,
                ));
                if !super::objops::prop_configurable(target, &k) && !listed.contains(&k) {
                    super::errslot::throw_js_error(
                        "TypeError",
                        "'ownKeys' on proxy: trap result did not include \
                         non-configurable property of the proxy target",
                    );
                    return res;
                }
            }
            return res;
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
            // A String/Number/Boolean WRAPPER box: its `__prim` slot is INTERNAL
            // data, never an own property (skipped below). A STRING wrapper's own
            // enumerable keys are its UTF-16 code-unit indices — emitted first
            // (they are integer-index keys, ahead of any expando key).
            if let Some(pi) = keys.iter().position(|k| k == "__prim") {
                let pw = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, 1 + pi as i64) as u64;
                let pv = PolyValue::from_raw(pw);
                if pv.is_string() {
                    let s = abi_adapter::resolve_poly(pv);
                    for i in 0..s.encode_utf16().count() {
                        let word = abi_adapter::intern_poly(&i.to_string()).raw() as i64;
                        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, word);
                    }
                }
            }
            let mut listed: Vec<String> = Vec::new();
            for k in reorder_enum_keys(keys) {
                // Symbol-keyed entries (`@@sym:<handle>` canonical repr, #798) are
                // NOT string-enumerable: `Object.keys`/for-in/`JSON.stringify`
                // all skip them (JS spec). `Reflect.ownKeys` uses the raw path.
                if k.starts_with("@@sym:") {
                    continue;
                }
                // The wrapper's internal primitive slot (handled above).
                if k == "__prim" {
                    continue;
                }
                // PRIVATE fields (`#x`, possibly mangled `#x@Class`) are not
                // properties at all — never enumerated (JS spec).
                if k.starts_with('#') {
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

/// for-in key list (`EnumerateObjectProperties`): the OWN enumerable string
/// keys PLUS the PROTOTYPE CHAIN's enumerable keys (deduped — an own key
/// shadows an inherited one). Arrays/strings keep the index behavior of
/// `obj_keys`; the chain walk only applies to keyed objects.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_for_in_keys(obj_word: u64) -> u64 {
    let own = __rtsadp_obj_keys(obj_word);
    let obj = PolyValue::from_raw(obj_word);
    if !(obj.is_object() && looks_like_object(obj)) {
        return own;
    }
    let out_h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(PolyValue::from_raw(own).as_handle());
    let mut seen: Vec<String> = {
        let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(out_h).max(0);
        (0..len)
            .map(|i| {
                abi_adapter::resolve_poly(PolyValue::from_raw(
                    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(out_h, i) as u64,
                ))
            })
            .collect()
    };
    let mut cur = obj_word;
    let mut depth = 0;
    while depth < 32 {
        depth += 1;
        let proto = match super::protos::proto_of(cur) {
            Some(0) | None => break,
            Some(p) => p,
        };
        let pv = PolyValue::from_raw(proto);
        if !(pv.is_object() && looks_like_object(pv)) {
            break;
        }
        let pk = __rtsadp_obj_keys(proto);
        let pk_h =
            rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(PolyValue::from_raw(pk).as_handle());
        let plen = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(pk_h).max(0);
        for i in 0..plen {
            let w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(pk_h, i) as u64;
            let s = abi_adapter::resolve_poly(PolyValue::from_raw(w));
            // `constructor` is NON-enumerable on real prototypes — the proto
            // objects this model builds store it as a plain (enumerable) slot,
            // so filter it here.
            if s == "constructor" || seen.contains(&s) {
                continue;
            }
            seen.push(s);
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out_h, w as i64);
        }
        cur = proto;
    }
    own
}

/// The custom `[Symbol.iterator]` METHOD of `word` (slot `@@iterator` from the
/// desugar, or a dynamic `@@sym:<h>` write; own or via the prototype chain), or
/// `None`.
fn custom_iterator_method(word: u64) -> Option<u64> {
    let v = PolyValue::from_raw(word);
    if !v.is_object() {
        return None;
    }
    let key = abi_adapter::intern_poly("@@iterator");
    let m = super::objops::__rtsadp_obj_get(word, key.raw());
    if PolyValue::from_raw(m).is_function() {
        return Some(m);
    }
    let iter_h = rts_runtime::namespaces::globals::symbol::__RTS_FN_GL_SYMBOL_ITERATOR();
    let skey = abi_adapter::intern_poly(&format!("@@sym:{iter_h}"));
    let m = super::objops::__rtsadp_obj_get(word, skey.raw());
    PolyValue::from_raw(m).is_function().then_some(m)
}

/// LAZY for-of open — a 3-slot CURSOR descriptor `[kind, payload, idx]`:
/// - `kind 0` = MATERIALIZED array cursor (payload = element array; `idx`
///   advances in `iter_next`; close is a no-op);
/// - `kind 1` = LIVE ITERATOR (payload = the iterator object from the source's
///   `[Symbol.iterator]()`; `iter_next` drives `next()`, `iter_close` runs
///   `return()` on early exit — spec IteratorClose).
/// A non-iterable source THROWS TypeError (pending-error slot).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_iter_open(word: u64) -> u64 {
    let undef = PolyValue::undefined().raw();
    let d = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    if let Some(m) = custom_iterator_method(word) {
        let it = super::funcops::__rtsadp_fn_invoke_method(m, word, undef, undef, undef, 0);
        let iv = PolyValue::from_raw(it);
        if iv.is_object() {
            // A GENERATOR `*[Symbol.iterator]` (the `.ts` stdlib Set/Map) returns
            // a lazy GenState handle, not a `{next}` object — DRAIN it to an
            // array cursor (the state machine runs to completion; close is a
            // no-op like any materialized source).
            let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(iv.as_handle());
            let is_gen = rt_handles::with_entry(h, |e| {
                matches!(e, Some(rt_handles::Entry::GenState(_)))
            });
            if is_gen {
                let arr_h =
                    rts_runtime::namespaces::collector::generator::__RTS_FN_NS_GC_GEN_SM_DRAIN(h);
                let arr = box_vec_as_array(arr_h);
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(d, PolyValue::from_i32(0).raw() as i64);
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(d, arr as i64);
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(d, PolyValue::from_i32(0).raw() as i64);
                return box_vec_as_array(d);
            }
            // An EAGER generator method returned the materialized element ARRAY
            // itself (the `.ts` stdlib Set/Map `*[Symbol.iterator]` shape):
            // walk it as a kind-0 array cursor.
            if !looks_like_object(iv) {
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(d, PolyValue::from_i32(0).raw() as i64);
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(d, it as i64);
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(d, PolyValue::from_i32(0).raw() as i64);
                return box_vec_as_array(d);
            }
            // A LIVE iterator must expose a callable `next` — a mis-reified
            // generator method (raw state-machine thunk) yields garbage here;
            // fall to the materialized shape path instead of a silent-empty
            // walk. Also DISCARD any pending error the probe invoke set.
            let nm = super::objops::__rtsadp_obj_get(
                it,
                abi_adapter::intern_poly("next").raw(),
            );
            if PolyValue::from_raw(nm).is_function() {
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(d, PolyValue::from_i32(1).raw() as i64);
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(d, it as i64);
                rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(d, PolyValue::from_i32(0).raw() as i64);
                return box_vec_as_array(d);
            }
        }
        if super::errslot::__rtsadp_err_pending() != 0 {
            let _ = super::errslot::__rtsadp_err_take();
        }
    }
    // Everything else — array/string/Set/Map by shape (or THROW inside).
    let arr = __rtsadp_to_iter_array(word);
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(d, PolyValue::from_i32(0).raw() as i64);
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(d, arr as i64);
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(d, PolyValue::from_i32(0).raw() as i64);
    box_vec_as_array(d)
}

/// LAZY for-of step over an [`__rtsadp_iter_open`] cursor — the next VALUE
/// word, or the reserved EMPTY singleton when exhausted (a user value can
/// never be EMPTY, so the loop tests one word). Array cursor: `arr[idx++]`
/// with the sparse HOLE reading `undefined`. Live iterator: `it.next()`,
/// `done` truthy → EMPTY; a broken iterator reads as done.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_iter_next(cursor: u64) -> u64 {
    let undef = PolyValue::undefined().raw();
    let ch = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(PolyValue::from_raw(cursor).as_handle());
    let kind = PolyValue::from_raw(rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(ch, 0) as u64);
    let payload = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(ch, 1) as u64;
    if kind.is_int32() && kind.as_i32() == 0 {
        let idx = PolyValue::from_raw(rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(ch, 2) as u64);
        let i = if idx.is_int32() { idx.as_i32() as i64 } else { 0 };
        let ah = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(
            PolyValue::from_raw(payload).as_handle(),
        );
        let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(ah).max(0);
        if i >= len {
            return PolyValue::empty().raw();
        }
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(
            ch,
            2,
            PolyValue::from_i32((i + 1) as i32).raw() as i64,
        );
        let w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(ah, i) as u64;
        if PolyValue::from_raw(w).is_hole() {
            return undef;
        }
        return w;
    }
    let it = payload;
    let nm = super::objops::__rtsadp_obj_get(it, abi_adapter::intern_poly("next").raw());
    if !PolyValue::from_raw(nm).is_function() {
        return PolyValue::empty().raw();
    }
    let r = super::funcops::__rtsadp_fn_invoke_method(nm, it, undef, undef, undef, 0);
    if !PolyValue::from_raw(r).is_object() {
        return PolyValue::empty().raw();
    }
    let done = super::objops::__rtsadp_obj_get(r, abi_adapter::intern_poly("done").raw());
    if super::genops::__rtsadp_to_boolean(done) != 0 {
        return PolyValue::empty().raw();
    }
    super::objops::__rtsadp_obj_get(r, abi_adapter::intern_poly("value").raw())
}

/// LAZY for-of early exit (spec IteratorClose on `break`): a LIVE-ITERATOR
/// cursor runs `it.return()` when defined; an array cursor / missing `return`
/// is a no-op.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_iter_close(cursor: u64) {
    let undef = PolyValue::undefined().raw();
    let ch = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(PolyValue::from_raw(cursor).as_handle());
    let kind = PolyValue::from_raw(rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(ch, 0) as u64);
    if !(kind.is_int32() && kind.as_i32() == 1) {
        return;
    }
    let it = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(ch, 1) as u64;
    let rm = super::objops::__rtsadp_obj_get(it, abi_adapter::intern_poly("return").raw());
    if PolyValue::from_raw(rm).is_function() {
        let _ = super::funcops::__rtsadp_fn_invoke_method(rm, it, undef, undef, undef, 0);
    }
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
