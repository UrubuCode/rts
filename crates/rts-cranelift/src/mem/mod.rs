//! Memory: where objects live and how a reference reaches one.
//!
//! # Why this is the module that matters
//!
//! Measurement on this repository put every large cost in one of three places: a
//! call where a load would do, a lock taken per access, and an allocation that
//! was not necessary. Constructing an object was dominated almost entirely by
//! allocation — not one cost among several, but effectively the whole operation.
//! Reaching a field through a call rather than an address computation was the
//! second. The value representation, by contrast, measured under a nanosecond
//! away from every alternative.
//!
//! So this module is where the work is, and the shape it takes follows from
//! those numbers rather than from what is tidy.
//!
//! # A reference is an index, and that is a decision
//!
//! A reference's payload is an index into a table, never an address. Two things
//! follow. A collector can move an object by writing one table entry, with no
//! pointer to find and patch anywhere else — measured as costing nothing extra,
//! *provided* allocation is compacting; against scattered blocks the same design
//! measured several times worse, from cache behaviour alone.
//!
//! **That paragraph is true of an INDIRECT table and was read as though it were
//! true of the region.** It is not, and the difference decides what a collector
//! here can be. `Slab<T>` is indirect: a handle names a row, the row holds the
//! thing, and moving the thing rewrites one row. [`Addressing`] is direct:
//! `address = base + cell * stride`, so a cell's index IS its location. Moving a
//! cell in the region therefore changes every reference to it — the opposite of
//! free — and additionally re-keys the eighteen `Aside` tables in
//! `rts-core-rwk`, which are keyed by that index.
//!
//! So the first collector for the region is **non-moving**, and the compacting
//! requirement below is deferred rather than met. The reason it survives at all
//! is that a region cell is FIXED SIZE — one header word and seven slots — so a
//! free list over cells cannot fragment, which is the one thing compaction would
//! have been for. What compaction still buys is locality, which is what the
//! measurement above actually measured; that trade is taken knowingly and is the
//! thing to re-measure before anyone calls the result final.
//!
//! And the alternative is not hypothetical. A production engine that puts raw
//! addresses in the same encoding needs per-platform heuristics to tell an
//! address from a payload, and those heuristics are tied by its own comments to
//! a shipped bug where a length read as zero. An index has no such question.
//!
//! # Address computation is instructions
//!
//! Turning a reference into an address is arithmetic on the index, not a call.
//! That is the second measured lever, and it is worth more than any change to
//! the representation. Collapsing the region routing to a single base is worth
//! more again, which is the empirical case for regional placement rather than an
//! aesthetic one.

mod address;
mod imports;
mod layout;

pub use address::{Addressing, RegionBase, RegionBases};
pub use imports::HeapImports;
pub use layout::{HeaderLayout, ObjectLayout, SLOT_BYTES};
