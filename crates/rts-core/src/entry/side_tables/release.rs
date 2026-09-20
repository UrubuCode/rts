//! What a cell's death takes with it, table by table.
//!
//! # Why the burial is a total pass and not a sequence
//!
//! A cell that `Region::free` gives back is empty of language content but not of
//! what is attached BESIDE it, and the entry is keyed by an index the free list
//! is about to hand to a stranger. So a table this misses is not merely a leak:
//! it is the stranger reading the previous occupant's data as its own.
//!
//! The crate has paid for that twice, and both are named at their arms below.
//! Being a total pass over [`super::SideTable`] is what stops there being a
//! third of the same shape — the compiler refuses the crate until a new table's
//! death is written down.

use super::SideTable;
use crate::entry::Context;

/// States the tables ONCE, and makes two things of the statement: the function
/// that buries a cell in each of them, and the list a test holds to
/// [`SideTable::ALL`].
///
/// # Why a straight line and not a loop
///
/// It was `for table in SideTable::ALL { match table { … } }`, which reads as a
/// walk and runs as an interpreter: one indirect jump per table, to twenty-two
/// different places, for every cell that dies. Measured 2026-09-19 with
/// `examples/alloc_cost` by visiting only the first N tables — 18.4 ns a cell at
/// none, 28.2 at six, 37.6 at twelve, 51.1 at eighteen, 62.4 at all of them. A
/// straight line in N, about two nanoseconds a table, and every table was
/// EMPTY: what cost was arriving at each arm, not what the arm did. Allocating
/// the cell is 13 ns, so three quarters of what a short-lived object cost was
/// this dispatch — and a string is a cell too.
///
/// Four explanations were tried before that measurement and each was reasoned,
/// built and measured at nothing: the allocator, the memory the removes touch,
/// the loop with the match still inside it, and the call into `Aside::remove`.
/// Counting tables is what answered.
///
/// # How it stays total
///
/// [`bury`] holds the one `match`, still exhaustive, so a new table does not
/// compile until its death is written. Each call passes a CONSTANT, so the match
/// folds to its arm and no dispatch is left. The list has as many entries as
/// `ALL` or this file does not compile, and the test at its foot says they are
/// the same ones — so a table can be neither forgotten nor named twice.
macro_rules! tables {
    ($($table:ident),* $(,)?) => {
        /// What [`release_tables`] buries a cell in, in the order it does.
        const LISTED: &[SideTable] = &[$(SideTable::$table),*];
        const _: () = assert!(LISTED.len() == SideTable::ALL.len());

        /// Clears every table keyed by a cell that is about to be reclaimed.
        ///
        /// Called from `collect_cycle::release`, which owns the order around it:
        /// the text payload and the weak watches are cleared before, and the
        /// region's own `free` comes after.
        pub(in crate::entry) fn release_tables(context: &mut Context, cell: u32) {
            $( bury(context, cell, SideTable::$table); )*
        }
    };
}

tables!(
    SpillOf, ArrayElements, Callables, Proxies, Bound, Views, Collections, Cursors, Generators,
    Helpers, Prototypes, Accessors, Boxed, ProtoTypes, PendingStacks, BufferOf, Detached, Regexes,
    Integrity, Attributes, Derived, Foreign,
);

/// What one table gives up when a cell dies. Inlined with a constant `table`,
/// so only the named arm survives.
#[inline(always)]
fn bury(context: &mut Context, cell: u32, table: SideTable) {
    {
        match table {
            // The overflow is region cells and it SPANS, so every cell it
            // covers comes back, not only the one its reference names.
            SideTable::SpillOf => {
                if let Some((block, slots)) = context.spill_of.remove(cell) {
                    context.region.free_spanning(block, (slots + 1) * 8);
                }
            }
            SideTable::ArrayElements => {
                if let Some(elements) = context.array_elements.remove(cell) {
                    context.arrays.free(elements);
                }
            }
            SideTable::BufferOf => {
                if let Some(buffer) = context.buffer_of.remove(cell) {
                    context.buffers.free(buffer);
                }
            }
            // Beside the buffer it is a fact about. See the paragraph above for
            // what leaving it behind did.
            SideTable::Detached => {
                context.detached.remove(cell);
            }
            SideTable::Prototypes => {
                context.prototypes.remove(cell);
            }
            // Keyed by the cell that WAS a prototype, so reclaiming it drops the
            // numbers minted against it — which is what stops the free list
            // handing the index back and an unrelated object inheriting a stale
            // discrimination.
            SideTable::ProtoTypes => {
                context.proto_types.remove(cell);
            }
            SideTable::Callables => {
                context.callables.remove(cell);
            }
            SideTable::Proxies => {
                context.proxies.remove(cell);
            }
            SideTable::Cursors => {
                context.cursors.remove(cell);
            }
            SideTable::Bound => {
                context.bound.remove(cell);
            }
            SideTable::Views => {
                context.views.remove(cell);
            }
            SideTable::Collections => {
                context.collections.remove(cell);
            }
            // A generator's FRAME is a spanning block of its own, and removing
            // the side-table entry only forgot where it was — the cells stayed
            // taken forever. `Region::free` reads the width out of the header,
            // so freeing the first cell gives the whole run back.
            //
            // Measured before that was done: a loop of 60 000 generators filled
            // a 65 536-cell region with dead frames and the program stopped with
            // "nothing left to reclaim" — a collection that ran, found
            // everything unreachable, and freed none of it.
            SideTable::Generators => {
                if let Some(state) = context.generators.remove(cell) {
                    context.region.free(state.frame_cell());
                }
            }
            SideTable::Helpers => {
                context.helpers.remove(cell);
            }
            SideTable::Regexes => {
                context.regexes.remove(cell);
            }
            SideTable::Accessors => {
                context.accessors.remove(cell);
            }
            SideTable::Integrity => {
                context.integrity.remove(cell);
            }
            SideTable::Attributes => {
                context.attributes.remove(cell);
            }
            SideTable::Derived => {
                context.derived.remove(cell);
            }
            SideTable::Boxed => {
                context.boxed.remove(cell);
            }
            // What an `Error` was constructed from, held until something asks
            // for `.stack`. **This line is new, and its absence was a defect the
            // hand-written sequence hid** — nothing outside `accessor::take`
            // dropped an entry, so an `Error` collected without its `.stack`
            // ever being read left its class name and its captured frames
            // behind for the life of the process.
            //
            // Two consequences, and the second is the sharper one. It retains a
            // `Vec` per such `Error`, which a program in a loop pays without
            // bound. And the entry is keyed by a cell the free list is about to
            // hand out, so the next `Error` to be born there answers the DEAD
            // one's `.stack` — a wrong answer that looks like a right one,
            // which is the failure this whole list exists to prevent.
            SideTable::PendingStacks => {
                context.pending_stacks.remove(cell);
            }
            // The word a client attached. Dropped with the cell and nothing is
            // called — `super::foreign` says so where an addon author will read
            // it.
            SideTable::Foreign => {
                context.foreign.remove(cell);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_table_is_buried_in_and_none_is_named_twice() {
        // The length is a compile error; this is the other half. A list of the
        // right LENGTH with one table twice and another missing would compile,
        // and the missing one is a stranger reading a dead cell's data.
        for table in SideTable::ALL {
            assert_eq!(
                LISTED.iter().filter(|listed| **listed == table).count(),
                1,
                "{table:?} is buried exactly once"
            );
        }
    }
}
