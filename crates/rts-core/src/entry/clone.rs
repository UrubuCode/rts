//! `structuredClone` — a deep copy that survives a cycle — and the graph walk
//! the pickle (`super::pickle`) shares with it.
//!
//! # Why the graph is built before anything is allocated
//!
//! The same reason [`super::json`] parses into a tree first, and here it is not
//! merely tidy — it is the only shape that works at all. `with_current` holds a
//! `RefCell` borrow for its body, a second borrow panics, and an `extern "C"`
//! frame cannot unwind, so a nested one **aborts the process**. A getter is
//! user code and user code takes borrows of its own, so a walk that allocated
//! as it descended, or called a getter while holding one, would abort.
//!
//! So this is two passes over a pure Rust arena that names no heap object it did
//! not put there: [`walk`] reads the source into [`Node`]s, and [`materialise`]
//! turns them into values.
//!
//! # Why the walk is a worklist and not a recursion
//!
//! It was a recursion that took a borrow per VALUE — one to classify, one per
//! key to turn it into text, one per member through `get_indexed` — and each of
//! those is a thread-local access plus a `RefCell` flag written and restored.
//! The pickle made that cost a first-class question, because a serializer is
//! measured against `JSON.stringify`, which pays none of it.
//!
//! What the walk does now is read every object whose members are plain data
//! INSIDE one borrow, and leave the borrow only for the one thing that forces
//! it: a member that is an accessor (or an object that is a proxy), whose read
//! runs user code. A graph of ordinary objects is therefore read in a single
//! borrow, whatever its size. The explicit stack is what lets the borrow be
//! given back in the middle of a walk and taken again without unwinding a
//! recursion, and it is also why nesting depth is no longer bounded by Rust's
//! stack.
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
//! really was that shape. [`DEPTH`] still exists for the clone, and says below
//! what it is still for.
//!
//! # What is not cloneable, and what this answers instead
//!
//! A function and a symbol have no clone: the specification throws a
//! `DataCloneError`. For `structuredClone`, **an uncloneable value becomes
//! `undefined` in the position it occupied**, and the rest of the structure is
//! copied — the same choice `JSON.stringify` already makes for a function, which
//! is the closest precedent the engine has.
//!
//! The divergence that leaves, named: a program relying on the throw to reject
//! bad input gets a copy with holes in it instead. The pickle does NOT share
//! that choice — [`Policy::Pickle`] refuses by name, because a byte stream that
//! silently lost a member is a file that is wrong forever.
//!
//! # What a clone does NOT carry
//!
//! The prototype of a plain object. A cloned object is a plain object, which is
//! the specification's rule and not a shortcut: `structuredClone` of a class
//! instance is defined to produce data, not an instance. `Date`, `Map`, `Set`
//! and `Error` keep theirs because those are the cloneable *kinds*, and the
//! prototype comes from the class registration rather than from the source cell
//! — so a clone answers to the same methods a fresh one does. The pickle is the
//! one that keeps a class instance's prototype, through the class registry.

mod build;
mod classify;
mod errors;
mod members;
mod walk;

pub(in crate::entry) use build::{Made, materialise, materialise_holding, populate, resolve};
pub(in crate::entry) use members::data_members;
pub(in crate::entry) use walk::walk;

use super::buffers::element::Kind;
use super::with_current;
use crate::object::Key;
use crate::text::Str;

/// How deep the clone's walk descends before a member becomes `undefined`.
///
/// No longer a guard on Rust's stack — the walk is a worklist — and kept for the
/// clone because it is the clone's observable behaviour, which this change was
/// not the place to alter. The pickle has no walk ceiling at all; its writer
/// carries one of its own, and refuses rather than truncating.
const DEPTH: usize = 200;

/// Which of the two readers of a graph is walking it.
///
/// One walk and two policies rather than two walks: the classification of a
/// value — string, array, `Map`, `Date`, error, buffer, view — is the same
/// question for both, and was already answered here. What differs is what each
/// does with the answer, and every such difference is a `match` on this.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::entry) enum Policy {
    /// `structuredClone`: data only, and `undefined` for what has no copy.
    Clone,
    /// `rts:serde`: class instances and functions by reference, and a refusal
    /// by name for what cannot be written.
    Pickle,
}

/// Why a walk stopped without a graph.
pub(in crate::entry) enum Refusal {
    /// A value the policy cannot represent, and what to tell the program. The
    /// caller raises it as a `TypeError`, outside any borrow.
    Unserializable(String),
    /// A getter the walk ran threw. Nothing is raised again: the error is
    /// already in flight, and the compiled call site above re-raises it — rule
    /// 8 of this crate's README.
    Thrown,
}

/// The name this module provides, in the shape [`super::global_fns::provided`]
/// has — one function, asked for the same way the other globals are.
/// A deep copy of a value, cycles included.
///
/// # Why a host gets this and not a `serialize`
///
/// Because "serialize" is a name from another runtime. What a host actually
/// needs when it is asked to round-trip a value inside one run is a COPY that
/// survives a cycle, which is what `structuredClone` already is; the byte
/// stream that survives a process is `super::pickle`.
///
/// Ambient rather than context-taking on purpose: the walk takes and releases
/// its own borrows, because a getter it runs cannot be run while one is held.
/// So this must NOT be called from inside `with_runtime`.
pub fn deep_copy(value: u64) -> u64 {
    cloned(value)
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

/// The clone of one value, or `undefined` when a getter it ran threw — the
/// error is in flight and the call site above re-raises it.
fn cloned(value: u64) -> u64 {
    match walk(Policy::Clone, value) {
        Ok((graph, root)) => with_current(|context| {
            let made = materialise(context, &graph);
            resolve(root, &made)
        }),
        Err(_) => with_current(|context| super::objects::undefined_of(context)),
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
    let result = cloned(value);
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
        crate::value::Value(list)
            .as_slot()
            .and_then(|cell| context.elements_at(cell).cloned())
    }) else {
        return;
    };
    with_current(|context| {
        for held in cells {
            let Some(cell) = crate::value::Value(held).as_slot() else {
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
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::entry) enum Slot {
    /// Passed through unchanged.
    ///
    /// A number, a boolean, a singleton, a bigint — and a **string**, which is
    /// where this is a decision rather than an omission. A string cell is
    /// immutable, so a copy of one could never be told apart from the original
    /// by any operation the language has. Cloning it would spend a cell per
    /// string to make a difference nothing can observe.
    Bits(u64),
    /// A node of the arena.
    At(usize),
    /// Text the arena holds rather than the heap — a string read out of a byte
    /// stream, or an error's message the clone converted. Interned only when
    /// the graph is materialised, because interning allocates and nothing in
    /// the arena is a root.
    Text(usize),
}

/// A class, as the pickle names one: which module declared it, and its name.
///
/// `prototype` is what an instance of it inherits from in THIS program — the
/// decoder resolves it before anything is built, and the walk leaves it `0`
/// because a writer has no use for it.
#[derive(Clone, Debug)]
pub(in crate::entry) struct ClassName {
    pub(in crate::entry) module: Str,
    pub(in crate::entry) name: Str,
    pub(in crate::entry) prototype: u64,
    /// The schema version the class declared, or `0` for none.
    pub(in crate::entry) version: u64,
}

/// Which class an error is an instance of.
#[derive(Clone, Debug)]
pub(in crate::entry) enum ErrorClass {
    /// One of the language's own, by name.
    Builtin(&'static str),
    /// A class the program declared, which extends one of those.
    Declared(ClassName),
}

/// What one cloneable object is, with its children as arena indices.
pub(in crate::entry) enum Node {
    Array {
        elements: Vec<Slot>,
        /// String-keyed properties beside the indices — `const a = [1, 2];
        /// a.tag = "x"` — which the specification clones too: `structuredClone`
        /// walks `[[OwnPropertyKeys]]` and an array's is not only its indices.
        extra: Vec<(Key, Slot)>,
    },
    /// Members in enumeration order, which is the order they are written back
    /// in — so the clone enumerates the way the original did.
    Object(Vec<(Key, Slot)>),
    /// An instance of a class the program declared, private fields included —
    /// the pickle's alone.
    Instance { class: ClassName, fields: Vec<(Key, Slot)> },
    Map(Vec<(Slot, Slot)>),
    Set(Vec<Slot>),
    /// The time value, which is all a `Date` is.
    Date(f64),
    /// A pattern, its flags, and where the next `exec` starts.
    ///
    /// It had no arm and cloned through the plain-object walk, which worked
    /// only while `source` and `flags` were own PROPERTIES. Rebuilt from the two
    /// texts instead. `last_index` is the pickle's: the clone leaves it `0`,
    /// which is the specification's rule for a clone.
    Regexp { source: String, flags: String, last_index: f64 },
    /// An error: its class, the three members the language gives one, and —
    /// for the pickle — every other own enumerable property. See
    /// [`errors`] for what the clone keeps, which is less.
    Error {
        class: ErrorClass,
        message: Option<Slot>,
        stack: Option<Slot>,
        cause: Option<Slot>,
        extra: Vec<(Key, Slot)>,
    },
    /// An `ArrayBuffer`'s raw bytes, copied — the source and the clone never
    /// share a store.
    Buffer(Vec<u8>),
    /// A typed array: which of the nine kinds, and the bytes its window
    /// covers — copied, for the reason [`Node::Buffer`] copies rather than
    /// shares. `DataView` is deliberately absent: its kind is [`Kind::Raw`],
    /// which the classification refuses before a node is reserved for it.
    ///
    /// The clone gets a private backing buffer of its own rather than sharing
    /// the one the source view named — stated here as the divergence rather
    /// than solved: `structuredClone([b, new Uint8Array(b)])` answers two
    /// buffers that do not alias, where the specification's graph would keep
    /// them one.
    View { kind: Kind, bytes: Vec<u8> },
    /// A Node `Buffer`, which is a `Uint8Array` with another prototype — the
    /// pickle's, which keeps the class where the clone keeps the kind.
    NodeBuffer(Vec<u8>),
    /// A bigint read out of a byte stream. A bigint the walk meets is a
    /// [`Slot::Bits`]: its digits are immutable, so sharing them is invisible.
    BigInt(crate::bigint::BigInt),
    /// `new Number(1)`, `new String("a")`, `new Boolean(true)` — the pickle's.
    Boxed(Slot),
    /// A top-level function, by the name it was declared under — the pickle's.
    Function(ClassName),
}

/// The arena, and which original cell each node stands for.
#[derive(Default)]
pub(in crate::entry) struct Graph {
    pub(in crate::entry) nodes: Vec<Node>,
    /// Text the arena holds; see [`Slot::Text`].
    pub(in crate::entry) texts: Vec<Str>,
    /// Original cell to arena index.
    ///
    /// A map rather than the vector and linear scan it was: the scan was
    /// written for the sizes `structuredClone` is called on, and a pickle of a
    /// save file is not that size — ten thousand objects made the walk
    /// quadratic in them.
    seen: std::collections::HashMap<u32, usize>,
}

impl Graph {
    /// The index a cell was already given, if it has one.
    fn found(&self, cell: u32) -> Option<usize> {
        self.seen.get(&cell).copied()
    }

    /// Reserves an index for a cell **before** its children are walked.
    ///
    /// The ordering is the cycle handling. A node registered after its children
    /// would not be in [`Self::seen`] when one of them reached back to it, and
    /// the walk would descend forever — which is the bug this whole arrangement
    /// exists to make unrepresentable rather than to catch.
    fn reserve(&mut self, cell: u32) -> usize {
        let at = self.push(Node::Set(Vec::new()));
        self.seen.insert(cell, at);
        at
    }

    /// Appends a node that no cell stands for — the decoder's, which has no
    /// source cells, and a leaf the walk fills in one step.
    pub(in crate::entry) fn push(&mut self, node: Node) -> usize {
        let at = self.nodes.len();
        self.nodes.push(node);
        at
    }

    /// Holds text in the arena and answers the slot naming it.
    pub(in crate::entry) fn text(&mut self, text: Str) -> Slot {
        let at = self.texts.len();
        self.texts.push(text);
        Slot::Text(at)
    }
}
