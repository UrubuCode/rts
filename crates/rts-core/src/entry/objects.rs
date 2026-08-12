//! Objects: making one, reading a property, writing one.
//!
//! # Why a property key crosses as a number
//!
//! A property name in a compiled program is known while compiling, so handing
//! over its text at every access would hand over something the compiler already
//! resolved. The machine's key registry numbers names, and the number is what
//! crosses.
//!
//! That makes the two sides agree about *which* registry — a compiler numbering
//! `x` as 3 and a runtime reading 3 as `y` is a program that runs and is wrong.
//! Nothing here can check it, because a number carries no evidence of where it
//! came from. The host wires one registry into both and says so, which is the
//! same shape as the singleton numbering and for the same reason.
//!
//! # Why these are entry points
//!
//! Making an object allocates. Reading a property walks a prototype chain
//! through the heap. Writing one may move the object to a new layout. For once
//! the membership rule needs no argument.
//!
//! # What is not here
//!
//! The fast path. Every access below is a call that looks a key up in a layout,
//! and the machine has `cached_get` and `guard_type` for a site that keeps
//! seeing the same shape. That is the next step and deliberately a separate one:
//! this makes property access *correct*, and a cache built before there was
//! something correct to cache would be a cache over a guess.

use super::{Context, with_current};
use crate::object::Key;
use crate::value::Value;
use rts_cranelift::repr::Repr;
use rts_cranelift::shape::Key as ShapeKey;

/// `{}` — a new object with no properties.
///
/// No prototype. `new F()` gives one and an object literal does not, which is
/// the language — but only because `Object.prototype` does not exist here, so
/// what a literal SHOULD inherit is absent rather than empty. Visible, rather
/// than wrong-looking.
#[rtse::entry]
pub fn object_new() -> u64 {
    with_current(object_new_in)
}

/// The body [`object_new`] wraps in `with_current`, taken directly by a
/// caller that already holds the borrow — [`super::object_global::empty_object`]
/// is one, and calling [`object_new`] from inside its own `with_current`
/// closure re-enters the `RefCell` and aborts the process rather than
/// panicking catchably (the entry point crosses an `extern "C"` boundary,
/// where an unwind is not allowed).
pub(super) fn object_new_in(context: &mut Context) -> u64 {
    // The empty layout, which every object that gains its first property
    // transitions out of — which is what makes two objects built the same
    // way share a shape.
    let shape = context.shapes.root();
    let ty = context.layout_of(shape).index() as u32;
    let cell = super::alloc::alloc_or_die(context, crate::heap::STRIDE, ty);
    Value::from_slot(cell).bits()
}

/// `object.name`, the name given as its key number.
///
/// Answers `undefined` for a property that is not there, which is what the
/// language does rather than an error being swallowed: reading an absent
/// property is legal.
///
/// It also answers `undefined` for a receiver that is not an object, and that is
/// **not** what the language does — `null.x` throws a `TypeError`. A stated gap:
/// throwing needs the machine's protected regions, and nothing emits those yet.
/// # Why this is two statements rather than one expression
///
/// Because the answer may be a **function to call**. An accessor's getter is
/// user code whose first act may be to call the runtime, and calling it from
/// inside the borrow below re-enters the `RefCell`. So the lookup answers which
/// function, the borrow ends, and the call happens after — the same two-step
/// shape `own_keys` and `exec` have, for the same reason.
#[rtse::entry]
pub fn get_property(object: u64, key: i64) -> u64 {
    // A proxy answers by running user code, so it is asked BEFORE any borrow
    // that a lookup would take — the trap may call straight back in here.
    // `None` means this is an ordinary object, which is every object in a
    // program that never wrote `new Proxy`.
    // Asked in the same borrow that resolves the key and does the lookup, and
    // only of an object that IS one — `proxy_at` is a table read. It used to
    // resolve the key in a borrow of its own, hand it to `proxy::get`, and then
    // resolve it again below: two lookups and two borrows on every read that
    // misses its cache, for a question almost every program answers "no" to.
    let proxied = with_current(|context| {
        let key = key_of(context, key)?;
        let slot = Value(object).as_slot()?;
        context.proxy_at(slot).map(|_| key)
    });
    if let Some(key) = proxied
        && let Some(answered) = super::proxy::get(object, key)
    {
        return answered;
    }
    let found = with_current(|context| {
        let Some(key) = key_of(context, key) else {
            return super::accessor::Found::Value(undefined_of(context));
        };
        let Some(slot) = Value(object).as_slot() else {
            // A number, a boolean, a symbol or a bigint — none of which has a
            // cell to walk from. See [`super::primitive_proto`] for why the
            // lookup starts on the shared prototype rather than on the value,
            // and [`super::computed::primitive_found`] for why the cascade is
            // stated there rather than twice: the computed spelling reaches the
            // same receivers, and for a while it did not.
            return super::computed::primitive_found(context, Value(object), key);
        };
        if let Some(answer) = super::string::text::string_property(context, slot, key) {
            return super::accessor::Found::Value(answer);
        }
        super::accessor::resolve(context, slot, key)
    });
    match found {
        super::accessor::Found::Value(value) => value,
        // The receiver is the object the read was written on, not the one the
        // getter was found on: `derived.x` running a getter defined on the
        // prototype must see `derived`.
        super::accessor::Found::Getter(getter) => {
            let undefined = with_current(|context| undefined_of(context));
            super::functions::call(getter, object, undefined, undefined, undefined, undefined)
        }
        super::accessor::Found::Absent => with_current(|context| undefined_of(context)),
    }
}

/// `object.name = value`. Answers the value, because an assignment is an
/// expression.
///
/// Split for the same reason the read is: a setter is user code, and it runs
/// after the borrow ends.
#[rtse::entry]
pub fn set_property(object: u64, key: i64, value: u64) -> u64 {
    // ONE borrow of the context for the ordinary write, and the two paths that
    // cannot finish inside it — a proxy trap and a setter, both of which call
    // user code — leave with what they need instead.
    //
    // It was three: the key was resolved once for the proxy question and again
    // for the write, and the proxy question was asked before anything had
    // established that this even is one. Resolving a key is a lookup and taking
    // the context is a thread-local and a borrow, so the common write — an
    // object, no proxy, no setter — paid both twice for answers it discarded.
    let decided = with_current(|context| {
        let slot = Value(object).as_slot()?;
        let key = key_of(context, key)?;
        if context.proxy_at(slot).is_some() {
            return Some(Handled::Proxy(key));
        }
        if let Some(setter) = super::accessor::setter_for(context, slot, key) {
            return Some(Handled::Setter(setter));
        }
        put(context, slot, key, value);
        None
    });
    match decided {
        // A write to a non-object is a silent no-op in sloppy mode and a
        // `TypeError` in strict. Neither is emitted yet, and the value comes
        // back either way because that is what the expression produces.
        None => value,
        Some(Handled::Proxy(key)) => super::proxy::set(object, key, value).unwrap_or(value),
        Some(Handled::Setter(setter)) => {
            let undefined = with_current(|context| undefined_of(context));
            super::functions::call(setter, object, value, undefined, undefined, undefined);
            value
        }
    }
}

/// What a write turned out to need that cannot be done while the context is
/// borrowed, because it calls user code.
enum Handled {
    /// A proxy's `set` trap, with the key already resolved.
    Proxy(Key),
    /// A setter found on the receiver or above it.
    Setter(u64),
}

/// `super.x` — resolved above the home object, called against the receiver.
///
/// # Why this is not [`get_property`] with a different first argument
///
/// `get_property` has one object argument and uses it for two things that
/// `super` keeps apart: which cell the walk starts at, and which object an
/// inherited getter is called with. Passing the home object's prototype as
/// both would run an inherited getter with the WRONG receiver — `this` inside
/// it would be the prototype, not the instance the method was called on. And
/// passing `this` as both would find the method's OWN definition again,
/// which is the whole reason `super.m()` does not recurse into itself.
///
/// # Why a plain field set in the constructor answers `undefined`
///
/// `this.x = 7` in a base constructor makes `x` an OWN property of the
/// instance, never of the base class's `prototype`. The walk here starts
/// ABOVE the home object — at the parent's `prototype` — and never redirects
/// back down to the receiver's own properties; only a call to an accessor
/// getter is handed the receiver. So a plain inherited field is invisible to
/// `super.x` and `this.x` finds it instead. That is not a gap in this
/// entry point: it is what the specification's `OrdinaryGet` does, verified
/// against Node before trusting the intuition that `super` "sees" a field.
#[rtse::entry]
pub fn get_super_property(receiver: u64, start: u64, key: i64) -> u64 {
    let found = with_current(|context| {
        let Some(key) = key_of(context, key) else {
            return super::accessor::Found::Value(undefined_of(context));
        };
        let Some(slot) = Value(start).as_slot() else {
            return super::accessor::Found::Value(undefined_of(context));
        };
        super::accessor::resolve(context, slot, key)
    });
    match found {
        super::accessor::Found::Value(value) => value,
        // The receiver the read was written against, not the object the
        // getter was found on — the same rule [`get_property`] states, kept
        // here because `super` is the one caller for which those two are
        // never the same value.
        super::accessor::Found::Getter(getter) => {
            let undefined = with_current(|context| undefined_of(context));
            super::functions::call(getter, receiver, undefined, undefined, undefined, undefined)
        }
        super::accessor::Found::Absent => with_current(|context| undefined_of(context)),
    }
}

/// `super.x = v` — a setter above the home object runs against the receiver;
/// otherwise the write lands on the receiver itself.
///
/// # Why an unintercepted write goes to the receiver, not to `start`
///
/// The specification's `OrdinarySet` does not write to wherever the search
/// stopped: once no setter is found anywhere from `start` upward, the value
/// is written as an ORDINARY own property of the receiver — the same place
/// `this.x = v` would put it. Writing it onto `start` instead would mutate
/// the shared prototype object every instance inherits from, which is not
/// what `super.x = v` does in Node, and would make a plain `super.x = v` in
/// one base's method poison every subclass's instances at once.
#[rtse::entry]
pub fn set_super_property(receiver: u64, start: u64, key: i64, value: u64) -> u64 {
    let setter = with_current(|context| {
        let Some(start_slot) = Value(start).as_slot() else {
            return None;
        };
        let key = key_of(context, key)?;
        if let Some(setter) = super::accessor::setter_for(context, start_slot, key) {
            return Some(setter);
        }
        let Some(receiver_slot) = Value(receiver).as_slot() else {
            return None;
        };
        put(context, receiver_slot, key, value);
        None
    });
    if let Some(setter) = setter {
        let undefined = with_current(|context| undefined_of(context));
        super::functions::call(setter, receiver, value, undefined, undefined, undefined);
    }
    value
}

/// Puts a value at a key, taking the shape transition if the object does not
/// have that property yet.
///
/// Shared by the named write and the computed one. The transition is the part
/// that is easy to get subtly wrong, and there is no version of it that differs
/// between the two: by the time either arrives here, a key is a key.
///
/// Named `put` rather than `write` because `write` is a macro in scope, and a
/// call that silently resolves to one instead is a compile error whose message
/// points at the wrong thing.
pub(super) fn put(context: &mut Context, slot: u32, key: Key, value: u64) {
    // `a.length = 1` TRUNCATES, and `a.length = 3` grows with absent elements.
    // Writing the number alone left the array disagreeing with itself: `length`
    // answered 1 while `a[1]` still answered what was stored there, which is two
    // readable facts about one array that cannot both be true.
    //
    // Here rather than in the two write paths above, because `a.length = 1` and
    // `a["length"] = 1` are one operation and this is where they meet. The
    // elements are resized DIRECTLY rather than through `array::set_length`,
    // which would come back through this function.
    reconcile_length(context, slot, key, value);
    let Some(machine) = machine_key(key) else {
        return;
    };
    let Some(ty) = context.region.type_of(slot) else {
        return;
    };
    let Some(shape) = context.shape_of(ty) else {
        // A string, or a layout nothing recorded. Writing a property to a
        // string is a silent no-op in sloppy mode, which is what this is.
        return;
    };

    // Already in the layout: a store, at the offset the layout decided.
    if let Some(at) = context.shapes.slot_of(shape, machine) {
        // A frozen object refuses it, and so does a property declared
        // non-writable. Silently, like every other write this engine cannot
        // report — the language throws in strict mode, and `super::throw`
        // cannot find a handler in a caller.
        if super::integrity::refuses_key_write(context, slot, machine) {
            return;
        }
        set_slot_value(context, slot, at, value);
        return;
    }

    // A property the object does not have yet is growth, which all three
    // integrity levels refuse — that is the whole of what `preventExtensions`
    // says, and the reason the check is here rather than beside the frozen one.
    if super::integrity::refuses_growth(context, slot) {
        return;
    }

    // A new property changes what the object IS, so the shape moves and the
    // header moves with it. Taking the transition rather than choosing a
    // slot is what keeps two objects built the same way at one layout.
    // The representation the shape records is what the VALUE turned out to
    // be, not `Tagged` unconditionally. A shape already carries one per
    // property — `transition` takes it and `repr_of` reads it back — and
    // writing `Tagged` for everything made that field a place where a fact
    // could have been and was not.
    //
    // It is an observation about one write rather than a promise about the
    // property: a later write of something else takes a different
    // transition, so the object arrives at a different shape and every site
    // that remembered the old one stops recognising it. Which is what a
    // shape is for.
    let observed = if Value(value).numeric().is_some() {
        Repr::F64
    } else {
        Repr::Tagged
    };
    let Ok(grown) = context.shapes.transition(shape, machine, observed) else {
        return;
    };
    let Some(at) = context.shapes.slot_of(grown, machine) else {
        return;
    };
    // Re-typed against the SAME link, not against nothing: an instance that
    // gains a field must not lose the discrimination it was allocated with, or
    // two classes collide again the moment one of their instances grows.
    let link = context.prototype_at(slot);
    let ty = context.typed_as(grown, link).index() as u32;
    context.region.set_type(slot, ty);
    // Past the seventh this goes to the spill beside the cell rather than
    // being refused, which is what "the overflow indirection, and it is not
    // implemented here" was waiting for.
    set_slot_value(context, slot, at, value);
}

/// Reads a property: header to type, type to shape, shape to offset, load.
///
/// Three lookups where there were a hash map and a call, and none of them is
/// what compiled code will do — it will guard the type and load at a constant
/// offset, with no lookup at all. This is the runtime's own path, for the case
/// where the shape was not known while compiling.
///
/// # There is no walk any more, and that is a REGRESSION
///
/// A prototype chain needs somewhere to put the prototype, and a region cell is
/// a header and seven slots with no field reserved for one. Objects made here
/// have no prototype — they did not before either, because `Object.prototype`
/// does not exist in this runtime — so nothing observable changes today.
///
/// What changes is that the mechanism is gone rather than unused. It comes back
/// when a cell has a place for a prototype, and this is where to look.
///
/// # What it does not implement, and does not pretend to
///
/// Accessors. `crate::object::ordinary_get` states the two rules a correct read
/// obeys — the receiver is threaded rather than replaced, and the walk stops on
/// *presence* rather than on a non-`undefined` value — and it is the one
/// implementation of them. This is data-only, because a getter is a call into
/// user code and an entry point may not make one.
///
/// The second rule is still obeyed here and it matters: an own property holding
/// `undefined` shadows an inherited one, so the walk stops when the key is
/// **found**, not when a non-`undefined` value is found.
pub(super) fn read_property(context: &mut Context, start: u32, key: Key) -> Option<Value> {
    read(context, start, key)
}

fn read(context: &mut Context, start: u32, key: Key) -> Option<Value> {
    let mut cell = start;
    // Bounded rather than trusting the chain to end. Nothing here builds a
    // cycle, but a prototype is a value a program can set, and a walk that
    // trusted it would hang instead of answering — which is a worse failure
    // than a wrong value, because nothing reports it at all.
    for _ in 0..CHAIN_LIMIT {
        if let Some(found) = own_property(context, cell, key) {
            return Some(found);
        }
        // Not here, so ask what this inherits from. An element or a string.s
        // length never reaches this loop: those are answered before it.
        cell = inherited_from(context, cell)?;
    }
    None
}

/// What one cell holds for a key, without looking at what it inherits from.
///
/// Split out because the accessor walk needs the two questions interleaved: an
/// own data property shadows an inherited getter and an own getter shadows an
/// inherited data property, so a walk that asked one question over the whole
/// chain and then the other would get both backwards.
pub(super) fn own_property(context: &mut Context, cell: u32, key: Key) -> Option<Value> {
    let machine = machine_key(key)?;
    let ty = context.region.type_of(cell)?;
    let shape = context.shape_of(ty)?;
    let at = context.shapes.slot_of(shape, machine)?;
    slot_value(context, cell, at).map(Value)
}

/// What a cell inherits from, including the one kind that has no link of its own.
///
/// # Why a string's prototype is substituted rather than stored
///
/// Every string in the program is a cell, and there are as many of them as there
/// are strings. Giving each a prototype link would spend a word per string to
/// record one fact shared by all of them — and the link would have to be written
/// at every allocation, including the ones interning performs while the
/// prototype is being built.
///
/// So the chain walk asks the question instead. A text cell has no own
/// prototype, so it reaches here, and here is where the shared object is named.
///
/// # Why this is not a special case the fast path can disagree with
///
/// `cache_resolve` answers negative for a cell with no shape, so every read of a
/// string takes the slow path — the same argument [`super::string::text::string_property`] records
/// for `length`. A cached read never reaches a string, so there is nothing for
/// this to contradict.
pub(super) fn inherited_from(context: &mut Context, cell: u32) -> Option<u32> {
    if let Some(own) = context.prototype_at(cell) {
        return Value(own).as_slot();
    }
    // A callable, like a string, has no link of its own: `closure_new` runs at
    // every function definition, and writing one there would spend a word per
    // function to record a fact all of them share. Asked before the two below
    // because a callable is neither text nor an array, so the order costs
    // nothing and reads in the order a walk actually resolves.
    if context.callable_at(cell).is_some() {
        return super::function_proto::prototype_of(context);
    }
    if context.text_at(cell).is_some() {
        return super::string::prototype_of(context);
    }
    // An array is an ordinary object with an ordinary shape, so unlike a string
    // it could carry a link — and does not, for the reason a string does not:
    // the link would be written at every allocation to record one fact every
    // array shares. `prototype_at` is asked first, so `Object.setPrototypeOf`
    // still wins over the substitution.
    if context.elements_at(cell).is_some() {
        return super::array_proto::prototype_of(context);
    }
    // Everything left is a plain object, and a plain object inherits from
    // `Object.prototype`. Last, because there is nothing further to distinguish
    // on — and substituted rather than linked at `object_new` for the reason
    // every substitution here exists: a link written at every allocation would
    // spend a word per object to record one fact all of them share.
    let root = super::object_proto::prototype_of(context)?;
    // The root inherits from nothing. Without this, `inherited_from(root)`
    // answers `root` and every miss on it walks `CHAIN_LIMIT` steps through
    // itself — bounded rather than a hang, which makes it a silent
    // sixty-five-thousand-fold slowdown instead of a visible failure.
    (cell != root).then_some(root)
}

/// How far a prototype walk goes before giving up.
///
/// A depth no real chain reaches. It is a guard against a cycle a program
/// built, not a limit on inheritance — and answering `undefined` for one is
/// better than not answering, which is what a plain loop would do.
pub(super) const CHAIN_LIMIT: usize = 1 << 16;

/// The key a number names, if the registry issued it.
///
/// `None` for a number nothing minted, which is a compiled program naming a
/// property the host never wired up. It reads as an absent property rather
/// than as an invented one — a key cannot be conjured from an integer, and the
/// registry refusing is what says so.
fn key_of(context: &Context, number: i64) -> Option<Key> {
    let number = u32::try_from(number).ok()?;
    context.keys.key(number).map(Key::Name)
}

/// The encoded `undefined`, from the numbering the language declared.
pub(super) fn undefined_of(context: &Context) -> u64 {
    rts_cranelift::tags::encode(
        rts_cranelift::tags::TAG_SINGLETON,
        u64::from(context.singletons.undefined),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::with_context;
    use crate::value::Singletons;

    fn hosted<T>(body: impl FnOnce() -> T) -> T {
        let singletons = Singletons { undefined: 0, null: 1, hole: 2 };
        let mut context = Context::new(singletons, crate::value::Kinds::in_declaration_order());
        // The keys a test names have to have been issued, exactly as a host
        // issues the ones a program names. A registry that minted nothing
        // refuses every number, which is the behaviour being relied on.
        context.keys.declare(16);
        with_context(context, body).1
    }

    #[test]
    fn a_property_written_is_the_property_read() {
        hosted(|| {
            let object = object_new();
            set_property(object, 7, Value::from_f64(42.0).bits());
            assert_eq!(
                rts_cranelift::tags::decode_double(get_property(object, 7)),
                42.0
            );
        });
    }

    #[test]
    fn an_absent_property_is_undefined_rather_than_an_error() {
        hosted(|| {
            let object = object_new();
            let read = get_property(object, 7);
            assert_eq!(
                rts_cranelift::tags::tag_of(read),
                rts_cranelift::tags::TAG_SINGLETON
            );
        });
    }

    #[test]
    fn writing_a_second_property_does_not_disturb_the_first() {
        // What the shape transition has to get right: the second key lands in a
        // new slot rather than over the first, and the first object's layout
        // moves with it.
        hosted(|| {
            let object = object_new();
            set_property(object, 1, Value::from_f64(10.0).bits());
            set_property(object, 2, Value::from_f64(20.0).bits());
            assert_eq!(
                rts_cranelift::tags::decode_double(get_property(object, 1)),
                10.0
            );
            assert_eq!(
                rts_cranelift::tags::decode_double(get_property(object, 2)),
                20.0
            );
        });
    }

    #[test]
    fn overwriting_a_property_reuses_its_slot_rather_than_growing() {
        // The other half of the transition: a key already in the layout is a
        // store, not a new property. An implementation that transitioned every
        // time would grow an object without bound and give two objects built the
        // same way different shapes.
        hosted(|| {
            let object = object_new();
            set_property(object, 1, Value::from_f64(10.0).bits());
            set_property(object, 1, Value::from_f64(11.0).bits());
            assert_eq!(
                rts_cranelift::tags::decode_double(get_property(object, 1)),
                11.0
            );
        });
    }

    #[test]
    fn two_objects_built_the_same_way_share_one_layout() {
        // The reason a shape is a tree rather than a per-object list, and the
        // property an inline cache depends on: a site that has seen one of these
        // recognises the other.
        hosted(|| {
            let first = object_new();
            let second = object_new();
            set_property(first, 3, Value::from_f64(1.0).bits());
            set_property(second, 3, Value::from_f64(2.0).bits());
            crate::entry::with_current(|context| {
                // The header IS the shape, now: a cell records the type its
                // layout arrived at, so two objects at one layout carry the
                // same word.
                let type_of = |value: u64| {
                    let cell = Value(value).as_slot().expect("an object");
                    context.region.type_of(cell).expect("a live cell")
                };
                assert_eq!(type_of(first), type_of(second));
            });
        });
    }
}

/// The machine key a [`Key`] is.
///
/// An index has no machine key, for the reason `crate::object` gives: indexed
/// storage is an array's problem and an array is not yet a thing.
pub(super) fn machine_key(key: Key) -> Option<ShapeKey> {
    match key {
        Key::Name(name) => Some(name),
        Key::Index(_) => None,
    }
}

/// `ToPropertyKey`, for a key a program computed rather than wrote.
///
/// # Why an index becomes a name here
///
/// [`Key`] distinguishes a canonical integer index from any other string,
/// because enumeration order does: indices come first, in numeric order. That
/// distinction is real and it is **not usable yet** — `machine_key` answers
/// `None` for an index, because indexed storage waits for arrays.
///
/// So an index is held under its own spelling instead: `o[0]` and `o["0"]` are
/// choice is here rather than at each of the four call sites.
pub(super) fn slot_value(context: &Context, cell: u32, at: u32) -> Option<u64> {
    match at.checked_sub(crate::heap::INLINE_SLOTS) {
        None => context.region.field(cell, at),
        Some(past) => context.spill_at(cell)?.get(past as usize).copied(),
    }
}

/// Writes a property's value, wherever the slot puts it.
pub(super) fn set_slot_value(context: &mut Context, cell: u32, at: u32, value: u64) {
    match at.checked_sub(crate::heap::INLINE_SLOTS) {
        None => {
            context.region.set_field(cell, at, value);
        }
        Some(past) => context.spill_set(cell, past as usize, value),
    }
}

impl Context {
    /// A cell's spilled properties, if it has any.
    fn spill_at(&self, cell: u32) -> Option<&Vec<u64>> {
        self.spills.at(self.spill_of.copied(cell)?).ok()
    }

    /// Puts a value in a cell's spill, making one and growing it as needed.
    ///
    /// Grown with `undefined` rather than zero: a gap here is a property the
    /// shape says exists, and zero is not a value — it decodes as a double, so
    /// a reader would get `0` where the language says `undefined`.
    fn spill_set(&mut self, cell: u32, past: usize, value: u64) {
        let absent = undefined_of(self);
        let slot = match self.spill_of.copied(cell) {
            Some(slot) => slot,
            None => {
                let slot = self.spills.insert(Vec::new()).slot();
                self.spill_of.set(cell, slot);
                slot
            }
        };
        let Ok(spill) = self.spills.at_mut(slot) else {
            return;
        };
        if spill.len() <= past {
            spill.resize(past + 1, absent);
        }
        spill[past] = value;
    }
}

/// Copies a source's own enumerable properties onto a target.
///
/// What `{ ...source }` is, and what `Object.assign` is over one source. A call
/// and not emitted code because it walks a shape the compiler cannot see: how
/// many properties a value has, and which, is a run-time fact.
///
/// # Why the read goes through `get_indexed`
///
/// A getter on the source RUNS, once, and what lands on the target is a plain
/// data property — that is what the language says a spread does, and it is the
/// difference from inheriting. Running it means calling user code, so the borrow
/// is taken and released around every step rather than held across the loop: a
/// borrow held across a call into a program aborts the process.
///
/// # Why the keys are collected first
///
/// The walk answers what the source had when the spread started. A getter that
/// adds a property to its own object would otherwise be observed halfway, and
/// which properties a spread copies would depend on the order they were visited
/// in.
#[rtse::entry("__rts_object_spread")]
pub fn object_spread(target: u64, source: u64) -> u64 {
    let keys = with_current(|context| {
        super::array::key_texts(context, source, true)
            .into_iter()
            .map(|text| context.intern_value(text).bits())
            .collect::<Vec<_>>()
    });
    for key in keys {
        let value = super::computed::get_indexed(source, key);
        super::computed::set_indexed(target, key, value);
    }
    // The same gap `Object.assign` had, for the same reason: `key_texts` is
    // the string enumeration and a symbol has no string spelling for it to
    // report, so `{ ...withSym }` used to answer an object with every key but
    // the symbol one.
    let symbols = with_current(|context| super::array::symbol_keyed(context, source));
    for (key, value) in symbols {
        with_current(|context| {
            if let Some(cell) = Value(target).as_slot() {
                put(context, cell, key, value);
            }
        });
    }
    target
}

/// Makes an array's elements agree with a `length` a program just wrote.
///
/// # Why only an array, and only this key
///
/// Because `length` is an ordinary property on everything else — `{length: 3}`
/// is an object with a number on it and nothing to reconcile. What makes an
/// array different is that it has a second store the property is supposed to
/// describe, and a `length` written without touching it is a description that
/// stopped being true.
///
/// # What a grown array holds
///
/// `undefined`, the same as the gap `a[9] = 1` leaves. The language says both
/// are HOLES — absent rather than present-and-undefined — and this engine cannot
/// yet say that, which `set_indexed` already records as a stated gap. Growing
/// with `undefined` keeps the two spellings agreeing with each other, which is
/// the part that is in reach.
fn reconcile_length(context: &mut Context, slot: u32, key: Key, value: u64) {
    if key != super::computed::length_key(context) {
        return;
    }
    let Some(wanted) = Value(value).numeric() else {
        return;
    };
    // A negative or fractional length is a `RangeError` in the language, which
    // this engine cannot raise where a handler could catch it. Left alone
    // rather than rounded: writing some other number would be answering a
    // question the program got wrong.
    if wanted < 0.0 || wanted.fract() != 0.0 {
        return;
    }
    let wanted = wanted as usize;
    let absent = undefined_of(context);
    let Some(elements) = context.elements_at_mut(slot) else {
        return;
    };
    if elements.len() != wanted {
        elements.resize(wanted, absent);
    }
}
