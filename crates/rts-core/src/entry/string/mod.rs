//! What a string inherits from.
//!
//! # Why a string needs a prototype at all, when `length` did not
//!
//! `"s".length` and `s[0]` are answered directly, by [`super::objects`], because
//! a string cell has no shape and every read of one takes the slow path — so
//! there is nothing for a special case to disagree with. That worked for exactly
//! two properties and stops at the third: `s.trim` has to produce a *callable*,
//! and a special case answering one would be inventing a function per read.
//!
//! So strings inherit, like everything else. The cell has no own prototype —
//! there are as many string cells as there are strings, and giving each a link
//! would be a word per string to record one shared fact — so the chain walk
//! substitutes the one prototype when it reaches a text cell. See
//! [`super::objects::inherited_from`].
//!
//! # Why that makes `String.prototype.mine = v` work
//!
//! It is an ordinary property write on an ordinary object, and every string in
//! the program already inherits from that object. Nothing about extending a
//! built-in is special-cased, which is the point: a program that adds a method
//! to `String.prototype` is doing what the language has always allowed, and an
//! engine that had to be taught about it would have been built wrong.
//!
//! # The wrapper object, and why it is here now
//!
//! `new String("a")` makes an object whose `typeof` is `"object"` and which
//! compares unequal to `"a"`. This file used to say it made none, on the
//! argument that a half-built wrapper behaving like a primitive is wrong in the
//! way that is hardest to find. The argument was right about an *implicit*
//! wrapper and does not reach this one: the program wrote `new`, and what it
//! got instead was the primitive — `typeof new String("x")` answered
//! `"string"` and `new String("")` was FALSY, which is the same class of
//! hard-to-find wrongness pointed the other way.
//!
//! What makes it whole rather than half is three separate facts, and each is
//! answered in one place: `typeof` and truthiness come from it being an
//! ordinary cell with `[[StringData]]` beside it ([`super::primitive_proto`]);
//! every method reads through [`receiver`], so `new String("ab").charAt(1)`
//! finds the text; and `length` and the index properties come from
//! [`text::string_property`]/[`text::string_element`], which the property path
//! already asked for a bare string cell.

mod basic;
mod coerce;
// Re-exported so every native still writes `super::coerce_receiver` — the
// prologue is the same sentence in eleven files and moving it to a module is a
// file-size split, not a change of who calls it.
use coerce::{coerce_receiver, is_regexp, number_arg, text_arg};
mod html;
mod more;
pub(super) mod pattern;
mod points;
mod replace;
mod repeat;
mod search;
mod split;
pub(in crate::entry) mod text;

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::text::Str;
use crate::value::Value;

/// What every string inherits from, made once.
///
/// Lazily, like the regular-expression prototype and for the same reason: a
/// program that never calls a string method should not spend a cell per built-in
/// out of a region fixed at construction.
pub(super) fn prototype_of(context: &mut Context) -> Option<u32> {
    if let Some(made) = context.string_prototype {
        return Some(made);
    }
    let cell = super::native::plain(context)?;
    // Recorded BEFORE the methods are installed. Installing them interns names,
    // and interning allocates strings — every one of which reaches this function
    // through the chain walk. Setting it afterwards would recurse until the
    // region ran out, which is not a hypothetical: it is what the first version
    // did.
    context.string_prototype = Some(cell);
    super::native::install(context, cell, basic::NATIVES);
    super::native::install(context, cell, search::NATIVES);
    super::native::install(context, cell, pattern::NATIVES);
    super::native::install(context, cell, split::NATIVES);
    super::native::install(context, cell, replace::NATIVES);
    super::native::install(context, cell, more::NATIVES);
    super::native::install(context, cell, points::NATIVES);
    // Annex B, through `install_with_arity` rather than `install`: their
    // `.length` is read by programs that introspect the prototype, and the four
    // that take an attribute answer 1 where the nine tag-only ones answer 0.
    super::native::install_with_arity(context, cell, html::NATIVES);
    iterator_method(context, cell);
    // `String.prototype.constructor` is written by `String`'s own lazy
    // registration, so a program that never spells `String` read
    // `"s".constructor === undefined`. Forcing the global here re-enters this
    // function, which answers from the cell recorded above.
    super::global::ensure(context, "String");
    Some(cell)
}

/// `String.prototype[Symbol.iterator]`, which is not installed by name.
///
/// # Why it is here at all, when `for`-`of` over a string already worked
///
/// Because they are different mechanisms. `for (const c of s)` reaches
/// [`super::iterate::iterate`], which recognises a text cell directly and never
/// asks the prototype anything — so a program that drives the protocol by hand,
/// `s[Symbol.iterator]().next()`, found `undefined` and called it. The method
/// was missing while the loop that is meant to be sugar for it worked, which is
/// the shape of gap a suite of `for`-`of` tests cannot see.
///
/// # Why it is `put` rather than `install`
///
/// [`super::native::install`] names each method by the key it stores it under,
/// and a symbol-keyed property is stored under the `@@`-prefixed text
/// [`super::symbol`] mints — so installing by that name would also write
/// `"@@iterator"` into `.name`, where the language says `"[Symbol.iterator]"`.
/// One key, two different strings; the only place they are both known is here.
fn iterator_method(context: &mut Context, cell: u32) {
    let method = super::native::callable(context, iterate_units);
    super::native::name_of(context, method, "[Symbol.iterator]");
    let key = context.well_known(&format!("{}iterator", super::symbol::PREFIX));
    super::objects::put(context, cell, key, method);
}

/// `s[Symbol.iterator]()` — an iterator over CODE POINTS.
///
/// Points and not units, which is the one thing this iterator is for:
/// `"a😀".length` is 3 and iterating it yields two elements, because the
/// surrogate pair is one character. [`super::iterate::iterate`] already decides
/// that for `for`-`of`, and this answers from it rather than deciding it again —
/// two spellings of where a character ends is how the loop and the method would
/// come to disagree about the same string.
extern "C" fn iterate_units(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    // Unwrapped first, for the reason [`receiver`] states: `iterate` recognises
    // a TEXT CELL, and a `new String("ab")` wrapper is not one — so a wrapper
    // reached the object path and iterated nothing, where the language iterates
    // its `[[StringData]]`.
    // Coerced like every other method's receiver: `String.prototype[Symbol.iterator]`
    // is specified over `ToString(RequireObjectCoercible(this))` too, so calling
    // it on a number iterates that number's DIGITS rather than nothing.
    let Some(held) = coerce_receiver(this) else {
        return refused();
    };
    super::list_iterator::over(super::iterate::iterate(held), "String Iterator")
}

/// `String` itself, as the value the name reads.
///
/// A callable with a `prototype` property, so `String.prototype.mine = v`
/// reaches the object strings inherit from and `String.yellow = f` is an
/// ordinary property write on the constructor.
pub(super) fn constructor(context: &mut Context) -> u64 {
    let callable = super::native::callable(context, convert);
    // `String.name`. Written here because a constructor built by hand is not a
    // `#[rtse::class]`, so nothing derives it — and answering `undefined` is
    // what left `Function.prototype.toString` unable to name it.
    super::native::name_of(context, callable, "String");
    let prototype = match prototype_of(context) {
        Some(cell) => Value::from_slot(cell).bits(),
        None => return undefined_of(context),
    };
    if let Some(cell) = Value(callable).as_slot() {
        // With arity, which is the spelling `Object`'s statics already use and
        // for the same reason: these three are read as function VALUES —
        // aliased, forwarded, introspected — so `.length` is observable rather
        // than decoration. See `points::STATICS` for why each number is stated
        // instead of defaulted.
        super::native::install_with_arity(context, cell, points::STATICS);
        let key = context.well_known("prototype");
        super::objects::put(context, cell, key, prototype);
    }
    // The other half of the link, which was missing: `"s".constructor` answered
    // `undefined` where every other runtime answers `String`, and
    // `.constructor.name` is how a program asks what a value is. Non-enumerable
    // like every built-in prototype member — `for (k in "")` walks the chain.
    if let Some(cell) = Value(prototype).as_slot() {
        let key = context.well_known("constructor");
        super::objects::put(context, cell, key, callable);
        super::native::hidden(context, cell, key);
    }
    callable
}

/// `String(x)` — the text of a value; `new String(x)` — a wrapper around it.
///
/// One body for both, because the conversion is the same and only the answer
/// differs. `construct` hands this a fresh object as its receiver, so the
/// wrapper already exists by the time this runs: recording the text into it is
/// `[[StringData]]`, and answering the object rather than the text is what
/// stops `construct` from keeping the primitive — a string is a cell here, and
/// `construct` keeps any cell a constructor returned.
///
/// The plain call is told apart by [`super::primitive_proto::wrap`]'s own test,
/// which is `new.target` rather than the receiver's class — see the note there
/// for why, and for the one spelling it does not cover.
extern "C" fn convert(
    _environment: u64,
    this: u64,
    value: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    // `ToPrimitive` first, and OUTSIDE the borrow: it may run a `toString`,
    // and that is user code whose first act may be to call the runtime. This
    // used to answer `undefined` for every object — `String([1, 2])` was
    // `"undefined"` — with a comment saying an entry point cannot call. It can;
    // `functions::call` is how every callback in this crate already runs.
    let value = super::primitive::to_primitive(value, crate::coerce::Hint::String);
    with_current(|context| {
        // `String(sym)` e a UNICA conversao de simbolo que a linguagem permite,
        // e e uma excecao escrita no proprio `String`: `"" + sym` continua a ser
        // um `TypeError` e `to_text` continua a recusar um simbolo, que e o que
        // impede uma conversao acidental. Sem esta linha respondia `undefined`.
        let text = match super::symbol::described(context, value) {
            Some(text) => context.intern_value(Str::from_str(&text)).bits(),
            None => match super::text::to_text(context, Value(value)) {
                Some(text) => context.intern_value(text).bits(),
                // Still not a primitive after the conversion. The absence stays.
                None => return undefined_of(context),
            },
        };
        // `new String(x)` answers the wrapper; `String(x)` answers the text.
        super::primitive_proto::wrap(context, this, text).unwrap_or(text)
    })
}

/// The string a method's `this` actually is.
///
/// `"a".trim()` gets the primitive and `new String("a").trim()` gets a WRAPPER
/// OBJECT — `[[StringData]]` in the specification, recorded beside the cell by
/// [`super::primitive_proto::wrap`]. Every accessor below goes through this, so
/// the wrapper case is written once instead of thirty times; a method reaching
/// for `text_at` directly is the spelling that would answer `undefined` for a
/// wrapper while the one beside it answered the text.
///
/// A primitive passes through unchanged, which is why this is safe to apply
/// unconditionally rather than behind a "is this a wrapper" test the caller
/// would have to remember to write.
pub(super) fn receiver(context: &Context, value: u64) -> u64 {
    super::primitive_proto::unwrap(context, value)
}


/// The receiver as code units.
///
/// Units and not bytes, and not characters: a JavaScript string IS a sequence of
/// UTF-16 code units, so every index a method takes or answers counts those.
/// Doing this in bytes makes `"é".indexOf("x")` disagree with `"é".length`,
/// which is the kind of wrong that survives a whole test suite of ASCII.
pub(super) fn units_of(context: &Context, value: u64) -> Option<Vec<u16>> {
    let text = context.text_at(Value(receiver(context, value)).as_slot()?)?;
    Some(text.units().collect())
}

/// How many code units the receiver has, without copying any of them.
///
/// For the methods that only need to know whether an index is in range. Paired
/// with [`indexed`], it is what stopped a single character read from costing the
/// whole string — see the note there.
pub(super) fn length_of(context: &Context, value: u64) -> Option<usize> {
    Some(context.text_at(Value(receiver(context, value)).as_slot()?)?.len())
}

/// One code unit of the receiver.
///
/// # Why this exists beside [`units_of`]
///
/// Because `units_of` copies. Every index read went through it, so
/// `s.charCodeAt(i)` materialised the entire string into a fresh `Vec<u16>` to
/// look at one unit of it — quadratic in a scan, which is what every lexer is.
/// Measured before the change: a loop over 10 000 characters took 0.86 s, 20 000
/// took 3.17 s, 40 000 took 14.4 s, 80 000 took 63.1 s. Exactly four times per
/// doubling, and the suite file that scans 100 000 never finished.
///
/// `Str::unit_at` was already there and answered in constant time for both
/// representations. Nothing had to be built; the copy simply had to stop.
pub(super) fn indexed(context: &Context, value: u64, at: usize) -> Option<u16> {
    context
        .text_at(Value(receiver(context, value)).as_slot()?)?
        .unit_at(at)
}

/// An argument as text, converting the way the language does.
///
/// `"abc".indexOf(1)` searches for `"1"`, because the specification runs
/// `ToString` on the argument. `None` is an object, whose conversion calls user
/// code — the boundary every conversion here stops at.
///
/// A **wrapper** is the one object this does convert, through [`receiver`]:
/// `ToString(new String("b"))` runs a `toString` that is `thisStringValue` and
/// nothing else, so reading `[[StringData]]` is that call's whole result rather
/// than a shortcut past user code. It matters twice — the methods that take
/// their receiver this way (`split`, `replace`, `match`) answered `undefined`
/// for a wrapper, and so did `"abc".indexOf(new String("b"))`.
///
/// A subclass that overrides `toString` still diverges, and that is the same
/// stated boundary: the honest spelling is `to_primitive`, which cannot run
/// inside the borrow every caller here holds.
pub(super) fn text_of(context: &Context, value: u64) -> Option<Str> {
    super::text::to_text(context, Value(receiver(context, value)))
}

/// The same, as code units.
pub(super) fn arg_units(context: &Context, value: u64) -> Option<Vec<u16>> {
    Some(text_of(context, value)?.units().collect())
}

/// Whether an argument was left out.
///
/// Distinguished from a value, because it decides a default rather than being
/// converted: `"abc".slice(1)` ends at the end and `"abc".slice(1, undefined)`
/// does too, while `"abc".slice(1, 0)` is empty.
pub(super) fn absent(context: &Context, value: u64) -> bool {
    value == undefined_of(context)
}

/// A string value over code units.
pub(super) fn answer(context: &mut Context, units: &[u16]) -> u64 {
    context.intern_value(Str::from_utf16(units)).bits()
}

/// A string containing exactly one UTF-16 code unit.
///
/// The context owns a finite cache for the Latin-1 range, which is the common
/// result of indexing ASCII and Latin-1 strings. Wide units retain the ordinary
/// allocation path because they cannot share the Latin-1 representation.
pub(super) fn answer_unit(context: &mut Context, unit: u16) -> u64 {
    context.single_unit_text(unit)
}

/// The same, from bytes that are already known to be one code unit each.
///
/// `Str::from_utf16` scans its input to decide whether the narrow form fits.
/// A caller that sliced a narrow string already knows it does — every byte of
/// a narrow string is below 256 by construction — so the scan is asking a
/// question whose answer it was handed.
/// A string answered from bytes the caller owns.
///
/// Takes the `Vec` rather than a slice, which is one copy instead of two: a
/// method that maps or slices already built the bytes, and handing over a
/// borrow made `Str` copy them again.
pub(super) fn answer_owned(context: &mut Context, bytes: Vec<u8>) -> u64 {
    context.intern_value(Str::owning_latin1(bytes)).bits()
}

/// The undefined a method answers when there is nothing to answer.
pub(super) fn nothing(context: &Context) -> u64 {
    undefined_of(context)
}

/// The same, for a method that has NOT taken the borrow yet.
///
/// Only [`coerce_receiver`]'s refusal needs it, and it needs it in eleven files:
/// the prologue runs before any `with_current`, so the answer for a receiver the
/// language refuses cannot come from a context the native is not holding.
pub(super) fn refused() -> u64 {
    with_current(|context| undefined_of(context))
}

/// An index argument, as the language reads one.
///
/// `ToIntegerOrInfinity`: `ToNumber` first, then `NaN` becomes zero and anything
/// else truncates **toward zero**. Both halves were missing and each is a wrong
/// answer of its own:
///
/// - Without `ToNumber`, `"hello".at("2")` read the string as no number at all
///   and answered `"h"` — index 0 — where every engine answers `"l"`. The same
///   for `true`, which is index 1.
/// - Without truncation toward zero, `"hello".at(-1.5)` computed `5 - 1.5` and
///   cast, which is `floor` for a positive result: index 3, `"l"`, where the
///   language truncates the ARGUMENT to `-1` and answers `"o"`. The two agree on
///   every whole number, which is why it survived.
///
/// The infinities pass through as themselves. A caller compares against a length
/// before it casts, so `±∞` is out of range by arithmetic rather than by a case.
///
/// The same conversion, performed OUTSIDE any borrow so an object converts.
///
/// [`integer_arg`] answers zero for an object, and its own documentation calls
/// that the stated gap: `ToNumber` on an object runs the program's `valueOf`,
/// and calling user code from inside a `with_current` re-enters the `RefCell`.
///
/// This is the other half. A method that wants an object argument to work calls
/// this BEFORE it borrows, and passes the number in — which is exactly the shape
/// `array_proto::more::at` already uses and the reason its own two statements
/// are two. `"abc".substring({valueOf: () => 1}, {valueOf: () => 3})` answered
/// the empty string and now answers `"bc"`.
///
/// A symbol raises here rather than answering zero, because [`super::class_support::to_number`]
/// is what performs the conversion and that refusal is the language's.
pub(super) fn integer_outside(value: u64) -> f64 {
    let number = super::class_support::to_number(value);
    match number.is_nan() {
        true => 0.0,
        false => number.trunc(),
    }
}

/// An object answers `NaN` and therefore zero, because `ToNumber` on one runs
/// user code and this is inside a borrow. The stated gap: `s.at({valueOf: …})`
/// reads index 0.
pub(super) fn integer_arg(context: &Context, value: u64) -> f64 {
    let number = super::operators::as_number(context, Value(value)).unwrap_or(f64::NAN);
    match number.is_nan() {
        true => 0.0,
        false => number.trunc(),
    }
}

/// An index a method was given, relative to a length.
///
/// Negative counts from the end and clamping is what the specification does at
/// every one of these — `"abc".slice(-99)` is the whole string rather than an
/// error. Written once because five methods need it and five copies is where
/// one of them would clamp differently.
pub(super) fn relative(index: f64, length: usize) -> usize {
    if index.is_nan() {
        return 0;
    }
    let length = length as f64;
    let at = if index < 0.0 { length + index } else { index };
    at.clamp(0.0, length) as usize
}
