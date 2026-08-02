//! A real card table for the write-barrier row of kernel W.
//!
//! Kernel B's `b1b_bump_direct_barrier` faked one: it masked the card index to
//! `0xFF` and stored into the ARENA's own base, so the barrier wrote over live
//! object words. That was tolerable when the only question was "how many
//! instructions does a card mark cost", but kernel W stores real field values
//! and checks a checksum, so the barrier must land somewhere that is not the
//! heap.
//!
//! Shape (HotSpot's, which is the one every published cost figure refers to):
//!
//! ```text
//! card = (block_addr >> CARD_SHIFT) & CARD_MASK
//! store byte 1 -> [CARD_TABLE + card]
//! ```
//!
//! A production barrier computes `(addr - heap_base) >> CARD_SHIFT` with no
//! mask, because the table is sized to cover the heap exactly. The probe masks
//! instead, purely to stay in bounds without knowing the heap's extent — same
//! instruction count (one `band` where the real one has one `isub`), so the
//! measured cost is not distorted by the substitution.
//!
//! What this does NOT model: the *conditional* barriers (generational
//! filtering, SATB / dirty-card enqueue, remembered-set maintenance) real
//! collectors layer on top. This is the unconditional store — the cheapest
//! honest barrier, i.e. a LOWER bound.
//!
//! And the barrier priced here is per FIELD STORE. §8.3 of
//! `RTS_CLASS_IMPLEMENTATION.md` is the reason that matters: with a precise
//! field map, a store of an unboxed double needs no barrier at all, so W3−W2 is
//! the cost of NOT having `fieldmap.rs`, not a fixed tax on every class.

use std::cell::UnsafeCell;
use std::sync::OnceLock;

/// 512 bytes per card — HotSpot's granularity.
pub const CARD_SHIFT: i64 = 9;
/// Table size. Large enough that unrelated blocks rarely alias onto the same
/// card, small enough to stay resident (64 KB = 16 pages).
pub const CARDS: usize = 1 << 16;
pub const CARD_MASK: i64 = (CARDS as i64) - 1;

struct Table(UnsafeCell<Vec<u8>>);
// SAFETY: single-threaded probe (see the crate README caveats).
unsafe impl Sync for Table {}

fn table() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| Table(UnsafeCell::new(vec![0u8; CARDS])))
}

/// Base address the emitted barrier adds the card index to. Stable for the
/// process: the `Vec` is allocated once at its final size and never grows.
pub fn table_addr() -> i64 {
    // SAFETY: single-threaded probe; the allocation is fixed after init.
    unsafe { (*table().0.get()).as_ptr() as i64 }
}

/// Clear every card. Not called from the timed path — a real collector clears
/// the table during a collection, which this probe does not run.
pub fn reset() {
    // SAFETY: single-threaded probe.
    let v = unsafe { &mut *table().0.get() };
    v.fill(0);
}
