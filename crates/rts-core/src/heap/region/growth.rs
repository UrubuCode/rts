//! Making the heap bigger without moving it.
//!
//! The base of a region is an **immediate** in the compiled code — every
//! reference already turned into an address was `base + index × stride` against
//! that number — so the one thing growth may not do is move it. That single
//! constraint is what this module is: a reservation claimed once at
//! construction, and a bound raised inside it.
//!
//! Split out of the region's own file rather than added to it because that file
//! is already past this crate's ceiling, and this is a cohesive piece: the
//! policy, the arithmetic the reservation is measured in, and the one operation
//! that spends it.
//!
//! # How the reservation works
//!
//! The whole span a region may ever use is claimed at construction with
//! `Vec::reserve_exact`, and `capacity` — the bound `alloc` actually enforces —
//! starts far below it and is raised by [`Region::grow`]. A `Vec` only moves its
//! buffer when it must claim more capacity, so extending the length inside the
//! reservation keeps `Region::base` bit-for-bit what compiled code was given.
//!
//! What that costs is address space and commit charge, not memory: the reserved
//! words are never written until `grow` extends the length over them, and an
//! untouched page has no physical page behind it. Measured on 2026-08-15, a
//! program of 200 000 live objects: 70.8 MiB of private commit against 51.3 MiB
//! of working set, and a program that never grows pays neither.
//!
//! # What was rejected
//!
//! **`realloc`.** The obvious way to grow a `Vec` and the one thing this cannot
//! do — a new base invalidates every address in every register, every inline
//! cache and every parked frame at once, and the failure is a wild store rather
//! than a crash.
//!
//! **A second region when the first fills.** The machine has the form for it,
//! `rts_cranelift::mem::Addressing::Sharded`, and it does not answer this: the
//! selector width is baked into every address computation *before* the program
//! is compiled, so a region added while running is one no compiled instruction
//! can reach. Sharding answers "several threads", not "more room".
//!
//! **An explicit OS reservation** — `VirtualAlloc`/`mmap` reserved and
//! committed on demand. It is the textbook answer, and here it is the same
//! answer: the allocator already reserves through the OS and the OS already
//! commits on touch. What it would add is an operating system inside a crate
//! whose membership rule is availability on every target, wasm included — see
//! this crate's README, rule 1 — for a difference this measured at nothing.

use super::{Region, SLOT_BYTES, STRIDE};

/// How many times its starting size a region may grow to.
///
/// This is the whole heap policy, and it is one number because the base cannot
/// move: the reservation is claimed once, at construction, so this multiplier
/// decides both the ceiling and the address space every program claims whether
/// or not it uses it. Eight takes the host's 65 536 cells to 524 288 — 64 MiB
/// of address space against 8 MiB of memory at the start.
///
/// # Why a multiplier and not an absolute number of cells
///
/// Because a `Region::with_capacity(2)` in a unit test would otherwise reserve
/// hundreds of megabytes to hold two objects. Scaling by what the caller asked
/// for keeps the reservation proportional to the intent, and keeps the dozen
/// constructions in this crate's own tests costing bytes.
///
/// # Why EIGHT, which is measured rather than chosen
///
/// The first value tried was sixty-four, and it broke the engine in a way no
/// amount of reading would have predicted: `rts-host`'s `running` suite began
/// failing inside `cranelift-jit`'s relocation with `TryFromIntError`, on a
/// different set of tests every run.
///
/// That error is a **PC-relative displacement that does not fit 32 bits**.
/// Generated code reaches its own functions and its data with `PCRel4`, so
/// everything one program compiles has to land within ±2 GiB. A test binary
/// builds hundreds of programs on several threads at once, and each program's
/// reservation sits in the address space *between* two code allocations — so
/// the reservation is exactly what pushes them out of reach of each other.
///
/// Measured on 2026-08-15, `cargo test -p rts-host --test running`, same tree,
/// only this constant changed:
///
/// | multiplier | per program | result |
/// |---|---|---|
/// | 1 (the old fixed region) | 8 MiB | 303 of 303 |
/// | 8 | 64 MiB | 304 of 304, four runs |
/// | 16 | 128 MiB | 304 of 304, three runs |
/// | 32 | 256 MiB | **FAILED every run** — 1, 8, 2 and 5 relocations out of range |
/// | 64 | 512 MiB | **FAILED** — 2 to 7 per run |
///
/// Eight is half of the largest value that passed, and the margin is the point:
/// how many programs are alive at once is how many test threads the machine
/// runs, so a value that passes here on eight threads is not evidence about
/// sixteen.
///
/// # What actually raises this, and why it is not this crate's to raise
///
/// Not a bigger number. The heap's ceiling is set by a **code** addressing
/// range, so what lifts it is compiled code and its data being allocated out of
/// a span reserved for them, where nothing a program allocates can land between
/// two of them. That is `rts_cranelift::target`'s question — where compiled
/// code goes — and until it is answered, raising this multiplier trades a heap
/// that is too small for a program that dies in a relocation, which is the
/// worse of the two.
pub const GROWTH_CEILING: u32 = 8;

/// How many machine words `cells` cells occupy.
pub(super) fn words_for(cells: u32) -> usize {
    (cells as usize) * (STRIDE as usize / SLOT_BYTES as usize)
}

impl Region {
    /// The ceiling [`Region::capacity`] may be raised to.
    pub fn reserved(&self) -> u32 {
        self.reserved
    }

    /// Raises the bound `alloc` enforces, without moving the base.
    ///
    /// Doubles, so a program that genuinely holds more objects than it started
    /// with reaches its working size in a logarithmic number of collections
    /// rather than one per page — and a program that does not never calls this
    /// at all, because its caller runs a collection first and only asks when
    /// the collection did not make room.
    ///
    /// Returns whether the bound moved. `false` means the reservation is spent,
    /// which is the one remaining way a heap is genuinely exhausted.
    ///
    /// # What makes this safe
    ///
    /// The `resize` extends the `Vec`'s length into capacity claimed at
    /// construction, and a `Vec` reallocates only when it must claim more. The
    /// assertion states that as an invariant rather than a hope, because the
    /// failure it guards is a base that moved under compiled code — a wild
    /// store, not a crash — and an invariant this layer states is one it
    /// enforces.
    pub fn grow(&mut self) -> bool {
        if self.capacity >= self.reserved {
            return false;
        }
        let want = self.capacity.saturating_mul(2).min(self.reserved);
        let base_before = self.base();
        self.words.resize(words_for(want), 0);
        assert_eq!(
            self.base(),
            base_before,
            "growing moved the region: every address compiled code holds was \
             computed against the old base, so the next field write would land \
             in memory this program no longer owns"
        );
        self.spanned_interior.resize(want as usize, false);
        self.capacity = want;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::INLINE_SLOTS;
    use super::*;

    #[test]
    fn growing_does_not_move_the_base_compiled_code_was_given() {
        // The one property everything else rests on. An address already
        // computed is `base + index * stride`, so a base that moved turns every
        // live reference into a wild store — which is why `grow` asserts this
        // itself and why it is stated again here, at the size where a `Vec`
        // that had NOT reserved would certainly have reallocated.
        let mut region = Region::with_capacity(64);
        let base = region.base();
        let first = region.alloc(16, 1).expect("fits");
        region.set_field(first, 0, 0xC0FFEE).expect("slot exists");
        while region.grow() {}
        assert_eq!(region.base(), base);
        assert_eq!(region.capacity(), 64 * GROWTH_CEILING);
        assert_eq!(
            region.field(first, 0),
            Some(0xC0FFEE),
            "an object allocated before the growth must still be where it was"
        );
    }

    #[test]
    fn a_full_region_accepts_allocations_again_once_it_has_grown() {
        // The limitation this replaces: a program holding more live objects
        // than the starting capacity died, however well the collector worked.
        let mut region = Region::with_capacity(2);
        let kept: Vec<u32> = (0..2).map(|i| region.alloc(16, i).expect("fits")).collect();
        assert_eq!(region.alloc(16, 9), None, "full at the starting bound");

        assert!(region.grow());
        let more = region.alloc(16, 9).expect("room after growing");
        assert!(
            !kept.contains(&more),
            "a grown region must not re-hand a live cell"
        );
        assert_eq!(region.type_of(kept[0]), Some(0), "and must not disturb one");
    }

    #[test]
    fn growth_stops_at_the_reservation_rather_than_reallocating() {
        // Where the loud failure still lives. The reservation is the ceiling
        // because it is the span whose base is fixed, so the honest end of the
        // heap is here rather than at a `realloc` that would move it.
        let mut region = Region::with_capacity(2);
        assert_eq!(region.reserved(), 2 * GROWTH_CEILING);
        while region.grow() {}
        assert_eq!(region.capacity(), region.reserved());
        assert!(
            !region.grow(),
            "a spent reservation refuses rather than moves"
        );
    }

    #[test]
    fn a_cell_past_the_starting_capacity_reads_and_writes_its_own_fields() {
        // That the words behind the grown span are real and reachable by the
        // same arithmetic — the failure otherwise is a write landing in memory
        // nobody owns, which no assertion about capacity would catch.
        let mut region = Region::with_capacity(2);
        while region.grow() {}
        let mut last = 0;
        while let Some(cell) = region.alloc(16, 7) {
            last = cell;
        }
        assert_eq!(last, region.reserved() - 1);
        region
            .set_field(last, INLINE_SLOTS - 1, 0xABCD)
            .expect("its own slot");
        assert_eq!(region.field(last, INLINE_SLOTS - 1), Some(0xABCD));
        assert_eq!(region.type_of(last), Some(7));
    }
}
