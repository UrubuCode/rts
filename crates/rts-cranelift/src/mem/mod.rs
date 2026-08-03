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
mod layout;

pub use address::{Addressing, RegionBase, RegionBases};
pub use layout::{HeaderLayout, ObjectLayout, SLOT_BYTES};
