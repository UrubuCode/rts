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

/// Clears every table keyed by a cell that is about to be reclaimed.
///
/// Called from `collect_cycle::release`, which owns the order around it: the
/// text payload and the weak watches are cleared before, and the region's own
/// `free` comes after.
pub(in crate::entry) fn release_tables(context: &mut Context, cell: u32) {
    for table in SideTable::ALL {
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
