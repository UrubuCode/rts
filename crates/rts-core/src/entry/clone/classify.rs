//! What a value IS, decided inside one borrow, for both readers of a graph.
//!
//! Split out of the walk because the classification is the part the two
//! policies share: every question below is asked the same way for a clone and
//! for a pickle, and only the ANSWER to a few of them differs — a function, a
//! symbol, an object with a prototype that is not `Object.prototype`. Those are
//! the `match policy` arms, and they are the whole of the difference.

use super::super::buffers::element::Kind;
use super::super::Context;
use super::{ClassName, Policy, Refusal};
use crate::value::Value;

/// What a value is, decided inside one borrow.
pub(super) enum Shape {
    Bits(u64),
    /// A function or a symbol, for the clone — see the module documentation.
    Uncloneable,
    Array(u32),
    /// A plain object. The first flag is `true` when reading its members runs
    /// user code — an accessor, or a proxy's trap — which is what sends it
    /// outside the borrow; the second when it has NO prototype
    /// (`Object.create(null)`), which only the pickle keeps.
    Object(u32, bool, bool),
    Instance(u32, ClassName),
    Map(u32),
    Set(u32),
    Date(u32, f64),
    Regexp(u32),
    Buffer(u32),
    View(u32, Kind),
    NodeBuffer(u32),
    Error(u32),
    Boxed(u32, u64),
    Function(ClassName),
}

/// What the classification compares values against, found once per walk
/// rather than once per value.
///
/// Every one of these was a lookup on each value: the registered classes are a
/// `Vec` searched by NAME (`class_support`'s own documentation says why that is
/// right for its callers, which ask once), and asking it for `Error`, `Map`,
/// `Set`, `Buffer` and `Object.prototype` per object made the classification of
/// a ten-thousand-object graph fifty thousand string scans. Only a FOUND answer
/// is kept: a class nothing has registered yet may be registered by a getter
/// the walk runs, and a remembered absence would then misclassify its
/// instances.
#[derive(Default)]
pub(super) struct Known {
    time: Option<crate::object::Key>,
    object: Option<u32>,
    error: Option<u64>,
    map: Option<u64>,
    set: Option<u64>,
    buffer: Option<u64>,
    /// What each constructor was declared as — a registry read, a string
    /// split and two allocations, once per class instead of once per
    /// instance.
    declared: std::collections::HashMap<u32, Option<ClassName>>,
    /// The private-name numbers of each prototype's chain, for the same
    /// reason.
    spaces: std::collections::HashMap<u32, std::rc::Rc<Vec<Option<u32>>>>,
}

impl Known {
    fn class(held: &mut Option<u64>, context: &mut Context, name: &str) -> Option<u64> {
        if held.is_none() {
            *held = super::super::class_support::prototype(context, name);
        }
        *held
    }

    fn declared(&mut self, context: &mut Context, cell: u32) -> Option<ClassName> {
        if let Some(found) = self.declared.get(&cell) {
            return found.clone();
        }
        let found = super::super::pickle::names::declared_as(context, cell);
        self.declared.insert(cell, found.clone());
        found
    }

    /// The private-name numbers of an instance's class chain.
    pub(super) fn spaces(&mut self, context: &mut Context, instance: u32) -> std::rc::Rc<Vec<Option<u32>>> {
        let Some(prototype) = context.prototype_at(instance).and_then(|found| Value(found).as_slot()) else {
            return std::rc::Rc::default();
        };
        if let Some(found) = self.spaces.get(&prototype) {
            return found.clone();
        }
        let found = std::rc::Rc::new(super::super::pickle::names::spaces(context, prototype));
        self.spaces.insert(prototype, found.clone());
        found
    }

    fn object(&mut self, context: &mut Context) -> Option<u32> {
        if self.object.is_none() {
            self.object = super::super::object_proto::prototype_of(context);
        }
        self.object
    }
}

/// Whether `target` is on `cell`'s prototype chain, `cell` itself included —
/// `object_proto::extends_class` with the target already in hand.
fn inherits(context: &mut Context, mut cell: u32, target: u32) -> bool {
    for _ in 0..super::super::objects::CHAIN_LIMIT {
        if cell == target {
            return true;
        }
        let Some(next) = super::super::objects::inherited_from(context, cell) else {
            return false;
        };
        cell = next;
    }
    false
}

/// Classifies one value.
///
/// A refusal carries the text the program is told, which names the kind — the
/// v1 format's rule, kept: "cannot serialize a Proxy" is actionable where
/// "cannot serialize" is not.
pub(super) fn shape_of(context: &mut Context, value: u64, policy: Policy, known: &mut Known) -> Result<Shape, Refusal> {
    let pickle = policy == Policy::Pickle;
    let Some(cell) = Value(value).as_slot() else {
        // A symbol is a primitive and is still not cloneable: the specification
        // refuses one because its identity is the whole of what it is, and a
        // copy would be a different symbol wearing the same description.
        //
        // The pickle WRITES one, because a save file crosses programs where a
        // clone never does: a shared symbol (`Symbol.for`, the well-known ones)
        // is the same symbol by its key text, and an unregistered one revives
        // as a new symbol of the same description with its identity kept
        // inside the stream — `pickle/symbols.rs`.
        //
        // A bigint IS copied, by bits, sharing its digits — unobservable for the
        // same reason sharing a string is: neither can be written to.
        if super::super::symbol::is_symbol(context, value) && !pickle {
            return Ok(Shape::Uncloneable);
        }
        return Ok(Shape::Bits(value));
    };
    if context.is_text_at(cell) {
        return Ok(Shape::Bits(value));
    }
    // Asked before anything structural, because a function is an object too.
    // Getting the order wrong clones its members into a plain object that is not
    // callable — a copy that looks like it worked.
    if context.callable_at(cell).is_some() {
        return match pickle {
            true => function(context, cell, known).map(Shape::Function),
            false => Ok(Shape::Uncloneable),
        };
    }
    if pickle && context.proxy_at(cell).is_some() {
        return Err(refuse("a Proxy"));
    }
    // An `ArrayBuffer` owns a byte store directly rather than through any
    // property a walk would find — checked before `Date`'s property probe and
    // the element/plain-object fallback, none of which know what a buffer is.
    if context.bytes_at(cell).is_some() {
        return Ok(Shape::Buffer(cell));
    }
    // A typed array names a view rather than owning bytes directly — checked
    // here, beside the buffer it is the other half of. `Kind::Raw` is a
    // `DataView`, which the clone copies as a plain object and the pickle
    // refuses: a view with no element kind has no bytes-and-kind spelling.
    if let Some(view) = context.view_at(cell) {
        if view.kind != Kind::Raw {
            let buffer = Known::class(&mut known.buffer, context, "Buffer");
            if pickle && buffer.is_some() && context.prototype_at(cell) == buffer {
                return Ok(Shape::NodeBuffer(cell));
            }
            return Ok(Shape::View(cell, view.kind));
        }
        if pickle {
            return Err(refuse("a DataView"));
        }
    }
    // Before the plain-object fallback: a regular expression answers
    // `source`/`flags` through PROTOTYPE accessors, so the member walk would
    // find no own members and copy it as an empty object.
    if context.regexp_at(cell).is_some() {
        return Ok(Shape::Regexp(cell));
    }
    if let Some(brand) = context.table_at(cell).map(|table| table.brand()) {
        return collection(context, cell, brand, pickle, known);
    }
    // A `Date` is recognised by the property its time value lives in, which is
    // what a `Date` IS here — `date`'s module documentation records why that is
    // an ordinary property rather than an internal slot.
    //
    // OWN, not inherited: a `Date`'s time value is written onto the object by
    // its constructor, so an object merely inheriting from one is not one — and
    // an own read is a slot lookup where the chain walk it replaced was one per
    // prototype, on every object of the graph.
    let key = *known.time.get_or_insert_with(|| context.well_known(super::super::date::TIME));
    if let Some(time) = super::super::objects::own_property(context, cell, key)
        && let Some(ms) = time.as_f64()
    {
        return Ok(Shape::Date(cell, ms));
    }
    if context.elements_at(cell).is_some() {
        return Ok(Shape::Array(cell));
    }
    // LAST of the structural questions but one, and deliberately: it walks a
    // prototype chain, so putting it earlier would charge every array, buffer
    // and collection for a question none of them can answer yes to.
    let error = Known::class(&mut known.error, context, "Error").and_then(|found| Value(found).as_slot());
    if error.is_some_and(|error| inherits(context, cell, error)) {
        return Ok(Shape::Error(cell));
    }
    if pickle {
        if let Some(inner) = context.boxed_at(cell) {
            return Ok(Shape::Boxed(cell, inner));
        }
        return object_of_pickle(context, cell, known);
    }
    let calls = context.proxy_at(cell).is_some() || !context.ranked_accessors(cell).is_empty();
    Ok(Shape::Object(cell, calls, false))
}

/// A table, which is a `Map`, a `Set` or one of the collections that are not.
///
/// Which of the two it is comes from the brand the constructor gave it. The
/// clone used to decide from the PROTOTYPE, and an unrecognised one — a
/// subclass — fell to `Map`; that is kept for the clone, because it is what the
/// clone did. The pickle asks the brand AND the prototype: a `WeakMap` has no
/// entries to write, and a subclass of `Map` is a class the stream cannot name
/// alongside its entries — both are refused by name rather than flattened.
fn collection(
    context: &mut Context,
    cell: u32,
    brand: super::super::collections::Brand,
    pickle: bool,
    known: &mut Known,
) -> Result<Shape, Refusal> {
    use super::super::collections::Brand;
    let prototype = context.prototype_at(cell);
    let set = Known::class(&mut known.set, context, "Set");
    if !pickle {
        return Ok(match prototype.is_some() && prototype == set {
            true => Shape::Set(cell),
            false => Shape::Map(cell),
        });
    }
    let map = Known::class(&mut known.map, context, "Map");
    match brand {
        Brand::Map if prototype == map => Ok(Shape::Map(cell)),
        Brand::Set if prototype == set => Ok(Shape::Set(cell)),
        Brand::Map => Err(refuse("a subclass of Map")),
        Brand::Set => Err(refuse("a subclass of Set")),
        Brand::WeakMap => Err(refuse("a WeakMap")),
        Brand::WeakSet => Err(refuse("a WeakSet")),
        Brand::Other => Err(refuse("a WeakRef or FinalizationRegistry")),
    }
}

/// A plain object or a class instance, for the pickle.
///
/// The prototype decides. `Object.prototype` or none at all is a plain object;
/// a prototype whose `constructor` the program declared as a class is an
/// instance of it. ANYTHING else is refused, by the constructor's name — which
/// is how a `Promise`, a generator, a `URL` or a host object is refused without
/// a list of them: each has a prototype that is neither of the two.
fn object_of_pickle(context: &mut Context, cell: u32, known: &mut Known) -> Result<Shape, Refusal> {
    let plain = || Ok(Shape::Object(cell, false, false));
    let Some(prototype) = context.prototype_at(cell) else {
        return with_calls(context, cell, plain());
    };
    let Some(link) = Value(prototype).as_slot() else {
        // `Object.create(null)` — no prototype at all, which the stream keeps
        // (BARE) so that the dictionary comes back as the dictionary it was.
        return with_calls(context, cell, Ok(Shape::Object(cell, false, true)));
    };
    if known.object(context) == Some(link) {
        return with_calls(context, cell, plain());
    }
    let key = context.well_known("constructor");
    let constructor = super::super::objects::own_property(context, link, key)
        .and_then(|found| found.as_slot());
    if let Some(constructor) = constructor
        && let Some(class) = known.declared(context, constructor)
    {
        return Ok(Shape::Instance(cell, class));
    }
    let named = constructor
        .and_then(|constructor| function_name(context, constructor))
        .unwrap_or_else(|| "an unnamed class".to_owned());
    let because = super::super::pickle::names::unregistered_because(context);
    Err(refuse(&format!(
        "an instance of {named}, which is not a class this program declared{because}"
    )))
}

/// A plain object, marked for the slow read when it has an accessor.
fn with_calls(context: &Context, cell: u32, shape: Result<Shape, Refusal>) -> Result<Shape, Refusal> {
    match shape {
        Ok(Shape::Object(cell_at, _, bare)) => {
            Ok(Shape::Object(cell_at, !context.ranked_accessors(cell).is_empty(), bare))
        }
        other => other,
    }
}

/// A function, for the pickle: by reference if the program declared it at
/// the top level of a module, refused otherwise.
fn function(context: &mut Context, cell: u32, known: &mut Known) -> Result<ClassName, Refusal> {
    if context.bound_at(cell).is_some() {
        return Err(refuse("a bound function"));
    }
    if let Some(declared) = known.declared(context, cell) {
        return Ok(declared);
    }
    let named = function_name(context, cell).filter(|name| !name.is_empty());
    let because = super::super::pickle::names::unregistered_because(context);
    Err(refuse(&match named {
        Some(name) => format!(
            "function '{name}': only a named function declared at the top level of a module \
             serializes, by reference{because}"
        ),
        None => "an anonymous function or arrow: only a named function declared at the top \
                 level of a module serializes, by reference"
            .to_owned(),
    }))
}

/// What a function says its name is, for a message.
fn function_name(context: &mut Context, cell: u32) -> Option<String> {
    let key = context.well_known("name");
    let found = super::super::objects::own_property(context, cell, key)?;
    context.text_at(found.as_slot()?)?.to_rust()
}

/// The refusal for a kind, spelled the way every one of them is.
pub(super) fn refuse(what: &str) -> Refusal {
    Refusal::Unserializable(format!("cannot serialize {what}"))
}
