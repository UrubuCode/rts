//! Which constructor a promise-producing operation builds its answer with.
//!
//! # The two protocols, and why they are not one function
//!
//! `then`, `catch` and `finally` ask `SpeciesConstructor(p, %Promise%)`: the
//! receiver's `constructor`, then that constructor's `Symbol.species`. The
//! statics — `resolve`, `reject`, the four combinators, `withResolvers` — ask
//! nothing: `this` **is** the constructor, exactly as `Array.from` reads it and
//! for the same reason the language states there. Written as one function the
//! difference would have to be a flag, and a flag is where the two eventually
//! come to share a bug.
//!
//! # `NewPromiseCapability` without a second settlement path
//!
//! The specification's capability is a triple — a promise and the two functions
//! that settle it — and every reaction would have to CALL one of them instead of
//! settling a `PromiseId`. That is a second answer to "how does a promise
//! settle", in a module whose whole shape ([`super::react::Step`]) exists to
//! keep the first one honest.
//!
//! So [`derive`] answers a `PromiseId` too, and there are two ways it gets one:
//!
//! - The constructed object **is** a promise — which is every `class Mine
//!   extends Promise`, because `Promise`'s own constructor registered it. Its
//!   own identifier is handed back, the settlement is ordinary, and the
//!   capability's functions are never called. That costs no extra microtask,
//!   which matters: a subclass's `.then` must resolve on the same turn a plain
//!   one's does.
//! - It is not — `Promise.all.call(SomeFunction, …)`. Then an internal promise
//!   carries the settlement and a [`super::react::Handler::Forward`] reaction
//!   hands it to the captured `resolve`/`reject`. That one DOES cost a
//!   microtask, and it is the honest price of a settlement that has to travel
//!   through user code.
//!
//! # Why an unusable constructor falls back instead of throwing
//!
//! The language throws `TypeError` for `Promise.all.call({}, …)`. This answers
//! the intrinsic path instead, which is what the call did before this module
//! existed. The reason is not tolerance for its own sake: a receiver that fails
//! to arrive would turn every `Promise.all` in every program into a throw, and
//! a wrong CLASS is a smaller divergence than a program that stops where the
//! language does not — the same trade `array_proto::species` records.

use rts_cranelift::sched::PromiseId;

use crate::entry::objects::undefined_of;
use crate::entry::{Context, class_support, functions, native, throw, with_current};
use crate::value::Value;

use super::react::Handler;
use super::state;

/// `Promise` itself, as the class registration recorded it.
fn intrinsic(context: &Context) -> Option<u64> {
    class_support::made(context, "Promise")
}

/// A constructor worth building with, or `None` for "the intrinsic path".
///
/// `undefined`, `null`, a non-callable and `Promise` itself all answer `None`,
/// which folds the four ways of meaning "nothing was asked for" into the one
/// answer the callers act on.
fn usable(value: u64) -> Option<u64> {
    with_current(|context| {
        let cell = Value(value).as_slot()?;
        context.callable_at(cell)?;
        match intrinsic(context) == Some(value) {
            true => None,
            false => Some(value),
        }
    })
}

/// The constructor a STATIC builds with — `this`, per `NewPromiseCapability`.
pub(super) fn chosen(this: u64) -> Option<u64> {
    usable(this)
}

/// `Promise` or the receiver, whichever a static's identity rule compares
/// against.
///
/// `PromiseResolve(C, x)` answers `x` itself when `x.constructor` is `C`, and
/// `C` is the receiver even when it is the intrinsic — so this cannot be
/// [`chosen`], which reports the intrinsic as "nothing was asked for".
pub(super) fn receiver_of(this: u64) -> Option<u64> {
    match chosen(this) {
        Some(constructor) => Some(constructor),
        None => with_current(|context| intrinsic(context)),
    }
}

/// `SpeciesConstructor(promise, %Promise%)` — what `then` builds with.
///
/// # Why the ordinary promise never reaches the protocol
///
/// Two property reads and a construction, all of them user code, in front of
/// every `.then` in every program — to answer "a plain promise" for all but the
/// ones that asked otherwise. An object whose prototype is exactly
/// `Promise.prototype` and which carries no own `constructor` reads the
/// intrinsic by construction, so the protocol can only answer the default, and
/// the reads are skipped.
///
/// What that misses is `Promise.prototype.constructor = X`, which is stated
/// rather than left to be found: the statics consult the protocol
/// unconditionally, so the one place a program is likely to notice — the
/// identity rule of `Promise.resolve` — is exact.
pub(super) fn species_of(promise: u64) -> Option<u64> {
    let ordinary = with_current(|context| {
        let Some(cell) = Value(promise).as_slot() else {
            return true;
        };
        let Some(prototype) = class_support::prototype(context, "Promise") else {
            return true;
        };
        let key = context.well_known("constructor");
        context.prototype_at(cell) == Some(prototype)
            && crate::entry::objects::own_property(context, cell, key).is_none()
    });
    if ordinary {
        return None;
    }
    let constructor = crate::entry::array_proto::species::property(promise, "constructor")?;
    let species =
        crate::entry::array_proto::species::property(constructor, crate::entry::symbol::SPECIES)
            .unwrap_or(constructor);
    usable(species)
}

/// `NewPromiseCapability(C)` — the promise and the two functions that settle it.
///
/// `None` when the construction threw or answered something the executor never
/// reached, and the throw is left in flight for the caller to see.
pub(super) fn made(constructor: u64) -> Option<(u64, u64, u64)> {
    let (executor, slot) = with_current(|context| {
        let slot = context.promises.open_capability();
        let made = native::callable(context, executor_native as native::Native);
        if let Some(cell) = Value(made).as_slot() {
            let code = executor_native as native::Native;
            context.mark_callable(cell, code as usize as u64, slot as u64);
        }
        // `GetCapabilitiesExecutor` is an anonymous function of two arguments,
        // and a constructor that inspects what it was handed reads both.
        native::name_of(context, made, "");
        native::length_of(context, made, 2);
        (made, slot)
    });
    let absent = super::undefined();
    let promise = functions::construct(constructor, executor, absent, absent, absent);
    // Rule 8: the constructor is user code. The pair is closed either way, so a
    // throw does not leave a root behind.
    let pair = with_current(|context| context.promises.close_capability(slot));
    if throw::in_flight() {
        return None;
    }
    let (resolve, reject) = pair?;
    Some((promise, resolve, reject))
}

/// The promise a derived operation ANSWERS, and the identifier its settlement
/// is written to.
///
/// See the module documentation for why those are sometimes the same promise
/// and sometimes two.
pub(super) fn derive(constructor: u64) -> Option<(u64, PromiseId)> {
    let (promise, resolve, reject) = made(constructor)?;
    let direct = with_current(|context| {
        Value(promise)
            .as_slot()
            .and_then(|cell| context.promises.id_of(cell))
    });
    if let Some(id) = direct {
        return Some((promise, id));
    }
    with_current(|context| {
        let (_, inner) = state::fresh(context)?;
        state::react(context, inner, Handler::Forward { resolve, reject });
        Some((promise, inner))
    })
}

/// `(resolve, reject)` as `NewPromiseCapability`'s executor records them.
///
/// The environment slot carries WHICH capability, the same way
/// [`super::settler`] carries which pair: a native closes over one word, and
/// two capabilities in flight must not overwrite each other's answer.
extern "C" fn executor_native(
    environment: u64,
    _this: u64,
    a0: u64,
    a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    with_current(|context| {
        context
            .promises
            .record_capability(environment as usize, a0, a1);
        undefined_of(context)
    })
}
