//! Property inline caches — the MISS half.
//!
//! A dynamic property read (`o.x` where `o`'s shape is not statically proven —
//! an `any` param, a `.map(q => q.x)` callback param, a call return) resolves
//! the slot through [`super::objops`]'s `resolve_slot`, which takes the
//! PROCESS-GLOBAL shape-registry mutex and scans the shape's key list. The
//! registry's own doc comment measures that at ~1.4 µs per field read, growing
//! with the field count, and it is serialized across the whole program.
//!
//! Measured cost of leaving it uncached (release build, 1M iterations, vs bun
//! 1.3.14): reading `o.x` through an `any`-typed receiver takes 436 ms where the
//! SAME read through a nominally-typed receiver — which lowers to a
//! constant-slot load — takes 13 ms. Same work, 33× apart, and the untyped form
//! is ordinary TypeScript.
//!
//! ## The cache
//!
//! One cell per CALL SITE, emitted as writable module data by
//! `rts_codegen_new::front::run::ic`:
//!
//! ```text
//! offset 0   i64  shape_word   the receiver's slot-0 word when this site last missed
//! offset 8   i64  len          that receiver's element count
//! offset 16  i64  slot         the resolved slot index (1 + key index)
//! ```
//!
//! Zero-initialised, and a zeroed cell can never produce a hit: a real
//! `shape_word` is a boxed tagged-int PolyValue, so its top bits are set and it
//! is never `0`.
//!
//! The emitted fast path checks `is_object(recv) && slot0 == shape_word &&
//! len == len` and then loads `slot` directly — no global mutex, no key scan, no
//! allocation. On a mismatch it calls [`__rtsadp_ic_miss`], which resolves the
//! slow way and refills the cell.
//!
//! ## Why the guard is exactly as sound as the engine already is
//!
//! `(slot0, len)` is precisely the pair `looks_like_object` tests to tell a
//! shaped OBJECT from an ARRAY — the two share one `Entry::Vec` representation,
//! so an array whose element 0 happens to be a shape-id-shaped word AND whose
//! length matches that shape's key count is already indistinguishable from an
//! object everywhere else in the engine. The cache reproduces that discriminator
//! rather than inventing a weaker one, so it introduces no ambiguity that
//! `__rtsadp_obj_get` does not already have. (Removing the ambiguity itself needs
//! a real array/object discriminant — see `docs/specs/shape-identity-remediation.md`.)
//!
//! ## AOT
//!
//! The cell is DATA, never patched code, so the same emission works in the JIT
//! and in an object file. A replayed/AOT-started program begins with a cold cell
//! and re-warms on first use; a stale cell cannot survive, because the shape ids
//! it holds are seeded from the same blob the baked code was compiled against.

use super::PolyValue;

/// Field count of the cell, in `i64` units. Keep in sync with the emitter.
pub const IC_CELL_WORDS: usize = 3;

/// Byte size of one cell.
pub const IC_CELL_BYTES: usize = IC_CELL_WORDS * 8;

/// Slow path for a property inline cache: resolve `key_word` on `recv_word` the
/// ordinary way, refill the cell at `cell_ptr`, and return the value word.
///
/// `cell_ptr` is the address of a 3-`i64` cell owned by ONE call site (see the
/// module docs). It is written only here, and only with a triple that the fast
/// path's guard can safely act on: the receiver's own slot-0 word, its element
/// count, and the resolved slot.
///
/// A receiver that is not a shaped keyed object (an array, a string, a Map, a
/// proxy, a primitive) leaves the cell UNTOUCHED and falls through to the
/// generic `__rtsadp_obj_get`, so such a site simply never warms up instead of
/// caching something the guard would misread.
#[rtse::abi]
pub fn rtsadp_ic_miss(recv_word: u64, key_word: u64, cell_ptr: u64) -> u64 {
    // The generic read is the authority for the RESULT — prototype chain,
    // getters, proxies, string/array exotics all live there. The cache only ever
    // short-circuits the shaped-object case on a later execution.
    let value = super::objops::__rtsadp_obj_get(recv_word, key_word);

    if cell_ptr == 0 {
        return value;
    }
    let Some((slot0, len, slot)) = super::objops::ic_probe(recv_word, key_word) else {
        return value;
    };

    // SAFETY: `cell_ptr` is the address of a `[i64; IC_CELL_WORDS]` emitted as
    // module data by this program's own codegen, 8-byte aligned and live for the
    // whole program. The engine passes no other value here, and a null pointer
    // returned early above.
    unsafe {
        let cell = cell_ptr as *mut i64;
        // Order matters: publish the SHAPE last. The fast path reads shape first
        // and only trusts `len`/`slot` once it matched, so a torn write can miss
        // but can never produce a hit on a half-filled cell.
        cell.add(1).write(len);
        cell.add(2).write(slot);
        cell.write(slot0);
    }
    value
}

/// Whether `word` could be a warm cache's `shape_word` (a boxed tagged value, so
/// never zero). Used by the emitter's uninit check and by tests.
pub fn is_plausible_shape_word(word: i64) -> bool {
    PolyValue::from_raw(word as u64).is_boxed()
}
