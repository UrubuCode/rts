//! The table of element runs compiled code reads without calling in.
//!
//! The machine's side of this contract is `rts_cranelift::mem::RunTable`, which
//! says what a row is, what the machine promises and what this side owes. This
//! file is the half that owes it.
//!
//! # Why it lives with the region and not with the context
//!
//! Its address is a number baked into compiled code, exactly as the region's base
//! is, and for the same reason it has to outlive the context that fills it: the
//! program is compiled once and run in a context built afterwards. The region is
//! already the thing a host creates first, hands to the code's addressing, and
//! moves into each context. Putting the table in it adds no second hand-off and
//! no second place for the two to disagree.
//!
//! # What is published, and what is not
//!
//! One row per cell whose elements are in a growable store the runtime owns: the
//! address of the first word and how many there are. Elements that sit in the
//! cell itself are not published — the cell lives in a region whose base the
//! code already knows, but its element count is a property read, and a row for it
//! would be a second copy of a number the layout already keeps. A cell with no
//! row reads as empty, and the machine takes its miss edge.
//!
//! # Why a row is rewritten by a guard and not by the callers
//!
//! The row names a buffer a `Vec` owns. Any push, pop, resize, sort or splice
//! can reallocate it, and there are more than twenty places that mutate one. A
//! convention that each of them refreshes the row afterwards is a discipline that
//! must hold at every one of them forever — rule 8's description of what does not
//! hold. So the only way to a mutable element store hands out a guard whose drop
//! republishes the row (`entry::array::ElementsMut`), and a caller cannot forget
//! because there is no call to forget.

/// One row: where a run starts and how many words it holds. Empty is both zero.
#[derive(Clone, Copy)]
#[repr(C)]
struct Row {
    start: *const u64,
    length: u64,
    /// What the elements are: `0` tagged words, otherwise a typed array's element
    /// kind as `rts_cranelift::mem::RunTable::KIND_*` names it.
    kind: u64,
    /// How many leading elements compiled code may OVERWRITE in place: the length
    /// for a writable run, zero for a frozen array. See
    /// `rts_cranelift::mem::RunTable::ROW_WRITE_LENGTH_OFFSET`.
    writable: u64,
}

const EMPTY: Row = Row {
    start: std::ptr::null(),
    length: 0,
    kind: 0,
    writable: 0,
};

/// The table, header first. `repr(C)` and boxed: compiled code reads the header
/// at a fixed address and the rows at wherever `rows` currently points.
#[repr(C)]
pub struct Runs {
    /// Address of the first row. Rewritten whenever `store` reallocates.
    rows: *const Row,
    /// How many rows compiled code may read. Zero turns the table off.
    count: u64,
    /// The word that means "no element here".
    absent: u64,
    /// Incremented by EVERY method below that changes what any row says (or whether
    /// the table is offered at all). Compiled code that resolved a row once reads this
    /// word to learn the copy is still true; see
    /// `rts_cranelift::mem::RunTable::GENERATION_OFFSET`. A new mutator that forgot
    /// this would make that copy stale silently -- `store` is private so that every
    /// mutation is in this file, and each calls [`Runs::touch`].
    generation: u64,
    store: Vec<Row>,
    armed: bool,
    /// Whether typed-array rows may be published. Cleared by a detach.
    typed_armed: bool,
    /// Cells whose array is frozen: their rows are published read-only, and stay
    /// so across every republish (`ElementsMut` rewrites a row without a context
    /// to ask). A cell number is cleared here when its row is retracted.
    frozen: Vec<bool>,
}

// SAFETY: the raw pointers name memory owned by the context that last filled the
// table, on that context's thread, and are only dereferenced by code running on
// it. A region moves between threads while NO context holds it, and the next
// context resets the table before anything is published — see [`Runs::reset`].
unsafe impl Send for Runs {}

impl Runs {
    /// An empty table, switched on.
    pub fn new() -> Box<Self> {
        Box::new(Runs {
            rows: std::ptr::null(),
            count: 0,
            absent: 0,
            generation: 0,
            store: Vec::new(),
            armed: true,
            typed_armed: true,
            frozen: Vec::new(),
        })
    }

    /// Where the header is: the number `RegionBases::with_runs` carries.
    pub fn address(&self) -> u64 {
        self as *const Runs as u64
    }

    /// Empties the table for a context that is about to adopt the region.
    ///
    /// Every row names storage the PREVIOUS context owned and has since freed, so
    /// leaving one would be a read of freed memory for the first array that
    /// happens to share its cell number. `absent` is the marker this context's
    /// numbering gives the thing the table must never hand to a program.
    pub fn reset(&mut self, absent: u64) {
        self.touch();
        self.store.clear();
        self.frozen.clear();
        self.armed = true;
        self.typed_armed = true;
        self.absent = absent;
        self.refresh();
    }

    /// Publishes a run for a cell, growing the table to hold it.
    pub fn publish(&mut self, cell: u32, start: *const u64, length: usize) {
        self.touch();
        let at = cell as usize;
        if at >= self.store.len() {
            self.store.resize(at + 1, EMPTY);
        }
        let writable = match self.frozen.get(at) {
            Some(true) => 0,
            _ => length as u64,
        };
        self.store[at] = Row {
            start,
            length: length as u64,
            kind: 0,
            writable,
        };
        self.refresh();
    }

    /// Marks a cell's array frozen: compiled code may still READ its run but no
    /// longer overwrites it. Sticky across republishes until the row is retracted.
    pub fn freeze(&mut self, cell: u32) {
        self.touch();
        let at = cell as usize;
        if at >= self.frozen.len() {
            self.frozen.resize(at + 1, false);
        }
        self.frozen[at] = true;
        if let Some(row) = self.store.get_mut(at) {
            row.writable = 0;
        }
    }

    /// Publishes a typed array's window: the address of its first byte, its
    /// element count and its element kind.
    ///
    /// Refused (the row stays empty) once any buffer has been detached, because a
    /// detach frees bytes that OTHER views over the same buffer have rows for and
    /// nothing indexes views by buffer. See [`Runs::retract_typed`].
    pub fn publish_typed(&mut self, cell: u32, start: *const u8, length: usize, kind: u64) {
        self.touch();
        if !self.typed_armed || cfg!(target_endian = "big") {
            self.retract(cell);
            return;
        }
        let at = cell as usize;
        if at >= self.store.len() {
            self.store.resize(at + 1, EMPTY);
        }
        self.store[at] = Row {
            start: start.cast::<u64>(),
            length: length as u64,
            kind,
            writable: length as u64,
        };
        self.refresh();
    }

    /// Empties every typed row and stops publishing new ones, for the rest of the
    /// context's life. Called when a buffer is detached: its bytes are freed, and
    /// every view over them is a row that would read freed memory. Detaching is
    /// rare, so the blunt answer (typed arrays take the runtime) is the sound one.
    pub fn retract_typed(&mut self) {
        self.touch();
        self.typed_armed = false;
        for row in &mut self.store {
            if row.kind != 0 {
                *row = EMPTY;
            }
        }
    }

    /// Empties a cell's row. A cell number is handed to a stranger after this.
    pub fn retract(&mut self, cell: u32) {
        self.touch();
        if let Some(row) = self.store.get_mut(cell as usize) {
            *row = EMPTY;
        }
        if let Some(held) = self.frozen.get_mut(cell as usize) {
            *held = false;
        }
    }

    /// Turns the table off for the rest of the context's life.
    ///
    /// For a fact that makes the machine's answer wrong where the runtime's is
    /// right and that cannot be checked per row. Permanent, because the fact is.
    pub fn disarm(&mut self) {
        self.touch();
        self.armed = false;
        self.refresh();
    }

    /// Says that some row (or the table's availability) may have changed.
    fn touch(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    /// Republishes the header after the rows may have moved or changed count.
    fn refresh(&mut self) {
        self.rows = self.store.as_ptr();
        self.count = if self.armed { self.store.len() as u64 } else { 0 };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rts_cranelift::mem::RunTable;
    use std::mem::offset_of;

    /// The machine bakes these offsets into code. A reordered field would make
    /// every read load the wrong word, with nothing failing until it ran.
    #[test]
    fn the_header_is_where_the_machine_reads_it() {
        assert_eq!(offset_of!(Runs, rows), RunTable::ROWS_OFFSET as usize);
        assert_eq!(offset_of!(Runs, count), RunTable::COUNT_OFFSET as usize);
        assert_eq!(offset_of!(Runs, absent), RunTable::ABSENT_OFFSET as usize);
        assert_eq!(offset_of!(Runs, generation), RunTable::GENERATION_OFFSET as usize);
        assert_eq!(std::mem::size_of::<Row>(), RunTable::ROW_BYTES as usize);
        assert_eq!(offset_of!(Row, start), RunTable::ROW_START_OFFSET as usize);
        assert_eq!(offset_of!(Row, length), RunTable::ROW_LENGTH_OFFSET as usize);
        assert_eq!(offset_of!(Row, kind), RunTable::ROW_KIND_OFFSET as usize);
        assert_eq!(offset_of!(Row, writable), RunTable::ROW_WRITE_LENGTH_OFFSET as usize);
    }

    #[test]
    fn a_disarmed_table_offers_no_rows_even_after_a_publish() {
        let mut runs = Runs::new();
        runs.reset(7);
        runs.disarm();
        let words = [1u64, 2];
        runs.publish(3, words.as_ptr(), words.len());
        assert_eq!(runs.count, 0, "disarmed means compiled code sees no rows");
    }

    #[test]
    fn a_retracted_row_reads_empty_and_a_reset_forgets_all() {
        let mut runs = Runs::new();
        runs.reset(7);
        let words = [1u64, 2];
        runs.publish(2, words.as_ptr(), words.len());
        assert_eq!(runs.count, 3);
        runs.retract(2);
        assert_eq!(runs.store[2].length, 0);
        assert!(runs.store[2].start.is_null());
        runs.reset(9);
        assert_eq!(runs.count, 0);
        assert_eq!(runs.absent, 9);
    }
}
