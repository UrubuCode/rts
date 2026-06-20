//! [`PolyKey`] — a PolyValue word used as a `Map`/`Set` key with JS
//! **SameValueZero** equality (the equality `Map`/`Set` use for keys).
//!
//! SameValueZero differs from `===` only on `NaN`: it treats `NaN` as equal to
//! `NaN` (so a `NaN` key is found again), while `+0` and `-0` are the same key.
//! Concretely, two [`PolyKey`]s are equal iff:
//!
//! - both are numbers (inline double OR boxed `int32`) with equal numeric value,
//!   where `NaN == NaN` and `+0.0 == -0.0` (so `1` and `1.0` are one key); or
//! - both are strings, with equal byte CONTENT (a string key found by value, not
//!   by handle identity); or
//! - otherwise, identical raw bits (reference identity for object/function/
//!   singleton handles).
//!
//! [`Hash`] is kept consistent with [`Eq`]: numbers hash by a canonicalized
//! `f64` (all `NaN` → one value, `-0` → `+0`), strings by their byte content,
//! everything else by raw bits. This is the runtime half of the P2 PolyValue
//! containers; the value model bits live in [`super::poly`].

use std::hash::{Hash, Hasher};

use super::handles::{Entry, with_entry};
use super::poly::{poly_handle_normalize, poly_is_string, poly_number_value};

/// A PolyValue word keyed by JS SameValueZero. `Box<IndexMap<PolyKey, u64>>` /
/// `Box<IndexSet<PolyKey>>` back the runtime `Map`/`Set` (see
/// [`Entry::MapPoly`]/[`Entry::SetPoly`]).
#[derive(Debug, Clone, Copy)]
pub struct PolyKey(pub u64);

/// Read a string PolyValue word's UTF-8 bytes (cloned) if it resolves to a live
/// `Entry::String`; `None` for a non-string or a dead handle.
fn string_bytes(word: u64) -> Option<Vec<u8>> {
    let handle = poly_handle_normalize(word)?;
    with_entry(handle, |e| match e {
        Some(Entry::String(bytes)) => Some(bytes.clone()),
        _ => None,
    })
}

impl PartialEq for PolyKey {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (self.0, other.0);
        // Fast path: identical bits ⇒ same number / same handle / same singleton.
        if a == b {
            return true;
        }
        // Numbers compare by value: `1`(int32) == `1.0`(double), `+0` == `-0`,
        // and SameValueZero's `NaN == NaN` (the explicit second clause, since
        // IEEE `==` says `NaN != NaN`).
        if let (Some(na), Some(nb)) = (poly_number_value(a), poly_number_value(b)) {
            return na == nb || (na.is_nan() && nb.is_nan());
        }
        // Strings compare by byte content (distinct handles, same text ⇒ equal).
        if poly_is_string(a) && poly_is_string(b) {
            return match (string_bytes(a), string_bytes(b)) {
                (Some(x), Some(y)) => x == y,
                _ => false,
            };
        }
        // Everything else: reference identity, already ruled out by `a == b`.
        false
    }
}

impl Eq for PolyKey {}

impl Hash for PolyKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let bits = self.0;
        if let Some(n) = poly_number_value(bits) {
            // Number domain (tag 0): canonicalize so eq-equal numbers hash equal —
            // every NaN → one value, `-0.0` → `+0.0`. A boxed `int32` and an inline
            // double of the same value land on the same `to_bits` here.
            0u8.hash(state);
            let canon = if n.is_nan() {
                f64::NAN.to_bits()
            } else if n == 0.0 {
                0.0f64.to_bits()
            } else {
                n.to_bits()
            };
            canon.hash(state);
        } else if let Some(bytes) = string_bytes(bits) {
            // String domain (tag 1): hash by content so same-text distinct handles
            // collide. A dead string handle falls through to the raw-bits domain.
            1u8.hash(state);
            bytes.hash(state);
        } else {
            // Reference domain (tag 2): object/function/singleton by raw bits.
            2u8.hash(state);
            bits.hash(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heap::handles::{__RTS_FN_NS_GC_POLY_FROM_HANDLE, alloc_entry, free_handle};
    use crate::heap::poly::{POLY_BOX_BASE, POLY_TAG_SHIFT, POLY_TAG_STR};
    use std::collections::hash_map::DefaultHasher;

    fn h(k: PolyKey) -> u64 {
        let mut s = DefaultHasher::new();
        k.hash(&mut s);
        s.finish()
    }

    /// Box a live string handle as a `TAG_STR` PolyValue word, exactly as codegen.
    fn str_word(bytes: &[u8]) -> (u64, u64) {
        let handle = alloc_entry(Entry::String(bytes.to_vec()));
        let slot = __RTS_FN_NS_GC_POLY_FROM_HANDLE(handle);
        let word = POLY_BOX_BASE | (POLY_TAG_STR << POLY_TAG_SHIFT) | slot;
        (word, handle)
    }

    #[test]
    fn int32_and_double_one_is_same_key() {
        // `1` boxed as int32 vs `1.0` inline double — same key, same hash.
        let int1 = crate::heap::poly::POLY_BOX_BASE
            | (crate::heap::poly::POLY_TAG_INT32 << POLY_TAG_SHIFT)
            | 1u64;
        let dbl1 = 1.0f64.to_bits();
        let (a, b) = (PolyKey(int1), PolyKey(dbl1));
        assert_eq!(a, b, "1 (int32) and 1.0 (double) must be SameValueZero-equal");
        assert_eq!(h(a), h(b), "eq keys must hash equal");
    }

    #[test]
    fn nan_keys_dedup() {
        let nan_a = f64::NAN.to_bits();
        let nan_b = (f64::NAN.to_bits()) ^ 0; // same canonical NaN
        assert_eq!(PolyKey(nan_a), PolyKey(nan_b), "NaN == NaN under SameValueZero");
        assert_eq!(h(PolyKey(nan_a)), h(PolyKey(nan_b)));
    }

    #[test]
    fn plus_zero_minus_zero_same_key() {
        let pz = 0.0f64.to_bits();
        let nz = (-0.0f64).to_bits();
        assert_ne!(pz, nz, "+0 and -0 have distinct bits");
        assert_eq!(PolyKey(pz), PolyKey(nz), "+0 and -0 are the same key");
        assert_eq!(h(PolyKey(pz)), h(PolyKey(nz)), "eq keys hash equal");
    }

    #[test]
    fn distinct_strings_same_content_collide() {
        let (w1, h1) = str_word(b"hello");
        let (w2, h2) = str_word(b"hello");
        assert_ne!(w1, w2, "distinct handles ⇒ distinct words");
        assert_eq!(PolyKey(w1), PolyKey(w2), "same content ⇒ same key");
        assert_eq!(h(PolyKey(w1)), h(PolyKey(w2)), "eq keys hash equal");
        let (w3, h3) = str_word(b"world");
        assert_ne!(PolyKey(w1), PolyKey(w3), "different content ⇒ different key");
        free_handle(h1);
        free_handle(h2);
        free_handle(h3);
    }

    #[test]
    fn store_dedup_order_and_miss() {
        // Mirrors what the rts-shared MAP_POLY_* externs do over IndexMap<PolyKey>.
        use crate::heap::poly::{POLY_TAG_INT32, POLY_UNDEFINED};
        use indexmap::IndexMap;
        let int32 = |i: i32| POLY_BOX_BASE | (POLY_TAG_INT32 << POLY_TAG_SHIFT) | (i as u32 as u64);

        let mut m: IndexMap<PolyKey, u64> = IndexMap::new();
        // int32 1 then double 1.0 → one entry (value updated), key kept.
        m.insert(PolyKey(int32(1)), 10.0f64.to_bits());
        m.insert(PolyKey(1.0f64.to_bits()), 20.0f64.to_bits());
        assert_eq!(m.len(), 1, "1 and 1.0 collapse to one key");
        assert_eq!(m.get(&PolyKey(int32(1))).copied(), Some(20.0f64.to_bits()));
        // NaN keys dedup.
        m.insert(PolyKey(f64::NAN.to_bits()), 1);
        m.insert(PolyKey(f64::NAN.to_bits()), 2);
        assert_eq!(m.get(&PolyKey(f64::NAN.to_bits())).copied(), Some(2));
        // string content keys collide across distinct handles.
        let (sa, ha) = str_word(b"key");
        let (sb, hb) = str_word(b"key");
        m.insert(PolyKey(sa), 100);
        assert_eq!(m.get(&PolyKey(sb)).copied(), Some(100), "string-content key");
        // miss → caller maps None to undefined.
        assert_eq!(
            m.get(&PolyKey(int32(99))).copied().unwrap_or(POLY_UNDEFINED),
            POLY_UNDEFINED
        );
        // insertion order preserved (1, NaN, "key").
        let order: Vec<u64> = m.keys().map(|k| k.0).collect();
        assert_eq!(order.len(), 3);
        assert_eq!(order[0], int32(1), "first inserted key kept and first in order");
        free_handle(ha);
        free_handle(hb);
    }

    #[test]
    fn distinct_objects_distinct_keys() {
        let o1 = alloc_entry(Entry::Vec(Box::new(vec![1i64])));
        let o2 = alloc_entry(Entry::Vec(Box::new(vec![1i64])));
        let s1 = __RTS_FN_NS_GC_POLY_FROM_HANDLE(o1);
        let s2 = __RTS_FN_NS_GC_POLY_FROM_HANDLE(o2);
        use crate::heap::poly::POLY_TAG_OBJECT;
        let w1 = POLY_BOX_BASE | (POLY_TAG_OBJECT << POLY_TAG_SHIFT) | s1;
        let w2 = POLY_BOX_BASE | (POLY_TAG_OBJECT << POLY_TAG_SHIFT) | s2;
        assert_ne!(PolyKey(w1), PolyKey(w2), "distinct objects ⇒ distinct keys");
        assert_eq!(PolyKey(w1), PolyKey(w1), "same object ⇒ same key");
        free_handle(o1);
        free_handle(o2);
    }
}
