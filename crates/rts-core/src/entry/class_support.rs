//! What a declared class needs at run time, and nothing a class declares.
//!
//! # Why the registry is a list and not a field per class
//!
//! `regexp_prototype`, `string_prototype` and `array_prototype` are fields on
//! the context because each has exactly one reader that knows its name. A
//! declared class does not: `#[rtse::class]` expands to code that has to find
//! *its own* prototype without the context having grown a field for it, and a
//! field per class would mean the attribute could not add one without editing
//! the struct — which is the "a proc macro cannot see its neighbours" limit
//! showing up as a build error instead of as a design.
//!
//! So the registry is keyed by the name the class declares, and looked up by a
//! linear scan. That is the right shape at this size for the reason
//! [`super::table`] gives about its own list, and the scan is over a handful of
//! entries reached once per class per program — the second read of `Error` is
//! answered by the global object's own property, not by this.
//!
//! # Why the coercions are here rather than in the expansion
//!
//! Because each one takes a borrow of the context and gives it back, and the
//! generated wrapper's whole safety argument is that the author's body runs with
//! none held. Expanding the borrow inline would put a `with_current` in every
//! wrapper and make that argument something a reader has to re-derive per
//! member.

use super::{Context, with_current};
use crate::value::Value;

/// What a class registration produced, remembered so the second read is the
/// same object.
///
/// The prototype is kept beside the constructor because something other than the
/// class asks for it: the chain walk substitutes `Function.prototype` for a
/// callable that has no link of its own, exactly as it substitutes
/// `String.prototype` for a text cell.
pub(super) struct Registered {
    /// The name JavaScript knows it by.
    pub(super) name: &'static str,
    /// The value the global name reads.
    pub(super) made: u64,
    /// What instances inherit from — the object itself, for a namespace.
    pub(super) prototype: u64,
    /// Where the OWNING registration — the one that installed real members,
    /// not an empty-list chain-read — called from. `Location::caller().file()`
    /// rather than the full location: two call sites in the same file are the
    /// same owner asking twice (`url::class.rs` registers `"URL"` five times),
    /// which is the idempotent case this must not flag.
    pub(super) owner: Option<&'static str>,
}

/// What a class registered as, if it has been registered.
pub(in crate::entry) fn made(context: &Context, name: &str) -> Option<u64> {
    context
        .classes
        .iter()
        .find(|entry| entry.name == name)
        .map(|entry| entry.made)
}

/// What instances of a registered class inherit from.
pub(in crate::entry) fn prototype(context: &Context, name: &str) -> Option<u64> {
    context
        .classes
        .iter()
        .find(|entry| entry.name == name)
        .map(|entry| entry.prototype)
}

/// Records a registration.
///
/// Called **before** the members are installed, and that ordering is the point:
/// installing interns names, interning allocates, and an allocation can reach
/// back into the same registration. Recording afterwards is what made the first
/// version of `string::prototype_of` recurse until the region ran out.
pub(in crate::entry) fn record(
    context: &mut Context,
    name: &'static str,
    made: u64,
    prototype: u64,
    owner: Option<&'static str>,
) {
    context.classes.push(Registered {
        name,
        made,
        prototype,
        owner,
    });
}

/// The owning file of an already-registered name, if it was registered with
/// real (non-empty) members rather than only ever chain-read.
pub(in crate::entry) fn owner(context: &Context, name: &str) -> Option<&'static str> {
    context
        .classes
        .iter()
        .find(|entry| entry.name == name)
        .and_then(|entry| entry.owner)
}

/// A constant a class declares, in the two spellings a property can hold
/// without running anything.
///
/// Two rather than one tagged value, because a `Value` is a word whose meaning
/// depends on a context that does not exist while the list is a `const`. Text in
/// particular has to be interned, which allocates — so what the declaration
/// carries is the source form and this is where it becomes a value.
pub(in crate::entry) enum Constant {
    /// `Math.PI`, `Number.MAX_SAFE_INTEGER`.
    Number(f64),
    /// `Error.prototype.name`.
    Text(&'static str),
}

/// Hangs constants on an object, by name.
///
/// Real properties rather than answers the runtime invents, for the reason
/// `regex::describe` records: compiled code reading a property that is in the
/// layout never asks the runtime at all, so a value only the slow path knows
/// about is one the fast path disagrees with the moment it starts working.
pub(in crate::entry) fn constants(context: &mut Context, cell: u32, constants: &[(&str, Constant)]) {
    for (name, held) in constants {
        let value = match held {
            Constant::Number(number) => Value::from_f64(*number).bits(),
            Constant::Text(text) => context
                .intern_value(crate::text::Str::from_str(text))
                .bits(),
        };
        let key = context.well_known(name);
        super::objects::put(context, cell, key, value);
        attributed(context, cell, key, held);
    }
}

/// The attribute set a class constant gets, decided by which spelling it is.
///
/// The two spellings are not the same kind of property and the specification
/// does not give them the same attributes, so the enum carries the decision
/// rather than a second list:
///
/// - A NUMBER is `Math.PI`, `Number.MAX_SAFE_INTEGER`, `Number.EPSILON`. Every
///   one of those is `{ writable: false, enumerable: false, configurable: false }`
///   — the language pins them, and `Math.PI = 3` is a silent no-op rather than
///   an assignment.
/// - TEXT is `Error.prototype.name` and `Error.prototype.message`, which are
///   ordinary `{ writable: true, enumerable: false, configurable: true }` data
///   properties: a subclass assigning `this.name` has to work.
///
/// Enumerability is the half a program notices first and the half that was
/// wrong for both: nothing marked these, so `Object.keys(Math)` answered its
/// eight constants where every runtime answers `[]`, and `JSON.stringify(Math)`
/// serialised them.
fn attributed(context: &mut Context, cell: u32, key: crate::object::Key, held: &Constant) {
    let crate::object::Key::Name(named) = key else {
        return;
    };
    let writable = matches!(held, Constant::Text(_));
    super::integrity::set_attributes(context, cell, named, super::integrity::Attributes {
        writable,
        enumerable: false,
        configurable: writable,
    });
}

/// `ToNumber` of an argument.
///
/// An object is converted by its `valueOf` first, outside any borrow, because
/// that is user code. It used to answer `NaN` there with a comment calling it
/// "the answer such an object would have produced anyway" — which was true of
/// `{}` and false of every object that defines a `valueOf`, so
/// `Number({ valueOf() { return 5 } })` was `NaN`.
///
/// `NaN` remains for what genuinely does not convert: an object whose two
/// methods both answer objects, and a symbol.
///
/// **For an ARGUMENT, never for a receiver.** [`this_number`] is the receiver's
/// spelling, and the difference is not a nicety: `Number.prototype.valueOf` is
/// reached through this if it uses the wrong one, and converting its receiver
/// looks up `valueOf` and calls it — itself. That recursed until the stack ran
/// out, in four suite files, the moment this learned to convert.
pub(in crate::entry) fn to_number(value: u64) -> f64 {
    // A value that already IS a double is its own `ToNumber`, and answering so
    // here costs a tag test where the three steps below cost three
    // `with_current` round trips — a thread-local access, a `RefCell` flag
    // written and restored, and a bounds-checked `Vec::last_mut`, each.
    //
    // Measured before this existed: `Math.imul` (two `f64` parameters) cost
    // 66.4 ns against `Math.sign`'s (one) 50.4 on the same run — 16 ns for one
    // more argument that needed no conversion at all.
    //
    // `Value::as_f64` is arithmetic on the word and reaches no heap
    // (`value/mod.rs`), so this asks nothing the slow path would answer
    // differently: `ToPrimitive` of a Number is that Number, a Number is not a
    // symbol, and `as_number` of one decodes the same bits. It fires ONLY for a
    // genuine unencoded double — an encoded `Int`, a string, a wrapper object
    // and a symbol all still take the path below, because for those the answer
    // is not identity and the middle step can run user code.
    if let Some(already) = Value(value).as_f64() {
        return already;
    }
    let value = super::primitive::to_primitive(value, crate::coerce::Hint::Number);
    // A SYMBOL has no numeric form and the language says so with a `TypeError`,
    // not with `NaN`. The difference is the whole point of the rule: `NaN`
    // propagates silently through arithmetic, so `Number(sym)` answering it made
    // a symbol that reached a numeric path produce a plausible-looking result
    // pages later instead of failing where it was written.
    //
    // Asked AFTER `ToPrimitive`, which is where the specification asks it: an
    // object whose `Symbol.toPrimitive` answers a symbol reaches the same
    // refusal as a bare symbol, and asking first would let it through.
    //
    // Raised outside the borrow below, for the reason every raising native has.
    if with_current(|context| super::symbol::is_symbol(context, value)) {
        super::throw::type_error("Cannot convert a Symbol value to a number");
        return f64::NAN;
    }
    // A BIGINT is the second value with no numeric form, and it refuses for the
    // same reason and with the same class. `+1n` and `Number(1n)` are not the
    // same operation and only the second converts: the unary operator is
    // `ToNumber`, which the specification makes a `TypeError` for a bigint
    // precisely so that a value chosen for its exactness cannot slip into a
    // double by being written next to a `+`.
    //
    // It answered `NaN`, which is the failure `Symbol` above already names and
    // costs more here: `NaN` is a plausible number, so `+bigValue` produced one
    // and the loss of range was invisible until an assertion far away.
    if with_current(|context| super::bigints::digits_of(context, value).is_some()) {
        super::throw::type_error("Cannot convert a BigInt value to a number");
        return f64::NAN;
    }
    with_current(|context| super::operators::as_number(context, Value(value)).unwrap_or(f64::NAN))
}

/// The number a `Number.prototype` method's receiver already IS.
///
/// `thisNumberValue`, which is a read rather than a conversion: the receiver of
/// `(5).toFixed(1)` is the primitive itself, and the specification requires it to
/// be one — it never runs `valueOf` to find out. That is the whole reason this
/// exists beside [`to_number`], and the reason is mechanical rather than
/// pedantic: `valueOf`'s own body would be the thing the conversion called.
///
/// A string receiver still reads as its numeric text, which is what `as_number`
/// answers and what the old shared spelling did — narrowing that too would be a
/// second change hiding inside a fix.
pub(in crate::entry) fn this_number(value: u64) -> f64 {
    with_current(|context| super::operators::as_number(context, Value(value)).unwrap_or(f64::NAN))
}

/// `ToBoolean` of an argument.
pub(in crate::entry) fn to_boolean(value: u64) -> bool {
    super::primitives::to_boolean(value)
}

/// The object a constructor writes onto: the one `new` made, or one made here.
///
/// # Why a constructor may be handed nothing
///
/// `Date()` and `Intl.NumberFormat()` are legal without `new` in the shapes
/// their own modules record, and a native reached that way has no receiver to
/// write to. Making one here — with the class's own prototype, so it is an
/// instance rather than a bare object — is what lets those bodies answer
/// something usable instead of branching on how they were called.
///
/// Shared rather than copied: `date` wrote this first and `intl` needed the
/// same eight lines, which is the point at which a second copy starts to drift
/// about which prototype an instance gets.
pub(in crate::entry) fn receiver(context: &mut Context, this: u64, class: &str) -> Option<u32> {
    if let Some(cell) = crate::value::Value(this).as_slot() {
        return Some(cell);
    }
    let cell = super::native::plain(context)?;
    if let Some(prototype) = prototype(context, class) {
        context.set_prototype(cell, prototype);
    }
    Some(cell)
}
