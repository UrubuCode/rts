//! What one heap slot holds.
//!
//! The union C1 said would arrive "when there is more than one kind to unify,
//! which is not yet". There are now two, so here it is.
//!
//! One table rather than one per kind, for the reason recorded then: the tag
//! space already spends a tag on "reference", and splitting the 48-bit payload
//! to re-encode *which* kind of reference would spend address bits to save a
//! branch — and the branch is one a shape check performs anyway. One table also
//! means one free list, so fragmentation is one problem rather than two.

use crate::collect::Trace;
use crate::heap::Slot;
use crate::object::Object;
use crate::text::Str;

/// A heap value, of whichever kind.
pub enum Cell {
    /// An object: a layout, a row of values, and a prototype.
    Object(Object),
    /// A string.
    Text(Str),
}

impl Trace for Cell {
    /// A string refers to nothing, which is most of why one table costs so
    /// little: the collector's walk over a string is a match arm.
    fn trace(&self, out: &mut Vec<Slot>) {
        match self {
            Cell::Object(object) => object.trace(out),
            Cell::Text(_) => {}
        }
    }
}
