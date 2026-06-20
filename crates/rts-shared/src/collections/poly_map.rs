//! P2 PolyValue containers — the runtime `Map`/`Set` store keyed by JS
//! **SameValueZero** ([`PolyKey`]), holding arbitrary PolyValue keys/values.
//!
//! This is the FOUNDATION for migrating `Map`/`Set` off the ambient `.ts` stdlib
//! onto Registry dispatch (the `Date` template). These externs are not wired into
//! codegen yet — increment B registers them in the `collections` SPEC alongside
//! the rewritten `register_mapset_class_spec` and removes the ambient
//! `map_set.ts`. Until then they are exercised by the Rust unit tests below.
//!
//! Every key/value crossing the ABI is a raw 64-bit PolyValue word (`u64`); the
//! store keys it by [`PolyKey`] so `1`/`1.0` collapse, `NaN` keys dedup, and
//! string keys match by content. A read miss returns the `undefined` sentinel
//! ([`POLY_UNDEFINED`]). GC traces both key and value words (see
//! `Entry::MapPoly`/`SetPoly` in `rts-engine`).

use indexmap::{IndexMap, IndexSet};

use rts_engine::heap::handles::{Entry, alloc_entry, free_handle, with_entry, with_entry_mut};
use rts_engine::heap::poly::POLY_UNDEFINED;
use rts_engine::heap::poly_key::PolyKey;

type Word = u64;

fn with_map<F, R>(h: u64, default: R, f: F) -> R
where
    F: FnOnce(&IndexMap<PolyKey, Word>) -> R,
{
    with_entry(h, |e| match e {
        Some(Entry::MapPoly(m)) => f(m.as_ref()),
        _ => default,
    })
}

fn with_map_mut<F, R>(h: u64, default: R, f: F) -> R
where
    F: FnOnce(&mut IndexMap<PolyKey, Word>) -> R,
{
    with_entry_mut(h, |e| match e {
        Some(Entry::MapPoly(m)) => f(m.as_mut()),
        _ => default,
    })
}

fn with_set<F, R>(h: u64, default: R, f: F) -> R
where
    F: FnOnce(&IndexSet<PolyKey>) -> R,
{
    with_entry(h, |e| match e {
        Some(Entry::SetPoly(s)) => f(s.as_ref()),
        _ => default,
    })
}

fn with_set_mut<F, R>(h: u64, default: R, f: F) -> R
where
    F: FnOnce(&mut IndexSet<PolyKey>) -> R,
{
    with_entry_mut(h, |e| match e {
        Some(Entry::SetPoly(s)) => f(s.as_mut()),
        _ => default,
    })
}

/// Build a `Entry::Vec` array handle from PolyValue words (the iteration result
/// arrays reuse the existing PolyValue array entry).
fn vec_of_words(words: Vec<i64>) -> u64 {
    alloc_entry(Entry::Vec(Box::new(words)))
}

// ── Map ────────────────────────────────────────────────────────────────────

/// New empty PolyValue `Map`. Returns its handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_POLY_NEW() -> u64 {
    alloc_entry(Entry::MapPoly(Box::new(IndexMap::new())))
}

/// Releases the map handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_POLY_FREE(h: u64) {
    free_handle(h);
}

/// `map.set(key, value)` — inserts/updates; returns the map handle (chaining).
/// On update the original key word is kept (JS `Map` keeps the first key).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_POLY_SET(h: u64, key: Word, val: Word) -> u64 {
    with_map_mut(h, (), |m| {
        m.insert(PolyKey(key), val);
    });
    h
}

/// `map.get(key)` — the value word, or the `undefined` sentinel if absent.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_POLY_GET(h: u64, key: Word) -> Word {
    with_map(h, POLY_UNDEFINED, |m| {
        m.get(&PolyKey(key)).copied().unwrap_or(POLY_UNDEFINED)
    })
}

/// `map.has(key)` — 1 if present, else 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_POLY_HAS(h: u64, key: Word) -> i64 {
    with_map(h, 0, |m| i64::from(m.contains_key(&PolyKey(key))))
}

/// `map.delete(key)` — 1 if the key existed (and was removed), else 0. Uses
/// `shift_remove` to preserve insertion order (JS `Map` delete keeps order).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_POLY_DELETE(h: u64, key: Word) -> i64 {
    with_map_mut(h, 0, |m| {
        i64::from(m.shift_remove(&PolyKey(key)).is_some())
    })
}

/// `map.size`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_POLY_SIZE(h: u64) -> i64 {
    with_map(h, 0, |m| m.len() as i64)
}

/// `map.clear()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_POLY_CLEAR(h: u64) {
    with_map_mut(h, (), |m| m.clear());
}

/// `[...map.keys()]` — a PolyValue array of the key words, in insertion order.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_POLY_KEYS(h: u64) -> u64 {
    let words = with_map(h, Vec::new(), |m| m.keys().map(|k| k.0 as i64).collect());
    vec_of_words(words)
}

/// `[...map.values()]` — a PolyValue array of the value words.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_POLY_VALUES(h: u64) -> u64 {
    let words = with_map(h, Vec::new(), |m| m.values().map(|v| *v as i64).collect());
    vec_of_words(words)
}

/// `[...map.entries()]` — a PolyValue array of `[key, value]` 2-element arrays,
/// each boxed as an OBJECT word.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_POLY_ENTRIES(h: u64) -> u64 {
    let pairs: Vec<(Word, Word)> =
        with_map(h, Vec::new(), |m| m.iter().map(|(k, v)| (k.0, *v)).collect());
    let mut outer: Vec<i64> = Vec::with_capacity(pairs.len());
    for (k, v) in pairs {
        let inner = vec_of_words(vec![k as i64, v as i64]);
        outer.push(box_object(inner) as i64);
    }
    vec_of_words(outer)
}

// ── Set ──────────────────────────────────────────────────────────────────────

/// New empty PolyValue `Set`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_POLY_NEW() -> u64 {
    alloc_entry(Entry::SetPoly(Box::new(IndexSet::new())))
}

/// Releases the set handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_POLY_FREE(h: u64) {
    free_handle(h);
}

/// `set.add(value)` — inserts (SameValueZero); returns the set handle (chaining).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_POLY_ADD(h: u64, val: Word) -> u64 {
    with_set_mut(h, (), |s| {
        s.insert(PolyKey(val));
    });
    h
}

/// `set.has(value)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_POLY_HAS(h: u64, val: Word) -> i64 {
    with_set(h, 0, |s| i64::from(s.contains(&PolyKey(val))))
}

/// `set.delete(value)` — order-preserving (`shift_remove`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_POLY_DELETE(h: u64, val: Word) -> i64 {
    with_set_mut(h, 0, |s| i64::from(s.shift_remove(&PolyKey(val))))
}

/// `set.size`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_POLY_SIZE(h: u64) -> i64 {
    with_set(h, 0, |s| s.len() as i64)
}

/// `set.clear()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_POLY_CLEAR(h: u64) {
    with_set_mut(h, (), |s| s.clear());
}

/// `[...set.values()]` — a PolyValue array of the member words, insertion order.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_SET_POLY_VALUES(h: u64) -> u64 {
    let words = with_set(h, Vec::new(), |s| s.iter().map(|k| k.0 as i64).collect());
    vec_of_words(words)
}

/// Box an array/object handle as a `TAG_OBJECT` PolyValue word (drops the 16-bit
/// generation, like codegen's `FROM_HANDLE` + box).
fn box_object(handle: u64) -> u64 {
    use rts_engine::heap::handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE;
    use rts_engine::heap::poly::{POLY_BOX_BASE, POLY_TAG_OBJECT, POLY_TAG_SHIFT};
    let slot = __RTS_FN_NS_GC_POLY_FROM_HANDLE(handle);
    POLY_BOX_BASE | (POLY_TAG_OBJECT << POLY_TAG_SHIFT) | slot
}

// NOTE: these externs are deliberately NOT unit-tested in `rts-shared` — the
// crate declares facade externs (`__RTS_FN_NS_GC_STRING_NEW`, …) implemented in
// `rts-std`, so a standalone `rts-shared` test binary does not link. The externs
// are thin wrappers over `IndexMap<PolyKey, _>` whose SameValueZero behavior
// (the only non-trivial part) is covered by the rts-engine tests in
// `heap/poly_key.rs` (+ the MapPoly/SetPoly store-behavior test there) and the
// GC-survival tests in `heap/handles.rs`. Increment B exercises them end-to-end
// through the running engine.
