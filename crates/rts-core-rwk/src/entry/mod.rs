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
mod chain;
mod computed;
#[cfg(test)]
#[path = "context_tests.rs"]
mod context_tests;
mod current;
mod functions;
mod global;
mod native;
mod objects;
mod operators;
mod primitives;
mod regex;
pub(super) mod string;
mod text;
mod throw;

// The operators are defined in their own module and named from here, because a
// caller wants "the entry points" in one place rather than a module tree.
pub use array::{array_new, own_keys};
pub use bitwise::{
    bit_and, bit_not, bit_or, bit_xor, exponent, shift_left, shift_right, shift_right_unsigned,
};
pub use computed::{delete_property, get_indexed, has_property, set_indexed};
pub use functions::{ARGUMENT_SLOTS, call, closure_new, construct, instance_of};
pub use global::{global_get, global_set};
pub use objects::{get_property, object_new, set_property};
pub use operators::{
    divide, greater, greater_equal, less, less_equal, loose_equals, multiply, remainder, subtract,
};
pub use primitives::{add, number_to_string, strict_equals, to_boolean};
pub use regex::regex_new;
pub use text::{declare_keys, declare_literals, string_const, type_of};
mod table;

pub use alloc::alloc;
pub use barrier::write_barrier;
pub use cache::cache_resolve;
pub use chain::{get_prototype, set_prototype};
pub use current::with_context;
pub(crate) use current::with_current;
pub use table::{CORE_ENTRY_COUNT, CoreEntry};
pub use throw::throw;

use rts_cranelift::shape::{KeyRegistry, ShapeTree};

use crate::heap::{Aside, Slab, Slot};
use crate::text::{Interner, Str};
use crate::value::{Singletons, Value};

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
    /// What each cell inherits from.
    ///
    /// A value rather than a cell index, because a prototype may be `null` —
    /// which is not "absent", it is the end of the chain, and the two have to
    /// be distinguishable from a cell that was never given one.
    ///
    /// Beside the cell for the reason every one of these is: seven inline
    /// slots are what a program's own properties get, and spending one on a
    /// link almost nothing reads would cost every object.
    prototypes: Aside<u64>,
    /// Which cells are callable, and what they call.
    ///
    /// # Why beside the cell and not in it
    ///
    /// Two things at once. The code address must not be reachable from
    /// JavaScript — a program able to store a number there would name the
    /// instruction the next call jumps to — and a function IS an object, so
    /// `f.x = 1` has to work.
    ///
    /// A reserved layout gave the first and lost the second: a cell with no
    /// shape cannot hold a property, so every write to a function was a silent
    /// no-op. Recording it beside the cell gives both, and is the third use of
    /// this pattern after arrays and the property spill.
    callables: Aside<(u64, u64)>,
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
    spill_of: Aside<Slot>,
    /// Which cells are compiled patterns, and what they compiled to.
    ///
    /// Beside the cell for the reason every one of these is, plus one specific
    /// to this: a regular expression is an object with properties — `source`,
    /// `flags`, and a `lastIndex` a program writes — so its cell carries an
    /// ordinary shape, and what it additionally *is* has nowhere else to live.
    ///
    /// `lastIndex` is deliberately NOT here. It is a real property, because the
    /// language lets a program assign it, and a copy kept beside the cell would
    /// be the one a search reads while the program wrote the other.
    regexes: Aside<regex::Regexp>,
    /// What every regular expression inherits from, once one exists.
    ///
    /// Made on demand rather than at construction: a program with no regular
    /// expression should not spend three cells of a fixed-size region on the
    /// object and the two native callables it holds.
    regexp_prototype: Option<u64>,
    /// Where the names the runtime provides live, once one has been read.
    ///
    /// An object rather than a table, because `RegExp.x = 1` is an ordinary
    /// property write and every mechanism for it already exists. See
    /// [`global`] for why this is not the global object.
    globals: Option<u32>,
    /// What every string inherits from, once one has been asked for a method.
    ///
    /// One object for every string in the program, substituted by the chain
    /// walk rather than linked from each cell — see
    /// [`objects::inherited_from`] for why a link per string would be a word
    /// spent on a fact they all share.
    string_prototype: Option<u32>,
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
    array_elements: Aside<Slot>,
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
        Context {
            cells: Slab::new(),
            arrays: Slab::new(),
            spills: Slab::new(),
            spill_of: Aside::new(),
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
            callables: Aside::new(),
            prototypes: Aside::new(),
            array_elements: Aside::new(),
            regexes: Aside::new(),
            regexp_prototype: None,
            globals: None,
            string_prototype: None,
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
    pub fn layout_of(
        &mut self,
        shape: rts_cranelift::shape::ShapeId,
    ) -> rts_cranelift::types::TypeId {
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
        if ty == self.text_type.index() {
            return None;
        }
        self.shape_of_type.get(ty).copied()
    }

    /// What a cell calls, if it is callable.
    ///
    /// A method rather than a public field so nothing outside this module can
    /// claim a cell is callable, which is what makes the code address
    /// unreachable from anything a program can write.
    pub(super) fn callable_at(&self, cell: u32) -> Option<(u64, u64)> {
        self.callables.copied(cell)
    }

    /// What a cell inherits from, if anything.
    pub(super) fn prototype_at(&self, cell: u32) -> Option<u64> {
        self.prototypes.copied(cell)
    }

    /// Sets what a cell inherits from.
    pub(super) fn set_prototype(&mut self, cell: u32, prototype: u64) {
        self.prototypes.set(cell, prototype);
    }

    /// Records that a cell calls this code with this environment.
    pub(super) fn mark_callable(&mut self, cell: u32, code: u64, environment: u64) {
        self.callables.set(cell, (code, environment));
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

impl Context {
    /// The key a name the runtime itself knows has.
    ///
    /// `length` and `prototype` are properties this crate reads by name rather
    /// than by a number a compilation resolved, because it is the runtime that
    /// wants them — an array answers `length` whether or not the program ever
    /// wrote it, and `new` reads `prototype` on a function the program may
    /// never have touched.
    ///
    /// Interned rather than held as a constant: the number is whatever the
    /// registry issued, and that registry was seeded from what the compilation
    /// resolved. A program that mentions the name already put it there and this
    /// finds the same number; one that never does mints one nothing else uses,
    /// which costs a key and changes no answer.
    ///
    /// One function because it was two, and "intern a name the runtime knows"
    /// is one rule — the second copy is the one that would have interned
    /// against a different registry the day there were two.
    pub(super) fn well_known(&mut self, name: &str) -> crate::object::Key {
        let text = Str::from_str(name);
        crate::object::Key::Name(self.interner.intern(&text, &mut self.keys))
    }
}
