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
mod walk;

use build::{materialise, resolve};
use walk::walk;

use super::buffers::element::Kind;
use super::objects::undefined_of;
use super::with_current;
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
    Array {
        elements: Vec<Slot>,
        /// String-keyed properties beside the indices — `const a = [1, 2];
        /// a.tag = "x"` — which the specification clones too: `structuredClone`
        /// walks `[[OwnPropertyKeys]]` and an array's is not only its indices.
        /// Kept apart from [`Node::Object`]'s field of the same shape rather
        /// than reused through a shared helper, because the source differs —
        /// every index below `elements.len()` is skipped here since
        /// `elements` already carries it, where an object has no such range to
        /// exclude.
        extra: Vec<(Str, Slot)>,
    },
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
    /// A typed array: which of the nine kinds, and the bytes its window
    /// covers — copied, for the reason [`Node::Buffer`] copies rather than
    /// shares. `DataView` is deliberately absent: its kind is [`Kind::Raw`],
    /// which [`shape_of`] refuses before a node is ever reserved for it, so
    /// one clones as a plain object today rather than as a hollow view.
    ///
    /// The clone gets a private backing buffer of its own rather than sharing
    /// the one the source view named — stated here as the divergence rather
    /// than solved, the same shape the module's own "what is not cloneable"
    /// section already keeps for other gaps: `structuredClone([b, new
    /// Uint8Array(b)])` answers two buffers that do not alias, where the
    /// specification's graph would keep them one. Solving it needs the view
    /// to walk its buffer as a CHILD the way [`Node::Array`] walks its
    /// elements, which needs the two-phase build this kind does not have yet.
    View { kind: Kind, bytes: Vec<u8> },
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

/// `undefined`, from outside a borrow.
fn absent() -> u64 {
    with_current(|context| undefined_of(context))
}
