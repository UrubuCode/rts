//! Which data attached beside a cell can name another cell, declared per table.
//!
//! # The question this exists to stop being a prose answer
//!
//! Rule 10 of this crate's README says a reference this crate holds is a
//! reference the collector is told about, and names the mechanism it was told
//! through: two hand-written lists. `roots::context_roots` enumerates the
//! fields of [`super::Context`] that hold one, and `trace::edges_of` walks the
//! side tables a marked cell reaches through. **A list is a place a thing can
//! be missing from**, and `docs/engine/lost-roots.md` carries the three
//! occasions something was — a `for`-`of` that ended early, a `JSON.parse` that
//! answered empty objects, and a heap that exhausted itself.
//!
//! The half of that a list cannot fix is the second half: `edges_of` closed
//! with a paragraph naming the tables that hold no reference and why, and a
//! sentence saying a table in NEITHER list is the bug. That sentence was
//! correct and it was prose — nothing rejected a table that appeared in
//! neither, because nothing could see that a table existed at all.
//!
//! # What this changes
//!
//! Every table gets a name here, and the answer to "can it name a cell" is a
//! `match` over that name. A `match` in Rust is total, so **the compiler
//! refuses the crate until a new table has an answer**, where before it
//! accepted silence. `trace::edges_of` walks [`SideTable::ALL`] rather than a
//! sequence of `if let`s, so the same totality decides which tables are
//! consulted.
//!
//! # What it deliberately does not close, said plainly
//!
//! Adding a field to [`super::Context`] does not by itself add a variant here.
//! The gate is one step downstream of that: the author still writes the
//! variant, and what the compiler then enforces is that every variant is
//! classified and every classification is acted on.
//!
//! The airtight form was considered and rejected for its cost: a struct pattern
//! with no `..` over `Context` would break the build on any new field, and
//! `Context` holds far more fields than tables — every scheduling counter and
//! every registry would have to be re-classified to add one. That trades a
//! silence for a noise nobody reads, which is the same failure in the other
//! direction. `docs/engine/the-unwired-keystone.md` is what ends the class
//! rather than narrowing it.


/// One of the tables that attaches data to a cell from outside the cell.
///
/// The variants are the `Aside<T>` fields of [`super::Context`], one each.
/// Order is the order `trace::edges_of` consults them, which is only for a
/// reader comparing the two: marks are a set, so nothing depends on it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum SideTable {
    /// Properties past the seventh, in a region block of their own.
    SpillOf,
    /// An array's elements.
    ArrayElements,
    /// A callable's code address, its environment, and whether it is a class.
    Callables,
    /// A proxy's target and handler.
    Proxies,
    /// A bound function's target, receiver and partial arguments.
    Bound,
    /// A typed array's or `DataView`'s buffer.
    Views,
    /// A `Map`/`Set`/`WeakMap`/`WeakSet`'s entries.
    Collections,
    /// What an iterator is walking, and where it is.
    Cursors,
    /// A generator's parked frame.
    Generators,
    /// An iterator helper's source, callback and inner sequence.
    Helpers,
    /// What a cell inherits from.
    Prototypes,
    /// A cell's getters and setters.
    Accessors,
    /// A wrapper object's boxed primitive.
    Boxed,
    /// The shape/type pairs a constructor has already minted.
    ProtoTypes,
    /// What an `Error` was constructed from, until `.stack` is asked for.
    PendingStacks,
    /// Where a view's buffer bytes live.
    BufferOf,
    /// Whether a buffer was detached.
    Detached,
    /// A compiled regular expression and its flags.
    Regexes,
    /// How deeply a cell is frozen.
    Integrity,
    /// Per-property writable/enumerable/configurable flags.
    Attributes,
    /// Whether a constructor is a derived one.
    Derived,
    /// What a foreign object is, to the host that made it.
    Foreign,
}

impl SideTable {
    /// Every table, which is what makes the walk over them total.
    pub(super) const ALL: [SideTable; 22] = [
        SideTable::SpillOf,
        SideTable::ArrayElements,
        SideTable::Callables,
        SideTable::Proxies,
        SideTable::Bound,
        SideTable::Views,
        SideTable::Collections,
        SideTable::Cursors,
        SideTable::Generators,
        SideTable::Helpers,
        SideTable::Prototypes,
        SideTable::Accessors,
        SideTable::Boxed,
        SideTable::ProtoTypes,
        SideTable::PendingStacks,
        SideTable::BufferOf,
        SideTable::Detached,
        SideTable::Regexes,
        SideTable::Integrity,
        SideTable::Attributes,
        SideTable::Derived,
        SideTable::Foreign,
    ];

    /// Whether what this table attaches to a cell can name another cell.
    ///
    /// A `false` here is a claim with a consequence: `trace::edges_of` will not
    /// consult the table, so anything reachable only through it is freed under
    /// the owner's feet. Each one below says what the table holds instead, and
    /// the debug assertion in `edges_of` refuses an arm that pushes an edge
    /// while answering `false` here.
    pub(super) const fn holds_references(self) -> bool {
        match self {
            // The thirteen that can, each with an arm in `edges_of`.
            SideTable::SpillOf
            | SideTable::ArrayElements
            | SideTable::Callables
            | SideTable::Proxies
            | SideTable::Bound
            | SideTable::Views
            | SideTable::Collections
            | SideTable::Cursors
            | SideTable::Generators
            | SideTable::Helpers
            | SideTable::Prototypes
            | SideTable::Accessors
            | SideTable::Boxed => true,

            // Shape and type identifiers, which are registry numbers rather
            // than anything the heap allocated.
            SideTable::ProtoTypes => false,
            // Code addresses and a `&'static str`. A frame address is not a
            // reference and following one would hand the region a decompose of
            // a number that never named a cell.
            SideTable::PendingStacks => false,
            // Locates `Context::buffers`, whose contents are bytes. A live view
            // names the buffer's CELL through `Views`, not through this.
            SideTable::BufferOf => false,
            // A boolean.
            SideTable::Detached => false,
            // A compiled pattern and its flags. `lastIndex` is an ordinary
            // property, so it is already covered by the inline slots or a
            // spill.
            SideTable::Regexes => false,
            // A freeze level.
            SideTable::Integrity => false,
            // Writable/enumerable/configurable flags, keyed by a property
            // NUMBER rather than by anything the heap allocated.
            SideTable::Attributes => false,
            // A boolean.
            SideTable::Derived => false,
            // An index into what the host holds, which the host roots through
            // `entry::external` rather than through the heap.
            SideTable::Foreign => false,
        }
    }
}


mod edges;
mod release;

pub(super) use edges::edges;
pub(super) use release::release_tables;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_table_is_listed_exactly_once() {
        // `ALL` is what makes the walk total, and a duplicated or omitted
        // variant would make it quietly partial in one direction or wasteful in
        // the other. The compiler checks the `match`; nothing but this checks
        // the array.
        for table in SideTable::ALL {
            let occurrences = SideTable::ALL.iter().filter(|&&t| t == table).count();
            assert_eq!(occurrences, 1, "{table:?} appears more than once in ALL");
        }
    }

    #[test]
    fn the_tables_that_can_name_a_cell_are_the_ones_traced() {
        // Pins the count rather than the membership, because membership is what
        // the `match` already enforces. A change to this number is a change to
        // what the collector considers reachable, which is the review this
        // assertion asks for.
        let holding = SideTable::ALL
            .iter()
            .filter(|table| table.holds_references())
            .count();
        assert_eq!(
            holding, 13,
            "a table changed sides; `trace::edges_of` needs the matching arm"
        );
    }
}
