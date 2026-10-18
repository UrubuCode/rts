//! The state a client keeps so that compiled code can take a cell without
//! calling in.
//!
//! # What the machine needs to know, and nothing else
//!
//! A heap that is one region hands out cells from two places: a list of cells
//! given back, and the unused space past the last cell handed out. Asking for a
//! cell is then a handful of loads and one store, and the call into the client
//! that did it costs several times more than the work. So the client publishes
//! the state the choice is made from, at an address that never changes, and the
//! machine performs the choice itself.
//!
//! ```text
//!   free list not empty ── yes ──► pop it: the cell's link is the new head
//!        │ no
//!   next < limit        ── yes ──► take `next`, advance it
//!        │ no
//!      miss                         (the client's own entry: collects, grows)
//! ```
//!
//! The same order the client's own allocation uses, so a cell taken here is the
//! cell the client would have handed out.
//!
//! # The contract the client owes, because the machine cannot check it
//!
//! - **`next` is the first cell never handed out, `limit` the bound it may
//!   reach.** Cells at or past `next` are zero. Every cell below `next` is either
//!   in use or on the free list.
//! - **`free` is `NONE` or a cell on the free list.** That cell's header word is
//!   a marker the client knows, and its first payload word is the next link --
//!   `NONE` or another such cell. The machine rewrites the cell completely, so
//!   a cell it takes needs nothing from the client afterwards.
//! - **`plain` is zero or the complete header word of the one kind of cell the
//!   machine takes.** Zero turns taking off, and the machine takes its miss edge.
//! - **Nothing else writes these words while compiled code is between a
//!   terminator's start and its end.** The client changes them only inside its own
//!   entries, which compiled code reaches through a call.
//!
//! What the machine promises in return: a cell is taken entirely inside one
//! terminator, no address survives it, and what comes out is a reference the
//! collector sweeps like any other. A cell is written in full -- the header, and
//! every payload word zero -- so what it holds is what the client's own
//! allocation leaves.

/// Where each word of the cursor lives.
///
/// The client's header is `repr(C)` and asserts these in a test of its own; they
/// are constants here because the machine is the half that bakes them into code.
pub struct CellCursor;

impl CellCursor {
    /// The word holding the first cell never handed out.
    pub const NEXT_OFFSET: i32 = 0;
    /// The word holding how many cells may be handed out.
    pub const LIMIT_OFFSET: i32 = 8;
    /// The word holding the head of the free list.
    pub const FREE_OFFSET: i32 = 16;
    /// The word holding the header of the cell the machine takes, or zero.
    pub const PLAIN_OFFSET: i32 = 24;
    /// The value of `free`, and of a link, that names no cell.
    pub const NONE: u64 = u64::MAX;
}
