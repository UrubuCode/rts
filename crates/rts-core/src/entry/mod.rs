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

mod accessor;
mod alloc;
mod array;
mod array_proto;
mod barrier;
mod bigint_class;
mod bigints;
mod bitwise;
mod buffer;
mod cache;
mod chain;
mod class_support;
mod collect_cycle;
mod context;
mod clone;
mod buffers;
mod collections;
mod computed;
#[cfg(test)]
#[path = "context_tests.rs"]
mod context_tests;
mod current;
pub mod declared;
mod date;
mod error;
pub mod external;
mod finalize;
mod foreign;
mod function_proto;
mod functions;
mod generator;
mod global;
mod global_fns;
mod integrity;
mod iterate;
mod json;
pub(in crate::entry) mod list_iterator;
mod loops;
mod math;
mod native;
mod number;
mod object_global;
mod object_proto;
mod modules;
mod objects;
mod operators;
mod primitive;
mod primitive_proto;
mod primitives;
mod promise;
mod proxy;
mod reflect;
mod regex;
pub mod roots;
pub(super) mod string;
mod symbol;
mod text;
mod throw;
pub mod trace;
mod weak;
mod uri;

// The operators are defined in their own module and named from here, because a
// caller wants "the entry points" in one place rather than a module tree.
pub use array::{array_new, own_keys};
pub use array_proto::arguments_at;
pub use loops::{Pending, Rest, Source, declare_loop_source, declare_rest, pump_sources};
pub use bitwise::{
    bit_and, bit_not, bit_or, bit_xor, exponent, shift_left, shift_right, shift_right_unsigned,
};
pub use computed::{delete_property, get_indexed, has_property, key_number, set_indexed};
pub use functions::{
    call_with_args, construct_with_args, rest_arguments,
    ARGUMENT_SLOTS, call, closure_new, construct, instance_of, mark_class_constructor,
    mark_derived, set_call_name, super_construct, super_construct_with_args,
};
pub use generator::{FrameShape, declare_frames, generator_new, generator_yield};
pub use global::{global_get, global_get_unbound, global_set};
pub use iterate::{array_append, array_append_all, iterate};
pub use modules::{
    module_publish_all,
    Provided, boolean_value, buffer_class, canonical_encoding, decode_base64, decode_bytes, declare_global,
    declare_module, declare_module_lazy, encode_base64, encode_text, get_member, make_array, make_array_in, make_callable,
    make_bigint, make_buffer, make_namespace, make_number, make_object, make_string,
    bytes_of, bytes_pointer, get_member_at, is_array, is_object, make_bytes, member_key, make_instance, make_prototype, module_at_name, module_binding, module_namespace, module_publish, module_specifiers, forget_module, null_value, number_of,
    Evaluator, declare_evaluator, evaluate, evaluator, is_array_in, is_callable_in, member_names, string_in, null_in, put_member, set_prototype_in, text_in,
    write_bytes,
    text_of, undefined_in, undefined_value, with_runtime,
};
pub use function_proto::running_function;
pub use objects::{
    get_property, get_super_property, object_new, object_spread, set_property,
    set_super_property,
};
pub use promise::{drain_microtasks, promise_await, promise_new, promise_settle, settled};
pub use operators::{
    divide, greater, greater_equal, less, less_equal, loose_equals, multiply, remainder, subtract,
};
pub use primitives::{add, number_to_string, same_value, strict_equals, to_boolean, to_boolean_in};
pub use bigint_class::{bigint_new, negate};
pub use bigints::{bigint_from_words, bigint_i64, bigint_u64, bigint_words};
pub use buffers::detach::{buffer_detached, detach_buffer};
pub use regex::regex_new;
pub use text::{
    declare_keys, declare_literals, declare_templates, described, string_const, template_strings,
    type_of,
};
mod table;

pub use accessor::{define_getter, define_setter};
pub use alloc::alloc;
pub use external::{held_current, hold_current, release_current};
pub use weak::{forget_current as weak_forget, peek_current as weak_peek, watch_current as weak_watch};
pub use foreign::{attach_current as foreign_attach, attached_current as foreign_attached, detach_current as foreign_detach};
pub use finalize::{
    OnDeath, Pending as OnDeathCall, cancel as cancel_on_death, collect_now,
    drain as drain_finalizers, on_death,
};
pub use barrier::write_barrier;
pub use cache::{cache_resolve, cache_resolve_store};
pub use chain::{get_prototype, set_prototype};
pub use clone::deep_copy;
pub use current::with_context;
pub(crate) use current::with_current;
pub use table::{CORE_ENTRY_COUNT, CoreEntry};
pub use throw::{
    declare_function_names, make_named_error, pending, take_thrown, throw, throw_type_error,
    throw_value, thrown,
};

use rts_cranelift::shape::{KeyRegistry, ShapeTree};

use crate::heap::{Aside, Slab, Slot};
use crate::text::{Interner, Str};
use crate::value::Singletons;

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
    /// A proxy's target and handler.
    ///
    /// Beside the cell like everything else about one, and the reason it is not
    /// IN the cell is the point of the design: a proxy has no own properties,
    /// so every access to it misses the compiled cache and reaches the entry
    /// point where the traps are.
    proxies: Aside<(u64, u64)>,
    /// Where an iterator is: the list it walks, and the index it is on.
    ///
    /// Beside the cell like every other per-object fact, and holding the LIST
    /// rather than a copy of it — `values()` already built one, and a second
    /// would be a second answer to what the iterator is walking.
    cursors: Aside<(u64, u32)>,
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
    /// Which cells are keyed collections, and what they hold.
    ///
    /// Beside the cell for the reason `array_elements` records: a `Map` IS an
    /// object, so `m.tag = 9` has to work and its cell carries an ordinary
    /// shape. A reserved layout would make that a silent no-op. The collector
    /// cannot see this table — see [`collections`] for the note.
    /// What each bound function remembers.
    ///
    /// Beside the cell rather than in it or on it: a program able to write a
    /// bound function's target would be choosing what the next call jumps to
    /// and with what receiver, which is the same argument the code address
    /// itself is kept out of reach for.
    bound: Aside<function_proto::Bound>,
    /// Which callable each call in progress is running.
    ///
    /// A stack because calls nest, and needed because a native closes over
    /// nothing: a bound function's one way to know WHICH binding is running is
    /// the call that reached it.
    callees: Vec<u64>,
    /// The literal index of the callee about to be called, as its source
    /// spelled it — `obj.foo` or `foo` — set by the call site immediately
    /// before the jump and taken (not merely read) by [`functions::invoke`]
    /// on entry, so it cannot leak into a nested call this one goes on to
    /// make.
    ///
    /// `None` for a callee `emit/call.rs` did not name — a computed callee
    /// such as `(a || b)()`, where no single spelling is right.
    pending_call_name: Option<u64>,
    /// The bytes every `ArrayBuffer` owns.
    ///
    /// A `Slab` for the reason `arrays` is one: a cell is sixty-four fixed
    /// bytes and a buffer is any length.
    pub buffers: Slab<Vec<u8>>,
    /// Which cell owns which byte store.
    ///
    /// The collector cannot see this — the same note every `Aside` here
    /// carries, and the same bet arrays already make.
    buffer_of: Aside<Slot>,
    /// Which cells had their bytes taken away.
    ///
    /// Beside the cell rather than in place of the store, because "detached"
    /// and "empty" are two states with one byte count — see
    /// `buffers/detach.rs`, which is the only thing that writes this.
    detached: Aside<bool>,
    /// What each `DataView` and typed array views: whose bytes, from where,
    /// how many, read as what.
    ///
    /// Never a copy. Two views over one buffer seeing each other's writes is
    /// the whole contract, and a copy would satisfy every test that used one
    /// view at a time.
    views: Aside<buffers::View>,
    collections: Aside<collections::Table>,
    /// Which generator each cell is, and how to re-enter it.
    ///
    /// Beside the cell for the reason `collections` is: a generator object's own
    /// slots stay ordinary properties, so hanging a field on one costs nothing
    /// and takes nothing from the runtime.
    generators: Aside<generator::State>,
    /// What the running generator's last `yield` produced.
    ///
    /// One slot rather than a stack, because it is written and immediately
    /// taken: whoever resumed reads it the instant the call returns. See
    /// `generator.rs` for why a stack is the thing that would drift.
    yielded: Option<u64>,
    /// What a parked frame looks like, per compiled generator body.
    ///
    /// Filled by the host before the program runs, keyed by code address — the
    /// one thing this crate holds about a compiled function.
    frames: Vec<generator::FrameShape>,
    /// What each compiled function is called, by its code address.
    ///
    /// For a stack trace to name a frame. Keyed by ADDRESS because that is what
    /// a callable holds, and filled by the host after placement for the reason
    /// `frames` is: the addresses do not exist until then.
    function_names: Vec<(u64, String)>,
    regexes: Aside<regex::Regexp>,
    /// What every regular expression inherits from, once one exists.
    ///
    /// Made on demand rather than at construction: a program with no regular
    /// expression should not spend three cells of a fixed-size region on the
    /// object and the two native callables it holds.
    regexp_prototype: Option<u64>,
    /// Which cells are symbols, which are shared, and how many were minted.
    ///
    /// One structure rather than three fields, because they are one fact with
    /// three parts and splitting them across the context is how the registry
    /// and the counter come to disagree about which key is next.
    /// Every promise, every reaction waiting on one, and the queue they run in.
    ///
    /// One structure rather than a table per part: the machine's promise table,
    /// the value each settled with and which waiter is which reaction are one
    /// fact in three pieces, and splitting them across the context is how the
    /// queue and the side table come to disagree about what a `ContinuationId`
    /// means.
    promises: promise::Machine,
    symbols: symbol::Symbols,
    /// What each declared class registered as, once it has been asked for.
    ///
    /// A list rather than a field per class, and the reason is
    /// [`class_support`]'s: `#[rtse::class]` expands to code that has to find
    /// its own prototype, and a field per class would mean the attribute could
    /// not add one without editing this struct — the "a proc macro cannot see
    /// its neighbours" limit showing up as a build error instead of a design.
    classes: Vec<class_support::Registered>,
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
    /// What every array inherits from, once one has been asked for a method.
    ///
    /// Substituted by the chain walk rather than linked from each cell, for the
    /// reason `string_prototype` records: `array_new` would otherwise write the
    /// link at every allocation to record one fact they all share.
    array_prototype: Option<u32>,
    /// Which keys on a cell are a pair of functions rather than a slot.
    ///
    /// Deliberately NOT in the shape: compiled code emits `cached_get`, which
    /// would find a getter recorded as an ordinary property and RETURN it
    /// instead of calling it. See [`accessor`] for why the absence is
    /// load-bearing rather than an omission.
    accessors: Aside<Vec<(u32, Option<u64>, Option<u64>)>>,
    /// Which cells refuse to be changed, and how much.
    ///
    /// # Why this is beside the cell and not in the shape
    ///
    /// A shape is a key, a slot and a representation, and the machine's own
    /// documentation says so. Integrity is a fact about one OBJECT rather than
    /// about the layout it shares with every other object of that shape, so a
    /// flag in the tree would freeze every `{x: 1}` in the program at once.
    ///
    /// # Why it is not enough on its own
    ///
    /// Because compiled code does not ask. `cached_set` compares the object's
    /// type against the one the site remembers and writes at the offset it
    /// remembers — so a site that warmed up before the freeze would keep
    /// writing. `objects::freeze` therefore also gives the cell a **new type**
    /// with the same layout, which makes every site miss and ask; the store
    /// resolver then answers negative and the write reaches the slow path,
    /// where this table is read. See [`cache::cache_resolve_store`].
    integrity: Aside<integrity::Integrity>,
    /// What individual properties permit, where they do not permit everything.
    ///
    /// Deviations only: a property a program wrote is writable, enumerable and
    /// configurable, so an object nobody called `defineProperty` on has no entry
    /// here. Beside the cell for the accessor table's reason — what is true of
    /// one cell's one key is not true of its layout.
    attributes: Aside<Vec<(rts_cranelift::shape::Key, integrity::Attributes)>>,
    /// The argument vector each call in progress supplied, if any.
    ///
    /// A stack, and pushed by EVERY call rather than only by the ones that
    /// allocate a vector: a callee reading its rest must not find the vector of
    /// an outer call that is still running. The cost is named where the
    /// operation is, along with what removes it.
    pub pending_arguments: Vec<u64>,
    /// Which callables must ask their parent for the object they build.
    ///
    /// A syntactic fact the compiler knows and this crate cannot see: a derived
    /// constructor and a plain function are the same kind of cell. Written at
    /// class definition time, read by `construct`.
    derived: Aside<bool>,
    /// Which callables are class constructors, and so must be reached through
    /// `new`.
    ///
    /// The same shape as `derived` and for the same reason: whether a
    /// callable came from a `class` declaration is syntax the compiler knows
    /// and this crate cannot see, because an ordinary function and a class
    /// constructor are the same kind of cell otherwise. Written at class
    /// definition time, read by `call` and `call_with_args` — never by
    /// `construct`, which is the one path that is allowed to reach one.
    class_constructors: Aside<bool>,
    /// The primitive a wrapper object stands for.
    ///
    /// `new Number(5)` is an object whose `[[NumberData]]` is `5`, and the
    /// language reads that slot from `valueOf`, `toString`, `ToPrimitive` and
    /// `JSON.stringify`. Nothing held it, so all four asked the object and got
    /// `NaN`, `"NaN"` or `{}` — wrong answers that ran.
    ///
    /// Beside the cell rather than as a property, and the difference is
    /// observable: `Object.getOwnPropertyNames(new Number(5))` is empty, so a
    /// hidden key would have to be filtered out of every enumeration — a rule
    /// stated once here against one stated in each of them. It is also why the
    /// three data slots share one table: `[[NumberData]]`, `[[BooleanData]]` and
    /// `[[StringData]]` are never both present on one object, and the value's
    /// own tag already says which it is.
    boxed: Aside<u64>,
    /// One word a client keeps beside an object, opaque here.
    ///
    /// Not a reference: nothing marks it and nothing follows it. See
    /// [`foreign`], which says what it is for and what happens when the
    /// object dies.
    foreign: Aside<usize>,
    /// Registrations waiting for their object to be collected.
    ///
    /// Not an `Aside`: a cell may have several, and what is looked up is the
    /// set on a DYING cell rather than the one on a live one. See [`finalize`].
    pub deaths: Vec<(u32, u32, finalize::Pending)>,
    /// The next identifier [`finalize::on_death`] hands out. Never reused.
    pub next_death: u32,
    /// What the sweep queued and the next drain will call.
    pub dying: Vec<finalize::Pending>,
    /// The class each `new` in progress actually named.
    ///
    /// A stack because construction nests, and a stack rather than an argument
    /// because the fact has to survive an arbitrary number of `super()` calls
    /// and the calling convention has no slot left to carry it.
    pub new_targets: Vec<u64>,
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
    /// The stores that made this region point at another.
    ///
    /// Empty in every program this engine can express, because nothing
    /// publishes a reference across a thread — which is what makes an entry
    /// here evidence of a defect rather than a fact about the heap. See
    /// `barrier` for why it is built before there is a collector to read it.
    pub remembered: barrier::Remembered,
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
    /// The digits of every bigint, apart from the words that name them.
    ///
    /// A slab for the reason `arrays` is one: a payload is forty-eight bits
    /// and arbitrary precision is not.
    pub bigints: Slab<crate::bigint::BigInt>,
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
    /// Values something OUTSIDE the heap is holding, and the collector must
    /// therefore keep.
    ///
    /// The stack scan already covers a value a Rust frame holds in a local. It
    /// covers nothing a native put on the HEAP — a `Vec` a foreign library owns,
    /// a slot an addon keeps across calls — because a conservative scan reads
    /// the stack and the stack alone. That is the gap this closes, and it is
    /// what an N-API `napi_ref` is: a value a `.node` addon holds after the call
    /// that produced it has returned.
    ///
    /// A `Vec` of `(u32, u64)` rather than a map: what holds one of these holds
    /// a handful, and the identifier has to survive removals without shifting
    /// (a caller keeps it), so it is minted rather than positional. See
    /// [`external`].
    pub external: Vec<(u32, u64)>,
    /// Values something outside the heap is WATCHING without keeping.
    ///
    /// The mirror of `external`, and the sweep clears an entry as it frees the
    /// cell — see [`weak`], which explains why a kept word is worse than a
    /// cleared one.
    pub weak: Vec<(u32, u32, Option<u64>)>,
    /// The next identifier [`weak::watch`] hands out. Never reused.
    pub next_weak: u32,
    /// The next identifier [`external::hold`] hands out.
    ///
    /// Never reused, so a released identifier presented again is refused rather
    /// than answering whatever took its place.
    pub next_external: u32,
    /// The namespace object each specifier the host provided names.
    ///
    /// A list rather than a map: a host provides a handful. See [`modules`] for
    /// what this is and, more importantly, what it is not.
    pub modules: Vec<modules::Registered>,
    /// How a host turns source text into a value, if it offered a way.
    ///
    /// Absent unless one installs it: this crate has no compiler and the crate
    /// that does depends on this one, so the capability can only arrive from
    /// above. See [`modules::evaluate`].
    pub evaluator: Option<modules::Evaluator>,
    /// What still has work to do after the program.s last statement.
    ///
    /// Registered by whoever owns a background thread, never by the host — see
    /// [`loops`] for the six copies of one recipe this replaced and for why four
    /// of them were never pumped at all.
    pub loop_sources: Vec<(&'static str, loops::Source)>,
    /// How this host makes time pass, if it offered a way. See [`loops::Rest`].
    pub rest: Option<loops::Rest>,
    /// The throw in flight, as (tag, payload).
    ///
    /// One slot and not a stack: a second throw cannot start before the first is
    /// caught or ends the program, because the only thing that runs in between
    /// is compiled code returning. See [`throw`].
    pub thrown: Option<(i64, u64)>,
    /// What each tagged-template site declared, and what it has been made into.
    ///
    /// Two positions per piece — cooked then raw, as literal numbers — and the
    /// object beside them, absent until the site is first evaluated. Built once
    /// because the specification says a site has ONE strings object: a tag using
    /// it as a map key has to see the same one on every pass, which is the whole
    /// reason this is a table rather than something the code emits.
    ///
    /// Lazily rather than at declaration time: a program declares every site it
    /// contains and runs a few of them, and building all of them would allocate
    /// two arrays per template nobody evaluated.
    pub templates: Vec<(Vec<u32>, Option<u64>)>,
    /// Which singleton number means what, as the language declared it.
    pub singletons: Singletons,
    /// Which tag number means which of the language's own kinds.
    ///
    /// Told rather than assumed, for the reason `singletons` is: the machine
    /// hands tags out by number and a second language on it would number its
    /// own differently.
    pub kinds: crate::value::Kinds,
    /// The top of this thread's stack, as the host's own OS call told it.
    ///
    /// A fact this crate cannot obtain itself — rule 1 says anything needing an
    /// operating system goes to `rts-host`, and `rts-core` must build for
    /// wasm, where the call does not exist. `None` until a host installs it,
    /// the same shape [`Self::evaluator`] and [`Self::rest`] already are: a
    /// capability that can only arrive from above.
    ///
    /// `alloc`'s collection trigger reads this and, when it is absent, does NOT
    /// scan the stack at all rather than scanning a wrong or partial range — a
    /// collection that cannot see every root must not run one, because it would
    /// free a live object rather than merely fail to reclaim one.
    pub stack_high: Option<usize>,
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
    ///
    /// This is also how a thread gets a heap of its own: one region out of a
    /// [`crate::heap::Regions`], installed on that thread. The context never
    /// crosses a thread boundary — it is built where it is used — so the region
    /// is the only thing that moves, and the references it hands out carry its
    /// number, which is what stops two threads naming the same cell.
    pub fn over(
        singletons: Singletons,
        kinds: crate::value::Kinds,
        region: crate::heap::Region,
    ) -> Self {
        let bits = region.selector_bits();
        Context {
            // Every `Aside` is rebuilt at the region's width rather than taken
            // from `new`'s single-region default. They index by cell, and the
            // cell is a reference with the region bits shifted off — so a table
            // built at the wrong width would index by a number that is not a
            // cell, and two objects would collide in it.
            // Numbered by the region for the same reason: `Delivery::Elsewhere`
            // compares scheduler numbers to decide whether a settled promise's
            // waiters are this thread's, and every thread calling itself zero
            // makes that comparison a constant.
            promises: promise::Machine::in_region(region.index()),
            prototypes: Aside::in_region(bits),
            callables: Aside::in_region(bits),
            proxies: Aside::in_region(bits),
            cursors: Aside::in_region(bits),
            spill_of: Aside::in_region(bits),
            bound: Aside::in_region(bits),
            collections: Aside::in_region(bits),
            generators: Aside::in_region(bits),
            yielded: None,
            frames: Vec::new(),
            function_names: Vec::new(),
            buffer_of: Aside::in_region(bits),
            detached: Aside::in_region(bits),
            views: Aside::in_region(bits),
            regexes: Aside::in_region(bits),
            accessors: Aside::in_region(bits),
            derived: Aside::in_region(bits),
            class_constructors: Aside::in_region(bits),
            boxed: Aside::in_region(bits),
            foreign: Aside::in_region(bits),
            deaths: Vec::new(),
            next_death: 1,
            dying: Vec::new(),
            integrity: Aside::in_region(bits),
            attributes: Aside::in_region(bits),
            array_elements: Aside::in_region(bits),
            region,
            ..Context::new(singletons, kinds)
        }
    }

    /// A context with a heap of its own.
    ///
    /// For the runtime's own tests and for anything that is not running
    /// compiled code. Anything that IS must use [`Self::over`], because the
    /// region's base is a constant inside the code.
    pub fn new(singletons: Singletons, kinds: crate::value::Kinds) -> Self {
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
            bigints: Slab::new(),
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
            proxies: Aside::new(),
            cursors: Aside::new(),
            prototypes: Aside::new(),
            array_elements: Aside::new(),
            accessors: Aside::new(),
            derived: Aside::new(),
            class_constructors: Aside::new(),
            boxed: Aside::new(),
            foreign: Aside::new(),
            deaths: Vec::new(),
            next_death: 1,
            dying: Vec::new(),
            integrity: Aside::new(),
            attributes: Aside::new(),
            pending_arguments: Vec::new(),
            new_targets: Vec::new(),
            bound: Aside::new(),
            callees: Vec::new(),
            pending_call_name: None,
            buffers: Slab::new(),
            buffer_of: Aside::new(),
            detached: Aside::new(),
            views: Aside::new(),
            collections: Aside::new(),
            generators: Aside::new(),
            yielded: None,
            frames: Vec::new(),
            function_names: Vec::new(),
            regexes: Aside::new(),
            regexp_prototype: None,
            classes: Vec::new(),
            promises: promise::Machine::in_region(0),
            symbols: symbol::Symbols::new(),
            globals: None,
            string_prototype: None,
            array_prototype: None,
            resolves: 0,
            barriers: 0,
            remembered: barrier::Remembered::default(),
            // Empty until a host seeds it. A program with no string literal
            // never reaches the table, and one that does gets it from the
            // compilation that produced the code.
            literals: Vec::new(),
            external: Vec::new(),
            weak: Vec::new(),
            next_weak: 1,
            next_external: 1,
            modules: Vec::new(),
            evaluator: None,
            loop_sources: Vec::new(),
            rest: None,
            thrown: None,
            templates: Vec::new(),
            singletons,
            kinds,
            stack_high: None,
        }
    }

}
