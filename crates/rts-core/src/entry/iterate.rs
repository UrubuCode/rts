//! Turning something iterable into the array a caller can walk.
//!
//! # Who still asks for the whole thing, and who no longer does
//!
//! `for-of` no longer does, and this note used to be written as though it were
//! the only caller. `emit/foreach.rs` asks a source for its `Symbol.iterator`
//! and STEPS it whenever stepping is observable — which is every source except
//! an array and a string, the two whose primordial iterators answer a copy of
//! something already in hand. So a `break` reaches `return()`, a `Map` mutated
//! by the loop body is seen as it is now, and an endless iterable ends the pass
//! it was told to.
//!
//! What still arrives here is everything that must consume the WHOLE sequence
//! to answer at all: `[...xs]`, `Array.from`, an array pattern over a source
//! that declares no `Symbol.iterator`, and `yield*` over the same. For those,
//! draining is not a divergence — it is the operation.
//!
//! # Why it materialises rather than stepping
//!
//! The specification's iterator is a pair of calls per element: `next()`
//! answering an object with `done` and `value`, allocated per element. A caller
//! that is going to hold every element anyway pays that for nothing, so this
//! answers the elements **as an array** and the allocation happens once.
//!
//! # What that still costs, stated
//!
//! An iterable that is endless cannot be answered this way, and this imposes no
//! cap on the walk — deliberately: a limit would turn a program that hangs into
//! a program that quietly walks part of a sequence, and a wrong answer that runs
//! is worse than one that visibly does not. `[...endless]` does not terminate
//! here, and does not terminate in any other engine either.
//!
//! Side effects in `next()` all happen before the caller sees the first element,
//! which for a spread is what the caller asked for and for a `yield*` over a
//! source with no `Symbol.iterator` is a real divergence — `emit/delegate.rs`
//! names it as the case where nothing is steppable.
//!
//! # Why a string iterates by code POINT
//!
//! `for (const c of "😀")` yields one element, not two, where `"😀".length` is 2
//! and `"😀"[0]` is half a surrogate pair. That difference is the whole reason
//! the language grew `for-of` over strings, so getting it wrong here would make
//! the construct pointless.

use super::with_current;
use crate::text::Str;
use crate::value::Value;

/// The elements of an iterable, as an array.
///
/// An array is answered unchanged in content but **copied**, because the loop
/// walks what it is given and a body that pushes to the original must not walk
/// its own additions forever.
///
/// Anything that is not an array, a string, a typed array, a `Map`, a `Set`, or
/// an object declaring `Symbol.iterator` **raises a `TypeError`**, which is what
/// the language says and what this answered an empty array for until a native
/// could raise at all. An empty answer was the visible failure while a throw
/// could not reach a caller's handler; it is the wrong one now that it can,
/// because `for (const x of {}) {}` running zero times and `[...undefined]`
/// answering `[]` are silent, and a program written against them is correct here
/// and wrong everywhere else.
#[rtse::entry]
pub fn iterate(value: u64) -> u64 {
    // Two shapes, because one of them still has to be turned into values and
    // interning needs the context mutably — which the borrow that read the
    // elements is holding.
    let found = with_current(|context| {
        let Some(cell) = Value(value).as_slot() else {
            return Found::Nothing;
        };
        if let Some(elements) = context.elements_at(cell) {
            // Convertido, não copiado verbatim: espalhar um array esparso
            // produz `undefined` em cada posição ausente — `[...[1,,3]]` tem
            // três elementos e nenhum buraco. Copiar o word do buraco fazia o
            // array NOVO ser esparso, que é o contrário do que o protocolo de
            // iteração faz, e entregava esse word a `Array.from` e a todo o
            // `for`-`of`.
            return Found::Values(
                elements
                    .iter()
                    .map(|held| super::array::visible(context, *held))
                    .collect(),
            );
        }
        // A typed array's elements are a byte range, not a slot vector, so
        // they never show up in `elements_at`. Without this, `for (const x of
        // typedArray)` fell all the way to `protocol`, found no
        // `Symbol.iterator` (typed arrays declare none here) and answered
        // zero elements — a loop that silently ran empty rather than walking
        // the view.
        if let Some(view) = super::buffers::view_of(context, value) {
            return Found::Values(super::buffers::typed::elements(context, &view));
        }
        // Before the text check and before the protocol, because a collection's
        // elements are already held here: asking it for them through two calls
        // per element into a method of its own would be the same answer, slower,
        // and one more thing to keep agreeing with `entries()`.
        if let Some(iterated) = super::collections::iterated(context, cell) {
            return match iterated {
                super::collections::Iterated::Members(values) => Found::Values(values),
                super::collections::Iterated::Pairs(pairs) => Found::Pairs(pairs),
            };
        }
        match context.text_at(cell) {
            Some(text) => Found::Text(code_points(text)),
            None => Found::Nothing,
        }
    });

    let values = match found {
        Found::Values(values) => values,
        // Each pair becomes its own array, which allocates — so it happens here
        // rather than inside the borrow that read the table.
        Found::Pairs(pairs) => pairs
            .into_iter()
            .map(|(key, value)| super::array_proto::built(vec![key, value]))
            .collect(),
        // Neither an array nor a string, so ask the object whether it declares
        // how to be iterated. Outside the borrow above, because every step of
        // the protocol is a call into user code.
        Found::Nothing => protocol(value).unwrap_or_else(|| {
            refuse(value);
            Vec::new()
        }),
        // Interned here, outside the borrow above.
        // ROOTED, and a loop rather than a `collect`: interning a string
        // ALLOCATES, so the strings interned so far are exposed between the
        // steps of the very loop that makes them. See `super::rooted`.
        Found::Text(points) => {
            let mut held = super::rooted::Rooted::new();
            with_current(|context| {
                for units in points {
                    let value = context.intern_value(Str::from_utf16(&units)).bits();
                    held.values().push(value);
                }
            });
            held.take()
        }
    };

    // Rooted again for the allocation below — a second exposure, and a separate
    // one: until the array exists and holds them, these values are named only
    // by a `Vec` on the Rust heap, and `array_new` allocates.
    let values = super::rooted::Rooted::with(values);
    let array = super::array::array_new(values.len() as i64);
    let values = values.take();
    with_current(|context| {
        if let Some(cell) = Value(array).as_slot()
            && let Some(elements) = context.elements_at_mut(cell)
        {
            *elements = values;
        }
        array
    })
}

/// Everything an object's own `Symbol.iterator` yields.
///
/// `None` for an object that declares none, which is what keeps a genuinely
/// non-iterable value distinct in the code from an iterator that legitimately
/// yielded nothing: the first is [`refuse`]'s `TypeError`, the second is an
/// empty `Some`, and a single empty answer for both is what made
/// `for (const x of {}) {}` a loop that ran zero times instead of a program that
/// stopped.
///
/// # Why every step is outside a borrow
///
/// All three are calls into user code — the method, `next`, and the property
/// reads on what it answered. `with_current` holds a `RefCell` borrow for the
/// length of its body and a callee's first act may be to call the runtime, so
/// each read here takes its own borrow and gives it straight back.
fn protocol(value: u64) -> Option<Vec<u64>> {
    let method = iterator_method(value);
    if !callable(method) {
        return None;
    }
    let absent = with_current(|context| super::objects::undefined_of(context));
    let iterator = super::functions::call(method, value, absent, absent, absent, absent);
    // Every call from here on is USER CODE, so every one of them is followed by
    // the question this file used not to ask. A throw leaves `invoke` answering
    // `undefined`, and `undefined` is a value: without asking, `done` reads
    // `undefined`, which is never true, and this loop fills a vector until the
    // process dies. That is not hypothetical — `{ [Symbol.iterator]() { return
    // { next: 3 } } }` spread into an array is what found it, as a test that
    // hung for over an hour instead of passing in 0.05 s.
    //
    // Answering `None` propagates: the throw stays in flight, the entry point
    // above returns, and the compiled call site that started this asks the same
    // question and re-raises. Nothing here handles anything.
    if super::throw::in_flight() {
        return None;
    }
    drained(iterator)
}

/// Everything an ITERATOR yields, driven by its own `next`.
///
/// Split out of [`protocol`] because a caller can arrive holding the iterator
/// rather than the iterable: `GetSetRecord`'s `keys()` answers an iterator
/// DIRECTLY, and the specification drives it with `next` — it never asks the
/// answer for a `Symbol.iterator`. Handing it to [`iterate`] instead refused
/// every hand-written set-like with "the value is not iterable", because an
/// iterator that is not also iterable is exactly what the protocol allows and
/// what a `keys()` written by hand returns.
///
/// Rule 8 at every step, for the reason stated above the loop.
pub(super) fn drained(iterator: u64) -> Option<Vec<u64>> {
    let absent = with_current(|context| super::objects::undefined_of(context));
    let next = member(iterator, "next");
    if !callable(next) {
        // An object that answered `Symbol.iterator` and then handed back
        // something without a callable `next` is not an iterable, and saying so
        // is a `TypeError` rather than an empty result. This is the one place
        // here that RAISES rather than propagating: the failure is this
        // function's own finding, not a callee's.
        //
        // Only after `Symbol.iterator` answered. A value with no such method at
        // all is answered elsewhere — a string and an array reach this file by
        // other paths — so raising on the way past would refuse things that
        // iterate perfectly well.
        super::throw::type_error("the value is not iterable: its iterator has no next()");
        return None;
    }

    let mut produced = Vec::new();
    loop {
        let step = super::functions::call(next, iterator, absent, absent, absent, absent);
        if super::throw::in_flight() {
            return None;
        }
        // An iterator result must be an OBJECT. Without this the reads below
        // answer `undefined` for `done`, `undefined` is never true, and this
        // loop fills a vector until the process dies — which is not
        // hypothetical: `{ next() { return 1 } }` spread into an array HUNG,
        // and the same shape is what rule 8 of the crate's README was written
        // about. A string is refused with the rest: `as_slot` is true for one,
        // so the check is what the language means by Object rather than what
        // the encoding means by reference.
        let object = with_current(|context| {
            Value(step)
                .as_slot()
                .is_some_and(|cell| context.text_at(cell).is_none())
        });
        if !object {
            super::throw::type_error("the iterator answered a result that is not an object");
            return None;
        }
        // `done` is read before `value`, which is the order the specification
        // states — an iterator whose `done` getter has a side effect observes
        // it first, and the other order is a difference nothing would notice
        // until something did.
        if super::primitives::to_boolean(member(step, "done")) {
            return Some(produced);
        }
        produced.push(member(step, "value"));
    }
}

/// Says that a value cannot be iterated, as the `TypeError` the language raises.
///
/// # Why it asks whether something is already in flight
///
/// [`protocol`] answers `None` for three different findings, and only one of
/// them is this one: the value declares no `Symbol.iterator`. The other two —
/// a `Symbol.iterator` or a `next` that threw, and an iterator whose `next` is
/// not callable — already left a throw behind, and raising a second over it
/// would replace the program's own error with this one. That is the `finally`
/// rule in [`super::throw::throw`] used where it does not apply.
///
/// # Why the value is described rather than named
///
/// `[...xs]` has a name at the *call site* and the runtime never sees it — the
/// entry point is handed a word. So the message carries what the value IS,
/// which [`super::text::described`] answers without running user code: an
/// object's `toString` is a call, and reaching back into the program to ask a
/// non-iterable what it would like to be called is not what a diagnostic should
/// do. The alternative was passing the source text down from the emitter, which
/// is a string per spread site in every program for a message almost none of
/// them will print.
fn refuse(value: u64) {
    if super::throw::in_flight() {
        return;
    }
    let described = super::text::described(value).unwrap_or_else(|| "the value".to_owned());
    super::throw::type_error(&format!("{described} is not iterable"));
}

/// `value[Symbol.iterator]`, asked the way the language asks it.
///
/// Apart from [`member`] because the two are not the same question, and reading
/// them alike is what made `[...new Proxy([1, 2], {})]` throw
/// `TypeError: the value is not iterable` where node answers `[1, 2]`. `member`
/// is a DATA read — no proxy, no getter — and that narrowing is right for `next`
/// and `done`, which are read off a step record the protocol just produced. It
/// is wrong here: `GetMethod(obj, @@iterator)` is an ordinary `Get`, so a proxy
/// must be asked and it is the only chance the handler gets to say what the
/// object iterates as.
///
/// The reach was wider than the one construct it was found through. `for`-`of`,
/// spread, `f(...args)`, `yield*`, `new Set(p)` and `Array.from(p)` all arrive
/// here, and every one of them refused a proxy — including a proxy over an
/// object carrying its OWN generator method, which is what rules out
/// "`Array.prototype` was replaced" as the explanation.
///
/// The proxy is asked BEFORE any borrow and only of an object that is one, which
/// is `objects::get_property`'s shape and its reason: a trap is user code whose
/// first act may be to call back into the runtime, and a second borrow in an
/// `extern "C"` frame aborts rather than unwinding.
///
/// What this still does NOT do is run an ACCESSOR `Symbol.iterator` on an
/// ordinary object — `member`'s data read is what answers below, and an object
/// that declares the method through a getter reads `undefined` here. Rare, real,
/// and left for a change that can be measured on its own.
fn iterator_method(value: u64) -> u64 {
    let proxied = with_current(|context| {
        let key = context.well_known(super::symbol::ITERATOR);
        let slot = Value(value).as_slot()?;
        context.proxy_at(slot).map(|_| key)
    });
    if let Some(key) = proxied
        && let Some(answered) = super::proxy::get(value, key)
    {
        return answered;
    }
    member(value, super::symbol::ITERATOR)
}

/// One property of a value, by a name the runtime knows.
///
/// A data read: a getter is not run, which is the same boundary
/// [`super::error::joined`] draws and for a smaller reason — an accessor on
/// `next` or `done` is not something a real iterator has.
pub(super) fn member(value: u64, name: &str) -> u64 {
    with_current(|context| {
        let Some(cell) = Value(value).as_slot() else {
            return super::objects::undefined_of(context);
        };
        let key = context.well_known(name);
        match super::objects::read_property(context, cell, key) {
            Some(found) => found.bits(),
            None => super::objects::undefined_of(context),
        }
    })
}

/// Whether a value can be called at all.
pub(super) fn callable(value: u64) -> bool {
    with_current(|context| {
        Value(value)
            .as_slot()
            .is_some_and(|cell| context.callable_at(cell).is_some())
    })
}

/// What an iterable turned out to be.
enum Found {
    /// Elements already, from an array or a `Set`.
    Values(Vec<u64>),
    /// A `Map`'s entries, each of which still has to become a two-element array.
    Pairs(Vec<(u64, u64)>),
    /// Code points that still have to become strings.
    Text(Vec<Vec<u16>>),
    /// Not something this engine iterates.
    Nothing,
}

/// A string's code points, as the units each is spelled with.
///
/// # Why this is not one element per unit
///
/// Because a surrogate pair is one character and two units. Splitting by unit
/// would make `for (const c of "😀")` run twice and hand the body half a
/// character each time — text that is not well formed and compares equal to
/// nothing. That difference is the whole reason the language grew `for-of` over
/// strings.
fn code_points(text: &Str) -> Vec<Vec<u16>> {
    let units: Vec<u16> = text.units().collect();
    let mut points = Vec::new();
    let mut at = 0;
    while at < units.len() {
        let wide = (0xD800..0xDC00).contains(&units[at])
            && at + 1 < units.len()
            && (0xDC00..0xE000).contains(&units[at + 1]);
        let span = if wide { 2 } else { 1 };
        points.push(units[at..at + span].to_vec());
        at += span;
    }
    points
}

/// Appends one value to an array, and answers the array.
///
/// Its own operation rather than a property write at a computed index: the index
/// is the current length, which the compiler does not know when a spread earlier
/// in the same literal contributed an unknown number of elements.
#[rtse::entry]
pub fn array_append(array: u64, value: u64) -> u64 {
    with_current(|context| {
        if let Some(cell) = Value(array).as_slot()
            && let Some(elements) = context.elements_at_mut(cell)
        {
            elements.push(value);
            let count = elements.len();
            super::array::set_length(context, cell, count);
        }
        array
    })
}

/// Appends everything an iterable yields, and answers the array.
///
/// What `...xs` is, wherever it is written. One operation rather than a loop the
/// compiler emits, because the count is not known while compiling and the loop
/// would be the same three instructions at every spread in the program.
#[rtse::entry]
pub fn array_append_all(array: u64, iterable: u64) -> u64 {
    let produced = iterate(iterable);
    with_current(|context| {
        let (Some(target), Some(source)) = (Value(array).as_slot(), Value(produced).as_slot())
        else {
            return array;
        };
        let Some(more) = context.elements_at(source).cloned() else {
            return array;
        };
        if let Some(elements) = context.elements_at_mut(target) {
            elements.extend(more);
            let count = elements.len();
            super::array::set_length(context, target, count);
        }
        array
    })
}
