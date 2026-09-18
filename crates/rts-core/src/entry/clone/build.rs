//! Turning an arena into values on the heap — for the clone, and for the
//! pickle's decoder, which fills the same arena from bytes.
//!
//! # One borrow, and what makes that safe
//!
//! It took a borrow per node twice over — one to make it, one to fill it — and
//! the array constructor was an entry point that took its own. Nothing here
//! calls user code, so there is no reason to give the borrow back, and
//! [`materialise`] runs inside the caller's.
//!
//! What that does NOT remove is the collector. Every node allocates, an
//! allocation may collect, and a value this has made is named by nothing but a
//! Rust vector until the container holding it is filled — the exact shape
//! `docs/engine/lost-roots.md` records twice. So the made values live in a
//! [`Rooted`], which is what the earlier version did not do: it held them in a
//! plain `Vec`, and a clone large enough to fill the region lost objects it had
//! already made.
//!
//! # Two passes, and why
//!
//! Every container exists and is empty before any of them is filled, so a
//! member pointing back at its own container has something to point at.
//! Filling as they were made would need the parent's value while the parent
//! was still being built.

use super::super::objects::undefined_of;
use super::super::rooted::Rooted;
use super::super::Context;
use super::{Graph, Node, Slot};
use crate::object::Key;
use crate::value::Value;

/// The values an arena's nodes and texts became, still visible to the
/// collector — nodes first, then texts.
pub(in crate::entry) struct Made {
    values: Rooted,
    texts: usize,
}

/// Makes every node and text of an arena, and fills the containers.
pub(in crate::entry) fn materialise(context: &mut Context, graph: &Graph) -> Made {
    materialise_holding(context, graph, &[])
}

/// The same, leaving the nodes `held` names (sorted) made but EMPTY — the
/// pickle's class instances whose fields a class's `upgrade` rewrites first.
/// They exist, so everything pointing at one points at it; they are filled
/// once the fields they will hold are known.
pub(in crate::entry) fn materialise_holding(context: &mut Context, graph: &Graph, held: &[usize]) -> Made {
    let mut made = Made {
        values: Rooted::with(Vec::with_capacity(graph.nodes.len() + graph.texts.len())),
        texts: graph.nodes.len(),
    };
    for node in &graph.nodes {
        let value = empty(context, node);
        made.values.values().push(value);
    }
    for text in &graph.texts {
        let value = context.intern_value(text.clone()).bits();
        made.values.values().push(value);
    }
    for (at, node) in graph.nodes.iter().enumerate() {
        if held.binary_search(&at).is_ok() {
            continue;
        }
        let value = made.values.as_slice()[at];
        fill(context, node, value, &made);
    }
    made
}

/// The container a node becomes, with nothing in it yet.
fn empty(context: &mut Context, node: &Node) -> u64 {
    match node {
        // An array holds its elements beside the cell, so the empty one is the
        // real one: `fill` replaces the vector whole, at the size it will have.
        Node::Array { .. } => super::super::array::built_in(context, Vec::new()),
        Node::Object(_) | Node::Boxed(_) => plain(context),
        Node::Instance { class, .. } => {
            let made = plain(context);
            if let Some(cell) = Value(made).as_slot() {
                context.set_prototype(cell, class.prototype);
            }
            made
        }
        Node::Map(_) => {
            class_prototype(context, "Map");
            super::super::collections::fresh(context, "Map")
        }
        Node::Set(_) => {
            class_prototype(context, "Set");
            super::super::collections::fresh(context, "Set")
        }
        // A `Date` is complete at this point: its whole state is the number,
        // and it has no members to fill in a second pass.
        Node::Date(ms) => dated(context, *ms),
        // Complete here as well, and rebuilt rather than copied: `make` is the
        // one definition of "a pattern from two texts" that the literal and the
        // constructor already share.
        Node::Regexp { source, flags, last_index } => {
            let made = super::super::regex::make(context, source, flags);
            if *last_index != 0.0
                && let Some(cell) = Value(made).as_slot()
            {
                let key = context.well_known("lastIndex");
                super::super::objects::put(context, cell, key, Value::from_f64(*last_index).bits());
            }
            made
        }
        Node::Error { class, .. } => super::errors::empty(context, class),
        Node::Buffer(bytes) => match super::super::buffers::new_buffer(context, bytes.len()) {
            Some(cell) => {
                if let Some(destination) = context.bytes_at_mut(cell) {
                    destination.copy_from_slice(bytes);
                }
                Value::from_slot(cell).bits()
            }
            None => undefined_of(context),
        },
        // A PRIVATE backing buffer, sized and filled from the copied bytes, then
        // `typed::made` gives it the right class's prototype and attaches the
        // view — the same two steps `new Uint8Array` itself ends with.
        Node::View { kind, bytes } => {
            let Some(buffer) = super::super::buffers::new_buffer(context, bytes.len()) else {
                return undefined_of(context);
            };
            if let Some(destination) = context.bytes_at_mut(buffer) {
                destination.copy_from_slice(bytes);
            }
            let view = super::super::buffers::View {
                buffer,
                offset: 0,
                length: bytes.len(),
                kind: *kind,
            };
            super::super::buffers::typed::made(context, view)
        }
        Node::NodeBuffer(bytes) => {
            class_prototype(context, "Buffer");
            super::super::modules::make_buffer(context, bytes)
        }
        Node::BigInt(digits) => context.bigint_value(digits.clone()),
        // Only a WRITER's arena holds one — the decoder resolves a function
        // while it reads — so nothing is built for it.
        Node::Function(_) => undefined_of(context),
    }
}

/// What instances of a built-in class inherit from, registering the class
/// first when nothing has named it yet.
///
/// The globals are made lazily, the first time a program reads one, and a
/// program decoding a `Date` out of a file may never have written the word —
/// the clone could assume otherwise, because its source WAS one. Through
/// `global::ensure`, so the class is recorded where a later read of the name
/// finds it rather than registered a second time.
pub(in crate::entry) fn class_prototype(context: &mut Context, name: &str) -> Option<u64> {
    if let Some(found) = super::super::class_support::prototype(context, name) {
        return Some(found);
    }
    super::super::global::ensure(context, name);
    super::super::class_support::prototype(context, name)
}

fn plain(context: &mut Context) -> u64 {
    match super::super::native::plain(context) {
        Some(cell) => Value::from_slot(cell).bits(),
        None => undefined_of(context),
    }
}

/// A `Date` holding this time value.
///
/// Built here rather than by calling the constructor, because that is an entry
/// point and this runs inside a borrow.
fn dated(context: &mut Context, ms: f64) -> u64 {
    let Some(cell) = super::super::native::plain(context) else {
        return undefined_of(context);
    };
    if let Some(prototype) = class_prototype(context, "Date") {
        context.set_prototype(cell, prototype);
    }
    let key = context.well_known(super::super::date::TIME);
    super::super::objects::put(context, cell, key, Value::from_f64(ms).bits());
    Value::from_slot(cell).bits()
}

/// Writes a node's children into the container [`empty`] made for it.
fn fill(context: &mut Context, node: &Node, value: u64, made: &Made) {
    let Some(cell) = Value(value).as_slot() else {
        return;
    };
    match node {
        Node::Array { elements, extra } => {
            let elements: Vec<u64> = elements.iter().map(|slot| resolve(*slot, made)).collect();
            // `length` is an ordinary property, not something a reader derives
            // from the element vector — so writing the elements alone would
            // leave it at the `0` the empty array was made with.
            let count = elements.len();
            if let Some(held) = context.elements_at_mut(cell) {
                *held = elements;
            }
            super::super::array::set_length(context, cell, count);
            for (key, slot) in extra {
                let key = named(context, *key);
                super::super::objects::put(context, cell, key, resolve(*slot, made));
            }
        }
        Node::Object(members) | Node::Instance { fields: members, .. } => {
            let members: Vec<(Key, u64)> = members
                .iter()
                .map(|(key, slot)| (named(context, *key), resolve(*slot, made)))
                .collect();
            populate(context, cell, &members);
        }
        Node::Map(entries) => {
            let Some(mut table) = super::super::collections::taken(context, cell) else {
                return;
            };
            for (key, held) in entries {
                table.set(context, resolve(*key, made), resolve(*held, made));
            }
            super::super::collections::restore_sized(context, cell, table);
        }
        Node::Set(members) => {
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
        }
        Node::Error { message, stack, cause, extra, .. } => {
            let at = |slot: &Option<Slot>| slot.map(|slot| resolve(slot, made));
            let extra: Vec<(Key, u64)> = extra.iter().map(|(key, slot)| (*key, resolve(*slot, made))).collect();
            super::errors::fill(
                context,
                cell,
                [("message", at(message)), ("stack", at(stack)), ("cause", at(cause))],
                &extra,
            );
        }
        Node::Boxed(inner) => {
            let primitive = resolve(*inner, made);
            let prototype = match context.is_text_at(Value(primitive).as_slot().unwrap_or(u32::MAX)) {
                true => super::super::string::prototype_of(context),
                false => super::super::primitive_proto::prototype_of(context, Value(primitive)),
            };
            if let Some(prototype) = prototype {
                context.set_prototype(cell, Value::from_slot(prototype).bits());
            }
            context.set_boxed(cell, primitive);
        }
        Node::Date(_)
        | Node::Regexp { .. }
        | Node::Buffer(_)
        | Node::View { .. }
        | Node::NodeBuffer(_)
        | Node::BigInt(_)
        | Node::Function(_) => {}
    }
}

/// A key as the property it is written under.
///
/// Always a NAME, for the reason `json`'s materialisation records: an
/// index-shaped name routed the other way is filed among the elements of an
/// object that has none, and the read afterwards does not find it.
pub(in crate::entry) fn named(context: &mut Context, key: Key) -> Key {
    match key {
        Key::Name(_) => key,
        Key::Index(index) => Key::Name(context.interner.intern_str(&index.to_string(), &mut context.keys)),
    }
}

/// Writes members into an object that has none yet, reaching the layout once.
///
/// A `put` per member is a shape transition, a slot lookup, a type mint and a
/// header write — and every type but the last is thrown away by the next
/// member. An object whose keys are all known before any is stored does not
/// need to discover its layout one property at a time: this walks the
/// transitions, types the cell once, and writes the slots. `json`'s reader
/// wrote this first; it is here so the clone and the pickle's decoder reach it
/// too, and there is one of it.
///
/// Falls back to `put` for a refused transition or a slot past the cell's
/// inline width — rare, and the general path is right for both.
pub(in crate::entry) fn populate(context: &mut Context, cell: u32, members: &[(Key, u64)]) {
    let mut shape = context.shapes.root();
    let mut placed: Vec<(u32, u64)> = Vec::with_capacity(members.len());
    let width = context.region.width_of(cell).unwrap_or(crate::heap::INLINE_SLOTS);
    let mut fallback = false;
    for (key, value) in members {
        let Key::Name(named) = *key else {
            fallback = true;
            break;
        };
        let Ok(grown) = context.shapes.transition(shape, named, rts_cranelift::repr::Repr::Tagged) else {
            fallback = true;
            break;
        };
        let Some(at) = context.shapes.slot_of(grown, named) else {
            fallback = true;
            break;
        };
        // Past the inline slots. `set_slot_value` does reach the spill —
        // `objects.rs` subtracts `owned_slots` and calls `spill_set` — so this
        // bound is not what keeps the fast arm off it. What it decides is which
        // arm RESOLVES the key, and the general path is right past it.
        //
        // A duplicate key is NOT a fallback: the transition answers the shape
        // it was asked from, the slot is the one already placed, and the later
        // write wins — which is `JSON.parse`'s rule for a repeated name.
        if at >= width {
            fallback = true;
            break;
        }
        placed.push((at, *value));
        shape = grown;
    }
    if fallback {
        for (key, value) in members {
            super::super::objects::put(context, cell, *key, *value);
        }
        return;
    }
    let link = context.prototype_at(cell);
    let ty = context.typed_as(shape, link).index() as u32;
    context.region.set_type(cell, ty);
    for (at, value) in placed {
        super::super::objects::set_slot_value(context, cell, at, value);
    }
}

/// What a slot of the arena became.
pub(in crate::entry) fn resolve(slot: Slot, made: &Made) -> u64 {
    match slot {
        Slot::Bits(bits) => bits,
        // Indexed rather than probed: an index is only ever handed out by the
        // arena and `made` has one entry per node and text, so a miss is a
        // broken invariant and a panic is the honest report of one.
        Slot::At(at) => made.values.as_slice()[at],
        Slot::Text(at) => made.values.as_slice()[made.texts + at],
    }
}
