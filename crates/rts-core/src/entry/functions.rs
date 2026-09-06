//! Callables: making one, and calling one.
//!
//! # Why a call is an entry point at all
//!
//! The machine has `call_indirect`, and a compiler that knew the callee's
//! address could use it directly. It does not know: a JavaScript callee is a
//! *value*, and finding out whether that value is code — and which code — reads
//! the heap. That is the membership rule, unmodified.
//!
//! There is a second reason, and it is the one that decided the shape. A value
//! that is **not** callable must not be jumped to, and the language says what
//! happens instead: `1()` throws a `TypeError`. Throwing needed the machine's
//! protected regions, and when this was written nothing emitted them, so the
//! check had to live somewhere that could fail without them. Compiled code
//! could not; this could.
//!
//! # That second reason has EXPIRED, and the shape it decided is still here
//!
//! `rts-codegen` emits protected regions today — `emit/protect.rs` opens two
//! and `emit/destructure/array.rs` a third — and a program catches the
//! `TypeError` this function raises and carries on:
//!
//! ```js
//! const n = 5;
//! try { n(); } catch (e) { /* "TypeError: n is not a function" */ }
//! ```
//!
//! So "compiled code cannot fail here" is no longer true, and the sentence
//! above is kept only to say when it stopped being. The same claim is written
//! in `rts-codegen`'s `RuntimeOp::Call`, which is the shape of the problem
//! rather than a coincidence: neither crate may see the other, so neither could
//! notice its half go stale.
//!
//! **The FIRST reason still stands** and is what any replacement has to answer:
//! a callee is a value, and finding out whether it is code reads the heap. What
//! has changed is that the failure path is now expressible, so the read can be
//! a cached guard at the call site with the slow path coming here, instead of
//! every call in every program arriving unconditionally.
//!
//! What that would remove is measured rather than guessed — see
//! `docs/codegen/machine-primitives.md`: the machine's own indirect call costs
//! 1.1 ns and this door costs about 33, of which roughly 2 ns is the
//! class-constructor check below, 8 ns is the three activation stacks, and the
//! rest is the crossing and the resolution.
//!
//! # What a callable is
//!
//! A region cell at a reserved layout, holding two words: where the code is,
//! and the environment it closed over. Not an object with two properties —
//! `code` would then be a key in the registry, readable and **writable** from
//! JavaScript, and a program that stored a number there would name the
//! instruction the next call jumps to.
//!
//! # The fixed arity, stated where it is paid
//!
//! Every compiled function has one shape:
//!
//! ```text
//! extern "C" fn(env, this, a0, a1, a2, a3) -> value
//! ```
//!
//! Four argument slots, missing ones filled with `undefined` by the caller.
//! JavaScript's arity is dynamic and this is not, which is a real restriction
//! and a named one: the compiler refuses a call with more arguments rather than
//! dropping them here, because a call whose fifth argument silently vanished is
//! a wrong program that runs.
//!
//! The alternative — a count and a pointer to a vector of arguments — is what a
//! real engine does and what this becomes. It needs somewhere to put the
//! vector, and a caller-allocated one is a stack slot this compiler does not
//! emit yet. Fixing the arity buys the whole of E5 without it, and the shape
//! above is what changes when that arrives.

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::value::Value;

/// How many argument slots a compiled function has.
///
/// Public because it is a **contract**, not a detail: the compiler emits
/// functions with exactly this many argument parameters and pads calls to it,
/// and the two agreeing is what makes the call below anything other than
/// undefined behaviour. A constant read by one side and remembered by the other
/// is the disagreement this crate keeps naming.
pub const ARGUMENT_SLOTS: usize = 4;

/// The shape every compiled JavaScript function has.
///
/// Written as a type alias so the `transmute` below names it once. Two spellings
/// of this, one at the definition and one at the call, is how an argument comes
/// to be read as the wrong thing — and a wrong signature here is not a wrong
/// answer, it is a jump with a corrupt stack.
type Compiled = extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64;

/// Writes one of a fresh callable's own properties.
///
/// A STORE at the remembered slot when this shape has been built before, and the
/// ordinary [`super::objects::put`] when it has not.
///
/// # Why the store is sound here and not in general
///
/// `put` does five things this cell provably does not need. It asks whether the
/// key refuses a write — a cell allocated three lines ago has no integrity level
/// and no attribute record, so the answer is no. It asks whether growth is
/// refused, for the same reason. It reconciles `length` against an array's
/// elements, and a callable has none. It resolves the shape from the header, and
/// the header is what the template just supplied. And it takes the transition,
/// which is the whole of what the template records.
///
/// None of that is a judgement about which writes look safe: the template only
/// exists because a previous closure of this exact shape went through `put` and
/// arrived here, so the slot being written is the slot `put` chose.
fn write_own(
    context: &mut Context,
    cell: u32,
    key: crate::object::Key,
    value: u64,
    template: Option<super::context::CallableTemplate>,
    at: usize,
) {
    match template.and_then(|template| template.slots[at]) {
        Some(slot) => super::objects::set_slot_value(context, cell, slot, value),
        None => super::objects::put(context, cell, key, value),
    }
}

/// Makes a callable out of a code address and an environment.
///
/// The code address comes from the machine's `FuncAddr`, which is a relocation
/// the destination filled in — so it is a real address by the time this runs,
/// and this never computes one.
#[rtse::entry]
pub fn closure_new(code: i64, environment: u64) -> u64 {
    with_current(|context| {
        // Looked up BEFORE the cell exists, which it was not: the two questions
        // it answers decide which layout the cell should be born at, and asking
        // afterwards meant every closure started at the empty layout and
        // transitioned into the right one through `objects::put`. Measured
        // 2026-08-25 by ablation: those transitions were 78 ns of 218.
        let described = context.described_at(code as u64);
        // A function this table does not describe keeps the `prototype`, which
        // is the safe direction: `rts-napi` and `eval` mint callables the
        // emitter never saw, and refusing `new` on one of those would break a
        // working program where an extra object only costs.
        let has_prototype = described.is_none_or(|(_, _, has_prototype, _)| has_prototype);
        let template_at = usize::from(has_prototype) | (usize::from(described.is_some()) << 1);
        let held_template = context.callable_template(template_at);

        // An ordinary object, because a function IS one: `f.x = 1` works, and
        // `f.prototype` will. What makes it callable is recorded beside the
        // cell, where nothing a program can write reaches it.
        //
        // Born at the layout a closure of this shape ENDS at when one has
        // already been built, so the writes below are stores rather than
        // transitions. The first closure of each shape has no template, starts
        // empty exactly as every closure used to, and records what it arrived at
        // — see `Context::callable_template` for why the template is a recording
        // of that path rather than a second statement of it.
        let ty = match held_template {
            Some(template) => template.ty,
            None => {
                let shape = context.shapes.root();
                context.layout_of(shape).index() as u32
            }
        };
        let cell = super::alloc::alloc_or_die(context, crate::heap::STRIDE, ty);
        context.mark_callable(cell, code as u64, environment);
        let made = Value::from_slot(cell).bits();
        // HELD across everything below, and this is not belt and braces — it is
        // the fix for a measured bug.
        //
        // Between the allocation above and the return, the only thing naming
        // this cell is a Rust local holding a raw `u32` index. `roots::scan_stack`
        // looks for words that are unambiguously ENCODED references, and a bare
        // index is not one — so the closure is invisible to a collection, and
        // the allocation on the next line is one. The symptom was
        // `TypeError: object is not a function` after a few hundred thousand
        // closures: the cell was swept, its `callables` entry went with it, and
        // the value handed back named whatever took the index. Diagnosed by
        // reading the cell at the failing call — type present, callable absent,
        // region full.
        //
        // `external::hold` is the mechanism that already answers "something
        // outside the heap is holding this", built for `rts-napi`; a second
        // one for a scoped hold would be two answers to one question.
        let held = super::external::hold(context, made);
        // Looked up ONCE, here, and read twice below — for the name and arity,
        // and for whether this function may be constructed with. It was two
        // scans of the same table for one address; the second was added when
        // `prototype` stopped being unconditional and merging them was cheaper
        // than repeating the walk.
        // A CONSTRUCTIBLE function gets a `prototype` object, because `new`
        // reads one. An arrow and a method do not get one, and that is the
        // language rather than an optimisation: `(() => {}).prototype` is
        // `undefined`, `({ m() {} }).m.prototype` is `undefined`, and `new`
        // on either is a `TypeError`. This built one for every function there
        // was, so all four answered wrongly — verified against Node on
        // 2026-08-25, which is also where the cost went: `prototype` is one of
        // the two allocations and two of the four `hidden` calls a closure paid
        // for, and two thirds of the functions in ordinary code are arrows or
        // methods.
        //
        // A function this table does not describe keeps the `prototype`, which
        // is the safe direction: `rts-napi` and `eval` mint callables the
        // emitter never saw, and refusing `new` on one of those would break a
        // working program where an extra object only costs.
        //
        // Made here rather than on demand: `F.prototype.m = …` before any
        // `new F()` is the ordinary way to write a method, so it has to exist
        // first. Laziness would need the property-read path to materialise it,
        // which is a second answer to what a callable's own keys are.
        let empty = context.shapes.root();
        let empty_ty = context.layout_of(empty).index() as u32;
        // Which keys this closure ends up owning, in the template's own order —
        // recorded below so a later closure of this shape is born holding them.
        let mut owned: [Option<crate::object::Key>; 3] = [None; 3];
        if let Some(prototype) = has_prototype
            .then(|| super::alloc::alloc_after_collecting(context, crate::heap::STRIDE, empty_ty))
            .flatten()
        {
            // The same hazard one line later, and held for the same reason:
            // `put` may need an overflow block, which allocates, which may
            // collect — and until the write lands, this object is named only by
            // a local of this frame.
            let prototype_value = Value::from_slot(prototype).bits();
            let held_prototype = super::external::hold(context, prototype_value);
            let key = prototype_key(context);
            owned[0] = Some(key);
            write_own(context, cell, key, prototype_value, held_template, 0);
            // Not enumerable, which the language says about all three of a
            // function's own properties. It went unseen for as long as
            // `for`-`in` walked own keys only; the moment it walked the chain,
            // `for (const k in C)` answered `prototype,name,length` before it
            // answered anything the program wrote.
            //
            // And NON-CONFIGURABLE, which `hidden` does not say and which is
            // the half that was wrong: `SetFunctionPrototype` gives it
            // `{ writable: true, enumerable: false, configurable: false }`, so
            // `f.prototype = X` assigns and `delete f.prototype` refuses. This
            // reported `configurable: true`, which is the one of the three a
            // program can act on — `Object.defineProperty(f, "prototype", …)`
            // succeeded where every runtime raises.
            //
            // Stated outright rather than through one of `native`'s three
            // helpers, because it matches none of them: `hidden` is
            // configurable, `pinned` is not writable, and this property is
            // writable and not configurable. A fourth helper for one caller
            // would be a name to look up instead of three words to read.
            if let crate::object::Key::Name(named) = key {
                super::integrity::set_attributes(context, cell, named, super::integrity::Attributes {
                    writable: true,
                    enumerable: false,
                    configurable: false,
                });
            }
            // And the BACK-link, which nothing wrote: `f.prototype.constructor`
            // is `f`, so `new f().constructor === f` and `x.constructor.name`
            // both work. Every engine has it, a great deal of ordinary code
            // reads it — a clone helper asking `obj.constructor` is the common
            // shape — and here `f.prototype` was an object with no properties
            // at all, so the whole family answered `undefined`.
            //
            // `{writable: true, enumerable: false, configurable: true}`, which
            // is `hidden` rather than `introspective`: the specification lets a
            // program replace `constructor`, and a class body that declares a
            // method called `constructor` is doing exactly that.
            let constructor = context.well_known("constructor");
            super::objects::put(context, prototype, constructor, made);
            super::native::hidden(context, prototype, constructor);
            super::external::release(context, held_prototype);
        }
        // `f.name` and `f.length`, which every function has and this wrote
        // neither of: `(function foo(a, b){}).name` read `undefined` and
        // `.length` did too, so a program forwarding, wrapping or introspecting
        // a function saw nothing.
        //
        // Read from the table the host seeds by code ADDRESS, which is the only
        // handle this has on a compiled function — and the same table a stack
        // trace already reads, rather than a second one that would have to be
        // kept in agreement with it. Widening `ClosureNew` to carry them was
        // the alternative: it costs two more operands on every closure ever
        // made, where this costs a lookup only.
        if let Some((name, arity, _, _)) = described {
            let key = context.well_known("name");
            // `key_text_value` and NOT `intern_value`, which despite its name
            // does not intern: its own documentation says it "inserts into the
            // slab and calls the allocator", so a name built here was a fresh
            // CELL per closure — and a cell is what the next collection has to
            // walk, not merely what costs time. The same function's name is the
            // same text every time it is made, so the first closure allocates it
            // and every one after reads the cache.
            //
            // That cache is `key_texts_as_values`, which `roots` already scans
            // (`roots.rs`), so nothing new has to be kept alive. A second table
            // keyed by code address was the alternative and is worse for exactly
            // that reason: it would be a third answer to "which strings does the
            // runtime hold", and one more thing to remember at the day there is
            // a moving collector.
            let text = context.key_value(name);
            owned[1] = Some(key);
            write_own(context, cell, key, text, held_template, 1);
            let length_key = context.well_known("length");
            let count = Value::from_f64(f64::from(arity)).bits();
            owned[2] = Some(length_key);
            write_own(context, cell, length_key, count, held_template, 2);
            // ONE reach into the attribute table for both, and the reach is what
            // costs: it is indexed by cell and grows to reach one it has never
            // held anything for, so two calls paid that growth twice on a cell
            // that is always fresh here. Measured 2026-08-25, the pair was 98 ns
            // of a 514 ns closure.
            //
            // `introspective_many` and NOT `hidden_many`, which is what it was.
            // `SetFunctionName` and `SetFunctionLength` both spell
            // `[[Writable]]: false`, so `fn.name = "x"` stores nothing and the
            // descriptor reports `writable: false` — which is what every runtime
            // reports and what this answered `true` to on every function in the
            // language.
            //
            // A note here used to say the correction was impossible on its own,
            // and it was right: `emit/class.rs` STORED `C.name` after this ran,
            // so a non-writable record made every class definition a
            // `TypeError`. That store is now `accessor::set_function_name`, a
            // define, and the two halves land together for that reason.
            super::native::introspective_many(context, cell, &[key, length_key]);
        }
        // Recorded only when this closure took the long way, and read back out
        // of the SHAPE it actually reached rather than from anything decided
        // here — see `Context::record_callable_template`. A closure built FROM a
        // template is not a witness to anything: it was born at the layout it is
        // being asked to describe.
        if held_template.is_none() {
            context.record_callable_template(template_at, cell, &owned);
        }
        super::external::release(context, held);
        made
    })
}

/// The ceiling on how many arguments an array-like may contribute.
///
/// See [`list_from_array_like`] for why there is one at all.
const MAX_APPLY_ARGUMENTS: usize = 65_535;

/// `CreateListFromArrayLike`, as an argument vector this crate's call paths can
/// read.
///
/// `apply`, `Reflect.apply` and `Reflect.construct` are all defined over an
/// ARRAY-LIKE and not over an array: `f.apply(o, arguments)` and
/// `f.apply(o, { length: 2, 0: "a", 1: "b" })` are both legal, and the vector
/// paths below read `elements_at`, which only a real array has. So every one of
/// them answered zero arguments for a receiver the specification says carries
/// some — `f.apply(o, {length:2,…})` ran `f` with nothing.
///
/// A real array goes through untouched rather than being re-read index by
/// index, because that read is the property path and a program can have put a
/// getter on `Array.prototype[0]`.
///
/// `undefined` and `null` mean "no arguments" for `apply` alone, which is why
/// the caller decides that and this answers `None` for them: `Reflect.apply`
/// makes the same value a `TypeError`.
///
/// The count is CAPPED by [`MAX_APPLY_ARGUMENTS`], and the cap is not arbitrary
/// even though the number is:
/// the specification allows 2^53-1 and every engine refuses far below it, so
/// without one `f.apply(t, {length: 1e9})` is a billion property reads and an
/// allocation to match — a hang rather than a wrong answer, which is the worse
/// of the two failures and the one this corpus has already caught twice.
pub(super) fn list_from_array_like(arguments: u64) -> Option<u64> {
    let ready = with_current(|context| {
        let cell = Value(arguments).as_slot()?;
        context.elements_at(cell).map(|_| arguments)
    });
    if let Some(ready) = ready {
        return Some(ready);
    }
    // Not "has a length": the specification refuses a PRIMITIVE and accepts
    // every object, so `Reflect.apply(f, o, new Set())` is a call with no
    // arguments rather than a `TypeError`, and `f.apply(o, "ab")` is the
    // refusal. `as_slot` is what separates the two here.
    Value(arguments).as_slot()?;
    let key = with_current(|context| context.well_known_text("length"));
    // `ToLength`: the property is read through the ordinary path — a getter on
    // it runs, and a program counts how many times — then coerced, because
    // `{length: "2"}` and `{length: 2.7}` both name two arguments.
    let raw = super::computed::get_indexed(arguments, key);
    if super::throw::in_flight() {
        return Some(super::array_proto::built(Vec::new()));
    }
    let length = super::class_support::to_number(raw);
    let count = match length.is_nan() || length <= 0.0 {
        true => 0usize,
        false => length.trunc().min(MAX_APPLY_ARGUMENTS as f64) as usize,
    };
    let mut found = Vec::with_capacity(count);
    for at in 0..count {
        let key = with_current(|context| context.well_known_text(&at.to_string()));
        found.push(super::computed::get_indexed(arguments, key));
        if super::throw::in_flight() {
            break;
        }
    }
    Some(super::array_proto::built(found))
}

/// Calling with more arguments than the convention carries.
///
/// # Why the vector is the runtime's and not a stack slot
///
/// A real engine hands the callee a count and a pointer to a caller-allocated
/// vector, which needs a stack slot this compiler does not emit — and choosing
/// something else *because* of that would be the language layer working around a
/// missing machine capability, which is the mistake rule 2 names.
///
/// This is not that. **Where the arguments of a running call live is a runtime
/// question**, the same kind as where a string's text lives or where an array's
/// elements do, and this crate is what answers those. The compiler says "call
/// this with these arguments" and never learns where they were put.
///
/// What it costs is named rather than hidden: a `Vec` push and pop per call,
/// because a callee reading its rest must not see an *outer* call's vector.
/// The stack-slot convention removes that, and this is what the language can do
/// correctly until the machine grows one.
#[rtse::entry]
pub fn call_with_args(callee: u64, this: u64, arguments: u64) -> u64 {
    if is_bare_class_constructor_call(callee) {
        super::throw::type_error("Class constructor cannot be invoked without 'new'");
        return with_current(|context| undefined_of(context));
    }
    // An ARRAY-LIKE is materialised into a real array first.
    //
    // `f.apply(t, {0:"x", 1:"y", length: 2})` is ordinary JavaScript and the
    // specification's own `CreateListFromArrayLike` — it reads `length` and then
    // the indices, and never asks whether the object is an Array. This read
    // `context.elements_at` alone, which answers only for a real array, so every
    // array-like passed to `apply` produced a call with NO arguments: the
    // callee saw `undefined` in every position and nothing said why. An
    // `arguments` object is the array-like a program passes most often, and
    // `f.apply(this, arguments)` is the reason `apply` exists at all.
    // `None` is a primitive, which for THIS caller means "no arguments" — the
    // tolerance `list_from_array_like` leaves to whoever calls it, and which
    // `Reflect.apply` spends on a `TypeError` instead.
    let arguments = list_from_array_like(arguments).unwrap_or(arguments);
    let (first, spelled) = with_current(|context| {
        let absent = undefined_of(context);
        let mut first = [absent; ARGUMENT_SLOTS];
        if let Some(cell) = Value(arguments).as_slot()
            && let Some(elements) = context.elements_at(cell)
        {
            for (slot, value) in first.iter_mut().zip(elements.iter()) {
                *slot = *value;
            }
        }
        // The vector this activation reads its rest from. Pushed rather than
        // stored, because a call inside the callee pushes its own.
        context.pending_arguments.push(arguments);
        // Paired with the vector, so a callee's count is its own — see `called`.
        context.pending_counts.push(None);
        // TAKEN here rather than inside `invoke`, which no longer reads it.
        // This door is the only one that still records a name out of band —
        // a call with a vector has no operand to carry one — so it is also the
        // one that has to clear it, or a name written for this call would
        // outlive it and be reported for the next call that has none.
        let spelled = context.pending_call_name.take();
        (first, spelled)
    });
    // Not through `call`, which pushes a marker of its own — that marker on top
    // would hide the vector from exactly the callee it was made for.
    let produced = invoke(
        callee,
        this,
        Spelling::Taken(spelled),
        first[0],
        first[1],
        first[2],
        first[3],
    );
    with_current(|context| {
        context.pending_arguments.pop();
        context.pending_counts.pop();
    });
    produced
}

/// `function f(a, ...rest)` — the arguments past the declared ones.
///
/// # Why the four slots are passed in
///
/// Because most calls do not allocate a vector, and a rest parameter over four
/// or fewer arguments has to work anyway: `f(1, 2, 3)` reaching
/// `function f(a, ...rest)` must see `[2, 3]`. So the callee hands over what it
/// was given, and the vector is consulted only when a caller supplied one.
///
/// Trailing `undefined` is dropped, which is what makes this answer `[]` for
/// `f(1)` rather than three padding values the call site invented.
#[rtse::entry]
pub fn rest_arguments(from: i64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    // ONE borrow, and the array is built through `array::built_in` rather than
    // through the `array_new` entry point.
    //
    // It was three crossings and three vectors: one borrow to collect, then
    // `array_new` — an entry point, so a borrow of its own, and it fills a
    // vector with holes — and a third borrow to throw that vector away and put
    // the collected one in its place.
    //
    // Measured: a function with a rest parameter cost 430 ns to call with ZERO
    // arguments, against 40 for the same call with fixed parameters. The count
    // barely moved it — three arguments cost 465 — so what was being paid was
    // the machinery and not the copying.
    with_current(|context| {
        let collected = collected(context, from, a0, a1, a2, a3);
        super::array::built_in(context, collected)
    })
}

/// What the running call really passed, from position `from` on.
///
/// Shared with [`super::arguments::arguments_object`], which builds an
/// array-LIKE object out of the same values: where the arguments of a running
/// call live is one question with one answer, and only what is built out of
/// them differs. Split out of `rest_arguments` when the second caller arrived
/// rather than copied, because a second copy of this reconstruction is a second
/// opinion about whether `f(undefined)` passed one argument or none.
pub(in crate::entry) fn collected(
    context: &Context,
    from: i64,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
) -> Vec<u64> {
    let absent = undefined_of(context);
    let from = from.max(0) as usize;
    match context.pending_arguments.last().copied() {
            Some(vector) if Value(vector).as_slot().is_some() => {
                let cell = Value(vector).as_slot().expect("just checked");
                match context.elements_at(cell) {
                    Some(elements) => elements.iter().skip(from).copied().collect::<Vec<u64>>(),
                    None => Vec::new(),
                }
            }
            // No vector, so the arguments are exactly what the convention
            // carried — and how many that is comes from the call SITE when it
            // said, which is what makes `f(undefined)` reach `...rest` as one
            // argument instead of none. Trailing padding is dropped only when
            // nobody said, which is a native calling another function.
            _ => {
                let given = [a0, a1, a2, a3];
                let real = match context.pending_counts.last().copied().flatten() {
                    Some(count) => count.min(given.len()),
                    None => {
                        let mut real = given.len();
                        while real > 0 && given[real - 1] == absent {
                            real -= 1;
                        }
                        real
                    }
                };
                given[from.min(real)..real].to_vec()
            }
        }
}

/// A call, told HOW MANY arguments the site wrote.
///
/// # Why this exists beside [`call`]
///
/// Because only a call SITE knows. The convention pads the four slots with
/// `undefined`, so a callee cannot tell `f(undefined)` from `f()` — and the
/// runtime was reconstructing the count by dropping `undefined` from the end,
/// which is exactly wrong for the program that passed one on purpose:
/// `console.log(undefined)` printed an empty line and `[].push(undefined)`
/// pushed nothing.
///
/// Compiled code reaches this one; a native calling another function reaches
/// [`call`], which pushes "unknown" rather than a number it does not have. Two
/// doors, one implementation — the alternative was widening `call` itself, and
/// that would have made every native in this crate declare a count it has no
/// way to know.
/// `name` is WHICH literal spells the callee, or [`NO_CALL_NAME`] for a callee
/// with nothing to name — the operand that replaced [`set_call_name`], a whole
/// crossing this door used to be preceded by. Measured 2026-08-28, minimum of
/// four alternations per binary: **2.3 to 2.9 ns on every named call**, against
/// a static-call control that did not move. It is read into
/// `pending_call_name` inside the borrow `called` already takes, so what is
/// left of it is a bounds check and a load rather than a jump.
#[rtse::entry]
pub fn call_counted(
    callee: u64,
    this: u64,
    count: i64,
    name: i64,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
) -> u64 {
    let counted = count.clamp(0, ARGUMENT_SLOTS as i64) as usize;
    called(callee, this, Some(counted), name, a0, a1, a2, a3)
}

/// A callee with nothing to name, as the `name` operand spells it.
///
/// Not an `Option` because the operand is an `i64` in the calling convention
/// and the runtime reads its literal table by index — so "no name" has to be a
/// number, and the one number that cannot be an index is the honest choice.
pub const NO_CALL_NAME: i64 = -1;

/// How the callee about to be jumped to was spelled, as far as the caller
/// knows — and *unresolved*, which is the whole point.
///
/// The name is used by exactly one branch of [`invoke`]: the one where the
/// callee turned out not to be callable and a `TypeError` has to say which
/// spelling failed. Every other call — that is, every call a working program
/// makes — needs the name never to be looked at. So this carries the cheapest
/// thing each door has and defers the reading to [`Self::resolve`].
enum Spelling {
    /// Nothing to name: a native calling another function has no call site,
    /// and a `[Symbol.iterator]()` the compiler wrote has no source text.
    None,
    /// WHICH literal, straight off the call's operand. Costs a register.
    Literal(i64),
    /// Already taken out of `pending_call_name` by the door that set it.
    ///
    /// [`call_with_args`] is that door: its site still records the name
    /// through [`set_call_name`], because a call with a vector has no operand
    /// to put it on. Taking it THERE rather than here is what keeps a name
    /// from outliving the call it was written for — a later call that has
    /// nothing to name must not inherit it.
    Taken(Option<u64>),
}

impl Spelling {
    /// The text, read only where it is about to be printed.
    fn resolve(self, context: &Context) -> Option<u64> {
        match self {
            Self::None => None,
            Self::Literal(NO_CALL_NAME) => None,
            Self::Literal(which) => context.literals.get(which as usize).copied(),
            Self::Taken(name) => name,
        }
    }
}

/// Calls a value, with a receiver and up to [`ARGUMENT_SLOTS`] arguments.
///
/// **Throws a `TypeError` for a callee that is not callable**, which a program
/// can catch. This paragraph said the opposite — "answers `undefined` … a
/// stated gap … throwing needs protected regions and nothing emits those yet"
/// — and both halves had stopped being true: `invoke` raises, and
/// `emit/protect.rs` emits the regions. `const n = 5; try { n(); } catch (e)`
/// catches it and carries on.
///
/// It is still worth naming here rather than leaving implicit, for the reason
/// the old text gave and which does survive: of everything that can fail in
/// this crate, *this* is the one that would otherwise be a jump to an arbitrary
/// address.
///
/// The count is `None` here, and honestly so: this door is for a NATIVE calling
/// another function, and a native has no call site to read one from.
#[rtse::entry]
pub fn call(callee: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    called(callee, this, None, NO_CALL_NAME, a0, a1, a2, a3)
}

/// What both doors do, with the count either known or not.
fn called(
    callee: u64,
    this: u64,
    count: Option<usize>,
    name: i64,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
) -> u64 {
    // Read what is needed, then LET GO of the context before jumping.
    //
    // Not a tidiness: `with_current` holds a `RefCell` borrow for as long as its
    // body runs, and the callee is compiled code whose very first act may be to
    // call `__rts_add`. Calling from inside the borrow panics on the re-entry —
    // a deadlock this repository has already paid for once, in a different
    // shard, and the reason this function is written as two statements rather
    // than one expression.
    // No vector for this activation, and saying so is not optional: a callee
    // reading its rest must not find the vector of an OUTER call that is still
    // running. One push and one pop per call is what that costs, and
    // `call_with_args` names what removes it.
    // A class constructor called without `new` is a `TypeError`, and it is
    // decided in the borrow this call already takes rather than in one of its
    // own. It was a separate `with_current` over a separate side table, and
    // ablating it entirely priced the pair at about 2 ns on every call in
    // every program — see `Context::is_class_constructor`.
    //
    // Answered before anything is pushed, so a refused call leaves the stacks
    // exactly as it found them, and still before the jump, so `1()` and
    // `ClassCtor()` do not share a path that decides this afterwards.
    let refused = with_current(|context| {
        if Value(callee)
            .as_slot()
            .is_some_and(|cell| context.is_class_constructor(cell))
        {
            return true;
        }
        let absent = undefined_of(context);
        context.pending_arguments.push(absent);
        // Pushed by EVERY call for the same reason the vector is: a callee
        // asking how many arguments it got must not find the answer belonging
        // to an outer call that is still running. `None` says "nobody told
        // me", which is what a native calling another function can say
        // honestly — and what makes `arguments_at` fall back to the old guess
        // there instead of inventing a number.
        context.pending_counts.push(count);
        false
    });
    if refused {
        super::throw::type_error("Class constructor cannot be invoked without 'new'");
        return with_current(|context| undefined_of(context));
    }
    // The spelling travels as a NUMBER and is not resolved here. Reading the
    // literal table on the way in would put a bounds check and a load on every
    // call, to answer a question only a call that turns out not to be callable
    // ever asks — so `invoke` resolves it in the branch that raises, and the
    // successful call pays a register.
    let produced = invoke(callee, this, Spelling::Literal(name), a0, a1, a2, a3);
    with_current(|context| {
        context.pending_arguments.pop();
        context.pending_counts.pop();
    });
    produced
}

/// The jump itself, with no argument vector of its own.
///
/// Split from [`call`] because `call_with_args` has already pushed the vector
/// this activation reads, and `call`'s marker on top of it would hide that
/// vector from the one callee it was made for.
fn invoke(
    callee: u64,
    this: u64,
    spelling: Spelling,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
) -> u64 {
    let found = with_current(|context| {
        // Which callable is about to run, recorded before the jump. A compiled
        // function never asks; a native does, because it closes over nothing —
        // so a bound function's only way to know WHICH binding it is comes from
        // here. See [`super::function_proto`].
        context.callees.push(callee);
        resolve(context, callee)
    });

    let Some((code, environment)) = found else {
        // Resolved HERE and nowhere earlier: this is the only branch that
        // needs the name, and it is the branch a working program never takes.
        let name = with_current(|context| spelling.resolve(context));
        with_current(|context| context.callees.pop());
        // A proxy has no code address — it must not have one, since an address
        // is the one thing a program may never choose — so calling one arrives
        // HERE, where the jump did not happen, rather than at a check before
        // every jump that every ordinary call would pay for.
        if let Some(answered) = super::proxy::apply(callee, this, [a0, a1, a2, a3]) {
            return answered;
        }
        // Calling something that is not a function is a `TypeError`, and the
        // program can now catch it. This answered `undefined` silently until
        // every native that calls user code learned to ask whether a throw was
        // left behind — raising before that turned one silent wrong answer into
        // a hang, which is why the two changes are one change.
        // Named where a name is known — `obj.foo is not a function`, as Node
        // reports it — and falling back to the KIND where it is not: a
        // computed callee such as `(a || b)()` has no single spelling, and
        // "not a function" alone does not say which mistake it was either —
        // a method this engine does not have reads `undefined`, and a name
        // shadowed by data reads whatever the data is. 91 files died on this
        // line before it said which, and a name beats a kind where both are
        // available because it says WHERE to look, not just what was found.
        let message = match name.and_then(|value| super::text::described(value)) {
            Some(name) => format!("{name} is not a function"),
            None => {
                let kind = super::text::described(super::type_of::type_of(callee))
                    .unwrap_or_else(|| "a value".to_owned());
                format!("{kind} is not a function")
            }
        };
        super::throw::type_error(&message);
        return with_current(|context| undefined_of(context));
    };

    // SAFETY: `code` was written by `closure_new` from a `FuncAddr`, which the
    // machine emits only for a function this compilation declared, and every
    // compiled JavaScript function is placed with the signature `Compiled`
    // spells. The cell it came from is at the closure layout, which nothing
    // else allocates and no JavaScript can write to — that check is what
    // `resolve` is for, and it is why the address cannot be a value a program
    // chose.
    let entry: Compiled = unsafe { std::mem::transmute::<u64, Compiled>(code) };
    let produced = entry(environment, this, a0, a1, a2, a3);
    with_current(|context| context.callees.pop());
    produced
}

/// Records how the callee about to be called was spelled, by the literal
/// index `rts-codegen` resolved for its text — `"obj.foo"` or `"foo"`.
///
/// Emitted immediately before the jump, once every argument is already
/// evaluated, so evaluating an argument that itself calls something cannot
/// overwrite what this call site recorded for itself. [`invoke`] takes what
/// is here rather than reading it, which is what keeps a nested call from
/// inheriting a name it never set.
#[rtse::entry]
pub fn set_call_name(literal: i64) -> u64 {
    with_current(|context| {
        context.pending_call_name = context.literals.get(literal as usize).copied();
        undefined_of(context)
    })
}

/// The code and environment of a value, when it is genuinely a callable.
///
/// The type check is the whole safety argument for the `transmute` above, which
/// is why it reads the cell's header rather than trusting the tag: a reference
/// says "a cell", not "a callable", and every object in the region answers the
/// same tag.
fn resolve(context: &mut Context, callee: u64) -> Option<(u64, u64)> {
    let slot = Value(callee).as_slot()?;
    context.callable_at(slot)
}

/// The key `prototype` has.
fn prototype_key(context: &mut Context) -> crate::object::Key {
    context.well_known("prototype")
}

/// `new f(…)`.
///
/// # What the operation actually is
///
/// Three steps the language keeps separate and a caller cannot: make an object
/// whose prototype is the callee's `prototype` property, run the callee with
/// that object as `this`, and answer the object — **unless** the callee
/// returned one of its own, in which case that wins.
///
/// The last clause is the one an implementation forgets. `function F() {
/// return {a: 1}; }` produces the returned object and not the fresh one, and a
/// factory written that way is ordinary JavaScript rather than a corner.
///
/// # Why the fresh object is made here and not by the compiler
///
/// Because its prototype comes from a value — `f.prototype` — that only exists
/// while running. A compiler could emit the allocation, and then it would have
/// to emit the property read and the link, which is three entry points where
/// this is one.
/// # Why a derived constructor is not handed an object
///
/// Because the object is not its to make. `class B extends A {}` builds an
/// instance by asking `A`, and only the **base** of a chain knows what kind of
/// object to allocate — `class Mine extends RegExp {}` needs the one the
/// regular-expression constructor makes, with its compiled pattern beside the
/// cell, and no amount of allocating a plain object here produces that.
///
/// So a derived callee is run with no receiver at all, and its `super()` is what
/// produces one. That is the specification's own shape: `this` does not exist in
/// a derived constructor until `super()` returns, which is why reading it before
/// that is a `ReferenceError` rather than `undefined`.
///
/// # Why `new.target` is a stack rather than an argument
///
/// The object a base constructor allocates must inherit from the prototype of
/// the class `new` actually named — `new B()` produces something whose chain
/// starts at `B.prototype`, even though `A` is what allocates it. That fact has
/// to survive an arbitrary number of `super()` calls between the two, and the
/// calling convention has no slot left to carry it.
///
/// A stack in the context rather than a field, because construction nests:
/// `new B(new C())` has `C` finished before `B` allocates.
#[rtse::entry]
pub fn construct(callee: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    // No vector for this construction, said before the callee runs — the same
    // isolation `call` establishes, and for the same reason.
    with_current(|context| {
        let absent = undefined_of(context);
        context.pending_arguments.push(absent);
        // Paired with the vector, so a callee's count is its own — see `called`.
        context.pending_counts.push(None);
    });
    let produced = construct_inner(callee, a0, a1, a2, a3);
    with_current(|context| {
        context.pending_arguments.pop();
        context.pending_counts.pop();
    });
    produced
}

/// The construction itself, with no argument vector of its own.
///
/// Split from [`construct`] for the reason [`invoke`] is split from [`call`]:
/// `construct_with_args` has already pushed the vector this construction reads.
fn construct_inner(callee: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    // Callable but NOT constructible, which is a different refusal from "not
    // callable" and needs its own: an arrow, a method and an `async function`
    // are all perfectly good functions that `new` may not reach. This answered
    // an object instead — `new (() => {})()` produced one and carried on, where
    // Node ends the program with `TypeError: … is not a constructor`, so a
    // program whose control flow depended on that catch took the wrong branch
    // in silence. Measured against Node 25.9 on 2026-08-25.
    //
    // The flag is the one the emitter already records and `closure_new` already
    // reads to decide the `prototype` — asked here through the same index, so
    // "may `new` reach this" has one answer rather than two that can disagree.
    // A callable the table does not describe is allowed through, exactly as it
    // is allowed to keep its `prototype`: `rts-napi` and `eval` mint callables
    // the emitter never saw, and refusing those would break working programs.
    let refused = with_current(|context| {
        let cell = Value(callee).as_slot()?;
        let (code, _) = context.callable_at(cell)?;
        // A class constructor is marked separately and is constructible by
        // definition, whatever the emitter recorded about the function it was
        // written as.
        if context.is_class_constructor(cell) {
            return None;
        }
        let (name, _, _, constructs) = context.described_at(code)?;
        if constructs {
            return None;
        }
        let spelled = context
            .interner
            .text(name)
            .and_then(|text| text.to_rust())
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| "anonymous".to_owned());
        Some(format!("{spelled} is not a constructor"))
    });
    if let Some(message) = refused {
        super::throw::type_error(&message);
        return with_current(|context| undefined_of(context));
    }

    let derived = with_current(|context| {
        let Some(cell) = Value(callee).as_slot() else {
            return None;
        };
        context.callable_at(cell)?;
        // The depth is `callees.len()` BEFORE `invoke` pushes this callee, so
        // the activation about to start is the one that matches. See the field
        // on `Context` for why an activation and not merely a target.
        let depth = context.callees.len();
        context.new_targets.push((callee, depth));
        Some(context.is_derived(cell))
    });

    let Some(derived) = derived else {
        if let Some(answered) = super::proxy::construct(callee, [a0, a1, a2, a3]) {
            return answered;
        }
        // Not callable. `new 1` is a `TypeError`, and throwing needs protected
        // regions — the same stated gap calling has.
        return with_current(|context| undefined_of(context));
    };

    let this = match derived {
        // Its `super()` makes one, using the target this call just pushed.
        true => with_current(|context| undefined_of(context)),
        false => match allocate_for_target(callee) {
            Some(fresh) => fresh,
            None => {
                with_current(|context| context.new_targets.pop());
                return with_current(|context| undefined_of(context));
            }
        },
    };

    let produced = invoke(callee, this, Spelling::None, a0, a1, a2, a3);
    with_current(|context| context.new_targets.pop());

    // A constructor that returned an object produced THAT. Anything else — a
    // number, `undefined`, the usual — leaves the fresh object as the answer.
    // For a derived one there is no fresh object, so what came back IS the
    // answer: the compiler makes such a constructor return its `this`.
    //
    // `is_object_in` and not "is it a cell": a STRING is a cell here, so
    // `function F() { return "new"; }` reached through `new` answered the
    // string and threw the fresh object away — a primitive winning over the
    // object, which is the one clause the specification is explicit about.
    if with_current(|context| super::primitive::is_object_in(context, produced)) {
        produced
    } else {
        this
    }
}

/// `new.target` — the constructor `new` named, for the activation ASKING.
///
/// # Why the answer is not simply the top of the stack
///
/// Because the stack is per CONSTRUCTION and the question is per ACTIVATION.
/// A construction in progress leaves its target on the stack for as long as the
/// constructor runs, so an ordinary function called from inside a constructor
/// would read it and answer the class as its own — where the language says
/// `undefined`, because that function was called rather than constructed.
///
/// So the target carries the activation depth it was pushed for, and this
/// answers it only when that depth is the one running. [`invoke`] pushes the
/// callee onto `callees` after `construct_inner` pushed the target, which is
/// why the comparison is `depth + 1` and not `depth`.
///
/// Answering `undefined` everywhere would make more programs than not print the
/// right thing — a function is called far more often than constructed — and it
/// is refused for exactly that reason: it is a wrong answer that reads like a
/// right one wherever the value is discarded.
#[rtse::entry]
pub fn new_target() -> u64 {
    with_current(|context| {
        let running = context.callees.len();
        match context.new_targets.last() {
            Some((target, depth)) if depth + 1 == running => *target,
            _ => undefined_of(context),
        }
    })
}

/// `super(…)` — the parent constructor, producing the object.
///
/// Not a call with a receiver: the parent may be the base of the chain, in which
/// case it allocates, and it may be derived itself, in which case its own
/// `super()` does. Either way the object inherits from the prototype of the
/// class `new` named, which is what the target stack carries.
#[rtse::entry]
pub fn super_construct(parent: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    super_construct_inner(parent, a0, a1, a2, a3)
}

/// The body [`super_construct`] and [`super_construct_with_args`] share.
///
/// Split out so the vector-shaped form does not restate what "produce the
/// object without pushing a `new.target`" means — a second copy of that is
/// where the two would come to disagree about which prototype `super()`
/// builds against.
fn super_construct_inner(parent: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let derived = with_current(|context| {
        let cell = Value(parent).as_slot()?;
        context.callable_at(cell)?;
        // The SAME target the chain already has, re-pushed at the parent
        // constructor's own depth. Not the parent: the object the whole chain
        // builds has to keep inheriting from the prototype of the class `new`
        // actually named, and pushing `parent` here is exactly the bug that
        // makes `new B()` produce something inheriting from `A.prototype`.
        //
        // Re-pushed rather than left alone because `new.target` is per
        // ACTIVATION: the parent constructor's body is a new activation, and
        // the entry only answers for the one the target was pushed for. This
        // is what makes `new.target` SURVIVE `super()`, which the language
        // requires — a base constructor reached through `super()` sees the
        // derived class, not `undefined`.
        let carried = context
            .new_targets
            .last()
            .map(|(target, _)| *target)
            .unwrap_or(parent);
        let depth = context.callees.len();
        context.new_targets.push((carried, depth));
        Some(context.is_derived(cell))
    });
    let Some(derived) = derived else {
        return with_current(|context| undefined_of(context));
    };

    let this = match derived {
        true => with_current(|context| undefined_of(context)),
        false => match allocate_for_target(parent) {
            Some(fresh) => fresh,
            None => {
                with_current(|context| context.new_targets.pop());
                return with_current(|context| undefined_of(context));
            }
        },
    };

    let produced = invoke(parent, this, Spelling::None, a0, a1, a2, a3);
    with_current(|context| context.new_targets.pop());
    // The same question `construct_inner` asks, and for the same reason: a
    // parent constructor that returns a string must not have it become the
    // object the chain is building.
    if with_current(|context| super::primitive::is_object_in(context, produced)) {
        produced
    } else {
        this
    }
}

/// `super(...args)`, or `super(…)` with more than four arguments — the parent
/// constructor, over an arbitrary-length vector, producing the object.
///
/// Vector-shaped like [`construct_with_args`], `new.target`-inert like
/// [`super_construct`]: nothing already here was both, which is why this
/// exists rather than routing through `construct_with_args` — that one PUSHES
/// a `new.target`, and `super()` must never establish one of its own. The
/// vector is pushed onto `pending_arguments` for the same reason
/// `construct_with_args` pushes it: a rest parameter inside the parent
/// constructor must see THIS vector, not an outer call's.
#[rtse::entry]
pub fn super_construct_with_args(parent: u64, arguments: u64) -> u64 {
    let first = with_current(|context| {
        let absent = undefined_of(context);
        let mut first = [absent; ARGUMENT_SLOTS];
        if let Some(cell) = Value(arguments).as_slot()
            && let Some(elements) = context.elements_at(cell)
        {
            for (slot, value) in first.iter_mut().zip(elements.iter()) {
                *slot = *value;
            }
        }
        context.pending_arguments.push(arguments);
        // Paired with the vector, so a callee's count is its own — see `called`.
        context.pending_counts.push(None);
        first
    });
    let produced = super_construct_inner(parent, first[0], first[1], first[2], first[3]);
    with_current(|context| {
        context.pending_arguments.pop();
        context.pending_counts.pop();
    });
    produced
}

/// `new f(…)` with more arguments than the convention carries.
///
/// The same trade the call side makes, and it has to be its own entry point for
/// the same reason `construct` is not `call` with a flag: it makes the receiver
/// rather than taking one, and it answers the object rather than what the callee
/// returned. See [`call_with_args`] for why the vector is the runtime's.
#[rtse::entry]
pub fn construct_with_args(callee: u64, arguments: u64) -> u64 {
    let first = with_current(|context| {
        let absent = undefined_of(context);
        let mut first = [absent; ARGUMENT_SLOTS];
        if let Some(cell) = Value(arguments).as_slot()
            && let Some(elements) = context.elements_at(cell)
        {
            for (slot, value) in first.iter_mut().zip(elements.iter()) {
                *slot = *value;
            }
        }
        context.pending_arguments.push(arguments);
        // Paired with the vector, so a callee's count is its own — see `called`.
        context.pending_counts.push(None);
        first
    });
    // Not through `construct`, which pushes a marker of its own — that marker
    // on top would hide the vector from the constructor it was made for.
    let produced = construct_inner(callee, first[0], first[1], first[2], first[3]);
    with_current(|context| {
        context.pending_arguments.pop();
        context.pending_counts.pop();
    });
    produced
}

/// An object inheriting from the prototype of the class `new` named.
///
/// Falls back to the callee's own `prototype` when there is no target, which is
/// a constructor reached some way other than `new` — a call, today, and nothing
/// else once every path is emitted.
fn allocate_for_target(callee: u64) -> Option<u64> {
    with_current(|context| {
        let target = context
            .new_targets
            .last()
            .map(|(target, _)| *target)
            .unwrap_or(callee);
        let cell = Value(target).as_slot().or_else(|| Value(callee).as_slot())?;
        // A `.bind()`-produced function has no OWN `prototype` — the language
        // never gives one a property by that name — so reading it here used
        // to answer `None` and `allocate_for_target` answered `None` right
        // behind it: `new boundFn()` and `Reflect.construct(boundFn, [])`
        // both silently produced `undefined` instead of delegating to the
        // function underneath. `[[Construct]]` on a bound function is defined
        // as `[[Construct]]` on its TARGET, so the prototype it allocates
        // against is the target's — walked through, because a bound function
        // may itself bind another one.
        // The walk KEEPS what it found. It used to answer only "stop here" and
        // then read the property a second time, so every `new` in a program did
        // the whole lookup twice — a key resolution and a property read that
        // walks the prototype chain, for an answer it had already had. Measured
        // 2026-08-11: `new C()` on a class with no fields at all cost 597 ns,
        // which is where looking for the cost of construction led.
        let key = prototype_key(context);
        let mut resolved = cell;
        let prototype = loop {
            if let Some(found) = super::objects::read_property(context, resolved, key) {
                break found;
            }
            // A `.bind()`-produced function has no OWN `prototype`, so the one
            // to allocate against is its target's — walked through, because a
            // bound function may itself bind another one.
            let next = context
                .bound_at(resolved)
                .and_then(|bound| Value(bound.target).as_slot())?;
            resolved = next;
        };

        let shape = context.shapes.root();
        // Typed by WHAT IT WILL INHERIT FROM, before it exists. Two classes
        // whose instances hold the same fields reach the same shape — that is
        // what a shape tree is for — so without this a site warmed on one
        // recognises the other and reads its methods. The prototype is already
        // in hand from the walk above, so this costs a scan of a list of one.
        let ty = context.typed_as(shape, Some(prototype.bits())).index() as u32;
        let fresh = super::alloc::alloc_after_collecting(context, crate::heap::STRIDE, ty)?;
        context.set_prototype(fresh, prototype.bits());
        Some(Value::from_slot(fresh).bits())
    })
}

/// What an object made by a **native** constructor should inherit from.
///
/// # Why a built-in has to ask
///
/// A native constructor makes its own object — that is the whole reason a
/// derived class has to ask its parent for one — and it would otherwise link it
/// to its own prototype. Then `class Mine extends RegExp { own() {} }` produces
/// something with no `own`, because the object never reached `Mine.prototype`.
///
/// So the same question `allocate_for_target` answers is asked here, and the
/// fallback is what the built-in would have chosen: a construction that is not
/// in progress — `RegExp("a")` without `new` — has no target to consult.
pub(super) fn prototype_for_new(context: &mut Context, fallback: u64) -> u64 {
    let Some(target) = context.new_targets.last().map(|(target, _)| *target) else {
        return fallback;
    };
    let Some(cell) = Value(target).as_slot() else {
        return fallback;
    };
    let key = prototype_key(context);
    match super::objects::read_property(context, cell, key) {
        Some(prototype) => prototype.bits(),
        None => fallback,
    }
}

/// Records that a constructor must ask its parent for the object.
///
/// Written by the class lowering at definition time, because whether a class has
/// an `extends` is a syntactic fact the compiler knows and the runtime cannot
/// see — a derived constructor and a plain function are the same kind of cell.
#[rtse::entry]
pub fn mark_derived(callee: u64) -> u64 {
    with_current(|context| {
        if let Some(cell) = Value(callee).as_slot() {
            context.mark_derived(cell);
        }
        callee
    })
}

/// Records that a callable is a class constructor.
///
/// Written by the class lowering at definition time, for the same reason
/// [`mark_derived`] is: whether a callable came from a `class` declaration is
/// syntax the compiler knows and the runtime cannot see.
#[rtse::entry]
pub fn mark_class_constructor(callee: u64) -> u64 {
    with_current(|context| {
        if let Some(cell) = Value(callee).as_slot() {
            context.mark_class_constructor(cell);
            // A CLASS's `prototype` is non-writable, where an ordinary
            // function's is writable — the one attribute the two differ in, and
            // the reason `class C {}` refuses `C.prototype = X` while
            // `function f() {}` accepts `f.prototype = X`.
            //
            // Applied here rather than in `closure_new`, because that is where
            // the property is written and it does not know yet: whether a
            // callable came from a `class` is syntax the compiler holds, and
            // this entry point is how it says so.
            let key = context.well_known("prototype");
            if let crate::object::Key::Name(named) = key {
                super::integrity::set_attributes(context, cell, named, super::integrity::Attributes {
                    writable: false,
                    enumerable: false,
                    configurable: false,
                });
            }
        }
        callee
    })
}

/// Whether a callee is a class constructor being reached some way other than
/// `new` — the one case [`call`] and [`call_with_args`] must refuse rather
/// than run.
fn is_bare_class_constructor_call(callee: u64) -> bool {
    with_current(|context| {
        let Some(cell) = Value(callee).as_slot() else {
            return false;
        };
        context.is_class_constructor(cell)
    })
}

/// `value instanceof callee`.
///
/// Walks what `value` inherits from, looking for the callee's `prototype`
/// **object** — not the callee. `x instanceof F` is false for an `x` whose
/// chain never reaches `F.prototype`, and true for one that reaches it however
/// many links away, which is why this is a loop rather than one comparison.
///
/// # The three refusals, and why answering `false` for them was worse
///
/// `InstanceofOperator` throws before it decides anything: a right-hand side
/// that is not an object, a `Symbol.hasInstance` that is present and not
/// callable, and — once the hook is out of the way — a right-hand side that is
/// not callable at all. `OrdinaryHasInstance` adds a fourth, for a `prototype`
/// property that is not an object.
///
/// All four answered `false`, and `false` is the one answer that cannot be told
/// from a legitimate one. `x instanceof someObject` is a typo — the author meant
/// a constructor — and it read as "no, x is not one of those" in every program
/// that made it, including the ones testing for the mistake.
#[rtse::entry]
pub fn instance_of(value: u64, callee: u64) -> bool {
    // Step 1, before anything else can run: the right-hand side is an OBJECT or
    // the operator refuses. Asked ahead of the hook because the hook is a
    // property of it, and a number has none to read.
    if !with_current(|context| super::objects::is_object(context, callee)) {
        super::throw::type_error("Right-hand side of 'instanceof' is not an object");
        return false;
    }
    // `Symbol.hasInstance` primeiro, que e o passo 1 do operador: uma classe que
    // o define decide ela propria o que e uma instancia dela, e sem isto a
    // decisao era sempre da cadeia de prototipos.
    //
    // # Why it is PROBED here and only sometimes read through `get_indexed`
    //
    // Because the ask is a MISS on essentially every execution — almost no class
    // defines `Symbol.hasInstance` — and a miss through the entry point is what
    // the whole operator costs. Measured 2026-08-29, `target/release/rts.exe`:
    // `d instanceof A` is 119 ns and a single absent-property read on the same
    // constructor is 118. Chain length barely enters it (112 at depth 1, 120 at
    // depth 4), which is what says the walk is not where the time goes — the
    // crossing is.
    //
    // `accessor::resolve` answers the same question inside a borrow this
    // function was taking anyway, and its `Absent` is DEFINITIVE for a receiver
    // that is not a proxy: it is the same walk `get_indexed` performs after its
    // own nullish and text checks, neither of which can fire for a key spelled
    // `@@hasInstance`. So the two shapes that CAN run user code — a proxy's trap
    // and an accessor's getter — keep the door they have, and everything else
    // stops paying for a door it never opens.
    let probed = with_current(|context| {
        let Some(slot) = Value(callee).as_slot() else {
            // A primitive callee: no cell to walk from, and `get_indexed`
            // reaches the shared prototype for it. Left to the old path rather
            // than answered here, because deciding it twice is how the two come
            // to disagree.
            return Probe::Slowly;
        };
        if context.proxy_at(slot).is_some() {
            return Probe::Slowly;
        }
        let key = context.well_known(super::symbol::HAS_INSTANCE);
        match super::accessor::resolve(context, slot, key) {
            // Nothing, along the whole chain. This is the answer for every
            // ordinary constructor in every program.
            super::accessor::Found::Absent => Probe::None,
            super::accessor::Found::Value(found) => Probe::Held(found),
            // A getter is user code and may not run inside this borrow.
            super::accessor::Found::Getter(_) => Probe::Slowly,
        }
    });
    let hook = match probed {
        Probe::None => None,
        Probe::Held(found) => Some(found),
        Probe::Slowly => {
            let key = with_current(|context| context.well_known_text(super::symbol::HAS_INSTANCE));
            let found = super::computed::get_indexed(callee, key);
            if super::throw::in_flight() {
                return false;
            }
            Some(found)
        }
    };
    if let Some(hook) = hook
        && !with_current(|context| super::objects::nullish(context, hook).is_some())
    {
        // `GetMethod` refuses a present, non-callable hook rather than ignoring
        // it. Falling through was the wrong repair for it: a class writing
        // `static [Symbol.hasInstance] = 3` got the ordinary chain walk and a
        // plausible answer, where the language says the class is malformed.
        if !with_current(|context| super::modules::is_callable_in(context, hook)) {
            super::throw::type_error("Symbol.hasInstance is not a function");
            return false;
        }
        let absent = with_current(|context| undefined_of(context));
        let answered = call(hook, callee, value, absent, absent, absent);
        if super::throw::in_flight() {
            return false;
        }
        return super::class_support::to_boolean(answered);
    }
    // `Err` is a refusal `OrdinaryHasInstance` states, raised outside the borrow
    // because `throw::type_error` builds the program's own error object and that
    // borrows the context again.
    //
    // The callable test is NOT written here as a separate step, and that is a
    // repair rather than a shortcut: `modules::is_callable_in` asks for a code
    // address, and a PROXY has none — so asking it up front refused
    // `x instanceof new Proxy(F, {})`, which the language delegates and which
    // the walk below already resolves by following the proxy to its target. The
    // loop is the one place that knows what "callable" means for a proxy and a
    // bound function, so the refusal is raised from inside it.
    let held = with_current(|context| {
        let Some(mut function) = Value(callee).as_slot() else {
            return Err(Refusal::NotCallable);
        };
        // A PROXY and a BOUND function are both callable with no code address
        // of their own, so `callable_at` answered `None` and the operator
        // answered false for `x instanceof P` and `x instanceof f.bind(null)`
        // — where the language says both delegate. The specification says so
        // in two places that agree: a bound function's `[[HasInstance]]` IS
        // its target's, and a proxy with no `get` trap forwards `prototype` to
        // its target. Walked rather than special-cased once, because a proxy
        // may wrap a bound function and a bound function may bind another.
        //
        // The step is taken when `prototype` is ABSENT rather than when the
        // cell is not callable, and the difference is the bound case: a bound
        // function IS a callable cell here — it carries `forward`'s address —
        // and merely has no `prototype` of its own, which the language is
        // explicit that `bind` never gives one. A proxy is the other half: it
        // has no code address at all, so a `callable_at` test refuses it before
        // any chain is walked.
        //
        // The divergence, named: a proxy WITH a `get` trap should have that
        // trap answer for `prototype`, and this reads the target's instead.
        // Running the trap needs a call, and a call may not happen inside this
        // borrow — the same limit `entry/primitive.rs` records.
        let key = prototype_key(context);
        let mut found = None;
        for _ in 0..super::objects::CHAIN_LIMIT {
            if context.callable_at(function).is_none() && context.proxy_at(function).is_none() {
                // Nothing callable here or anywhere down the bound/proxy chain.
                // A plain object and an array are what reach this in real code,
                // and both are the typo the doc above names.
                return Err(Refusal::NotCallable);
            }
            if let Some(value) = super::objects::read_property(context, function, key) {
                found = Some(value);
                break;
            }
            let underneath = context
                .proxy_at(function)
                .map(|(target, _)| target)
                .or_else(|| context.bound_at(function).map(|bound| bound.target));
            match underneath.and_then(|value| Value(value).as_slot()) {
                Some(next) => function = next,
                None => break,
            }
        }
        // Step 5 of `OrdinaryHasInstance`: `C.prototype` must be an OBJECT, and
        // a constructor whose `prototype` was assigned a number or `null` is
        // refused rather than compared against. Both spellings reached the walk
        // and answered `false`, which reads as "not an instance" for a
        // constructor nothing can ever be an instance of.
        let Some(wanted) = found.filter(|held| super::objects::is_object(context, held.bits()))
        else {
            return Err(Refusal::BadPrototype);
        };
        let Some(mut cell) = Value(value).as_slot() else {
            return Ok(false);
        };
        // Step 3 of `OrdinaryHasInstance`: if the left operand is not an
        // OBJECT, the answer is false before any chain is walked. A number, a
        // symbol and a bigint are refused by the line above — none of them is a
        // reference — but a STRING is a primitive that lives in a cell, so it
        // reached the walk, and the walk substitutes `String.prototype` by kind
        // (see the note below). `"s" instanceof Object` answered true where
        // every engine answers false.
        //
        // Asked of the cell rather than of the tag, which is the same question
        // `text::type_of` asks to tell a string from an object and the reason
        // `new String("s")` still answers true: a wrapper is an ordinary object
        // cell that merely inherits from the same prototype.
        if context.text_at(cell).is_some() {
            return Ok(false);
        }
        // Stepped with `inherited_from` rather than `prototype_at`, so the
        // prototypes that are SUBSTITUTED by kind rather than linked from the
        // cell count too. Without it `[] instanceof Array` and
        // `({}) instanceof Object` are both false — the object really does
        // inherit, and asking only for an own link is asking the wrong
        // question.
        for _ in 0..super::objects::CHAIN_LIMIT {
            let Some(next) = super::objects::inherited_from(context, cell) else {
                return Ok(false);
            };
            if Value::from_slot(next).bits() == wanted.bits() {
                return Ok(true);
            }
            cell = next;
        }
        Ok(false)
    });
    match held {
        Ok(answer) => answer,
        Err(why) => {
            super::throw::type_error(match why {
                Refusal::NotCallable => "Right-hand side of 'instanceof' is not callable",
                Refusal::BadPrototype => {
                    "Function has non-object prototype in instanceof check"
                }
            });
            false
        }
    }
}

/// Which of `OrdinaryHasInstance`'s two refusals the walk in [`instance_of`]
/// reached.
///
/// Carried out of the borrow rather than raised inside it, for the reason every
/// raising native here states: building the error object borrows the context a
/// second time, and the panic that produces cannot unwind through an entry
/// point — it aborts.
enum Refusal {
    /// The right-hand side is not callable, following bound targets and proxies
    /// as far as they go.
    NotCallable,
    /// It is callable, and its `prototype` is not an object — `undefined`
    /// included, which is what an arrow function and a bound function have.
    BadPrototype,
}

/// What the `Symbol.hasInstance` probe in [`instance_of`] found, before
/// anything that could run user code has run.
///
/// Three answers and not two: "nothing is there" and "here it is" are both
/// settled inside the borrow, and the third says the question has to be asked
/// again through the door that can run a getter or a proxy trap. Without the
/// third, `instance_of` would either answer for a proxy without asking it or
/// run user code inside a `RefCell` borrow, and both are the failures this
/// crate's rule 8 is about.
enum Probe {
    /// No `Symbol.hasInstance` anywhere on the chain. Every ordinary
    /// constructor in every program.
    None,
    /// A data property, already read.
    Held(u64),
    /// A proxy, a getter, or a callee with no cell — ask through
    /// `computed::get_indexed`, exactly as this function always did.
    Slowly,
}
