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
//! happens instead: `1()` throws a `TypeError`. Throwing needs the machine's
//! protected regions and nothing emits those yet, so the check has to live
//! somewhere that can fail without them. Compiled code cannot; this can.
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

/// Makes a callable out of a code address and an environment.
///
/// The code address comes from the machine's `FuncAddr`, which is a relocation
/// the destination filled in — so it is a real address by the time this runs,
/// and this never computes one.
#[rtse::entry]
pub fn closure_new(code: i64, environment: u64) -> u64 {
    with_current(|context| {
        // An ordinary object, because a function IS one: `f.x = 1` works, and
        // `f.prototype` will. What makes it callable is recorded beside the
        // cell, where nothing a program can write reaches it.
        let shape = context.shapes.root();
        let ty = context.layout_of(shape).index() as u32;
        let cell = super::alloc::alloc_or_die(context, crate::heap::STRIDE, ty);
        context.mark_callable(cell, code as u64, environment);
        // Every function gets a `prototype` object, because `new` reads one
        // and a function that could not be constructed with would be a
        // different kind of function. Made here rather than on demand:
        // `F.prototype.m = …` before any `new F()` is the ordinary way to
        // write a method, so it has to exist first.
        let shape = context.shapes.root();
        let ty = context.layout_of(shape).index() as u32;
        if let Some(prototype) = super::alloc::alloc_after_collecting(context, crate::heap::STRIDE, ty) {
            let key = prototype_key(context);
            super::objects::put(context, cell, key, Value::from_slot(prototype).bits());
        }
        Value::from_slot(cell).bits()
    })
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
        // The vector this activation reads its rest from. Pushed rather than
        // stored, because a call inside the callee pushes its own.
        context.pending_arguments.push(arguments);
        first
    });
    // Not through `call`, which pushes a marker of its own — that marker on top
    // would hide the vector from exactly the callee it was made for.
    let produced = invoke(callee, this, first[0], first[1], first[2], first[3]);
    with_current(|context| context.pending_arguments.pop());
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
    let collected = with_current(|context| {
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
            // carried. Trailing padding is dropped rather than reported.
            _ => {
                let mut given = vec![a0, a1, a2, a3];
                while given.last() == Some(&absent) {
                    given.pop();
                }
                given.into_iter().skip(from).collect()
            }
        }
    });
    let array = super::array::array_new(collected.len() as i64);
    with_current(|context| {
        if let Some(cell) = Value(array).as_slot()
            && let Some(elements) = context.elements_at_mut(cell)
        {
            *elements = collected;
        }
        array
    })
}

/// Calls a value, with a receiver and up to [`ARGUMENT_SLOTS`] arguments.
///
/// Answers `undefined` for a callee that is not callable, where the language
/// throws a `TypeError`. A stated gap and the same one property access has:
/// throwing needs protected regions and nothing emits those yet. It is named
/// here rather than left implicit because *this* gap is the one that would
/// otherwise be a jump to an arbitrary address.
#[rtse::entry]
pub fn call(callee: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    // A class constructor called without `new` is a `TypeError`, checked
    // before anything else so `1()` and `ClassCtor()` do not share a path
    // that decides this AFTER the jump.
    if is_bare_class_constructor_call(callee) {
        super::throw::type_error("Class constructor cannot be invoked without 'new'");
        return with_current(|context| undefined_of(context));
    }
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
    with_current(|context| {
        let absent = undefined_of(context);
        context.pending_arguments.push(absent);
    });
    let produced = invoke(callee, this, a0, a1, a2, a3);
    with_current(|context| context.pending_arguments.pop());
    produced
}

/// The jump itself, with no argument vector of its own.
///
/// Split from [`call`] because `call_with_args` has already pushed the vector
/// this activation reads, and `call`'s marker on top of it would hide that
/// vector from the one callee it was made for.
fn invoke(callee: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let (found, name) = with_current(|context| {
        // Which callable is about to run, recorded before the jump. A compiled
        // function never asks; a native does, because it closes over nothing —
        // so a bound function's only way to know WHICH binding it is comes from
        // here. See [`super::function_proto`].
        context.callees.push(callee);
        // Taken, not merely read: this activation's own callee may itself
        // fail to be a function only ONCE, and taking it here means a nested
        // call this one goes on to make starts with nothing set, rather than
        // inheriting the name written for an outer call that never used it.
        let name = context.pending_call_name.take();
        (resolve(context, callee), name)
    });

    let Some((code, environment)) = found else {
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
                let kind = super::text::described(super::text::type_of(callee))
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
    });
    let produced = construct_inner(callee, a0, a1, a2, a3);
    with_current(|context| context.pending_arguments.pop());
    produced
}

/// The construction itself, with no argument vector of its own.
///
/// Split from [`construct`] for the reason [`invoke`] is split from [`call`]:
/// `construct_with_args` has already pushed the vector this construction reads.
fn construct_inner(callee: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let derived = with_current(|context| {
        let Some(cell) = Value(callee).as_slot() else {
            return None;
        };
        context.callable_at(cell)?;
        context.new_targets.push(callee);
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

    let produced = invoke(callee, this, a0, a1, a2, a3);
    with_current(|context| context.new_targets.pop());

    // A constructor that returned an object produced THAT. Anything else — a
    // number, `undefined`, the usual — leaves the fresh object as the answer.
    // For a derived one there is no fresh object, so what came back IS the
    // answer: the compiler makes such a constructor return its `this`.
    if Value(produced).as_slot().is_some() {
        produced
    } else {
        this
    }
}

/// `super(…)` — the parent constructor, producing the object.
///
/// Not a call with a receiver: the parent may be the base of the chain, in which
/// case it allocates, and it may be derived itself, in which case its own
/// `super()` does. Either way the object inherits from the prototype of the
/// class `new` named, which is what the target stack carries.
#[rtse::entry]
pub fn super_construct(parent: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let derived = with_current(|context| {
        let cell = Value(parent).as_slot()?;
        context.callable_at(cell)?;
        Some(context.is_derived(cell))
    });
    let Some(derived) = derived else {
        return with_current(|context| undefined_of(context));
    };

    // Deliberately NOT pushing a target: the one `new` established is the one
    // the whole chain builds against. Pushing the parent here is exactly the
    // bug that makes `new B()` produce something inheriting from `A.prototype`.
    let this = match derived {
        true => with_current(|context| undefined_of(context)),
        false => match allocate_for_target(parent) {
            Some(fresh) => fresh,
            None => return with_current(|context| undefined_of(context)),
        },
    };

    let produced = invoke(parent, this, a0, a1, a2, a3);
    if Value(produced).as_slot().is_some() {
        produced
    } else {
        this
    }
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
        first
    });
    // Not through `construct`, which pushes a marker of its own — that marker
    // on top would hide the vector from the constructor it was made for.
    let produced = construct_inner(callee, first[0], first[1], first[2], first[3]);
    with_current(|context| context.pending_arguments.pop());
    produced
}

/// An object inheriting from the prototype of the class `new` named.
///
/// Falls back to the callee's own `prototype` when there is no target, which is
/// a constructor reached some way other than `new` — a call, today, and nothing
/// else once every path is emitted.
fn allocate_for_target(callee: u64) -> Option<u64> {
    with_current(|context| {
        let target = context.new_targets.last().copied().unwrap_or(callee);
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
        let mut resolved = cell;
        loop {
            let key = prototype_key(context);
            if super::objects::read_property(context, resolved, key).is_some() {
                break;
            }
            let Some(next) = context.bound_at(resolved).and_then(|bound| Value(bound.target).as_slot())
            else {
                break;
            };
            resolved = next;
        }
        let key = prototype_key(context);
        let prototype = super::objects::read_property(context, resolved, key)?;

        let shape = context.shapes.root();
        let ty = context.layout_of(shape).index() as u32;
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
    let Some(target) = context.new_targets.last().copied() else {
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
#[rtse::entry]
pub fn instance_of(value: u64, callee: u64) -> bool {
    with_current(|context| {
        let Some(function) = Value(callee).as_slot() else {
            return false;
        };
        if context.callable_at(function).is_none() {
            // `1 instanceof 2` is a `TypeError`. Answering false is the same
            // stated gap every other operation has while throwing is missing.
            return false;
        }
        let key = prototype_key(context);
        let Some(wanted) = super::objects::read_property(context, function, key) else {
            return false;
        };
        let Some(mut cell) = Value(value).as_slot() else {
            return false;
        };
        // Stepped with `inherited_from` rather than `prototype_at`, so the
        // prototypes that are SUBSTITUTED by kind rather than linked from the
        // cell count too. Without it `[] instanceof Array` and
        // `({}) instanceof Object` are both false — the object really does
        // inherit, and asking only for an own link is asking the wrong
        // question.
        for _ in 0..super::objects::CHAIN_LIMIT {
            let Some(next) = super::objects::inherited_from(context, cell) else {
                return false;
            };
            if Value::from_slot(next).bits() == wanted.bits() {
                return true;
            }
            cell = next;
        }
        false
    })
}
