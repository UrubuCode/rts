//! `Promise` as a program names it.
//!
//! Every member here is the same three steps the rest of this crate's
//! callback-taking methods are: decide under a borrow, drop it, call. The one
//! that is not obvious is the constructor — an executor runs **synchronously**,
//! which is what makes `new Promise(r => r(1))` already resolved when the
//! constructor returns, and it is user code, so it runs with the borrow gone.

use rts_cranelift::sched::{PromiseId, Settlement};

use super::react::Handler;
use super::settler;
use super::state;
use crate::entry::objects::undefined_of;
use crate::entry::with_current;
use crate::value::Value;

/// `Promise`.
#[rtse::class("Promise", tag)]
impl Promise {
    /// `new Promise(executor)`.
    ///
    /// The executor is called at once with a `resolve` and a `reject` — and with
    /// no borrow held, because it is arbitrary user code. What it is *not*
    /// wrapped in is a `try`: the language rejects the promise when an executor
    /// throws, and a throw here ends the program, which `entry::throw`
    /// records as the boundary the whole engine currently draws.
    #[construct]
    fn build(this: u64, executor: u64) -> u64 {
        let prepared = with_current(|context| {
            let (cell, id) = state::built(context, this)?;
            let resolve_fn = settler::settler(context, id, Settlement::Fulfilled);
            let reject_fn = settler::settler(context, id, Settlement::Rejected);
            Some((Value::from_slot(cell).bits(), resolve_fn, reject_fn))
        });
        let Some((promise, resolve_fn, reject_fn)) = prepared else {
            return super::undefined();
        };
        let absent = super::undefined();
        crate::entry::functions::call(executor, absent, resolve_fn, reject_fn, absent, absent);
        promise
    }

    /// `p.then(onFulfilled, onRejected)` — a new promise for the handler's answer.
    fn then(this: u64, on_fulfilled: u64, on_rejected: u64) -> u64 {
        attached(this, |derived| Handler::Js {
            on_fulfilled,
            on_rejected,
            derived,
        })
    }

    /// `p.catch(f)`, which the language defines as `p.then(undefined, f)`.
    ///
    /// Written that way rather than as its own path, so that a rejection reaches
    /// one handler-dispatch and not two — and so `catch` cannot come to differ
    /// from `then` about what a non-callable argument means.
    #[js("catch")]
    fn caught(this: u64, on_rejected: u64) -> u64 {
        let absent = super::undefined();
        attached(this, |derived| Handler::Js {
            on_fulfilled: absent,
            on_rejected,
            derived,
        })
    }

    /// `p.finally(f)` — run either way, and pass the settlement through.
    ///
    /// The value is passed on untouched, which is the point of `finally` and the
    /// thing a `then(f, f)` spelling gets wrong: that one would resolve the
    /// derived promise with what `f` answered.
    fn finally(this: u64, callback: u64) -> u64 {
        attached(this, |derived| Handler::Finally { callback, derived })
    }

    /// `Promise.resolve(v)`.
    ///
    /// A promise is answered as it stands rather than wrapped, which the
    /// language requires: `Promise.resolve(p) === p`, and a wrapper would be a
    /// second promise settling one microtask later than every program expects.
    #[stat]
    fn resolve(value: u64) -> u64 {
        with_current(|context| {
            if let Some(cell) = Value(value).as_slot()
                && context.promises.id_of(cell).is_some()
            {
                return value;
            }
            let Some((cell, id)) = state::fresh(context) else {
                return undefined_of(context);
            };
            // Through `resolve` rather than a straight fulfilment, so that
            // `Promise.resolve(thenable)` adopts it — which is what the language
            // says and the reason this is not `settle(Fulfilled)`.
            state::resolve(context, id, value);
            Value::from_slot(cell).bits()
        })
    }

    /// `Promise.reject(reason)`.
    ///
    /// A promise reason is **not** adopted: `Promise.reject(p)` rejects with the
    /// promise itself. That asymmetry is in the language, and it is the one a
    /// symmetrical implementation gets wrong quietly.
    #[stat]
    fn reject(reason: u64) -> u64 {
        with_current(|context| {
            let Some((cell, id)) = state::fresh(context) else {
                return undefined_of(context);
            };
            state::reject(context, id, reason);
            Value::from_slot(cell).bits()
        })
    }

    /// `Promise.all(values)` — every value, or the first rejection.
    #[stat]
    fn all(values: u64) -> u64 {
        super::combinators::combine(values, super::group::Kind::All)
    }

    /// `Promise.allSettled(values)` — a record per element, and never a rejection.
    #[stat]
    fn all_settled(values: u64) -> u64 {
        super::combinators::combine(values, super::group::Kind::AllSettled)
    }

    /// `Promise.race(values)` — the first settlement, either way.
    #[stat]
    fn race(values: u64) -> u64 {
        super::combinators::combine(values, super::group::Kind::Race)
    }

    /// `Promise.any(values)` — the first fulfilment, or an aggregate of the
    /// rejections.
    #[stat]
    fn any(values: u64) -> u64 {
        super::combinators::combine(values, super::group::Kind::Any)
    }

    /// `Promise.withResolvers()` — the promise and its two settlers, ES2024.
    ///
    /// # Why this is not sugar a program could write
    ///
    /// It is, almost: `let r, j; const p = new Promise((a, b) => { r = a; j = b })`
    /// is the pattern it replaces, and it works here already. What it is not is
    /// the same COST — that spelling allocates a closure, hands it to the
    /// executor, and relies on the executor running synchronously, which is the
    /// one property of the constructor most people writing it are not sure of.
    ///
    /// Built from [`settler::settler`] rather than by calling this class's own
    /// constructor with a native executor: the settlers are exactly what the
    /// executor would have been handed, so going through a constructor would be
    /// the same three values with a call in the middle of them.
    #[stat]
    fn with_resolvers() -> u64 {
        with_current(|context| {
            let Some((cell, id)) = state::fresh(context) else {
                return undefined_of(context);
            };
            // Rooted, because each of the three allocates and the record that
            // will hold them does too — so until the last `put` runs they are
            // named by a Rust local and nothing else, which is the hole
            // `crate::entry::rooted` was written for.
            let mut held = crate::entry::rooted::Rooted::new();
            held.values().push(Value::from_slot(cell).bits());
            held.values()
                .push(settler::settler(context, id, Settlement::Fulfilled));
            held.values()
                .push(settler::settler(context, id, Settlement::Rejected));
            let Some(kit) = crate::entry::native::plain(context) else {
                return undefined_of(context);
            };
            for (name, value) in ["promise", "resolve", "reject"].into_iter().zip(held.take()) {
                let key = context.well_known(name);
                crate::entry::objects::put(context, kit, key, value);
            }
            Value::from_slot(kit).bits()
        })
    }
}

/// A reaction on the receiver, and the promise it answers.
///
/// Shared by the three prototype methods because they differ only in which
/// handler they build — and the part they share is the part with the mistake in
/// it: a `.then` on something that is not a promise must answer a value rather
/// than reach into the machine with an identifier it does not have.
fn attached(this: u64, make: impl FnOnce(PromiseId) -> Handler) -> u64 {
    with_current(|context| {
        let source = Value(this)
            .as_slot()
            .and_then(|cell| context.promises.id_of(cell));
        let Some(source) = source else {
            // `Promise.prototype.then.call({})`. The language throws a
            // `TypeError`; this answers `undefined`, the same stated gap every
            // refusal in this crate settles on while there is nowhere for a
            // throw to land.
            return undefined_of(context);
        };
        let Some((cell, derived)) = state::fresh(context) else {
            return undefined_of(context);
        };
        state::react(context, source, make(derived));
        Value::from_slot(cell).bits()
    })
}
