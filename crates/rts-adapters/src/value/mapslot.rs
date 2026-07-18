//! The `Entry::Map` FIELD-SLOT encoding — how a dictionary/instance field is
//! stored in, and read back out of, an object-backed value's `IndexMap`.
//!
//! An object-backed Registry class instance (a `Hash`, a `Stats`, a `net.Server`)
//! is an `Entry::Map` tagged `__rts_class`, not a shape-vec. Its fields live in
//! that map, whose value type is a single `i64` — and TWO different producers
//! write those slots:
//!
//! - **Native code**, which stores a RAW value: `__rts_class`'s string handle, a
//!   `Stats` field's `f64::to_bits`, a `Hash`'s algorithm index.
//! - **A JS property write** (`server.maxConnections = 1`), which stores the
//!   **PolyValue word verbatim**. Storing the tagged word — rather than an
//!   unwrapped payload — is what makes the write lossless for EVERY value kind
//!   instead of only the numbers a raw slot can hold: `true` stays a boolean,
//!   and `null`/`undefined` stay distinguishable from each other and from `0`.
//!
//! This module is the single place that knows both, so the read and write arms
//! cannot drift apart.
//!
//! ## Why the two encodings are distinguishable
//!
//! By the value model's own NaN-box discriminator, not a side flag. A PolyValue
//! is boxed iff `(w & BOX_BASE) == BOX_BASE` — the negative-quiet-NaN quadrant —
//! and nothing the native side stores lands there:
//!
//! - A raw HANDLE carries a low generation in its top bits.
//! - `f64::to_bits` of a real double never sets the negative-qNaN pattern, since
//!   [`PolyValue::from_f64`] canonicalizes every NaN to the POSITIVE quiet NaN
//!   precisely so real doubles cannot collide with the boxed space.
//! - An inline-double PolyValue word is not boxed either — but its bits ARE its
//!   `f64`, so the raw arm reads it back as the same number.
//!
//! The one theoretical overlap is a live handle whose generation reached
//! `0xFFF8` after 65528 reuses of a single slot. It is benign for exactly the
//! reason [`rts_engine::heap::poly`] documents: such a word reconstructs to
//! itself, because the live slot's generation is what the discriminator would
//! read back.

use rts_runtime::namespaces::gc::handles as rt_handles;

use super::PolyValue;
use super::layout::{TAG_FUNCTION, TAG_INT32, TAG_OBJECT, TAG_SINGLETON, TAG_STR};

/// One `Entry::Map` field slot → its PolyValue word. The exact inverse of
/// [`of_poly`] for a JS-written slot, and the historical raw decode for a
/// natively-written one (see the module docs for why the two cannot be
/// confused).
pub(crate) fn to_poly(slot: i64) -> u64 {
    let w = slot as u64;
    // A JS-written slot is already a PolyValue — hand it back untouched.
    let v = PolyValue::from_raw(w);
    if v.is_boxed()
        && matches!(
            v.tag(),
            TAG_INT32 | TAG_SINGLETON | TAG_STR | TAG_OBJECT | TAG_FUNCTION
        )
    {
        return w;
    }
    // A natively-written raw slot, decoded by kind.
    if let Ok(i) = i32::try_from(slot) {
        PolyValue::from_i32(i).raw()
    } else if w >= (1u64 << 48) {
        // Ambiguous on its face: a real heap handle OR the bits of a stored f64
        // (the object-backed numeric fields — `Stats.size`, `mtimeMs` — store
        // `f64::to_bits`, which is `>= 1<<48`). Disambiguate by LIVENESS: a
        // genuine handle decodes to a live table entry, an f64-bits word does
        // not — so box it as a handle only when it is live, else read the double.
        let live = rt_handles::with_entry(w, |e| e.is_some());
        if live {
            super::genops::__rtsadp_box_handle_auto(w)
        } else {
            PolyValue::from_f64(f64::from_bits(w)).raw()
        }
    } else {
        PolyValue::from_f64(slot as f64).raw()
    }
}

/// A JS value → the `Entry::Map` field slot holding it: the PolyValue word
/// verbatim, which [`to_poly`] reads back as itself.
///
/// The GC is already correct for this: `Entry::Map` values are visited as
/// candidate roots, and the marker normalizes a NaN-boxed word through
/// `poly_handle_normalize` — so a stored object/string/function word keeps its
/// referent alive.
pub(crate) fn of_poly(val_word: u64) -> i64 {
    val_word as i64
}
