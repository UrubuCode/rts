//! What an array inherits from.
//!
//! # Why this is a folder and not one file
//!
//! Because the callback-taking methods are a different *shape* of function, not
//! merely more of them. Everything here reads the elements inside one borrow of
//! the context and answers; `map` and its seven relatives have to let the borrow
//! go in the middle, because the thing they call is user code whose first act may
//! be to call the runtime. Keeping the two kinds in one file put a reader one
//! scroll away from copying the wrong one, and copying the wrong one is a
//! re-entrant `RefCell` borrow — a hang, not a wrong answer. See [`iterate`].
//!
//! # Why arrays inherit the way strings do
//!
//! There is one prototype for every array in the program, and it is substituted
//! by the chain walk rather than linked from each cell. `array_new` would
//! otherwise write the link at every allocation — including the ones performed
//! while the prototype itself is being built — and a word per array to record one
//! fact they all share is the cost [`super::objects::inherited_from`] already
//! refused for text.
//!
//! The alternative considered and rejected: giving `array_new` the link and
//! making the prototype eager. That spends the cells of nineteen natives on a
//! program that only ever indexes, and the region is fixed at construction.
//!
//! # Why `length` is written after every mutation
//!
//! Because it is a real **property**, not something the runtime invents on
//! demand — [`super::array::set_length`] records why. Compiled code reads it
//! through `cached_get` and never asks the runtime, so a `push` that grew the
//! elements and left the property alone produces a program where `a.length` is
//! stale and the loop over it is short. Every mutation here goes through
//! [`store`] for exactly that reason.

mod arguments;
mod concat;
mod construct;
mod cursor;
mod generic;
pub(super) mod iterate;
mod joining;
mod like;
pub(in crate::entry) mod more;
mod numeric;
mod positional;
pub(in crate::entry) mod species;
mod stack;

pub use arguments::{Arguments, arguments_at, arguments_owned_at};

use super::objects::undefined_of;
use super::rooted::Rooted;
use super::{Context, with_current};
use crate::value::Value;

/// What an array's prototype holds, apart from the ones that call back.
const NATIVES: &[(&str, super::native::Native, u32)] = &[
    ("push", stack::push, 1),
    ("pop", stack::pop, 0),
    ("shift", stack::shift, 0),
    ("unshift", stack::unshift, 1),
    ("indexOf", positional::index_of, 1),
    ("includes", positional::includes, 1),
    ("join", joining::join, 1),
    ("slice", positional::slice, 2),
    ("concat", concat::concat, 1),
    ("reverse", positional::reverse, 0),
    ("fill", positional::fill, 1),
];

/// What `Array` itself holds.
///
/// Statics rather than prototype methods, and the language put them there on
/// purpose: `Array.isArray(x)` has to answer for an `x` whose own prototype was
/// replaced, which a method reached through the chain cannot.
const STATICS: &[(&str, super::native::Native, u32)] =
    &[("isArray", construct::is_array, 1), ("of", construct::of, 0)];

/// What every array inherits from, made once.
///
/// Lazily, like the string and regular-expression prototypes and for the same
/// reason: nineteen natives is nineteen cells out of a region fixed at
/// construction, and a program that only indexes should not spend them.
pub(super) fn prototype_of(context: &mut Context) -> Option<u32> {
    if let Some(made) = context.array_prototype {
        return Some(made);
    }
    // An ARRAY, not a plain object, which is what the specification says
    // `Array.prototype` is: `Array.isArray(Array.prototype)` is `true`,
    // `Array.prototype.length` is `0`, and
    // `Object.prototype.toString.call(Array.prototype)` is `[object Array]`.
    // All three read the other way while this was `native::plain`'s object —
    // and the first is the one a library actually asks, since `isArray` is how
    // a program decides whether something it was handed is one.
    //
    // Through `built_in` so that "what makes a cell an array" stays one
    // answer: the element store, the mark and the `length` are attached
    // together there, and a hand-rolled version here is where the three would
    // come to disagree.
    let Some(cell) = Value(super::array::built_in(context, Vec::new())).as_slot() else {
        return None;
    };
    // Recorded BEFORE the methods are installed, for the reason the string
    // prototype records: installing interns names, interning allocates, and an
    // allocation is one chain walk away from asking this function again. The
    // string version recursed until the region ran out before the order was
    // fixed, and the same order is the fix here.
    context.array_prototype = Some(cell);
    super::native::install_with_arity(context, cell, NATIVES);
    super::native::install_with_arity(context, cell, iterate::NATIVES);
    super::native::install_with_arity(context, cell, more::NATIVES);
    // `Symbol.iterator`, which those three lists cannot carry: a native is named
    // by a string there and this key is a symbol. It IS `values` — the same
    // function, not a second one, because `[...a]` and `a.values()` walking an
    // array differently is the failure that would be found last.
    let key = context.well_known(super::symbol::ITERATOR);
    // The INSTALLED `values`, read back, and not a second callable over the
    // same function pointer. The line below minted one, so
    // `[][Symbol.iterator] === [].values` was `false` — and the comment above
    // said the opposite of what the code did. Two cells is not merely
    // redundant: the language makes them one object, a program compares them,
    // and a replacement written to `Array.prototype.values` would leave
    // spreading untouched.
    let named = context.well_known("values");
    let values = super::objects::read_property(context, cell, named)
        .map(|found| found.bits())
        .unwrap_or_else(|| super::native::callable(context, more::values));
    super::objects::put(context, cell, key, values);
    // Remembered as it is installed, which is the only moment it is knowably
    // the primordial: `super::pattern::array_pattern_direct` licenses reading an
    // array pattern's source by index only when the source's `Symbol.iterator`
    // is still THIS function, and a value read later could already be a
    // program's replacement. Recording it is not a protector — nothing
    // invalidates it, because the guard compares against the CURRENT property
    // every time it is asked.
    context.array_iterator_method = Some(values);
    install_unscopables(context, cell);
    // `Array.prototype.constructor` is written by `Array`'s registration, and
    // that registration is lazy — so a program that never spells `Array` read
    // `[].constructor === undefined`. Forcing the global here re-enters
    // `constructor`, which calls this function and gets the cell recorded
    // above, so the recursion terminates on the line that already had to be
    // there for interning.
    super::global::ensure(context, "Array");
    Some(cell)
}

/// `Array.prototype[Symbol.unscopables]`.
///
/// # Why an array of all things needs one
///
/// Every name on this list was added to `Array.prototype` AFTER `with` already
/// existed, and a `with (array)` in code written before them would silently
/// change meaning the day one arrived — `with (a) { keys }` reaching
/// `Array.prototype.keys` instead of the program's own `keys`. The list is how
/// the language kept that from happening, and it is the reason
/// [`super::computed::with_has`] is not `in`.
///
/// Written out rather than derived from the method lists above, because it is
/// not "the methods": `push`, `join` and `slice` are old enough to predate the
/// problem and are deliberately absent, so an array still unscopes exactly what
/// the specification says and nothing else. A list derived from what this engine
/// happens to install would change meaning every time a method is added.
fn install_unscopables(context: &mut Context, prototype: u32) {
    const BLOCKED: &[&str] = &[
        "at",
        "copyWithin",
        "entries",
        "fill",
        "find",
        "findIndex",
        "findLast",
        "findLastIndex",
        "flat",
        "flatMap",
        "includes",
        "keys",
        "toReversed",
        "toSorted",
        "toSpliced",
        "values",
    ];
    let Some(list) = super::native::plain(context) else {
        return;
    };
    // `OrdinaryObjectCreate(null)` — the list inherits NOTHING, and the
    // specification is explicit about it for a reason a program can see:
    // `Symbol.unscopables` is consulted with `HasProperty`, so a list
    // inheriting from `Object.prototype` answers `true` for `toString`,
    // `constructor` and `hasOwnProperty`, and `with (a) { toString() }` then
    // resolves past the array to whatever the enclosing scope has. This built a
    // plain object, so all three were blocked that nothing asked to block.
    let null = Value::from_singleton(context.singletons.null).bits();
    context.set_prototype(list, null);
    let yes = Value::from_bool(true).bits();
    for name in BLOCKED {
        let key = context.well_known(name);
        super::objects::put(context, list, key, yes);
    }
    let key = context.well_known(&format!("{}unscopables", super::symbol::PREFIX));
    let list = Value::from_slot(list).bits();
    super::objects::put(context, prototype, key, list);
    // `{ writable: false, enumerable: false, configurable: true }`, which is
    // what the specification gives every well-known-symbol member — so
    // `Array.prototype[Symbol.unscopables] = x` stores nothing and
    // `Reflect.set` reports the refusal. Unmarked it read as the defaults,
    // which say writable and enumerable.
    super::native::introspective(context, prototype, key);
}

/// `Array` itself, as the value the name reads.
///
/// A callable with a `prototype` property, so `Array.prototype.last = f` reaches
/// the object every array inherits from and `Array.from = g` is an ordinary
/// property write on the constructor.
pub(super) fn constructor(context: &mut Context) -> u64 {
    let callable = super::native::callable(context, construct::make);
    // `Array.name`, for the reason `string::constructor` gives: a hand-built
    // constructor has nothing deriving its name.
    super::native::name_of(context, callable, "Array");
    // `Array.length` is 1 — the `SetFunctionLength` every constructor gets, and
    // it answered `undefined`. A program forwarding a constructor reads it, and
    // `undefined` there is not a smaller answer than 1: `f.length` participates
    // in arithmetic, and `NaN` is what a currying helper got.
    super::native::length_of(context, callable, 1);
    let prototype = match prototype_of(context) {
        Some(cell) => Value::from_slot(cell).bits(),
        None => return undefined_of(context),
    };
    if let Some(cell) = Value(callable).as_slot() {
        super::native::install_with_arity(context, cell, STATICS);
        super::native::install_with_arity(context, cell, more::STATICS);
        let key = context.well_known("prototype");
        super::objects::put(context, cell, key, prototype);
    }
    // `Array.prototype.constructor`, which was missing and is not decoration:
    // the species protocol starts by reading `constructor` off the receiver, so
    // without it `[].constructor` is `undefined` and `[1].map(f)` cannot tell
    // the built-in class from a subclass that overrode it. Non-enumerable, like
    // every other member of a built-in prototype — `for (k in [])` walks the
    // chain, and an enumerable one appears in the most ordinary loop a program
    // writes.
    if let Some(cell) = Value(prototype).as_slot() {
        let key = context.well_known("constructor");
        super::objects::put(context, cell, key, callable);
        super::native::hidden(context, cell, key);
    }
    // `Array[Symbol.species]`, the other half of the protocol the comment above
    // describes. Without it the read answered `undefined` and the derivation
    // fell back to the intrinsic — which looks identical for `Array` itself and
    // is exactly wrong for a subclass, whose whole reason to inherit the hook is
    // to be answered instead.
    super::native::species(context, callable);
    callable
}

/// The receiver's cell and a copy of its elements, when it is an array.
///
/// A **copy**, and that is what makes the two-stage shape in [`iterate`]
/// possible at all: the borrow of the context ends with this function, so the
/// caller holds elements rather than a reference into the store. A method that
/// held the reference could not call anything.
///
/// `None` for a receiver that is not an array. Answering `undefined` rather than
/// panicking is the rule every entry point here follows — a runtime that aborts
/// on `Array.prototype.push.call(1)` turns a `TypeError` into a dead process. Such a receiver takes [`generic`]'s arm instead.
pub(super) fn staged(context: &Context, this: u64) -> Option<(u32, Vec<u64>)> {
    let cell = Value(this).as_slot()?;
    Some((cell, context.elements_at(cell)?.clone()))
}

/// The receiver's elements, BORROWED, for a method that only reads them.
///
/// [`staged`] copies so that a method which calls user code can drop the
/// borrow before calling — the two-stage shape `iterate` needs. A method that
/// calls nothing does not need that, and copying a thousand-element array to
/// answer whether it contains a number is the whole cost of the answer.
///
/// The borrow is what enforces it: a caller holding this cannot call anything
/// that takes the context, so the distinction cannot be got wrong quietly.
pub(super) fn borrowed(context: &Context, this: u64) -> Option<&Vec<u64>> {
    context.elements_at(Value(this).as_slot()?)
}

/// Writes elements back, and the `length` that goes with them.
///
/// The two together, always. Splitting them is what leaves a program whose
/// `a.length` disagrees with what a loop over `a[i]` finds — and the fast path
/// reads the property without ever asking the runtime, so nothing would report
/// it.
pub(super) fn store(context: &mut Context, cell: u32, values: Vec<u64>) {
    let count = values.len();
    if let Some(elements) = context.elements_at_mut(cell) {
        *elements = values;
    }
    super::array::set_length(context, cell, count);
}

/// A fresh array holding the given values.
///
/// Called with **no borrow held**: `array_new` takes the context itself, so
/// calling this from inside `with_current` re-enters the `RefCell`.
/// Takes a [`Rooted`] and not a `Vec` because `array_new` ALLOCATES, and until
/// this returns the values are named by nothing the collector walks: a `Vec`'s
/// buffer is on the Rust heap, which no scan of ours reaches. Measured — nine of
/// three hundred `map` rounds came back with wrong data. See `super::rooted`.
///
/// The guard is released only after the array exists, and the store that
/// follows allocates nothing, which is what makes that window safe rather than
/// merely short.
pub(super) fn built(values: Vec<u64>) -> u64 {
    // Wrapped HERE rather than at the nineteen call sites: every one of them
    // reaches this line, so one guard covers all of them, and a twentieth
    // written tomorrow is covered without anybody remembering to.
    //
    // What it does NOT cover is a caller that accumulates across calls into
    // user code — `map` and `filter` do — because the values are exposed while
    // that loop runs, before this is ever reached. Those hold a guard of their
    // own for the loop.
    let values = Rooted::with(values);
    // Through `built_in_rooted` rather than `array_new` plus [`store`], and the
    // difference is a whole vector. `array_new(n)` builds `vec![hole; n]` — a
    // malloc of eight bytes per element and n writes — inserts it, and calls
    // `set_length`; the store then REPLACED that vector with this one, freeing
    // the holes nobody read, and called `set_length` a second time with the
    // same count. Nineteen call sites paid it, including `map`, `filter`,
    // `split`, `Object.keys`, `subarray`, `join` and a regular expression's
    // match array.
    //
    // `built_in_rooted` exists for exactly this transfer and states the rule it
    // keeps: the CELL is allocated first, while the values are still registered,
    // and nothing between the take and the insert allocates. So this is the same
    // window, not a shorter one.
    with_current(|context| super::array::built_in_rooted(context, values))
}

