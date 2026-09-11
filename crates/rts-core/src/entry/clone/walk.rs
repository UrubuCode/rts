//! The descent: what a value IS, decided inside one borrow, and the graph
//! walk that follows it with no borrow held.
//!
//! Split out of `clone.rs` when that file passed the 500-line ceiling with the
//! typed-array node and the array extras. The seam is the one the parent
//! module's header already draws — classification and descent are separate
//! phases — so nothing here reaches back except through the node types it
//! fills, which stay beside their builder.

use super::super::buffers::element::Kind;
use super::super::{with_current, Context};
use super::{absent, Graph, Node, Slot, DEPTH};
use crate::text::Str;
use crate::value::Value;

/// What a value is, decided inside one borrow and carried out of it.
///
/// The classification and the descent are separate for the reason the module
/// gives: everything below `Shape` runs with no borrow held.
#[derive(Clone, Copy)]
pub(super) enum Shape {
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
    /// A typed array, recognised by naming a [`super::super::buffers::View`] whose
    /// kind is not [`Kind::Raw`] — a `DataView` keeps the plain-object walk,
    /// for the reason [`Node::View`] states.
    View(u32, Kind),
    /// An error, recognised by its prototype chain reaching `Error.prototype`.
    Error(u32),
    /// A function or a symbol — see the module documentation.
    Uncloneable,
}

pub(super) fn shape_of(context: &mut Context, value: u64) -> Shape {
    let Some(cell) = Value(value).as_slot() else {
        // A symbol is a primitive and is still not cloneable: the specification
        // refuses one because its identity is the whole of what it is, and a
        // copy would be a different symbol wearing the same description.
        //
        // A bigint IS copied, by bits, sharing its digits — unobservable for the
        // same reason sharing a string is: neither can be written to.
        if super::super::symbol::is_symbol(context, value) {
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
    // A typed array names a view rather than owning bytes directly — checked
    // here, beside the buffer it is the other half of. `Kind::Raw` is a
    // `DataView`, refused for the reason [`Node::View`] documents.
    if let Some(view) = context.view_at(cell)
        && view.kind != Kind::Raw
    {
        return Shape::View(cell, view.kind);
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
        if prototype.is_some() && prototype == super::super::class_support::prototype(context, "Set") {
            return Shape::Set(cell);
        }
        return Shape::Map(cell);
    }
    // A `Date` is recognised by the property its time value lives in, which is
    // what a `Date` IS here — `date`'s module documentation records why that is
    // an ordinary property rather than an internal slot.
    let key = context.well_known(super::super::date::TIME);
    if let Some(time) = super::super::objects::read_property(context, cell, key)
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
    if super::super::object_proto::extends_class(context, cell, "Error") {
        return Shape::Error(cell);
    }
    Shape::Object(cell)
}


/// Reads one value into the arena.
///
/// Recursive over Rust's stack and over nothing else: every heap read below
/// takes its own borrow and gives it back before the recursive call.
pub(super) fn walk(graph: &mut Graph, value: u64, depth: usize) -> Slot {
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
            graph.nodes[at] = super::errors::walked(cell);
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
        Shape::View(cell, kind) => {
            // Same reasoning as `Buffer`: no child VALUES — the bytes are
            // copied whole, per [`Node::View`]'s stated gap — and still
            // registered, so one view referenced twice comes back as one
            // object twice rather than two independent copies.
            if let Some(at) = graph.found(cell) {
                return Slot::At(at);
            }
            let at = graph.reserve(cell);
            let bytes = with_current(|context| {
                super::super::buffers::view_of(context, value)
                    .and_then(|view| super::super::buffers::window(context, &view).map(<[u8]>::to_vec))
                    .unwrap_or_default()
            });
            graph.nodes[at] = Node::View { kind, bytes };
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
            let count = elements.len();
            let elements: Vec<Slot> = elements
                .into_iter()
                .map(|element| walk(graph, element, depth + 1))
                .collect();
            let extra = array_extras(graph, value, depth, count);
            Node::Array { elements, extra }
        }
        Shape::Map(_) => Node::Map(
            super::super::collections::entries_of(value)
                .into_iter()
                .map(|(key, held)| (walk(graph, key, depth + 1), walk(graph, held, depth + 1)))
                .collect(),
        ),
        Shape::Set(_) => Node::Set(
            super::super::collections::entries_of(value)
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
    let names = super::super::array::own_keys(value);
    let names = with_current(|context| {
        Value(names)
            .as_slot()
            .and_then(|cell| context.elements_at(cell).cloned())
            .unwrap_or_default()
    });
    let mut built = Vec::with_capacity(names.len());
    for name in names {
        let held = super::super::computed::get_indexed(value, name);
        let key = with_current(|context| super::super::text::to_text(context, Value(name)));
        let Some(key) = key else {
            continue;
        };
        built.push((key, walk(graph, held, depth + 1)));
    }
    built
}

/// An array's own members that are NOT one of its `0..count` indices — a
/// `length`, and every named property a program hung on it after the literal.
///
/// Reads `own_keys` the same way [`members`] does and for the same reason —
/// enumeration order is the runtime's one answer, not something re-derived
/// here — and skips every key `[`Node::Array`]`'s own field already carries:
/// `length`, whose value the array's own `length` write already reproduces,
/// and every index below `count`, whose value (including a hole) came from
/// [`Context::elements_at`] rather than from a property read that would
/// materialise it.
fn array_extras(graph: &mut Graph, value: u64, depth: usize, count: usize) -> Vec<(Str, Slot)> {
    let names = super::super::array::own_keys(value);
    let names = with_current(|context| {
        Value(names)
            .as_slot()
            .and_then(|cell| context.elements_at(cell).cloned())
            .unwrap_or_default()
    });
    let mut built = Vec::new();
    for name in names {
        let key = with_current(|context| super::super::text::to_text(context, Value(name)));
        let Some(key) = key else {
            continue;
        };
        if let Some(rust) = key.to_rust() {
            if rust == "length" {
                continue;
            }
            if let Ok(index) = rust.parse::<usize>()
                && index < count
            {
                continue;
            }
        }
        let held = super::super::computed::get_indexed(value, name);
        built.push((key, walk(graph, held, depth + 1)));
    }
    built
}
