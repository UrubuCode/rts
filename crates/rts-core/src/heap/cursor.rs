//! The region's allocation state, kept where compiled code can take a cell.
//!
//! The machine's side of this contract is `rts_cranelift::mem::CellCursor`, which
//! says what each word is and what the machine promises. This file is the half
//! that owes it.
//!
//! # Why the region's own state lives HERE and is not mirrored
//!
//! The three numbers that decide where the next cell comes from -- the bump
//! position, the bound it may reach and the head of the free list -- are the
//! region's own, and compiled code now reads and writes them without calling in.
//! Keeping a copy in the region and publishing it after each change would be a
//! discipline that must hold at every writer forever; instead the region has no
//! other copy. Every writer goes through [`Cursor`], so the two halves cannot
//! disagree.
//!
//! # Why `Cell`
//!
//! Compiled code stores through the address while the runtime holds a `&Region`
//! between entries. Interior mutability is the honest type for memory somebody
//! else writes; a plain field would let the compiler assume it unchanged.

use std::cell::Cell;

/// The word that means "no cell" in `free`: the same value a freed cell's link
/// carries when nothing follows it, so a pop copies the link without a test.
pub const NONE: u64 = u64::MAX;

/// Allocation state, header first. `repr(C)` and boxed: compiled code bakes this
/// address and these offsets.
#[repr(C)]
pub struct Cursor {
    /// The first cell never handed out.
    pub next: Cell<u64>,
    /// How many cells the region may hand out right now.
    pub limit: Cell<u64>,
    /// The head of the free list, or [`NONE`].
    pub free: Cell<u64>,
    /// The header word of a plain cell, or zero when compiled code must not take
    /// one.
    pub plain: Cell<u64>,
}

impl Cursor {
    /// A cursor over `limit` cells, nothing handed out, taking switched off.
    pub fn new(limit: u32) -> Box<Self> {
        Box::new(Cursor {
            next: Cell::new(0),
            limit: Cell::new(u64::from(limit)),
            free: Cell::new(NONE),
            plain: Cell::new(0),
        })
    }

    /// Where the header is: the number `RegionBases::with_cursor` carries.
    pub fn address(&self) -> u64 {
        self as *const Cursor as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rts_cranelift::mem::CellCursor;
    use std::mem::offset_of;

    /// The machine bakes these offsets into code. A reordered field would make
    /// every allocation read the wrong word, with nothing failing until it ran.
    #[test]
    fn the_header_is_where_the_machine_reads_it() {
        assert_eq!(offset_of!(Cursor, next), CellCursor::NEXT_OFFSET as usize);
        assert_eq!(offset_of!(Cursor, limit), CellCursor::LIMIT_OFFSET as usize);
        assert_eq!(offset_of!(Cursor, free), CellCursor::FREE_OFFSET as usize);
        assert_eq!(offset_of!(Cursor, plain), CellCursor::PLAIN_OFFSET as usize);
        assert_eq!(NONE, CellCursor::NONE);
    }
}
