//! How compiled code reaches this crate.
//!
//! # The boundary is scalars, so the state cannot be a parameter
//!
//! An entry point is `extern "C"` over ABI types, and those are `u64`, `i64`,
//! `i32`, `f64`, `bool` and strings. A `&mut ShapeTree` does not cross that
//! boundary and never will.
//!
//! So an operation that needs the heap cannot *receive* it — it reaches ambient
//! state. That is the decision this module is, and the alternative it rejects is
//! threading a context pointer through every call site: it works, it costs a
//! register and an argument everywhere, and it lets a caller pass the wrong one.
//!
//! # One context per thread, not one per process
//!
//! A global behind a lock would serialise every property read in the program,
//! which is the opposite of what a per-region heap is for. The machine already
//! has [`rts_cranelift::sched::SchedulerId`] per region and
//! `Delivery::Elsewhere` for what crosses; a thread-local context is the same
//! shape on the data side.
//!
//! # What qualifies as an entry point
//!
//! The machine's own rule, unchanged:
//!
//! > An entry point exists if and only if the operation touches the heap, the
//! > operating system, or global mutable state. Pure computation is
//! > instructions.
//!
//! So `to_int32` is **not** here — it is arithmetic, and belongs in what the
//! lowering emits. `add` is, because two strings joined is an allocation.
//! Declaring the whole crate would put hundreds of rows in a table whose entire
//! argument is that a small closed set beats a large open one.

mod alloc;
mod array;
mod barrier;
mod bitwise;
mod cache;
mod functions;
mod objects;
mod text;
mod operators;

// The operators are defined in their own module and named from here, because a
// caller wants "the entry points" in one place rather than a module tree.
pub use array::{array_new, own_keys};
pub use bitwise::{
    bit_and, bit_not, bit_or, bit_xor, exponent, shift_left, shift_right,
    shift_right_unsigned,
};
pub use functions::{ARGUMENT_SLOTS, call, closure_new};
pub use objects::{
    delete_property, get_indexed, get_property, has_property, object_new, set_indexed,
    set_property,
};
pub use text::{declare_keys, declare_literals, string_const, type_of};
pub use operators::{
    divide, greater, greater_equal, less, less_equal, loose_equals, multiply, remainder, subtract,
};
mod table;

pub use alloc::alloc;
pub use barrier::write_barrier;
pub use cache::cache_resolve;
pub use table::{CORE_ENTRY_COUNT, CoreEntry};

use std::cell::RefCell;

use rts_cranelift::shape::{KeyRegistry, ShapeTree};

use crate::coerce::{Sum, add as add_primitives, number_to_string as print_number};
use crate::heap::{Slab, Slot};
use crate::text::{Interner, Str};
use crate::value::{
    Singletons, Value, strict_equals as values_strict_equals, to_boolean as values_to_boolean,
};

/// Everything a running program's operations need and cannot be handed.
pub struct Context {
    /// Every heap value, of whichever kind.
    ///
    /// One table, not one per kind — the decision recorded in [`crate::heap`]:
    /// the tag space already spends a tag on "reference", and splitting the
    /// payload to re-encode which kind would spend address bits to save a branch
    /// a shape check performs anyway.
    pub cells: Slab<Str>,
    /// Every layout. The machine's, because there is exactly one.
    pub shapes: ShapeTree,
    /// Where property keys are numbered, shared with the compiler.
    pub keys: KeyRegistry,
    /// Strings that have been used as keys while running.
    pub interner: Interner,
    /// The region compiled code allocates in and addresses with arithmetic.
    ///
    /// Beside the slab rather than replacing it: the slab holds what the
    /// RUNTIME reaches for in Rust, and this holds what COMPILED CODE reaches
    /// for with a base and a stride. Two heaps is a state to get out of, not a
    /// design — see `docs/engine/objects-are-aggregates.md` for which one wins.
    pub region: crate::heap::Region,
    /// What the layouts a shape arrives at look like.
    ///
    /// A shape answers *which field*; the aggregate it becomes answers *where*.
    /// Held here because the runtime is what turns one into the other today —
    /// and that is a state to get out of, because compiled code guarding a type
    /// has to name the SAME `TypeId`. A third agreement, not yet needed and
    /// recorded before it is.
    pub types: rts_cranelift::types::TypeRegistry,
    /// Which shape each layout came from, by `TypeId` index.
    ///
    /// The reverse of `ShapeTree::layout`, which the header makes necessary: a
    /// cell records the type, and finding a property needs the shape. Kept
    /// rather than searched, because a linear scan of every layout per property
    /// access is the cost this whole exercise is removing.
    shape_of_type: Vec<rts_cranelift::shape::ShapeId>,
    /// The layout a string's identity cell has.
    ///
    /// One word, holding where the text is. A string's bytes are not in the
    /// region — they are any length and a cell is 64 bytes — so the cell is the
    /// identity and the text lives beside it. That is also what a real engine
    /// does: string data is separate from string identity.
    text_type: rts_cranelift::types::TypeId,
    /// The layout a callable's cell has.
    ///
    /// Two words: where the code is, and the environment it closes over. A
    /// reserved layout rather than an object shape, for the same reason text
    /// has one — a closure is not a thing whose fields a program names, and
    /// giving it a shape would put `code` in the key registry as a property any
    /// JavaScript could read and, worse, write.
    ///
    /// That last point is not tidiness. The first word is a raw code address,
    /// and a program able to store a number there would name the instruction
    /// the next call jumps to.
    closure_type: rts_cranelift::types::TypeId,
    /// Where a cell's properties past the seventh live.
    ///
    /// A cell holds seven inline slots, and an object with more used to lose
    /// them: the write was refused and the read answered `undefined`. The
    /// region's own documentation calls that "a wrong answer that looks like a
    /// right one" while describing the refusal — which is what it became once
    /// the read had no way to say so.
    ///
    /// This is the overflow indirection that documentation names. Compiled
    /// code never reaches it: `cache_resolve` already answers negative for a
    /// slot past the inline ones, so such a read takes the slow path — which
    /// is why the fast path needed no change at all.
    spills: Slab<Vec<u64>>,
    /// Which spill each cell uses, by region index.
    spill_of: Vec<Option<Slot>>,
    /// Which cells are arrays, and where their elements are.
    ///
    /// # Why a side table and not a reserved layout
    ///
    /// It WAS a reserved layout, like text and callables, and that made an
    /// array a thing with no shape — so `a.tag = 9` was a silent no-op and
    /// `a.tag` read `undefined`. A wrong program that runs, which is worse
    /// than a refusal.
    ///
    /// An array IS an object: it has properties, a prototype eventually, and
    /// elements as well. So its cell carries an ordinary shape like any other
    /// object's, and being an array is recorded beside it rather than instead
    /// of it.
    ///
    /// Keyed by region index, which a moving collector would have to update.
    /// Noted rather than solved: there is no collector, and the alternative —
    /// a word inside the cell — spends one of seven inline slots on every
    /// object to record something almost none of them are.
    array_elements: Vec<Option<Slot>>,
    /// How many reference stores told the collector about themselves.
    ///
    /// Counted rather than acted on, because there is no collector. It exists
    /// so the call site does not have to be found again the day there is one.
    pub barriers: u64,
    /// How many times a cached read site asked where a property is.
    ///
    /// A hit does not reach the runtime at all, so this counts MISSES — which
    /// makes it the one number that separates "the cache works" from "the cache
    /// is a slower way of calling". Both produce the same wall clock scaling,
    /// and no measurement already taken can tell them apart.
    pub resolves: u64,
    /// The elements of every array, apart from the cells that identify them.
    ///
    /// A second store beside `cells`, and not a contradiction of the one-table
    /// decision that module records: that one is about the ENCODING — a
    /// reference stays a region index and what it names is read from the
    /// cell's header, rather than from bits carved out of the payload. How the
    /// runtime holds the bytes on the Rust side is a different question, and
    /// elements are a `Vec<u64>` where text is a `Str`.
    pub arrays: Slab<Vec<u64>>,
    /// Every string literal the running program can name, by its number.
    ///
    /// Values rather than text: a literal evaluated twice is the same string,
    /// so making one per evaluation would both allocate on every pass of a loop
    /// and answer a different identity each time.
    ///
    /// Seeded by the host from what the compilation collected, in that order —
    /// the number the code carries is a position in this list, which is the
    /// same shape as the key and singleton numberings.
    pub literals: Vec<u64>,
    /// Which singleton number means what, as the language declared it.
    pub singletons: Singletons,
}

impl Context {
    /// A context holding nothing.
    /// A context around a heap that already exists.
    ///
    /// The region has to come from outside, and the reason is the whole of why
    /// this constructor exists beside [`Self::new`]: **its base address is a
    /// number baked into compiled code**. A context that made its own region
    /// would be a second heap, and every address a compiled program computed
    /// would point into the first one — which nothing would be allocating in.
    pub fn over(singletons: Singletons, region: crate::heap::Region) -> Self {
        Context {
            region,
            ..Context::new(singletons)
        }
    }

    /// A context with a heap of its own.
    ///
    /// For the runtime's own tests and for anything that is not running
    /// compiled code. Anything that IS must use [`Self::over`], because the
    /// region's base is a constant inside the code.
    pub fn new(singletons: Singletons) -> Self {
        let mut types = rts_cranelift::types::TypeRegistry::new();
        // One word: where the text is. Declared before anything else so its
        // number is stable across contexts, which a test comparing two of them
        // would otherwise depend on the order of unrelated allocations for.
        let text_type = types.declare(&[rts_cranelift::repr::Repr::I64]);
        // Code address, then environment. Declared here beside text and for the
        // same reason: a number that depends on which allocation happened first
        // is a number two contexts disagree about.
        let closure_type = types.declare(&[
            rts_cranelift::repr::Repr::I64,
            rts_cranelift::repr::Repr::Tagged,
        ]);
        Context {
            cells: Slab::new(),
            arrays: Slab::new(),
            spills: Slab::new(),
            spill_of: Vec::new(),
            shapes: ShapeTree::new(),
            keys: KeyRegistry::new(),
            interner: Interner::new(),
            // A capacity fixed at construction, because growing moves the base
            // and every reference compiled code holds was turned into an
            // address against the old one. Growing is the collector's job.
            region: crate::heap::Region::with_capacity(1 << 16),
            types,
            shape_of_type: Vec::new(),
            text_type,
            closure_type,
            array_elements: Vec::new(),
            resolves: 0,
            barriers: 0,
            // Empty until a host seeds it. A program with no string literal
            // never reaches the table, and one that does gets it from the
            // compilation that produced the code.
            literals: Vec::new(),
            singletons,
        }
    }

    /// The layout a shape arrives at, remembering the way back.
    ///
    /// `ShapeTree::layout` is what turns a shape into an aggregate. The reverse
    /// is recorded here because a cell's header holds the TYPE and a property
    /// lookup needs the SHAPE — and searching every layout per access is the
    /// cost this design exists to remove.
    pub fn layout_of(&mut self, shape: rts_cranelift::shape::ShapeId) -> rts_cranelift::types::TypeId {
        let ty = self.shapes.layout(shape, &mut self.types);
        if self.shape_of_type.len() <= ty.index() {
            self.shape_of_type.resize(ty.index() + 1, shape);
        }
        self.shape_of_type[ty.index()] = shape;
        ty
    }

    /// Which shape a cell's type came from, if it is an object's.
    ///
    /// `None` for a string's layout and for a callable's, which is what makes a
    /// reference's kind readable from the object rather than from the encoding
    /// — the machine's own answer to a tag space that has no room for one.
    ///
    /// # Why both reserved layouts have to be named here
    ///
    /// Because `shape_of_type` is grown with `resize(index + 1, shape)`, which
    /// fills every new position with the shape being recorded. So the moment an
    /// object's layout is numbered above a reserved one, the reserved one's
    /// position holds a real shape that was never its own — and a callable
    /// would answer property reads by interpreting its code address as a field.
    ///
    /// Excluding by index rather than fixing the fill: the reserved layouts are
    /// the two positions that legitimately have no shape, and saying so is the
    /// fact, where a sentinel fill would be a way of encoding it.
    pub fn shape_of(&self, ty: u32) -> Option<rts_cranelift::shape::ShapeId> {
        let ty = ty as usize;
        if ty == self.text_type.index() || ty == self.closure_type.index() {
            return None;
        }
        self.shape_of_type.get(ty).copied()
    }

    /// The layout a callable's cell has.
    ///
    /// Read by the two entry points that make and call one. Exposed as a method
    /// rather than a public field so nothing outside can claim a cell is
    /// callable by writing the number into a header.
    pub fn closure_type(&self) -> rts_cranelift::types::TypeId {
        self.closure_type
    }

    /// The text a reference names, if it names one.
    ///
    /// A reference is a REGION index now, uniformly — for a string as much as
    /// for an object. Its cell holds the string's type in the header and where
    /// the text is in its first slot; the text itself is in the slab, because a
    /// string is any length and a cell is 64 bytes.
    ///
    /// That indirection is not a compromise. String identity and string data are
    /// separate things in every engine that moves either one, and putting the
    /// identity in the region is what lets one reference space serve both kinds.
    pub fn text_at(&self, reference: u32) -> Option<&Str> {
        if self.region.type_of(reference)? as usize != self.text_type.index() {
            return None;
        }
        let slot = self.region.field(reference, 0)? as u32;
        self.cells.at(Slot(slot)).ok()
    }

    /// Put a string on the heap and return the value naming it.
    pub fn intern_value(&mut self, text: Str) -> Value {
        let slot = self.cells.insert(text).slot();
        let size = crate::heap::STRIDE;
        let ty = self.text_type.index() as u32;
        let cell = self.region.alloc(size, ty).expect("the region has room");
        self.region
            .set_field(cell, 0, u64::from(slot.0))
            .expect("a string cell has a first slot");
        Value::from_slot(cell)
    }

    /// Whether two slots hold equal text.
    ///
    /// What `===` needs and cannot answer alone: two strings are equal when
    /// their text is, however they were allocated, while two objects are equal
    /// only when they are the same object.
    pub fn same_text(&self, left: u32, right: u32) -> bool {
        match (self.text_at(left), self.text_at(right)) {
            (Some(a), Some(b)) => a.same_units(b),
            _ => false,
        }
    }
}

thread_local! {
    /// This thread's context, absent until something installs one.
    static CONTEXT: RefCell<Option<Context>> = const { RefCell::new(None) };
}

/// Install a context for this thread, and run something with it.
///
/// Returns the context afterwards so a caller can inspect what a program left
/// behind — which is how the tests below work, and how a host would read a
/// result out.
pub fn with_context<T>(context: Context, body: impl FnOnce() -> T) -> (Context, T) {
    CONTEXT.with(|slot| *slot.borrow_mut() = Some(context));
    let value = body();
    let context = CONTEXT.with(|slot| slot.borrow_mut().take());
    (
        context.expect("the context installed above is still installed"),
        value,
    )
}

/// Run something against this thread's context.
///
/// Aborts when there is none. That is not a runtime condition a program can
/// reach — it means compiled code ran before anything installed a heap, which is
/// a broken embedding — and unwinding out of an `extern "C"` frame is undefined
/// behaviour, so there is nothing better to do than say so and stop.
fn with_current<T>(body: impl FnOnce(&mut Context) -> T) -> T {
    CONTEXT.with(|slot| {
        let mut borrowed = slot.borrow_mut();
        let Some(context) = borrowed.as_mut() else {
            eprintln!("rts: an entry point ran with no context installed on this thread");
            std::process::abort();
        };
        body(context)
    })
}

/// `a + b`, on values already reduced to primitives.
///
/// An entry point because joining two strings allocates. The caller has already
/// resolved `ToPrimitive` in the order [`crate::coerce::add_operand_order`]
/// states — this cannot do it, because running a `valueOf` is calling.
#[rtse::entry]
pub fn add(left: u64, right: u64) -> u64 {
    with_current(|context| {
        let text_of = |value: Value| {
            value
                .as_slot()
                .and_then(|slot| context.text_at(slot))
                .cloned()
        };

        // `ToString` of a primitive, which is what the non-string side of a
        // concatenation becomes. Separate from `text_of` because that one
        // answers "is this already a string" and decides *whether* to
        // concatenate — a single function doing both would make `1 + 2` answer
        // `"12"`.
        let stringify = |value: Value| text::to_text(context, value);

        match add_primitives(Value(left), Value(right), text_of, stringify) {
            Some(Sum::Number(number)) => Value::from_f64(number).bits(),
            Some(Sum::Text(text)) => context.intern_value(text).bits(),
            // Neither a number nor a string: the caller handed over something
            // still needing ToPrimitive. Answering NaN would be a wrong number;
            // this is a contract violation, and saying so beats inventing one.
            None => Value::from_f64(f64::NAN).bits(),
        }
    })
}

/// `a === b`.
///
/// An entry point because two strings are equal when their *text* is, which
/// needs the heap. Everything else about it is arithmetic.
#[rtse::entry]
pub fn strict_equals(left: u64, right: u64) -> bool {
    with_current(|context| {
        values_strict_equals(Value(left), Value(right), |a, b| context.same_text(a, b))
    })
}

/// `ToBoolean`.
///
/// An entry point for one case out of seven: the empty string. Every other
/// falsy value is decided by arithmetic, and a lowering that proved its operand
/// is a number should emit the comparison rather than call this.
#[rtse::entry]
pub fn to_boolean(value: u64) -> bool {
    with_current(|context| {
        let singletons = context.singletons;
        values_to_boolean(Value(value), singletons, |slot| {
            context.text_at(slot as u32).is_some_and(Str::is_empty)
        })
    })
}

/// `String(n)`.
///
/// An entry point because the result is allocated.
#[rtse::entry]
pub fn number_to_string(value: f64) -> u64 {
    with_current(|context| {
        let text = print_number(value);
        context.intern_value(text).bits()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn singletons() -> Singletons {
        Singletons {
            undefined: 0,
            null: 1,
        }
    }

    fn fresh() -> Context {
        Context::new(singletons())
    }

    #[test]
    fn two_separately_allocated_strings_are_strictly_equal() {
        let mut context = fresh();
        let first = context.intern_value(Str::from_str("a"));
        let second = context.intern_value(Str::from_str("a"));
        assert_ne!(first.bits(), second.bits(), "different slots");

        let (_, equal) = with_context(context, || strict_equals(first.bits(), second.bits()));

        assert!(
            equal,
            "strings compare by text under ===; comparing the reference would \
             make \"a\" === \"a\" false whenever the two were built separately"
        );
    }

    #[test]
    fn two_distinct_objects_are_not_strictly_equal() {
        let mut context = fresh();
        // Two cells in the region, which is where an object's identity is now.
        let root = context.shapes.root();
        let ty = context.layout_of(root).index() as u32;
        let first = context.region.alloc(crate::heap::STRIDE, ty).expect("room");
        let second = context.region.alloc(crate::heap::STRIDE, ty).expect("room");

        let left = Value::from_slot(first);
        let right = Value::from_slot(second);

        let (_, equal) = with_context(context, || strict_equals(left.bits(), right.bits()));
        assert!(
            !equal,
            "objects compare by identity, which is exactly what strings do not"
        );
    }

    #[test]
    fn adding_two_numbers_stays_a_number_and_adding_a_string_allocates() {
        let mut context = fresh();
        let text = context.intern_value(Str::from_str("n="));

        let (context, sum) = with_context(context, || {
            add(Value::from_i32(2).bits(), Value::from_i32(3).bits())
        });
        assert_eq!(Value(sum).as_f64(), Some(5.0));

        let number_text = {
            let (mut context, printed) = with_context(context, || number_to_string(1.0));
            let joined = with_context(context, || add(text.bits(), printed));
            context = joined.0;
            let value = Value(joined.1);
            context
                .text_at(value.as_slot().unwrap())
                .and_then(Str::to_rust)
        };
        assert_eq!(number_text.as_deref(), Some("n=1"));
    }

    #[test]
    fn the_empty_string_is_the_one_falsy_value_that_needs_the_heap() {
        let mut context = fresh();
        let empty = context.intern_value(Str::empty());
        let filled = context.intern_value(Str::from_str("x"));

        let (_, answers) = with_context(context, || {
            [
                to_boolean(empty.bits()),
                to_boolean(filled.bits()),
                to_boolean(Value::from_i32(0).bits()),
                to_boolean(Value::from_i32(1).bits()),
            ]
        });

        assert_eq!(answers, [false, true, false, true]);
    }

    #[test]
    fn a_number_prints_through_the_entry_point_as_it_prints_anywhere() {
        let (context, printed) = with_context(fresh(), || number_to_string(0.1 + 0.2));
        let text = context
            .text_at(Value(printed).as_slot().unwrap())
            .and_then(Str::to_rust);
        assert_eq!(text.as_deref(), Some("0.30000000000000004"));
    }
}
