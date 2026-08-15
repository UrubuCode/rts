//! Turning the arena [`super::walk`] filled into values on the heap.
//!
//! # Why this is a module and not the second half of one file
//!
//! `clone.rs` had reached the crate's 500-line ceiling (rule 6 of
//! `crates/rts-core/README.md`) before the error case was written, and the rule
//! says new code lands in a focused module rather than on the end of something
//! already over. The seam it splits on is the one the parent's own
//! documentation already names: **reading** a graph and **building** one are two
//! passes that share nothing but the arena, and neither calls back into the
//! other.
//!
//! Everything here runs with no borrow held on entry and takes its own, for the
//! reason the parent module's first page gives: an `extern "C"` frame cannot
//! unwind, so a nested borrow aborts the process rather than failing.

use super::super::objects::undefined_of;
use super::super::{Context, with_current};
use super::{Graph, Node, Slot};
use crate::object::Key;
use crate::value::Value;

/// The value each arena node becomes.
///
/// Two passes, and the split is what makes a cycle expressible: every container
/// exists and is empty before any of them is filled, so a member pointing back
/// at its own container has something to point at. Filling as they were made
/// would need the parent's value while the parent was still being built.
pub(super) fn materialise(graph: &Graph) -> Vec<u64> {
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
        Node::Array(_) => super::super::array::array_new(0),
        // `native::plain` rather than the `object_new` entry point, which is
        // the spelling `json`'s materialisation settled on: both make an object
        // with no prototype, and this one is a plain function, so the arm below
        // it can stay inside the same borrow discipline.
        Node::Object(_) => with_current(|context| match super::super::native::plain(context) {
            Some(cell) => Value::from_slot(cell).bits(),
            None => undefined_of(context),
        }),
        Node::Map(_) => with_current(|context| super::super::collections::fresh(context, "Map")),
        Node::Set(_) => with_current(|context| super::super::collections::fresh(context, "Set")),
        // A `Date` is complete at this point: its whole state is the number,
        // and it has no members to fill in a second pass.
        Node::Date(ms) => with_current(|context| dated(context, *ms)),
        // Complete here too, for the same reason and with one more field: an
        // error's whole serialised state is its class, its message and its
        // stack — see [`super::Node::Error`] for why nothing else of it
        // survives.
        Node::Error {
            class,
            message,
            stack,
        } => with_current(|context| {
            super::errors::made(context, class, message.as_ref(), stack.as_ref())
        }),
        // Also complete here: the bytes are already copied, so there is
        // nothing left for `fill` to do — `super::buffers::new_buffer` makes
        // the store and the prototype together.
        Node::Buffer(bytes) => with_current(|context| {
            match super::super::buffers::new_buffer(context, bytes.len()) {
                Some(cell) => {
                    if let Some(destination) = context.bytes_at_mut(cell) {
                        destination.copy_from_slice(bytes);
                    }
                    Value::from_slot(cell).bits()
                }
                None => undefined_of(context),
            }
        }),
    }
}

/// A `Date` holding this time value.
///
/// Built here rather than by calling the constructor, because that is an entry
/// point and this runs inside a borrow. What it costs is one restatement — the
/// prototype and the property — and what it buys is not having to invert the
/// materialisation to make one call.
fn dated(context: &mut Context, ms: f64) -> u64 {
    let Some(cell) = super::super::native::plain(context) else {
        return undefined_of(context);
    };
    // Absent only if nothing has read `Date` yet, which cannot happen when the
    // source of this clone was one.
    if let Some(prototype) = super::super::class_support::prototype(context, "Date") {
        context.set_prototype(cell, prototype);
    }
    let key = context.well_known(super::super::date::TIME);
    super::super::objects::put(context, cell, key, Value::from_f64(ms).bits());
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
                // `length` is an ordinary property (`array::set_length`'s own
                // doc comment says so), not something a reader derives from the
                // element vector — so writing the elements alone leaves it at
                // whatever `array_new(0)` wrote, which is 0, while the elements
                // sit there populated. Every other reader of a cloned array
                // (`.length`, `for`-`of`, `JSON.stringify`) goes through the
                // property, not the vector, so this was answering 0 for a
                // visibly non-empty array.
                let count = elements.len();
                if let Some(held) = context.elements_at_mut(cell) {
                    *held = elements;
                }
                super::super::array::set_length(context, cell, count);
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
                super::super::objects::put(context, cell, key, held);
            }
        }),
        Node::Map(entries) => with_current(|context| {
            let Some(mut table) = super::super::collections::taken(context, cell) else {
                return;
            };
            for (key, held) in entries {
                table.set(context, resolve(*key, made), resolve(*held, made));
            }
            super::super::collections::restore_sized(context, cell, table);
        }),
        Node::Set(members) => with_current(|context| {
            let Some(mut table) = super::super::collections::taken(context, cell) else {
                return;
            };
            for member in members {
                // Both halves, because that is how a `Set` stores a member —
                // `collections::table` records why one type serves both.
                let member = resolve(*member, made);
                table.set(context, member, member);
            }
            super::super::collections::restore_sized(context, cell, table);
        }),
        Node::Date(_) | Node::Buffer(_) | Node::Error { .. } => {}
    }
}

pub(super) fn resolve(slot: Slot, made: &[u64]) -> u64 {
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
