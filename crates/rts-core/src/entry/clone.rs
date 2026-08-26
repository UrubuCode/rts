//! `structuredClone` — a deep copy that survives a cycle.
//!
//! # Why the graph is built before anything is allocated
//!
//! The same reason [`super::json`] parses into a tree first, and here it is not
//! merely tidy — it is the only shape that works at all. `with_current` holds a
//! `RefCell` borrow for its body, the operations this needs (`own_keys`,
//! `get_indexed`, `array_new`) each take one of their own, a second borrow
//! panics, and an `extern "C"` frame cannot unwind, so a nested one **aborts the
//! process**. A recursive clone that allocated as it descended would hold a
//! borrow at one depth across the call taking the next.
//!
//! So this is two passes over a pure Rust arena that names no heap object it did
//! not put there: [`walk`] reads the source into [`Node`]s, and [`materialise`]
//! turns them into values. Neither recurses where it touches the heap.
//!
//! # Why an arena and not a tree
//!
//! Because a cycle is not a tree. `const a = {}; a.self = a` is a graph with one
//! node and one edge into itself, and a tree can only represent that by
//! unrolling it forever. A [`Slot`] is therefore an arena index rather than a
//! nested value, and a cell already walked resolves to the index it was given —
//! which is what makes the clone's own self-reference point at the *clone*, as
//! the specification requires, rather than at the original.
//!
//! # Why cycle detection is a memo and not a depth cap
//!
//! [`Graph::seen`] maps an original cell to the arena index standing for it, and
//! it is consulted before every descent. That is what makes a self-referential
//! object **terminate**, and it terminates for the right reason: the second
//! visit to a cell is recognised as the same object.
//!
//! A depth cap was the alternative and it is worse in the way that matters. It
//! also terminates, and it terminates by **silently truncating** — a cycle would
//! come back as a deep chain of copies with `undefined` at the bottom, which is
//! not the input, is not an error, and is indistinguishable from data that
//! really was that shape. [`DEPTH`] still exists below, but it guards Rust's own
//! stack against genuinely deep nesting; it is not how cycles are handled.
//!
//! # What is not cloneable, and what this answers instead
//!
//! A function and a symbol have no clone: the specification throws a
//! `DataCloneError`. [`super::throw`] ends the program rather than reaching a
//! handler in a caller, so **an uncloneable value becomes `undefined` in the
//! position it occupied**, and the rest of the structure is copied. The same
//! choice `JSON.stringify` already makes for a function, which is the closest
//! precedent the engine has — and it is recoverable, where killing the program
//! over one unexpected member is not.
//!
//! The divergence that leaves, named: a program relying on the throw to reject
//! bad input gets a copy with holes in it instead. Cloning `undefined` itself is
//! legal and produces `undefined`, so the answer alone does not say which
//! happened.
//!
//! # What a clone does NOT carry
//!
//! The prototype of a plain object. A cloned object is a plain object, which is
//! the specification's rule and not a shortcut: `structuredClone` of a class
//! instance is defined to produce data, not an instance. `Date`, `Map`, `Set`
//! and `Error` keep theirs because those are the cloneable *kinds*, and the
//! prototype comes from the class registration rather than from the source cell
//! — so a clone answers to the same methods a fresh one does.

mod build;
mod errors;

use build::{materialise, resolve};

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::text::Str;
use crate::value::Value;

/// How deep the walk descends before it gives up.
///
/// A guard on Rust's stack, which an `extern "C"` frame cannot survive
/// overflowing — not a cycle mechanism; the module documentation says why those
/// are two different problems. The number matches [`super::json`]'s because the
/// constraint is the same one, and a structure this deep and not cyclic is
/// machine-made.
const DEPTH: usize = 200;

/// The name this module provides, in the shape [`super::global_fns::provided`]
/// has — one function, asked for the same way the other globals are.
/// A deep copy of a value, cycles included.
///
/// # Why a host gets this and not a `serialize`
///
/// Because "serialize" is a name from another runtime, and this crate holds no
/// knowledge of one — the same rule that keeps the machine layer free of
/// language names applies here. What a host actually needs when it is asked to
/// round-trip a value is a COPY that survives a cycle, which is what
/// `structuredClone` already is, and a module wearing another runtime's name can
/// build its own surface on top of it.
///
/// Ambient rather than context-taking on purpose: the walk takes and releases
/// its own borrows between steps, because it reads properties and allocates,
/// and it cannot do either while one is held. So this must NOT be called from
/// inside `with_runtime`.
pub fn deep_copy(value: u64) -> u64 {
    let mut graph = Graph::default();
    let root = walk(&mut graph, value, 0);
    let made = materialise(&graph);
    resolve(root, &made)
}

pub(super) fn provided(name: &str) -> Option<(super::native::Native, u32)> {
    match name {
        // `structuredClone(value, options)` — arity 1, because `options` is
        // optional. See `super::global_fns::provided` for why the number is
        // here rather than in a table beside it.
        "structuredClone" => Some((structured_clone, 1)),
        _ => None,
    }
}

/// `structuredClone(value, options)`.
///
/// `options.transfer` is a list of `ArrayBuffer`s the source gives up rather
/// than copies. This engine has exactly one transferable kind — an
/// `ArrayBuffer`; a port is not something it has — so the list is read for
/// that one case: each listed buffer's clone still comes from the same
/// byte-copying walk every buffer goes through, and afterwards the ORIGINAL is
/// detached — its own bytes truncated to nothing and `byteLength` set to `0` —
/// which is what makes a transferred buffer's copy independent (the language's
/// requirement) rather than a second reference to bytes it no longer owns.
extern "C" fn structured_clone(_e: u64, _t: u64, value: u64, options: u64, _a2: u64, _a3: u64) -> u64 {
    let mut graph = Graph::default();
    let root = walk(&mut graph, value, 0);
    let made = materialise(&graph);
    let result = resolve(root, &made);
    detach_transferred(options);
    result
}

/// Detaches every `ArrayBuffer` named in `options.transfer`, if any.
///
/// Read and applied AFTER the clone is fully materialised: a buffer transfers
/// itself (`transfer: [buffer]` cloning `buffer`), and detaching it first would
/// have the walk copy zero bytes instead of the ones the clone is supposed to
/// carry away.
fn detach_transferred(options: u64) {
    let transfer_name = with_current(|context| context.intern_value(Str::from_str("transfer")).bits());
    let list = super::computed::get_indexed(options, transfer_name);
    let Some(cells) = with_current(|context| {
        Value(list)
            .as_slot()
            .and_then(|cell| context.elements_at(cell).cloned())
    }) else {
        return;
    };
    with_current(|context| {
        for held in cells {
            let Some(cell) = Value(held).as_slot() else {
                continue;
            };
            // Use the canonical detach operation so the mark, byteLength and
            // detached state stay in sync for every consumer, including
            // `buffer.isAscii`/`isUtf8` and N-API's detached query.
            context.detach_buffer(cell);
        }
    });
}

/// A value in the arena: either a copy of something with no structure, or the
/// node standing for something that has.
#[derive(Clone, Copy)]
enum Slot {
    /// Passed through unchanged.
    ///
    /// A number, a boolean, a singleton — and a **string**, which is where this
    /// is a decision rather than an omission. A string cell here is immutable
    /// and interned, so a copy of one could never be told apart from the
    /// original by any operation the language has. Cloning it would spend a cell
    /// per string to make a difference nothing can observe.
    Bits(u64),
    At(usize),
}

/// What one cloneable object is, with its children as arena indices.
enum Node {
    Array(Vec<Slot>),
    /// Members in enumeration order, which is the order they are written back
    /// in — so the clone enumerates the way the original did.
    Object(Vec<(Str, Slot)>),
    Map(Vec<(Slot, Slot)>),
    Set(Vec<Slot>),
    /// The time value, which is all a `Date` is.
    Date(f64),
    /// A pattern and its flags, which is all a `RegExp` is.
    ///
    /// It had no arm and cloned through the plain-object walk, which worked
    /// only while `source` and `flags` were own PROPERTIES — the clone was a
    /// plain object answering them, and `structuredClone(/a/g).exec` was
    /// already `undefined`. Once they became prototype accessors
    /// (`regex::accessors` says why) the walk had nothing to copy and the wrong
    /// answer became visible. Rebuilt from the two texts instead.
    Regexp(String, String),
    /// An error, as the three things the specification says survives one.
    ///
    /// `class` is the name of the registered class whose prototype the clone
    /// gets, and it is one of [`STANDARD`] rather than whatever the source
    /// answered: a subclass, or an instance whose `name` was overwritten,
    /// clones as a plain `Error`. That is the HTML specification's own rule and
    /// it is checkable — Bun and Node both answer `Error` for
    /// `structuredClone(new (class My extends Error{}))`.
    ///
    /// Everything else the source object owned is DROPPED, which is the one
    /// place this kind differs from the plain-object walk beside it: an error
    /// with `err.code = "ENOENT"` clones without it. Measured against both
    /// runtimes rather than assumed, because it is the surprising half.
    Error {
        class: &'static str,
        message: Option<Str>,
        stack: Option<Str>,
    },
    /// An `ArrayBuffer`'s raw bytes, copied — the source and the clone never
    /// share a store, so a write through one is invisible to the other, which
    /// is what the specification's "cloned, not shared" actually means for a
    /// buffer with no members to walk.
    Buffer(Vec<u8>),
}

/// The arena, and which original cell each node stands for.
#[derive(Default)]
struct Graph {
    nodes: Vec<Node>,
    /// Original cell to arena index.
    ///
    /// A vector and a linear scan, for the reason [`super::json`]'s writer gives
    /// about its own open list: the cost is a `u32` comparison per entry, and a
    /// hash of a `u32` is not cheaper until the structure is far larger than
    /// anything a clone is called on.
    seen: Vec<(u32, usize)>,
}

impl Graph {
    /// The index a cell was already given, if it has one.
    fn found(&self, cell: u32) -> Option<usize> {
        self.seen
            .iter()
            .find(|(held, _)| *held == cell)
            .map(|(_, at)| *at)
    }

    /// Reserves an index for a cell **before** its children are walked.
    ///
    /// The ordering is the cycle handling. A node registered after its children
    /// would not be in [`Self::seen`] when one of them reached back to it, and
    /// the walk would descend forever — which is the bug this whole arrangement
    /// exists to make unrepresentable rather than to catch.
    fn reserve(&mut self, cell: u32) -> usize {
        let at = self.nodes.len();
        // A placeholder, overwritten by the caller once the children are known.
        self.nodes.push(Node::Set(Vec::new()));
        self.seen.push((cell, at));
        at
    }
}

/// What a value is, decided inside one borrow and carried out of it.
///
/// The classification and the descent are separate for the reason the module
/// gives: everything below `Shape` runs with no borrow held.
#[derive(Clone, Copy)]
enum Shape {
    Bits(u64),
    Array(u32),
    Object(u32),
    Map(u32),
    Set(u32),
    Date(u32, f64),
    /// A regular expression, recognised by carrying a compiled pattern. The
    /// cell alone, unlike [`Shape::Date`]'s number: `Shape` is `Copy` so that
    /// classification costs nothing, and two owned strings are not.
    Regexp(u32),
    /// An `ArrayBuffer`, recognised by owning a byte store — see
    /// [`super::buffers`], the one thing here that reaches into another
    /// module's storage rather than reading properties like everything else.
    Buffer(u32),
    /// An error, recognised by its prototype chain reaching `Error.prototype`.
    Error(u32),
    /// A function or a symbol — see the module documentation.
    Uncloneable,
}

fn shape_of(context: &mut Context, value: u64) -> Shape {
    let Some(cell) = Value(value).as_slot() else {
        // A symbol is a primitive and is still not cloneable: the specification
        // refuses one because its identity is the whole of what it is, and a
        // copy would be a different symbol wearing the same description.
        //
        // A bigint IS copied, by bits, sharing its digits — unobservable for the
        // same reason sharing a string is: neither can be written to.
        if super::symbol::is_symbol(context, value) {
            return Shape::Uncloneable;
        }
        return Shape::Bits(value);
    };
    if context.text_at(cell).is_some() {
        return Shape::Bits(value);
    }
    // Asked before anything structural, because a function is an object too.
    // Getting the order wrong clones its members into a plain object that is not
    // callable — a copy that looks like it worked.
    //
    // A symbol used to be checked here and no longer needs to be: it is a
    // primitive now, so it never reaches a cell at all and is refused by the
    // caller with the other non-cell values.
    if context.callable_at(cell).is_some() {
        return Shape::Uncloneable;
    }
    // An `ArrayBuffer` owns a byte store directly rather than through any
    // property a walk would find — checked before `Date`'s property probe and
    // the element/plain-object fallback, none of which know what a buffer is.
    if context.bytes_at(cell).is_some() {
        return Shape::Buffer(cell);
    }
    // Before the plain-object fallback: a regular expression answers
    // `source`/`flags` through PROTOTYPE accessors, so the walk below would
    // find no own members and clone it as an empty object.
    if context.regexp_at(cell).is_some() {
        return Shape::Regexp(cell);
    }
    if context.table_at(cell).is_some() {
        // Which of the two it is comes from the prototype the class
        // registration gave it, because the table itself cannot say: a `Set`
        // stores each member as both key and value, so its entries are
        // indistinguishable from a `Map`'s of identical pairs. An unrecognised
        // prototype — a subclass whose prototype is its own — is treated as a
        // `Map`, which keeps both halves of every entry where guessing `Set`
        // would discard the values.
        let prototype = context.prototype_at(cell);
        if prototype.is_some() && prototype == super::class_support::prototype(context, "Set") {
            return Shape::Set(cell);
        }
        return Shape::Map(cell);
    }
    // A `Date` is recognised by the property its time value lives in, which is
    // what a `Date` IS here — `date`'s module documentation records why that is
    // an ordinary property rather than an internal slot.
    let key = context.well_known(super::date::TIME);
    if let Some(time) = super::objects::read_property(context, cell, key)
        && let Some(ms) = time.as_f64()
    {
        return Shape::Date(cell, ms);
    }
    if context.elements_at(cell).is_some() {
        return Shape::Array(cell);
    }
    // LAST of the structural questions, and deliberately: it is the only one
    // that walks a prototype chain, so putting it earlier would charge every
    // array, buffer and collection for a question none of them can answer yes
    // to. A plain object pays one walk, which ends at `Object.prototype` after
    // a step or two — and ends immediately when nothing in the program has
    // reached `Error` at all, because there is then no prototype to compare
    // against.
    if super::object_proto::extends_class(context, cell, "Error") {
        return Shape::Error(cell);
    }
    Shape::Object(cell)
}


/// Reads one value into the arena.
///
/// Recursive over Rust's stack and over nothing else: every heap read below
/// takes its own borrow and gives it back before the recursive call.
fn walk(graph: &mut Graph, value: u64, depth: usize) -> Slot {
    if depth >= DEPTH {
        return Slot::Bits(absent());
    }
    let shape = with_current(|context| shape_of(context, value));
    let cell = match shape {
        Shape::Bits(bits) => return Slot::Bits(bits),
        Shape::Uncloneable => return Slot::Bits(absent()),
        Shape::Array(cell) | Shape::Object(cell) | Shape::Map(cell) | Shape::Set(cell) => cell,
        Shape::Date(cell, ms) => {
            // No children, so nothing can reach back to it — but it is still
            // registered, because the same `Date` appearing twice in one
            // structure must come back as one object twice, not two.
            if let Some(at) = graph.found(cell) {
                return Slot::At(at);
            }
            let at = graph.reserve(cell);
            graph.nodes[at] = Node::Date(ms);
            return Slot::At(at);
        }
        Shape::Regexp(cell) => {
            // `Date`'s reasoning: no child VALUES, and still registered so one
            // pattern appearing twice comes back as one object twice.
            if let Some(at) = graph.found(cell) {
                return Slot::At(at);
            }
            let at = graph.reserve(cell);
            let read = with_current(|context| {
                let pattern = context.regexp_at(cell)?;
                Some((pattern.source().to_owned(), pattern.flags().to_owned()))
            });
            // The classification saw a pattern under a borrow since given back,
            // so the absence is unreachable rather than unhandled.
            let (source, flags) = read.unwrap_or_default();
            graph.nodes[at] = Node::Regexp(source, flags);
            return Slot::At(at);
        }
        Shape::Error(cell) => {
            // Same reasoning as `Date`: nothing below it to walk — the three
            // texts are read here and there are no child VALUES — and still
            // registered, so one error appearing twice in a structure comes
            // back as one object twice.
            if let Some(at) = graph.found(cell) {
                return Slot::At(at);
            }
            let at = graph.reserve(cell);
            graph.nodes[at] = errors::walked(cell);
            return Slot::At(at);
        }
        Shape::Buffer(cell) => {
            // Same reasoning as `Date`: no children to walk, registered so a
            // buffer referenced twice in one structure clones once.
            if let Some(at) = graph.found(cell) {
                return Slot::At(at);
            }
            let at = graph.reserve(cell);
            let bytes = with_current(|context| {
                context.bytes_at(cell).cloned().unwrap_or_default()
            });
            graph.nodes[at] = Node::Buffer(bytes);
            return Slot::At(at);
        }
    };
    if let Some(at) = graph.found(cell) {
        return Slot::At(at);
    }
    let at = graph.reserve(cell);
    // Built into a local first: the children are walked through `graph`, and
    // assigning into `graph.nodes[at]` in the same expression would hold it
    // borrowed across that.
    let node = match shape {
        Shape::Array(_) => {
            // Copied out of the borrow rather than iterated inside one, because
            // walking each element takes borrows of its own.
            let elements =
                with_current(|context| context.elements_at(cell).cloned().unwrap_or_default());
            Node::Array(
                elements
                    .into_iter()
                    .map(|element| walk(graph, element, depth + 1))
                    .collect(),
            )
        }
        Shape::Map(_) => Node::Map(
            super::collections::entries_of(value)
                .into_iter()
                .map(|(key, held)| (walk(graph, key, depth + 1), walk(graph, held, depth + 1)))
                .collect(),
        ),
        Shape::Set(_) => Node::Set(
            super::collections::entries_of(value)
                .into_iter()
                .map(|(key, _)| walk(graph, key, depth + 1))
                .collect(),
        ),
        _ => Node::Object(members(graph, value, depth)),
    };
    graph.nodes[at] = node;
    Slot::At(at)
}

/// An object's own members, read the way the language reads them.
///
/// Through `own_keys` and `get_indexed` rather than off the layout, so that
/// enumeration order is the runtime's one answer to that question and an
/// accessor runs its getter — which is what a real `structuredClone` observably
/// does, and what reading slots directly would have skipped.
fn members(graph: &mut Graph, value: u64, depth: usize) -> Vec<(Str, Slot)> {
    let names = super::array::own_keys(value);
    let names = with_current(|context| {
        Value(names)
            .as_slot()
            .and_then(|cell| context.elements_at(cell).cloned())
            .unwrap_or_default()
    });
    let mut built = Vec::with_capacity(names.len());
    for name in names {
        let held = super::computed::get_indexed(value, name);
        let key = with_current(|context| super::text::to_text(context, Value(name)));
        let Some(key) = key else {
            continue;
        };
        built.push((key, walk(graph, held, depth + 1)));
    }
    built
}

/// `undefined`, from outside a borrow.
fn absent() -> u64 {
    with_current(|context| undefined_of(context))
}
