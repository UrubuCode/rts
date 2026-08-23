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
mod arguments;
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
mod eval;
mod eval_scope;
pub mod external;
mod finalize;
mod foreign;
mod function_proto;
mod functions;
mod generator;
mod global;
mod host_class;
mod global_fns;
mod integrity;
mod intl;
mod iterate;
mod iterator;
mod json;
pub(in crate::entry) mod list_iterator;
mod loops;
mod math;
mod native;
mod number;
mod object_global;
mod object_proto;
mod dynamic_module;
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
mod rooted;
pub(super) mod string;
mod switches;
mod symbol;
mod text;
mod throw;
pub mod trace;
mod weak;
mod uri;

// The operators are defined in their own module and named from here, because a
// caller wants "the entry points" in one place rather than a module tree.
pub use array::{array_new, array_of, element_at, elements_base, enumerate_keys, own_keys};
pub use math::math_random;
pub use array_proto::arguments_at;
pub use arguments::arguments_object;
pub use loops::{Pending, Rest, Source, declare_loop_source, declare_rest, pump_sources};
pub use bitwise::{
    bit_and, bit_not, bit_or, bit_xor, exponent, number_exponent, shift_left, shift_right,
    shift_right_unsigned,
};
pub use computed::{delete_property, get_indexed, has_property, key_number, set_indexed, with_has};
pub use functions::{
    call_counted, call_with_args, construct_with_args, rest_arguments,
    ARGUMENT_SLOTS, call, closure_new, construct, instance_of, mark_class_constructor,
    mark_derived, new_target, set_call_name, super_construct, super_construct_with_args,
};
pub use eval::{
    Addition, Agreement, FunctionCompiler, SourceParser, adopt, agreement,
    declare_function_compiler, declare_source_parser,
};
pub use eval_scope::{
    EvalCompiler, declare_eval_compiler, environment_names, eval_direct,
};
pub use generator::{FrameShape, declare_frames, delegate_step, generator_new, generator_yield};
pub use global::{global_get, global_get_unbound, global_set, sloppy_this};
pub use iterate::{array_append, array_append_all, iterate};
pub use dynamic_module::{Resolver, declare_module_meta, declare_resolver, import_meta, module_import};
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
pub use host_class::{declare_host_class, describe_callable};
pub use objects::{
    get_property, get_super_property, object_new, object_spread, set_property,
    set_super_property,
};
pub use promise::{async_start, drain_microtasks, promise_await, promise_new, promise_settle, settled};
pub use operators::{
    divide, greater, greater_equal, less, less_equal, loose_equals, multiply, number_remainder,
    remainder, subtract,
};
pub use primitives::{add, number_to_string, same_value, strict_equals, to_boolean, to_boolean_in};
pub use bigint_class::{bigint_new, negate};
pub use bigints::{bigint_from_words, bigint_i64, bigint_u64, bigint_words};
pub use buffers::detach::{buffer_detached, detach_buffer};
pub use regex::regex_new;
pub use text::{
    declare_keys, declare_literals, declare_templates, described, string_const, string_of,
    template_join, template_strings,
    type_of,
};
mod table;

pub use accessor::{define_getter, define_method, define_setter};
pub use alloc::alloc;
pub use external::{held_current, hold_current, release_current};
pub use weak::{forget_current as weak_forget, peek_current as weak_peek, watch_current as weak_watch};
pub use foreign::{attach_current as foreign_attach, attached_current as foreign_attached, detach_current as foreign_detach};
pub use finalize::{
    OnDeath, Pending as OnDeathCall, cancel as cancel_on_death, collect_now,
    drain as drain_finalizers, on_death,
};
pub use barrier::write_barrier;
pub use cache::{cache_resolve, cache_resolve_indirect, cache_resolve_store, census_report};
pub use chain::{get_prototype, set_prototype};
pub use clone::deep_copy;
pub use current::with_context;
pub(crate) use current::with_current;
pub use table::{CORE_ENTRY_COUNT, CoreEntry};
pub use throw::{
    declare_function_names, make_named_error, pending, take_thrown, throw, throw_type_error,
    throw_value, thrown, thrown_address,
};

use rts_cranelift::shape::{KeyRegistry, ShapeTree};

use crate::heap::{Aside, Slab, Slot};
use crate::text::{Interner, Str};
use crate::value::Singletons;

/// The names the runtime asks for BY NAME on a path that runs per operation.
///
/// On this list because a measurement put them here, not because they are
/// special: `Context::well_known` remembers exactly these and interns
/// everything else, and moving a name on or off changes only the cost.
/// `length` is asked before every property write, `prototype` by every `new`,
/// and the last three are stamped onto every typed array as it is built.
/// Which inline slot of a string's cell holds its length.
///
/// Slot zero is the slab position the text lives at. Slot one is the length,
/// and it is there so that `s.length` can be answered by a LOAD — a string has
/// no shape, so `cache_resolve` could never answer for it and every read went
/// to the runtime, forever, at 99 ns against 4.8 for an ordinary property.
///
/// Safe to store because a string is immutable: written once, at creation, and
/// nothing can make it disagree with the text it describes. The same trick
/// would NOT be safe for an array's length, which is why that one is a real
/// property that `objects::put` reconciles.
pub const TEXT_LENGTH_SLOT: u32 = 1;

/// The names the runtime asks for BY NAME on a path that runs per operation.
pub const CACHED_KEYS: [&str; 6] = [
    "length",
    "prototype",
    "byteLength",
    "byteOffset",
    "buffer",
    // Asked of every object value `JSON.stringify` reaches.
    "toJSON",
];

/// The strings the runtime builds as VALUES on a path that runs per operation.
///
/// Different from [`CACHED_KEYS`] in what is saved. A key is a number and
/// interning one hashes text; a string here is a **cell**, so building one
/// allocates — and an allocation is what brings the next collection closer, not
/// merely what costs time.
///
/// `toJSON` is why this exists: `JSON.stringify` asks every object value
/// whether it has one, and asking built the name as a fresh cell each time.
///
/// `@@hasInstance` is the same shape one operation further out. `instanceof`
/// consults it before it walks anything — a class may decide for itself what an
/// instance of it is — so a name that never changes was formatted, allocated
/// and interned once per operation. It is spelled from
/// [`symbol::HAS_INSTANCE`] rather than written out, because the `@@` in it is
/// that module's encoding and a second copy here is where the two would come to
/// disagree.
pub const CACHED_TEXTS: [&str; 3] = ["toJSON", "", symbol::HAS_INSTANCE];

/// Every string `typeof` can answer, in the order [`Context::type_names`]
/// caches them.
///
/// The list is closed by the language rather than by this crate — ES says which
/// nine — with `"unknown"` the tenth, which is not JavaScript's: it is what a
/// client tag nothing wired produces, and it is here so that a wiring mistake
/// reads as a wiring mistake instead of as `"undefined"`.
pub const TYPE_NAMES: [&str; 9] = [
    "number",
    "boolean",
    "undefined",
    "object",
    "symbol",
    "bigint",
    "string",
    "function",
    "unknown",
];

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
    /// The layouts each cell has been made the prototype OF, and the type
    /// number each of those got.
    ///
    /// # Why a second number for one layout
    ///
    /// Because the inline cache compares the type and nothing else. Two classes
    /// whose instances hold the same fields arrive at the same shape — that is
    /// what a shape tree is for — and therefore at the same type, so a site
    /// warmed on one would recognise the other and read its prototype's method.
    /// Giving instances of different classes different NUMBERS over one shape is
    /// what makes the comparison the cache already performs answer the question
    /// a chain read needs answered, and it is the same mechanism
    /// `integrity::retype` already uses for a frozen cell.
    ///
    /// Nothing about the object moves: same shape, same slots, same offsets.
    ///
    /// # Why it is keyed by the PROTOTYPE and not by the instance
    ///
    /// So that it dies with it. `collect_cycle::release` clears every `Aside`
    /// for a cell it reclaims, so a recycled cell index starts with an empty
    /// memo and mints fresh numbers — which is what stops a stale type from
    /// being reissued to an unrelated object after the free list hands the same
    /// index back. Keyed by the instance it would grow with the program.
    ///
    /// A `Vec` scanned linearly rather than a map, for the reason `accessor_at`
    /// records: one cell is the prototype of one or two shapes in practice, and
    /// hashing would cost more than walking them.
    proto_types: Aside<Vec<(rts_cranelift::shape::ShapeId, rts_cranelift::types::TypeId)>>,
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
    /// Which spill each cell uses, by region index.
    /// Where a cell's overflow lives, and how many slots it has.
    ///
    /// A REGION reference and not a slab slot. It was a `Vec<u64>` in a slab,
    /// and a `Vec` moves when it grows — so a property past the fifteenth had
    /// no stable address and no inline cache could ever reach one.
    spill_of: Aside<(u32, u32)>,
    /// The layout a spill block carries in its header.
    ///
    /// Its own, so a sweep and a trace can tell one from an object: a spill
    /// holds VALUES at every slot and has no shape, where an object has a shape
    /// and its slots are the properties that shape names.
    spill_type: rts_cranelift::types::TypeId,
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
    /// What an ES2025 iterator helper is doing, and to what.
    ///
    /// Beside the cell for the reason `generators` is, and holding an OPERATION
    /// rather than a list: a helper is lazy, so what it holds is what it will
    /// do on the next pull rather than what it already did. See
    /// `iterator::helper`.
    helpers: Aside<iterator::Helper>,
    /// What the running generator's last `yield` produced.
    ///
    /// One slot rather than a stack, because it is written and immediately
    /// taken: whoever resumed reads it the instant the call returns. See
    /// `generator.rs` for why a stack is the thing that would drift.
    yielded: Option<u64>,
    /// Which generator cell each re-entry currently in progress belongs to.
    ///
    /// A stack, unlike `yielded`, and for the reason `callees` is one: a
    /// generator advanced from inside another's body is a re-entry inside a
    /// re-entry, and the inner one must not be mistaken for the outer when it
    /// returns. `yield*` is what asks — see `generator::delegate` — and it asks
    /// while the body is running, which is exactly when the cell is not
    /// available any other way.
    ///
    /// Cells rather than values, and not a root: the generator being resumed is
    /// the receiver of the `.next()` that started this, so it is already held by
    /// the caller. `generator::resume` has depended on that since it was
    /// written — it reads `generators.get_mut(cell)` after the body returns.
    resuming: Vec<u32>,
    /// The async frames being driven right now, and this one IS a root.
    ///
    /// The difference from `resuming` is who holds the object. A generator is
    /// the receiver of the `.next()` that started the re-entry, so the caller
    /// holds it; an async function's frame owner is never handed to the
    /// program at all — between the call that made it and the reaction that
    /// waits on it, this crate is the only thing that names it.
    ///
    /// The window is not hypothetical and was measured: `async_start` makes the
    /// frame, then allocates a promise, then runs a body that allocates
    /// hundreds of objects. A collection anywhere in there freed the frame the
    /// body was standing in, and 500 concurrent async calls turned `console`
    /// itself into `undefined`. While the frame is PARKED the reaction holds
    /// it — see `promise::state::Machine::root_words` — so the two together
    /// cover its whole life.
    pub(in crate::entry) driving: Vec<u32>,
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
    function_names: Vec<(u64, String, u32)>,
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
    ///
    /// Four fields and not three: the last is WHERE the property belongs in
    /// enumeration order — how many properties the cell's shape already held
    /// when the accessor was defined. Insertion order is what `Object.keys`
    /// reports, and with the pair out of the layout there is nothing in the
    /// shape to interleave it with; recording the prefix is what lets
    /// [`array::key_texts`] merge the two sequences without the shape holding a
    /// property it deliberately does not hold.
    accessors: Aside<Vec<(u32, Option<u64>, Option<u64>, u32)>>,
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
    /// How many arguments each call in progress WROTE, when its site said.
    ///
    /// Beside the vector and pushed by the same calls, because it answers the
    /// same question for the case that has no vector: the convention pads its
    /// four slots with `undefined`, so a callee cannot tell `f(undefined)` from
    /// `f()` and the runtime was guessing by dropping `undefined` from the end.
    ///
    /// `None` is an honest "nobody said" — what a native calling another
    /// function pushes, since only a compiled call site knows the number.
    pub pending_counts: Vec<Option<usize>>,
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
    /// The class each `new` in progress actually named, and the activation it
    /// belongs to.
    ///
    /// A stack because construction nests, and a stack rather than an argument
    /// because the fact has to survive an arbitrary number of `super()` calls
    /// and the calling convention has no slot left to carry it.
    ///
    /// # Why the second half exists
    ///
    /// Because a stack of targets alone answers "is a construction in
    /// progress?", and `new.target` asks "was THIS activation constructed?".
    /// An ordinary function called from inside a constructor would read the
    /// constructor's target and answer it as its own — `function F() { return
    /// new.target; }` called from a class body is `undefined` in the language
    /// and would have been the class here.
    ///
    /// The number is `callees.len()` at the moment of the push, which is one
    /// less than it will be once [`functions::invoke`] has pushed the callee:
    /// the entry point matches when `depth + 1 == callees.len()`, so exactly
    /// the activation the target was pushed for sees it. A depth rather than a
    /// callable identity, because the same function may be constructed and then
    /// call itself plainly.
    pub new_targets: Vec<(u64, usize)>,
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
    /// The layout every array is born at — the empty shape plus `length`.
    ///
    /// Computed on the first array and remembered, because it is the same
    /// answer for every one of them. See `array::built_in`.
    array_layout: Option<u32>,
    /// The sweep's scratch list of cells to free, kept across cycles for its
    /// capacity. See `collect_cycle::sweep`.
    doomed: Vec<u32>,
    /// Every cache miss, counted by reason, key and SITE — or `None`, which is
    /// what a run that was not asked for a census pays: no map, no lookup, one
    /// `Option` test per miss.
    ///
    /// The site is the cache cell's address, and it is in the key because
    /// "one site missing a million times" and "a million sites missing once"
    /// are opposite problems that a total cannot tell apart. The first is a
    /// polymorphic or unarmable site; the second is a program with a million
    /// reads.
    pub census: Option<std::collections::BTreeMap<(&'static str, u32, u64), u64>>,
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
    /// The key each of [`CACHED_KEYS`] has, once something has asked for it.
    ///
    /// Not constants: the number is whatever the registry issued for the name,
    /// and the registry is seeded per run from what the compilation resolved —
    /// so it is per context, like everything else here.
    pub(super) well_known_keys: [Option<crate::object::Key>; CACHED_KEYS.len()],
    /// The cell each of [`CACHED_TEXTS`] interned to, once something asked.
    ///
    /// Rooted by [`roots`], like `type_names`: nothing else holds these, and a
    /// collection between two uses would free the one the next use hands back.
    pub(super) well_known_texts: [Option<u64>; CACHED_TEXTS.len()],
    /// The string CELL each interned key text has been handed out as.
    ///
    /// # Why a second table beside the interner, rather than a field in it
    ///
    /// Because they hold different things and one of them is collectable. The
    /// interner maps text to a `Key` — a number, which no collection can
    /// invalidate. This maps that number to a **cell**, which is a heap object
    /// and has to be a root, so it belongs where roots are enumerated. Putting
    /// it inside `text::Interner` would put a collectable thing in a module
    /// that knows nothing about the heap.
    ///
    /// # What it is worth
    ///
    /// `intern_value` does not intern, despite its name: it allocates a fresh
    /// cell every call. So enumeration built a new string for every key on
    /// every call — `Object.keys` of a four-property object allocated four,
    /// each one text the interner already held and that can never change.
    ///
    /// # Why it cannot grow without bound
    ///
    /// One entry per DISTINCT property name the program enumerates, and the
    /// interner already holds every one of those texts forever. So this adds a
    /// cell per name the interner already refused to forget, rather than a new
    /// class of retention.
    pub(super) key_texts_as_values: std::collections::HashMap<crate::object::Key, u64>,
    /// The nine strings `typeof` can answer, each built at most once.
    ///
    /// # Why a cache rather than building the answer
    ///
    /// Because building it ALLOCATES. `intern_value` does not intern despite
    /// the name — it inserts into the slab and calls `alloc_or_die` — so every
    /// `typeof` in a program produced a new cell holding one of nine constant
    /// words, and a cell is what makes a collection arrive sooner.
    ///
    /// Measured 2026-08-11, release, `bench/analytic.ts`: `typeof` cost 363 ns
    /// against 32 ns for an optional chain that stays in compiled code. It does
    /// no work — a tag switch — so the number was never about what it computes.
    ///
    /// # Why in the context and not a `static`
    ///
    /// A cell belongs to the region that allocated it, and there is one region
    /// per run: a `static` would hand a second run a reference into a heap that
    /// no longer exists. That is the same reason `literals` is seeded per run
    /// rather than built once.
    ///
    /// Filled lazily, so a program that never asks pays nothing, and rooted by
    /// [`super::roots`] because nothing else holds these.
    pub type_names: [Option<u64>; TYPE_NAMES.len()],
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
    /// How a host turns a dynamic `import()` specifier into the name the module
    /// table is keyed by, if it offered a way. The same shape and the same
    /// reason as [`Self::evaluator`]: paths are the host.s and this crate is
    /// below it. See [`dynamic_module`].
    pub resolver: Option<dynamic_module::Resolver>,
    /// How a host compiles source text into a callable IN THIS CONTEXT, if it
    /// offered a way — what `new Function` needs and what [`Self::evaluator`]
    /// cannot answer, since that one builds a region of its own. See [`eval`].
    pub function_compiler: Option<eval::FunctionCompiler>,
    /// How a host answers whether source text parses, if it offered a way —
    /// which is as much of `eval` as this engine can perform. See [`eval`].
    pub source_parser: Option<eval::SourceParser>,
    /// How a host RUNS source text, in a scope this program hands it — what
    /// `eval` needs, direct or indirect. See [`eval_scope::EvalCompiler`] for why the
    /// function compiler beside it cannot answer the same question.
    pub eval_compiler: Option<eval_scope::EvalCompiler>,
    /// What still has work to do after the program.s last statement.
    ///
    /// Registered by whoever owns a background thread, never by the host — see
    /// [`loops`] for the six copies of one recipe this replaced and for why four
    /// of them were never pumped at all.
    pub loop_sources: Vec<(&'static str, loops::Source)>,
    /// How this host makes time pass, if it offered a way. See [`loops::Rest`].
    pub rest: Option<loops::Rest>,
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
        // Read before the region moves into the struct below.
        //
        // Every `Aside` is built at the region's width, and every one of them
        // indexes BY CELL: a reference is `(cell << selector_bits) | region`, so
        // a table built at the wrong width would index by a number that is not a
        // cell and two objects would collide in it. The promise machine is
        // numbered by the region for a neighbouring reason —
        // `Delivery::Elsewhere` compares scheduler numbers to decide whether a
        // settled promise's waiters are this thread's, and every thread calling
        // itself zero makes that comparison a constant.
        //
        // Both are zero for a single region, which is what makes [`Self::new`]
        // able to delegate here rather than keep a second field list.
        let bits = region.selector_bits();
        let region_index = region.index();
        let mut types = rts_cranelift::types::TypeRegistry::new();
        // One word: where the text is. Declared before anything else so its
        // number is stable across contexts, which a test comparing two of them
        // would otherwise depend on the order of unrelated allocations for.
        let text_type = types.declare(&[rts_cranelift::repr::Repr::I64]);
        let spill_type = types.declare(&[rts_cranelift::repr::Repr::Tagged]);
        // Code address, then environment. Declared here beside text and for the
        // same reason: a number that depends on which allocation happened first
        // is a number two contexts disagree about.
        Context {
            cells: Slab::new(),
            arrays: Slab::new(),
            bigints: Slab::new(),
            spill_of: Aside::in_region(bits),
            shapes: ShapeTree::new(),
            keys: KeyRegistry::new(),
            interner: Interner::new(),
            // Handed in rather than made here. Its base is a number baked into
            // compiled code, so a context that built its own would be a second
            // heap that nothing compiled could reach — see [`Self::new`], which
            // supplies one for a caller that is not running compiled code.
            region,
            types,
            shape_of_type: Vec::new(),
            text_type,
            spill_type,
            callables: Aside::in_region(bits),
            proxies: Aside::in_region(bits),
            cursors: Aside::in_region(bits),
            prototypes: Aside::in_region(bits),
            proto_types: Aside::in_region(bits),
            array_elements: Aside::in_region(bits),
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
            pending_arguments: Vec::new(),
            pending_counts: Vec::new(),
            new_targets: Vec::new(),
            bound: Aside::in_region(bits),
            callees: Vec::new(),
            pending_call_name: None,
            buffers: Slab::new(),
            buffer_of: Aside::in_region(bits),
            detached: Aside::in_region(bits),
            views: Aside::in_region(bits),
            collections: Aside::in_region(bits),
            generators: Aside::in_region(bits),
            helpers: Aside::in_region(bits),
            yielded: None,
            resuming: Vec::new(),
            driving: Vec::new(),
            frames: Vec::new(),
            function_names: Vec::new(),
            regexes: Aside::in_region(bits),
            regexp_prototype: None,
            classes: Vec::new(),
            promises: promise::Machine::in_region(region_index),
            symbols: symbol::Symbols::new(),
            globals: None,
            string_prototype: None,
            array_prototype: None,
            resolves: 0,
            array_layout: None,
            doomed: Vec::new(),
            census: std::env::var_os("RTS_CACHE_CENSUS")
                .map(|_| std::collections::BTreeMap::new()),
            barriers: 0,
            remembered: barrier::Remembered::default(),
            // Empty until a host seeds it. A program with no string literal
            // never reaches the table, and one that does gets it from the
            // compilation that produced the code.
            well_known_keys: [None; CACHED_KEYS.len()],
            well_known_texts: [None; CACHED_TEXTS.len()],
            key_texts_as_values: std::collections::HashMap::new(),
            literals: Vec::new(),
            type_names: [None; TYPE_NAMES.len()],
            external: Vec::new(),
            weak: Vec::new(),
            next_weak: 1,
            next_external: 1,
            modules: Vec::new(),
            evaluator: None,
            resolver: None,
            function_compiler: None,
            source_parser: None,
            eval_compiler: None,
            loop_sources: Vec::new(),
            rest: None,
            templates: Vec::new(),
            singletons,
            kinds,
            stack_high: None,
        }
    }

    /// A context with a heap of its own.
    ///
    /// For the runtime's own tests and for anything that is not running
    /// compiled code. Anything that IS must use [`Self::over`], because the
    /// region's base is a constant inside the code.
    ///
    /// # Why this delegates instead of holding its own field list
    ///
    /// It held one, and [`Self::over`] finished with
    /// `..Context::new(singletons, kinds)` to borrow it. Rust evaluates that
    /// base expression in full before moving the fields it keeps, so **every
    /// `Context::over` built a whole second region and dropped it** — 64 MiB
    /// reserved, 8 MiB of it written, and freed, on every `rts run`, in
    /// addition to the one the host had already made.
    ///
    /// The writing is gone too, and separately: `Region::sharded` reached the
    /// starting bound with `resize(_, 0)`, a `memset` over memory the operating
    /// system had just handed over already zeroed. It uses `alloc_zeroed` now.
    /// That one is measured — `docs/codegen/startup.md` — and the two together
    /// are most of the 2.47 ms `rts run empty.ts` lost.
    ///
    /// The two lists had also drifted into being one list written twice: the
    /// only difference was that `over` re-stated twenty-two `Aside`s to give
    /// them the region's width, and `Aside::new()` is defined as
    /// `Aside::in_region(0)` — so the override was passing zero where zero was
    /// already there for the single-region case, and the real width for the
    /// sharded one. Passing the region to one constructor says the same thing
    /// once.
    pub fn new(singletons: Singletons, kinds: crate::value::Kinds) -> Self {
        // A capacity fixed at construction, because growing moves the base and
        // every reference compiled code holds was turned into an address
        // against the old one. Growing is the collector's job.
        Self::over(singletons, kinds, crate::heap::Region::with_capacity(1 << 16))
    }

}
