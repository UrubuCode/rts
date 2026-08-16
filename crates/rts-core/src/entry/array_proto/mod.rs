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
pub(super) mod iterate;
mod joining;
mod like;
mod more;
mod numeric;
pub(in crate::entry) mod species;

pub use arguments::arguments_at;

use super::objects::undefined_of;
use super::rooted::Rooted;
use super::string::{absent, relative};
use super::{Context, with_current};
use crate::value::Value;

/// What an array's prototype holds, apart from the ones that call back.
const NATIVES: &[(&str, super::native::Native)] = &[
    ("push", push),
    ("pop", pop),
    ("shift", shift),
    ("unshift", unshift),
    ("indexOf", index_of),
    ("includes", includes),
    ("join", joining::join),
    ("slice", slice),
    ("concat", concat::concat),
    ("reverse", reverse),
    ("fill", fill),
];

/// What `Array` itself holds.
///
/// Statics rather than prototype methods, and the language put them there on
/// purpose: `Array.isArray(x)` has to answer for an `x` whose own prototype was
/// replaced, which a method reached through the chain cannot.
const STATICS: &[(&str, super::native::Native)] =
    &[("isArray", construct::is_array), ("of", construct::of)];

/// What every array inherits from, made once.
///
/// Lazily, like the string and regular-expression prototypes and for the same
/// reason: nineteen natives is nineteen cells out of a region fixed at
/// construction, and a program that only indexes should not spend them.
pub(super) fn prototype_of(context: &mut Context) -> Option<u32> {
    if let Some(made) = context.array_prototype {
        return Some(made);
    }
    let cell = super::native::plain(context)?;
    // Recorded BEFORE the methods are installed, for the reason the string
    // prototype records: installing interns names, interning allocates, and an
    // allocation is one chain walk away from asking this function again. The
    // string version recursed until the region ran out before the order was
    // fixed, and the same order is the fix here.
    context.array_prototype = Some(cell);
    super::native::install(context, cell, NATIVES);
    super::native::install(context, cell, iterate::NATIVES);
    super::native::install(context, cell, more::NATIVES);
    // `Symbol.iterator`, which those three lists cannot carry: a native is named
    // by a string there and this key is a symbol. It IS `values` — the same
    // function, not a second one, because `[...a]` and `a.values()` walking an
    // array differently is the failure that would be found last.
    let key = context.well_known(&format!("{}iterator", super::symbol::PREFIX));
    let values = super::native::callable(context, more::values);
    super::objects::put(context, cell, key, values);
    install_unscopables(context, cell);
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
    let yes = Value::from_bool(true).bits();
    for name in BLOCKED {
        let key = context.well_known(name);
        super::objects::put(context, list, key, yes);
    }
    let key = context.well_known(&format!("{}unscopables", super::symbol::PREFIX));
    let list = Value::from_slot(list).bits();
    super::objects::put(context, prototype, key, list);
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
    let prototype = match prototype_of(context) {
        Some(cell) => Value::from_slot(cell).bits(),
        None => return undefined_of(context),
    };
    if let Some(cell) = Value(callable).as_slot() {
        super::native::install(context, cell, STATICS);
        super::native::install(context, cell, more::STATICS);
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
    callable
}

/// `a.push(…)` — answers the new length, or throws when the array refuses one.
///
/// # Why the refusal is a throw and why it is decided BEFORE the append
///
/// `push` is defined as `Set(O, ToString(len), E, true)` followed by
/// `Set(O, "length", len, true)`, and an array's `[[DefineOwnProperty]]`
/// rejects an index at or past a `length` that is not writable — so the throw
/// happens on the first element and nothing is stored. Appending first and
/// letting `set_length` quietly fail is what this used to do, and it produced
/// an array disagreeing with itself: `Object.defineProperty(a, "length",
/// {writable: false}); a.push(4)` left `a.length` at 3 with four elements in
/// it, which every read of `a[3]` could see and every loop over `a.length`
/// could not.
///
/// The raise is OUTSIDE the borrow. `throw::type_error` builds the program's
/// own `TypeError`, which takes the context — raising from inside would
/// re-enter the `RefCell`, and an `extern "C"` frame cannot unwind out of that,
/// so it ends the process rather than the call. The message is therefore built
/// in and thrown out, which is the two-stage shape every native that raises
/// here uses.
extern "C" fn push(_e: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let (answer, refused) = with_current(|context| {
        let more = arguments_at(context, 0, [a0, a1, a2, a3]);
        let Some(cell) = Value(this).as_slot() else {
            return (undefined_of(context), None);
        };
        // Nothing to add is nothing to refuse: `a.push()` re-states the length
        // it already has, and re-stating the same value is permitted even on a
        // non-writable property.
        if !more.is_empty() && refuses_append(context, cell) {
            return (undefined_of(context), Some(refusal(context, cell)));
        }
        // Appended IN PLACE. `staged` copies, and it copies for a reason —
        // a method that calls user code cannot hold a borrow of the context
        // across the call — but `push` calls nothing. Copying here made
        // building an array O(N^2): `Vec::clone` allocates capacity exactly
        // equal to length, so the `extend` that follows reallocated every
        // time. Two allocations and two O(n) copies per element appended.
        //
        // The borrow ends before `set_length`, which is why this is two
        // statements and not one.
        let Some(elements) = context.elements_at_mut(cell) else {
            return (undefined_of(context), None);
        };
        elements.extend_from_slice(&more);
        let count = elements.len();
        super::array::set_length(context, cell, count);
        (Value::from_f64(count as f64).bits(), None)
    });
    if let Some(message) = refused {
        super::throw::type_error(&message);
    }
    answer
}

/// Whether an array refuses to grow.
///
/// Asked of `length` rather than of the elements, because that is where the
/// language records it: `Object.defineProperty(a, "length", {writable: false})`
/// and `Object.freeze(a)` are the two ways to reach this, and
/// [`super::integrity::refuses_key_write`] already folds the object's own
/// refusal into the property's — so one question answers both instead of two
/// that could come to disagree.
fn refuses_append(context: &mut Context, cell: u32) -> bool {
    match super::computed::length_key(context) {
        crate::object::Key::Name(named) => {
            super::integrity::refuses_key_write(context, cell, named)
        }
        // `length` is a name, always. A key registry that answered otherwise is
        // malformed, and refusing every push over it would be a wrong answer
        // dressed as caution.
        crate::object::Key::Index(_) => false,
    }
}

/// What the refusal SAYS, which differs by which of the two caused it.
///
/// The message is the only part of a `TypeError` a program usually reads, and
/// the two causes are genuinely different repairs: a frozen array needs the
/// freeze removed and an array with a pinned `length` needs the descriptor
/// changed. One message for both would name the wrong one half the time.
fn refusal(context: &Context, cell: u32) -> String {
    if super::integrity::refuses_write(context, cell) {
        let at = context.elements_at(cell).map_or(0, Vec::len);
        return format!("Cannot add property {at}, object is not extensible");
    }
    "Cannot assign to read only property 'length' of object '[object Array]'".to_owned()
}

/// `a.pop()` — the last element, removed.
extern "C" fn pop(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(cell) = Value(this).as_slot() else {
            return undefined_of(context);
        };
        // An empty array answers `undefined` and stays empty. Not a special
        // case: the length is written back either way, so a `pop` on an empty
        // array cannot leave the property saying -1.
        // `visible`: um buraco no fim sai como `undefined`, não como o
        // marcador — este é um dos quatro pontos que devolvem o word CRU ao
        // programa sem passar por `get_indexed`.
        // Removed in place, for the reason `push` appends in place: nothing
        // here calls user code, so nothing needs the copy.
        let taken = match context.elements_at_mut(cell) {
            Some(elements) => elements.pop(),
            None => return undefined_of(context),
        };
        let count = context.elements_at(cell).map_or(0, Vec::len);
        super::array::set_length(context, cell, count);
        let taken = taken.unwrap_or_else(|| undefined_of(context));
        super::array::visible(context, taken)
    })
}

/// `a.shift()` — the first element, removed.
extern "C" fn shift(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some((cell, mut elements)) = staged(context, this) else {
            return undefined_of(context);
        };
        if elements.is_empty() {
            return undefined_of(context);
        }
        let taken = super::array::visible(context, elements.remove(0));
        store(context, cell, elements);
        taken
    })
}

/// `a.unshift(…)` — answers the new length.
extern "C" fn unshift(_e: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    with_current(|context| {
        let more = arguments_at(context, 0, [a0, a1, a2, a3]);
        let Some((cell, elements)) = staged(context, this) else {
            return undefined_of(context);
        };
        // The arguments keep their order at the front, which a loop of
        // `insert(0, …)` would reverse — the corner that makes
        // `[3].unshift(1, 2)` produce `[2, 1, 3]`.
        let mut joined = more;
        joined.extend_from_slice(&elements);
        let count = joined.len();
        store(context, cell, joined);
        Value::from_f64(count as f64).bits()
    })
}

/// `a.indexOf(x, from)` — where `x` first is at or after `from`, or -1.
///
/// Strict equality, which is what the language says and why this is not
/// `includes` with a different answer: `[NaN].indexOf(NaN)` is -1 and
/// `[NaN].includes(NaN)` is true. One shared implementation would have to pick
/// one of those, and either choice is wrong half the time.
extern "C" fn index_of(_e: u64, this: u64, search: u64, from: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        // Borrowed: nothing below calls user code, so there is nothing to
        // drop the borrow for — and copying the array was the whole cost of
        // answering a question about it.
        let Some(elements) = borrowed(context, this) else {
            return undefined_of(context);
        };
        let start = forward_from(context, from, elements.len());
        // The position is relative to what was skipped, so it is offset back
        // before it is answered. Without that `a.indexOf(x, 2)` reports where the
        // element is in the TAIL — a number that looks like an index and is one,
        // of a different array.
        let at = elements
            .iter()
            .skip(start)
            .position(|held| {
                crate::value::strict_equals(Value(*held), Value(search), |a, b| {
                    context.same_text(a, b)
                })
            })
            .map(|at| at + start);
        Value::from_f64(at.map_or(-1.0, |at| at as f64)).bits()
    })
}

/// Where a forward search starts, given the second argument.
///
/// Absent is zero and not `ToNumber(undefined)`, which is `NaN` — the difference
/// between searching the whole array and searching none of it. Negative counts
/// from the end, which is `relative`'s whole job and why this is not a clamp
/// written here.
fn forward_from(context: &Context, from: u64, count: usize) -> usize {
    match absent(context, from) {
        true => 0,
        false => relative(Value(from).numeric().unwrap_or(0.0), count),
    }
}

/// `a.includes(x, from)` — `SameValueZero`, so `NaN` finds itself.
extern "C" fn includes(_e: u64, this: u64, search: u64, from: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some((_, elements)) = staged(context, this) else {
            // False rather than `undefined`, because a predicate that answered a
            // non-boolean would make `if (x.includes(y))` on a non-array take
            // the same branch as one that found nothing — and look right.
            return Value::from_bool(false).bits();
        };
        let start = forward_from(context, from, elements.len());
        let found = elements.iter().skip(start).any(|held| {
            // `includes` é o método que ACHA um buraco: `[,1].includes(undefined)`
            // é `true`, porque ele percorre `0..length` em vez das chaves que
            // existem. É o oposto de `indexOf`, que pula — e a diferença entre
            // os dois é justamente esta linha.
            //
            // O `skip(start)` e o buraco são perguntas independentes: onde a
            // busca COMEÇA e o que ela VÊ. `[,1].includes(undefined, 1)` é
            // `false` porque começa depois do buraco, não porque não o vê.
            let held = super::array::visible(context, *held);
            crate::value::same_value_zero(Value(held), Value(search), |a, b| {
                context.same_text(a, b)
            })
        });
        Value::from_bool(found).bits()
    })
}

/// `a.slice(from, to)` — a new array, negative counting from the end.
extern "C" fn slice(_e: u64, this: u64, from: u64, to: u64, _a2: u64, _a3: u64) -> u64 {
    let taken = with_current(|context| {
        // `slice` is GENERIC — the specification defines it over
        // `LengthOfArrayLike(ToObject(this))` rather than over an array — and
        // the oldest idiom in JavaScript is exactly the generic use:
        // `Array.prototype.slice.call(arguments, 1)`. It answered `undefined`
        // for every non-array receiver, which went unnoticed for as long as
        // `arguments` WAS an array; it stopped being one, and the idiom broke.
        //
        // Only the read-only methods can take this fallback. `reverse` and
        // `fill` are generic in the specification too and are NOT given it
        // here: they would have to write the positions back through the
        // property path, and a version that quietly wrote nothing is the hollow
        // surface CLAUDE.md refuses.
        let elements = match staged(context, this) {
            Some((_, elements)) => elements,
            None => array_like(context, this)?,
        };
        let start = relative(Value(from).numeric().unwrap_or(0.0), elements.len());
        let end = if absent(context, to) {
            elements.len()
        } else {
            relative(Value(to).numeric().unwrap_or(0.0), elements.len())
        };
        // Crossed rather than swapped, the same as the string method:
        // `[1,2,3].slice(2, 1)` is empty.
        Some(if start >= end {
            Vec::new()
        } else {
            elements[start..end].to_vec()
        })
    });
    match taken {
        Some(taken) => built(taken),
        None => with_current(|context| undefined_of(context)),
    }
}

/// `a.reverse()` — in place, answering the receiver.
///
/// In place and not a copy, because the language says so and programs rely on
/// it: `b = a.reverse()` leaves `a` reversed too, and a version that copied
/// would be correct at the assignment and wrong everywhere else.
extern "C" fn reverse(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some((cell, mut elements)) = staged(context, this) else {
            return undefined_of(context);
        };
        elements.reverse();
        store(context, cell, elements);
        this
    })
}

/// `a.fill(v, from, to)` — in place, answering the receiver.
extern "C" fn fill(_e: u64, this: u64, value: u64, from: u64, to: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some((cell, mut elements)) = staged(context, this) else {
            return undefined_of(context);
        };
        let start = relative(Value(from).numeric().unwrap_or(0.0), elements.len());
        let end = if absent(context, to) {
            elements.len()
        } else {
            relative(Value(to).numeric().unwrap_or(0.0), elements.len())
        };
        for slot in elements.iter_mut().take(end).skip(start) {
            *slot = value;
        }
        store(context, cell, elements);
        this
    })
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
/// on `Array.prototype.push.call(1)` turns a `TypeError` into a dead process.
/// The elements of an array-LIKE receiver: its `length`, then that many reads.
///
/// For a receiver [`staged`] answers `None` for — one that carries no elements
/// vector, which in this runtime is what "is not an array" means. `arguments`
/// is the receiver this exists for, and the specification's own definition of
/// the generic methods is what it implements: `LengthOfArrayLike` then `Get`
/// per index.
///
/// `None` when there is no `length` to read, which keeps
/// `Array.prototype.slice.call(1)` answering `undefined` rather than an empty
/// array it invented.
fn array_like(context: &mut Context, this: u64) -> Option<Vec<u64>> {
    let cell = Value(this).as_slot()?;
    let key = context.well_known("length");
    let length = super::objects::read_property(context, cell, key)?.numeric()?;
    if !length.is_finite() || length <= 0.0 {
        return Some(Vec::new());
    }
    let count = length as usize;
    let mut found = Vec::with_capacity(count);
    for at in 0..count {
        // The key an index NAMES. Not `objects::key_for`, which maps a key
        // NUMBER back to its key — passing an index to that reads whatever
        // property happens to hold that number, and it answered four nulls
        // before the difference was noticed.
        let key = context.well_known(&at.to_string());
        let value = match super::objects::read_property(context, cell, key) {
            Some(value) => value.bits(),
            // A hole in an array-like reads `undefined`, which is what the
            // specification's `Get` answers for an absent index.
            None => super::objects::undefined_of(context),
        };
        found.push(value);
    }
    Some(found)
}

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
    let array = super::array::array_new(values.len() as i64);
    // Taken out here, before the borrow below, so the rule stays visible:
    // nothing between the take and the store may allocate.
    let values = values.take();
    with_current(|context| {
        if let Some(cell) = Value(array).as_slot() {
            store(context, cell, values);
        }
        array
    })
}

