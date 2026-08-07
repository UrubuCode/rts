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
//! turns them into values. Neither is recursive at the point it touches the
//! heap.
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
//! instance is defined to produce data, not an instance. `Date`, `Map` and `Set`
//! keep theirs because those are the cloneable *kinds*, and the prototype comes
//! from the class registration rather than from the source cell — so a clone
//! answers to the same methods a fresh one does.

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::object::Key;
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

pub(super) fn provided(name: &str) -> Option<super::native::Native> {
    match name {
        "structuredClone" => Some(structured_clone),
        _ => None,
    }
}

/// `structuredClone(value)`.
///
/// `options` is not declared. Its only member is `transfer`, a list of objects
/// to move rather than copy, and there is nothing here to move — a transferable
/// is an `ArrayBuffer` or a port, none of which this runtime has. Declaring and
/// ignoring it would accept a call whose whole purpose was the thing ignored.
extern "C" fn structured_clone(_e: u64, _t: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let mut graph = Graph::default();
    let root = walk(&mut graph, value, 0);
    let made = materialise(&graph);
    resolve(root, &made)
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

/// The value each arena node becomes.
///
/// Two passes, and the split is what makes a cycle expressible: every container
/// exists and is empty before any of them is filled, so a member pointing back
/// at its own container has something to point at. Filling as they were made
/// would need the parent's value while the parent was still being built.
fn materialise(graph: &Graph) -> Vec<u64> {
    let made: Vec<u64> = graph.nodes.iter().map(empty).collect();
    for (node, value) in graph.nodes.iter().zip(made.iter()) {
        fill(node, *value, &made);
    }
    made
}

/// The container a node becomes, with nothing in it yet.
fn empty(node: &Node) -> u64 {
    match node {
        // The entry points, called with no borrow held — which they must be,
        // since each takes one.
        Node::Array(_) => super::array::array_new(0),
        // `native::plain` rather than the `object_new` entry point, which is
        // the spelling `json`'s materialisation settled on: both make an object
        // with no prototype, and this one is a plain function, so the arm below
        // it can stay inside the same borrow discipline.
        Node::Object(_) => with_current(|context| match super::native::plain(context) {
            Some(cell) => Value::from_slot(cell).bits(),
            None => undefined_of(context),
        }),
        Node::Map(_) => with_current(|context| super::collections::fresh(context, "Map")),
        Node::Set(_) => with_current(|context| super::collections::fresh(context, "Set")),
        // A `Date` is complete at this point: its whole state is the number,
        // and it has no members to fill in a second pass.
        Node::Date(ms) => with_current(|context| dated(context, *ms)),
    }
}

/// A `Date` holding this time value.
///
/// Built here rather than by calling the constructor, because that is an entry
/// point and this runs inside a borrow. What it costs is one restatement — the
/// prototype and the property — and what it buys is not having to invert the
/// materialisation to make one call.
fn dated(context: &mut Context, ms: f64) -> u64 {
    let Some(cell) = super::native::plain(context) else {
        return undefined_of(context);
    };
    // Absent only if nothing has read `Date` yet, which cannot happen when the
    // source of this clone was one.
    if let Some(prototype) = super::class_support::prototype(context, "Date") {
        context.set_prototype(cell, prototype);
    }
    let key = context.well_known(super::date::TIME);
    super::objects::put(context, cell, key, Value::from_f64(ms).bits());
    Value::from_slot(cell).bits()
}

/// Writes a node's children into the container [`empty`] made for it.
fn fill(node: &Node, value: u64, made: &[u64]) {
    let Some(cell) = Value(value).as_slot() else {
        return;
    };
    match node {
        Node::Array(slots) => {
            let elements: Vec<u64> = slots.iter().map(|slot| resolve(*slot, made)).collect();
            with_current(|context| {
                if let Some(held) = context.elements_at_mut(cell) {
                    *held = elements;
                }
            });
        }
        Node::Object(members) => with_current(|context| {
            for (name, slot) in members {
                let held = resolve(*slot, made);
                // Interned as a NAME, never through `Key::from_str`, for the
                // reason `json`'s materialisation records: an index-shaped name
                // routed the other way is filed among the elements of an object
                // that has none, and the read afterwards does not find it.
                let key = Key::Name(context.interner.intern(name, &mut context.keys));
                super::objects::put(context, cell, key, held);
            }
        }),
        Node::Map(entries) => with_current(|context| {
            let Some(mut table) = super::collections::taken(context, cell) else {
                return;
            };
            for (key, held) in entries {
                table.set(context, resolve(*key, made), resolve(*held, made));
            }
            super::collections::restore_sized(context, cell, table);
        }),
        Node::Set(members) => with_current(|context| {
            let Some(mut table) = super::collections::taken(context, cell) else {
                return;
            };
            for member in members {
                // Both halves, because that is how a `Set` stores a member —
                // `collections::table` records why one type serves both.
                let member = resolve(*member, made);
                table.set(context, member, member);
            }
            super::collections::restore_sized(context, cell, table);
        }),
        Node::Date(_) => {}
    }
}

fn resolve(slot: Slot, made: &[u64]) -> u64 {
    match slot {
        Slot::Bits(bits) => bits,
        // Indexed rather than probed: an index is only ever handed out by
        // `Graph::reserve` and `made` has one entry per node, so a miss is a
        // broken invariant and a panic is the honest report of one. An
        // `unwrap_or` here would answer a value that is not `undefined` and not
        // a clone of anything.
        Slot::At(at) => made[at],
    }
}

/// `undefined`, from outside a borrow.
fn absent() -> u64 {
    with_current(|context| undefined_of(context))
}
