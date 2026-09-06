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


use crate::value::Value;


use super::Context;
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

/// Every word one cell holds that lives BESIDE it rather than in it.
///
/// Appends, for the same reason `trace::edges_of` does: the caller owns the
/// buffer and clears it between cells, so a collection allocates once rather
/// than once per cell.
///
/// # Why the walk is here and not beside the tracer
///
/// A table and the answer to "can it name a cell" are one decision. Held apart,
/// the answer was a paragraph at the bottom of the tracer and the walk was a
/// sequence of `if let`s, and nothing connected them — which is how a table
/// could be in neither. Here the `match` below and
/// [`SideTable::holds_references`] are checked against each other on every
/// debug run.
pub(super) fn edges(context: &Context, cell: u32, out: &mut Vec<u64>) {
    // Walked as a total pass over `SideTable::ALL` rather than as a sequence
    // of `if let`s, and that is the whole point of the shape: a table with no
    // arm below is a compile error, where a table missing from a sequence was
    // a silence. `side_tables` carries why that mattered — the closing
    // paragraph this loop replaced said "a field in NEITHER list is the bug",
    // which was true and was prose.
    //
    // Each arm that pushes nothing says what the table holds instead. The
    // classification itself lives once, in `SideTable::holds_references`, and
    // the assertion after the match refuses an arm that contradicts it.
    for table in SideTable::ALL {
        let pushed_before = out.len();

        match table {
            // The spill is a REGION block and not a slab vector, so its slots
            // are reached the way an inline slot is — and that is exactly why
            // the block ITSELF has to be marked. The comment here used to say
            // the opposite: that pushing it "would only keep it alive by
            // itself", which was true of the `Vec` in a slab it replaced and
            // false the moment it became a region cell. `alloc_spanning` marks a
            // span's cells 1..n as interior and leaves the FIRST one ordinary,
            // so `Region::live_refs` offers it to the sweep as an object of its
            // own — and nothing marked it, so every collection freed the
            // overflow of an object that was still alive. Its cells went back on
            // the free list, an unrelated allocation took them, and the
            // sixteenth property onward read somebody else's fields.
            //
            // Pushed the same way a generator's parked frame is, for the same
            // reason and through the same mechanism: this is the one place that
            // knows the block exists, and it must be marked even on a cycle
            // where none of its slots happens to hold a reference.
            // `collect_cycle::release` still frees it explicitly with its owner
            // — marking keeps the SWEEP off a block whose owner survived, which
            // is a different question.
            SideTable::SpillOf => {
                if let Some((block, slots)) = context.spill_of.copied(cell) {
                    out.push(Value::from_slot(block).bits());
                    for slot in 0..slots {
                        if let Some(word) = context.region.spanning_field(block, slot, slots) {
                            out.push(word);
                        }
                    }
                }
            }

            SideTable::ArrayElements => {
                if let Some(elements) = context.array_elements.copied(cell)
                    && let Ok(words) = context.arrays.at(elements)
                {
                    out.extend_from_slice(words);
                }
            }

            // A closure's environment. Never its code — the first member is an
            // address, not a value, and following it as one would hand the
            // region a decompose of a number that is not a reference at all. The
            // third member is the class-constructor flag, which is a boolean.
            SideTable::Callables => {
                if let Some((_, environment, _)) = context.callables.copied(cell) {
                    out.push(environment);
                }
            }

            // Never reachable through an inline slot, because a proxy has no own
            // properties by design.
            SideTable::Proxies => {
                if let Some((target, handler)) = context.proxies.copied(cell) {
                    out.push(target);
                    out.push(handler);
                }
            }

            SideTable::Bound => {
                if let Some(bound) = context.bound.get(cell) {
                    bound.trace(out);
                }
            }

            // Stored as a bare cell index rather than an encoded `Value`
            // (`buffers::View::buffer` — see its own module documentation for
            // why a view never caches the derived `Slot`), so it is encoded here
            // before joining the rest: `follow` only ever reads `Value`s.
            SideTable::Views => {
                if let Some(view) = context.views.get(cell) {
                    out.push(Value::from_slot(view.buffer).bits());
                }
            }

            // Keys AND values — see the module documentation for why the weak
            // pair is traced identically to the strong one.
            SideTable::Collections => {
                if let Some(table) = context.collections.get(cell) {
                    table.trace(out);
                }
            }

            // WHAT AN ITERATOR IS WALKING, and this table is the only path to
            // it: the array an array or string iterator steps, or the Map/Set
            // whose table a collection cursor walks. Neither is reachable from
            // the iterator by any other route — an iterator has no own property
            // naming its source, exactly as a helper does not.
            //
            // It was MISSING, and the failure it produced was silence rather
            // than a crash. `const it = [1,2,3][Symbol.iterator]()` leaves the
            // array named by nothing else; the first collection reclaimed it,
            // `stepped` then found no elements at the cell, and `next()`
            // answered `{done: true}`. A `for`-`of` over such an iterator ENDS
            // EARLY and reports nothing. Reproduced 2026-08-29 for an array, a
            // Set and a Map at once.
            SideTable::Cursors => {
                if let Some((listed, _)) = context.cursors.copied(cell) {
                    out.push(listed);
                }
            }

            // Its own cell reference is pushed directly rather than through
            // `follow`: `alloc_spanning` never wrote it as an encoded `Value`
            // anywhere a decode could find it, this is the one place that knows
            // the frame exists at all, and the frame must be marked live even on
            // a turn where every one of its fields happens to hold no reference.
            // Its fields are walked separately, through `Region::spanning_field`
            // rather than `Region::field`, because a frame is not bounded by
            // `INLINE_SLOTS`.
            SideTable::Generators => {
                if let Some(state) = context.generators.get(cell) {
                    out.push(Value::from_slot(state.frame_cell()).bits());
                    state.trace(&context.region, out);
                }
            }

            // A helper's source, its callback, and the inner sequence a
            // `flatMap` is in the middle of. None of the three is reachable
            // through an own property — a helper has none — so this table is the
            // only path to them while the helper is alive.
            SideTable::Helpers => {
                if let Some(state) = context.helpers.get(cell) {
                    state.trace(out);
                }
            }

            SideTable::Prototypes => {
                if let Some(prototype) = context.prototypes.copied(cell) {
                    out.push(prototype);
                }
            }

            // A getter and a setter are callables; an ordinary cached property
            // read never reaches this table (`cache_resolve` already answers
            // negative for an accessor), so this is the only path that visits
            // it.
            SideTable::Accessors => {
                if let Some(list) = context.accessors.get(cell) {
                    for (_, getter, setter, _) in list {
                        out.extend(getter.iter().copied());
                        out.extend(setter.iter().copied());
                    }
                }
            }

            // A wrapper object's boxed primitive — `new String("x")`'s
            // `[[StringData]]`, which is itself a text reference and the one
            // `Aside<u64>` among these that is not obviously an object link.
            SideTable::Boxed => {
                if let Some(primitive) = context.boxed.copied(cell) {
                    out.push(primitive);
                }
            }

            // The nine below hold no reference, and each says what it holds
            // instead. An empty arm here is a DECISION — that is the property
            // the prose list could not offer, because a reader could not tell it
            // from an omission.
            //
            // Shape and type identifiers: registry numbers, not allocations.
            SideTable::ProtoTypes => {}
            // Code addresses and a `&'static str`. A frame address never named
            // a cell, so following one would hand the region a decompose of a
            // number that is not a reference.
            SideTable::PendingStacks => {}
            // Locates `Context::buffers`, whose contents are bytes — an
            // `ArrayBuffer`'s bytes are exactly that. A live view already names
            // the buffer's CELL through `Views` above.
            SideTable::BufferOf => {}
            // A boolean.
            SideTable::Detached => {}
            // A compiled pattern and its flags. `lastIndex` is an ordinary
            // property, already covered by the inline slots or a spill.
            SideTable::Regexes => {}
            // A freeze level.
            SideTable::Integrity => {}
            // Writable/enumerable/configurable flags, keyed by a property NUMBER
            // rather than by anything the heap allocated.
            SideTable::Attributes => {}
            // A boolean.
            SideTable::Derived => {}
            // An index into what the host holds. The host roots those through
            // `entry::external`, which is a root source rather than a heap edge.
            SideTable::Foreign => {}
        }

        // The two halves cannot drift apart silently. An arm that pushes while
        // its table answers `false` is the exact shape of the bug this module
        // exists for, caught here rather than at a collection three seconds
        // later. The other direction — a table that answers `true` and whose arm
        // pushes nothing — is not checkable per cell, because a table is legally
        // empty for most cells; `side_tables`'s own test pins the count instead.
        debug_assert!(
            out.len() == pushed_before || table.holds_references(),
            "{table:?} contributed an edge while declaring that it holds none"
        );
    }
}

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
