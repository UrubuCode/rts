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
    /// An executor that throws REJECTS the promise — unless it had already
    /// resolved it, in which case the throw is dropped. This doc said the throw
    /// "ends the program, which `entry::throw` records as the boundary the whole
    /// engine currently draws"; that boundary moved when a native could raise.
    #[construct]
    fn build(this: u64, executor: u64) -> u64 {
        // The two refusals the constructor owes, and both used to be silent.
        // `new` hands over a fresh instance; a plain call hands over a receiver
        // that is not a cell, and `Promise(f)` must be a `TypeError` rather than
        // a promise nobody can be sure of.
        let refusal = with_current(|context| match Value(this).as_slot() {
            None => Some("Promise constructor cannot be invoked without 'new'"),
            Some(_) if !crate::entry::modules::is_callable_in(context, executor) => {
                Some("Promise resolver is not a function")
            }
            Some(_) => None,
        });
        if let Some(message) = refusal {
            crate::entry::throw::type_error(message);
            return super::undefined();
        }
        let prepared = with_current(|context| {
            let (cell, id) = state::built(context, this)?;
            let pair = context.promises.open_pair(id);
            let resolve_fn = settler::settler(context, pair, Settlement::Fulfilled);
            let reject_fn = settler::settler(context, pair, Settlement::Rejected);
            Some((Value::from_slot(cell).bits(), resolve_fn, reject_fn, pair))
        });
        let Some((promise, resolve_fn, reject_fn, pair)) = prepared else {
            return super::undefined();
        };
        let absent = super::undefined();
        crate::entry::functions::call(executor, absent, resolve_fn, reject_fn, absent, absent);
        // Rule 8, in its HANDLING form: an executor that throws REJECTS the
        // promise with what it threw, and a promise already settled ignores it.
        // The doc above said the throw "ends the program, which `entry::throw`
        // records as the boundary the whole engine currently draws" — that
        // boundary moved when a native could raise, and this is the one place
        // where the language says a throw becomes a rejection rather than
        // travelling.
        if let Some(reason) = crate::entry::throw::caught() {
            with_current(|context| {
                // Only while the executor's own pair is unspent: one that
                // called `resolve` and THEN threw leaves that resolution
                // standing, which is what "a promise resolves once" means.
                // Asked of `alreadyResolved` and not of the settlement, because
                // `resolve(thenable)` is resolved while still pending — the
                // case an "is it settled" test overwrote.
                if !context.promises.pair_spent(pair)
                    && let Some(cell) = Value(promise).as_slot()
                    && let Some(id) = context.promises.id_of(cell)
                {
                    state::reject(context, id, reason);
                }
            });
        }
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
    /// A promise is answered as it stands rather than wrapped — but only one
    /// whose `constructor` IS the receiver, which is the half the identity rule
    /// is usually written without: `Promise.resolve(subclassInstance)` builds a
    /// new plain promise, because the answer has to be of the class that was
    /// asked. The rules for everything else — adopt a thenable, fulfil with an
    /// ordinary value — live in [`super::resolved_with`], because `import()`
    /// answers a promise for a namespace and must obey the same two.
    #[stat]
    fn resolve(this: u64, value: u64) -> u64 {
        super::resolved_by(this, value)
    }

    /// `Promise.reject(reason)`.
    ///
    /// A promise reason is **not** adopted: `Promise.reject(p)` rejects with the
    /// promise itself. That asymmetry is in the language, and it is the one a
    /// symmetrical implementation gets wrong quietly.
    #[stat]
    fn reject(this: u64, reason: u64) -> u64 {
        super::rejected_by(this, reason)
    }

    /// `Promise.all(values)` — every value, or the first rejection.
    #[stat]
    fn all(this: u64, values: u64) -> u64 {
        super::combinators::combine(this, values, super::group::Kind::All)
    }

    /// `Promise.allSettled(values)` — a record per element, and never a rejection.
    #[stat]
    fn all_settled(this: u64, values: u64) -> u64 {
        super::combinators::combine(this, values, super::group::Kind::AllSettled)
    }

    /// `Promise.race(values)` — the first settlement, either way.
    #[stat]
    fn race(this: u64, values: u64) -> u64 {
        super::combinators::combine(this, values, super::group::Kind::Race)
    }

    /// `Promise.any(values)` — the first fulfilment, or an aggregate of the
    /// rejections.
    #[stat]
    fn any(this: u64, values: u64) -> u64 {
        super::combinators::combine(this, values, super::group::Kind::Any)
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
    ///
    /// On a SUBCLASS the three parts are the capability's own — the promise
    /// `new Sub(executor)` built and the very functions that executor was
    /// handed — rather than a plain promise with settlers of this module's
    /// making. Anything else would answer a `promise` of the right class whose
    /// `resolve` settled a different object.
    #[stat]
    fn with_resolvers(this: u64) -> u64 {
        let made = match super::capability::chosen(this) {
            Some(constructor) => super::capability::made(constructor),
            None => with_current(|context| {
                let (cell, id) = state::fresh(context)?;
                let pair = context.promises.open_pair(id);
                Some((
                    Value::from_slot(cell).bits(),
                    settler::settler(context, pair, Settlement::Fulfilled),
                    settler::settler(context, pair, Settlement::Rejected),
                ))
            }),
        };
        let Some((promise, resolve, reject)) = made else {
            return super::undefined();
        };
        record_of(promise, resolve, reject)
    }
}

/// The `{ promise, resolve, reject }` both paths answer.
///
/// Rooted, because the record itself allocates and until the last `put` runs
/// the three are named by a `Vec` on the Rust heap — which the conservative
/// scan does not reach, and which is the hole `crate::entry::rooted` was
/// written for.
fn record_of(promise: u64, resolve: u64, reject: u64) -> u64 {
    with_current(|context| {
        let held = crate::entry::rooted::Rooted::with(vec![promise, resolve, reject]);
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

/// A reaction on the receiver, and the promise it answers.
///
/// Shared by the three prototype methods because they differ only in which
/// handler they build — and the part they share is the part with the mistake in
/// it: a `.then` on something that is not a promise must answer a value rather
/// than reach into the machine with an identifier it does not have.
///
/// The species is consulted BEFORE the borrow, because both halves of it —
/// reading `constructor`, then constructing — are user code. See
/// [`super::capability`] for why an ordinary promise never reaches either.
fn attached(this: u64, make: impl FnOnce(PromiseId) -> Handler) -> u64 {
    let source = with_current(|context| {
        Value(this)
            .as_slot()
            .and_then(|cell| context.promises.id_of(cell))
    });
    let Some(source) = source else {
        // `Promise.prototype.then.call({})`. The language throws a
        // `TypeError`; this answers `undefined`, the same stated gap every
        // refusal in this crate settles on while there is nowhere for a
        // throw to land.
        return super::undefined();
    };
    let built = match super::capability::species_of(this) {
        Some(constructor) => super::capability::derive(constructor),
        None => None,
    };
    // Rule 8: reading `constructor`, running a `Symbol.species` getter and
    // running the constructor are three chances for the program to throw, and
    // this native PROPAGATES — the call site above re-raises.
    if crate::entry::throw::in_flight() {
        return super::undefined();
    }
    with_current(|context| {
        let derived = match built {
            Some((promise, derived)) => (promise, derived),
            None => match state::fresh(context) {
                Some((cell, id)) => (Value::from_slot(cell).bits(), id),
                None => return undefined_of(context),
            },
        };
        state::react(context, source, make(derived.1));
        derived.0
    })
}
