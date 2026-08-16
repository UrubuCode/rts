//! `FinalizationRegistry` — over the collector's own death notice.
//!
//! # Reuse-check: what was searched, and what answers it
//!
//! [`super::weakref`]'s module doc says this "is not implemented; it needs the
//! same collector hook and a callback queue the drain loop would have to pump,
//! and nothing here builds either". **Both exist**, and neither is built here:
//!
//! - [`crate::entry::finalize`] IS the collector hook. The sweep queues a
//!   registration whose cell it freed, and the queue is drained where microtasks
//!   are — `promise::drain_microtasks` calls `finalize::drain()`, which is the one
//!   point every host in this repository already pumps. It was written for
//!   `rts-napi`'s `napi_wrap`, and this is its second client.
//! - [`crate::entry::external`] is what keeps a queued registration's own state
//!   alive. `entry::roots` scans `context.external` and does **not** scan
//!   `context.deaths`, which is exactly the pair of facts this needs: registering
//!   does not keep the target alive, and what the callback will need afterwards
//!   is kept.
//!
//! So nothing here re-derives a weak reference, a death notice or a queue. What
//! this file owns is the JavaScript shape: which object the callback belongs to,
//! what `unregister` cancels, and what the cleanup callback is handed.
//!
//! # What a program must not assume, and it is the language's own answer
//!
//! **When.** A callback runs at the first microtask drain after the collection
//! that freed the target, which may be much later than the last use and is never
//! at it.
//!
//! **Whether.** A program that ends before that drain runs nothing at all, and a
//! target the conservative stack scan still sees a word for is not collected at
//! all. Every JavaScript engine says the same thing about finalizers — the
//! specification requires no callback ever to run — so this is the contract
//! rather than a shortfall. Code that MUST run belongs somewhere it can be
//! called.
//!
//! # The divergence, named
//!
//! A pending registration keeps its registry alive. The record the death notice
//! carries names the registry, so that the callback can be found and the
//! registration pruned, and that record is held where the root scan sees it —
//! so a program that drops its last reference to a registry with registrations
//! outstanding still gets the callbacks. The specification permits an engine to
//! run nothing once the registry is unreachable; running them is the more useful
//! half of a "may", and it is what makes `unregister` reachable from the one
//! place that knows a target died.
//!
//! `cleanupSome` is absent: it is a separate proposal, not ES2021, and it exists
//! to let a program force a drain — which here would mean forcing a COLLECTION,
//! and this engine deliberately exposes no `gc` to the language (`CLAUDE.md`
//! records that decision).

use super::{Context, with_current};
use crate::entry::objects::undefined_of;
use crate::entry::{external, finalize, modules, objects, primitives, throw};
use crate::value::Value;

/// The cleanup callback a registry was built with.
const CLEANUP: &str = "__cleanup__";

/// The registrations still waiting, as an array of records.
const WAITING: &str = "__registrations__";

/// What one record of [`WAITING`] carries.
///
/// Named as constants rather than written at each site because the death notice
/// reads them from a different function than the one that wrote them, and a typo
/// in one of the two is a callback that answers `undefined` for its held value —
/// which is a wrong answer that runs rather than a failure.
const RECORD: Record = Record {
    registry: "registry",
    held: "held",
    token: "token",
    death: "death",
    hold: "hold",
};

/// The five properties of a registration record.
struct Record {
    /// The `FinalizationRegistry` this registration belongs to.
    registry: &'static str,
    /// The value the cleanup callback is handed.
    held: &'static str,
    /// What `unregister` matches against, or `undefined`.
    token: &'static str,
    /// The identifier [`finalize::cancel`] withdraws.
    death: &'static str,
    /// The identifier [`external::release`] gives back.
    hold: &'static str,
}

/// `FinalizationRegistry`.
#[rtse::class("FinalizationRegistry", tag)]
impl FinalizationRegistry {
    /// `new FinalizationRegistry(cleanupCallback)`.
    ///
    /// A callback that is not callable is a `TypeError` in the language, and it
    /// is raised rather than tolerated because the alternative is a registry
    /// that accepts every `register` and can never do anything with one.
    #[construct]
    fn build(this: u64, cleanup: u64) -> u64 {
        // `FinalizationRegistry(f)` without `new` is a `TypeError`, and the test
        // is the one `WeakRef`'s constructor uses: `new` hands over a fresh
        // instance, an ordinary call hands over a receiver that is not a cell.
        if Value(this).as_slot().is_none() {
            throw::type_error("Constructor FinalizationRegistry requires 'new'");
            return nothing();
        }
        if !callable(cleanup) {
            throw::type_error("FinalizationRegistry: the cleanup callback must be a function");
            return nothing();
        }
        // The array is made OUTSIDE the borrow below: `array_of` reaches
        // `array_new`, which is an entry point and takes the context for itself.
        let waiting = super::array_of(Vec::new());
        with_current(|context| {
            let Some(cell) = Value(this).as_slot() else {
                return undefined_of(context);
            };
            let key = context.well_known(CLEANUP);
            objects::put(context, cell, key, cleanup);
            let key = context.well_known(WAITING);
            objects::put(context, cell, key, waiting);
            this
        })
    }

    /// `registry.register(target, heldValue, unregisterToken?)`.
    ///
    /// The two refusals are the specification's, and both are raised rather than
    /// ignored: a primitive target is something that cannot die, so registering
    /// one is a subscription to an event that cannot happen; and a `heldValue`
    /// that IS the target would keep the target alive through the very record
    /// that is supposed to outlive it, so the callback could never run.
    #[arity(2)]
    fn register(this: u64, target: u64, held: u64, token: u64) -> u64 {
        if !is_registry(this) {
            throw::type_error("FinalizationRegistry.register called on a non-registry");
            return nothing();
        }
        // An unregistered SYMBOL is a target too, which ES2023 admits for the
        // same reason `WeakMap` admits one: it can die. A REGISTERED one cannot
        // — see [`super::weak::holdable_in`] — so it stays a `TypeError`.
        if !with_current(|context| super::weak::holdable_in(context, target)) {
            throw::type_error(
                "FinalizationRegistry.register: the target must be an object or an \
                 unregistered symbol",
            );
            return nothing();
        }
        if primitives::same_value(target, held) {
            throw::type_error(
                "FinalizationRegistry.register: the held value must not be the target",
            );
            return nothing();
        }
        // The token is held weakly as well, so it obeys the same rule the target
        // does — and `undefined` means "no token", which is the one value that
        // is not a token and not an error.
        if !with_current(|context| {
            token == undefined_of(context) || super::weak::holdable_in(context, token)
        }) {
            throw::type_error(
                "FinalizationRegistry.register: the unregister token must be an object or an \
                 unregistered symbol",
            );
            return nothing();
        }
        let record = with_current(|context| {
            let record = modules::make_object(context);
            let Some(cell) = Value(record).as_slot() else {
                return None;
            };
            for (name, value) in [
                (RECORD.registry, this),
                (RECORD.held, held),
                (RECORD.token, token),
            ] {
                let key = context.well_known(name);
                objects::put(context, cell, key, value);
            }
            // HELD before the death notice is asked for, because the notice
            // carries the identifier: `external` is what keeps this record
            // reachable while the collector runs, and `entry::roots` scans that
            // table and not `context.deaths`.
            let hold = external::hold(context, record);
            let waiting = finalize::on_death(
                context,
                target,
                finalize::Pending {
                    code: fire,
                    data: hold as usize,
                    hint: 0,
                },
            );
            match waiting {
                Some(waiting) => {
                    for (name, value) in
                        [(RECORD.death, waiting as f64), (RECORD.hold, hold as f64)]
                    {
                        let key = context.well_known(name);
                        objects::put(context, cell, key, Value::from_f64(value).bits());
                    }
                }
                // A SYMBOL target, which the check above now admits. Nothing
                // watches one — `finalize::on_death` is over cells — so the
                // registration carries no death notice and the callback simply
                // never fires, which is what a symbol that outlives the program
                // means. The record is still kept, because `unregister` must
                // find it: dropping it here would make a legal registration
                // silently unwithdrawable. The hold goes back, since the list
                // itself is what keeps the record reachable.
                None => {
                    external::release(context, hold);
                }
            }
            Some(record)
        });
        if let Some(record) = record {
            // Outside the borrow: appending is an entry point of its own.
            super::append(list_of(this), record);
        }
        nothing()
    }

    /// `registry.unregister(unregisterToken)` — whether anything was withdrawn.
    ///
    /// Every registration made with that token, not the first: the specification
    /// says a token may be used for several targets, and unregistering one of
    /// them while leaving the rest would be a leak a program cannot see.
    fn unregister(this: u64, token: u64) -> u64 {
        if !is_registry(this) {
            throw::type_error("FinalizationRegistry.unregister called on a non-registry");
            return nothing();
        }
        // A token that could never have been registered is a `TypeError` rather
        // than `false`, and the difference is the point: `false` says "nothing
        // matched", which is a truthful-looking answer to a call that was never
        // capable of matching anything.
        if !with_current(|context| super::weak::holdable_in(context, token)) {
            throw::type_error(
                "FinalizationRegistry.unregister: the token must be an object or an \
                 unregistered symbol",
            );
            return nothing();
        }
        let list = list_of(this);
        let records = elements(list);
        // SameValue, outside every borrow — it is an entry point that takes one.
        let (dropped, kept): (Vec<u64>, Vec<u64>) = records
            .into_iter()
            .partition(|&record| primitives::same_value(field(record, RECORD.token), token));
        if dropped.is_empty() {
            return modules::boolean_value(false);
        }
        with_current(|context| {
            for record in &dropped {
                if let Some(waiting) = number(context, *record, RECORD.death) {
                    finalize::cancel(context, waiting as u32);
                }
                if let Some(hold) = number(context, *record, RECORD.hold) {
                    external::release(context, hold as u32);
                }
            }
        });
        store(this, kept);
        modules::boolean_value(true)
    }
}

/// What the sweep queued: one target has died.
///
/// Called by [`finalize::drain`] with **no borrow held**, which is the whole
/// reason that module queues rather than calling from the sweep — see its doc.
///
/// `data` is the record's [`external`] identifier, and releasing it here is what
/// stops a registration that has fired from keeping its own state alive.
extern "C" fn fire(data: usize, _hint: usize) {
    let Some(record) = external::release_current(data as u32) else {
        return;
    };
    let registry = field(record, RECORD.registry);
    let held = field(record, RECORD.held);
    let cleanup = field(registry, CLEANUP);
    // Pruned BEFORE the callback runs: a cleanup callback that calls
    // `unregister` with this registration's token must not find a record whose
    // death notice has already been consumed, and one that inspects the registry
    // must not see a registration that has fired.
    let kept: Vec<u64> = elements(list_of(registry))
        .into_iter()
        .filter(|&waiting| waiting != record)
        .collect();
    store(registry, kept);
    if !callable(cleanup) {
        return;
    }
    let absent = nothing();
    // Rule 8, with the answer DISCARDED rather than checked: a cleanup callback
    // answers nothing the language reads, so there is no value here to inherit
    // wrongly. A throw it left stays in flight and is reported where the program
    // ends, which is what the specification asks a host to do with one.
    let _ = super::invoke(cleanup, absent, held, absent, absent);
}

/// The registry's array of records, made now if it has none.
///
/// Lazy rather than assumed, because an object can reach these methods without
/// having run the constructor — `Object.create(FinalizationRegistry.prototype)`
/// is the ordinary way. The alternative is a silent drop: appending to something
/// that is `undefined` is a no-op, so every `register` would succeed and nothing
/// would ever be pruned or unregistered.
/// Whether a value is a registry rather than something wearing the prototype.
///
/// By the callback the constructor writes: `Object.create(FR.prototype)` and
/// `FR.prototype` itself both answer the methods and neither ever ran the
/// constructor, so `register` on one used to succeed and queue a death notice
/// against a registry that has no callback to run. The specification's brand
/// check is an internal slot; the callback IS this engine's, which is why it is
/// asked for rather than a second marker property being invented.
fn is_registry(value: u64) -> bool {
    with_current(|context| {
        let Some(cell) = Value(value).as_slot() else {
            return false;
        };
        let key = context.well_known(CLEANUP);
        objects::own_property(context, cell, key).is_some()
    })
}

fn list_of(registry: u64) -> u64 {
    let held = field(registry, WAITING);
    if Value(held).as_slot().is_some() {
        return held;
    }
    let made = super::array_of(Vec::new());
    with_current(|context| {
        if let Some(cell) = Value(registry).as_slot() {
            let key = context.well_known(WAITING);
            objects::put(context, cell, key, made);
        }
    });
    made
}

/// Replaces the registry's array of records.
fn store(registry: u64, records: Vec<u64>) {
    let made = super::array_of(records);
    with_current(|context| {
        if let Some(cell) = Value(registry).as_slot() {
            let key = context.well_known(WAITING);
            objects::put(context, cell, key, made);
        }
    });
}

/// The elements of an array this module made.
fn elements(array: u64) -> Vec<u64> {
    with_current(|context| {
        Value(array)
            .as_slot()
            .and_then(|cell| context.elements_at(cell).cloned())
            .unwrap_or_default()
    })
}

/// One property of an object, by a name this module wrote.
///
/// A data read: every one of these is a property this file put there, so there
/// is no getter to run and no borrow to give up.
fn field(object: u64, name: &str) -> u64 {
    with_current(|context| {
        let Some(cell) = Value(object).as_slot() else {
            return undefined_of(context);
        };
        let key = context.well_known(name);
        match objects::read_property(context, cell, key) {
            Some(found) => found.bits(),
            None => undefined_of(context),
        }
    })
}

/// The same, as the number it was stored as, for a caller already in a borrow.
fn number(context: &mut Context, object: u64, name: &str) -> Option<f64> {
    let cell = Value(object).as_slot()?;
    let key = context.well_known(name);
    objects::read_property(context, cell, key).and_then(|found| found.numeric())
}

/// Whether a value can be called at all.
fn callable(value: u64) -> bool {
    with_current(|context| {
        Value(value)
            .as_slot()
            .is_some_and(|cell| context.callable_at(cell).is_some())
    })
}

/// `undefined`, from outside a borrow.
fn nothing() -> u64 {
    with_current(|context| undefined_of(context))
}
