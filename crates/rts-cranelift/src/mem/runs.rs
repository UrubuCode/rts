//! A table of contiguous element runs, kept by the client beside the cells.
//!
//! # What the machine needs to know, and nothing else
//!
//! Some objects keep a sequence of tagged words outside their cell, in storage
//! the client owns and may grow, shrink, move or free. A read of one element by
//! position is then a pointer chase through structures this crate cannot lay
//! out — and a call into the client for every element, which is the cost this
//! table exists to remove.
//!
//! So the client publishes one row per cell: where the run starts and how many
//! words it holds. The machine indexes the table by the cell a reference names,
//! bounds-checks the position against the row, and loads. It never learns what
//! owns the run, what the words mean, or how the client keeps the table true.
//!
//! # The contract the client owes, because the machine cannot check it
//!
//! A row is trusted without a second look, so it must be exactly true whenever
//! compiled code can run:
//!
//! - **Every row that is not empty names a run valid for its whole length.** The
//!   client rewrites a row each time its storage can have moved or changed
//!   length, before control returns to compiled code. A row that outlives its
//!   storage is a read of freed memory.
//! - **A cell that dies has its row emptied** before the cell number can be
//!   handed to another object. An empty row is an address of zero and a length
//!   of zero.
//! - **`count` never exceeds the rows readable at `rows`.** Raising it past the
//!   table reads beyond it; lowering it to zero is how a client turns the whole
//!   table off.
//! - **`absent` is a word the client never stores as a real element.** A run
//!   may contain it, and the machine treats an element equal to it as "not
//!   here" and takes the miss edge rather than handing it to the program.
//!
//! What the machine promises in return is that it holds no address across any
//! point where the client could run: the run is read inside one terminator, from
//! the OWNER the instruction was given — a value liveness already tracks — and
//! never from an address hoisted out of a loop. That is the property a withdrawn
//! raw-address element load lacked, and the reason this table is indexed by the
//! owner instead of being handed a base.

/// Where each word of the table header and of a row lives.
///
/// The client's header is `repr(C)` and asserts these in a test of its own; they
/// are constants here because the machine is the half that bakes them into code.
pub struct RunTable;

impl RunTable {
    /// The header word holding the address of the first row.
    pub const ROWS_OFFSET: i32 = 0;
    /// The header word holding how many rows compiled code may read.
    pub const COUNT_OFFSET: i32 = 8;
    /// The header word holding the marker for "no element here".
    pub const ABSENT_OFFSET: i32 = 16;
    /// The header word holding the table's GENERATION: a counter the client
    /// increments on EVERY change to any row (a publish, a freeze, a retract, a
    /// disarm, a reset) -- a row the machine copied under generation `g` is still
    /// the row exactly while this word still reads `g`. That is what lets a loop
    /// resolve a row once, ahead of itself, and re-check one word per access
    /// instead of walking the table; see `lower::row_hoist`.
    pub const GENERATION_OFFSET: i32 = 24;
    /// How far apart consecutive rows are.
    pub const ROW_BYTES: i64 = 32;
    /// The word of a row holding the address of the run's first element.
    pub const ROW_START_OFFSET: i32 = 0;
    /// The word of a row holding how many elements the run has.
    pub const ROW_LENGTH_OFFSET: i32 = 8;
    /// The word of a row holding what the elements ARE.
    ///
    /// [`Self::KIND_WORDS`] is a run of tagged words, the original meaning of a
    /// row. Every other kind is a run of raw numeric machine elements — a typed
    /// array's bytes — that the machine loads at the kind's width and converts to
    /// a double. A kind the machine does not know is a miss, never a guess.
    pub const ROW_KIND_OFFSET: i32 = 16;
    /// The word of a row holding how many leading elements a STORE may overwrite.
    ///
    /// Equal to the length for a run that is writable and zero for one that is
    /// not (a frozen array). A store below this index is an in-place overwrite
    /// of an element that already exists; everything else -- growth, a hole past
    /// the end, a refused write -- is the client's.
    pub const ROW_WRITE_LENGTH_OFFSET: i32 = 24;

    /// Elements are tagged words (a plain array's store).
    pub const KIND_WORDS: i64 = 0;
    /// Elements are little-endian `f64`.
    pub const KIND_F64: i64 = 1;
    /// Elements are little-endian `f32`.
    pub const KIND_F32: i64 = 2;
    /// Elements are little-endian `i32`.
    pub const KIND_I32: i64 = 3;
    /// Elements are little-endian `u32`.
    pub const KIND_U32: i64 = 4;
    /// Elements are little-endian `i16`.
    pub const KIND_I16: i64 = 5;
    /// Elements are little-endian `u16`.
    pub const KIND_U16: i64 = 6;
    /// Elements are `i8`.
    pub const KIND_I8: i64 = 7;
    /// Elements are `u8`.
    pub const KIND_U8: i64 = 8;
    /// Elements are `u8`, and a store CLAMPS (round half to even) instead of
    /// wrapping. A load is the same as [`Self::KIND_U8`].
    pub const KIND_U8_CLAMPED: i64 = 9;
}
