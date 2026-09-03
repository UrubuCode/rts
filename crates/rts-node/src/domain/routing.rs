//! `domain.add`/`.remove`'s membership bookkeeping, the `'error'`-routing
//! `emit` override [`add`] installs, and `bind`/`intercept` — split out of
//! `domain/mod.rs` (see that file's own doc for why) because all of it is
//! the SECOND half of what a `Domain` does, once construction and the
//! active-domain stack already exist. `bind`/`intercept` live here rather
//! than in `mod.rs` because their wrappers push the very stack `mod.rs`
//! owns (`super::STACK`) and need `super::refresh_active_namespace()` the
//! same way `run`/`enter`/`exit` do.
//!
//! # `bind`/`intercept` — the "no environment slot" premise was stale
//!
//! [`rts_core::entry::closure_new`] takes a code address AND an environment
//! and delivers the environment back as the callee's first argument —
//! `crate::async_hooks`'s `AsyncResource.prototype.bind` reached the
//! identical shape first, and this module reuses it rather than re-deriving
//! it. Rerouting `add`/`remove` needs reading a value's own inherited method
//! WITHOUT invoking it: [`rts_core::entry::get_member`] already does exactly
//! this — a plain property read that walks the prototype chain, the same
//! way every other native in this crate reads `.name`/`.length` off values
//! it did not build.
//!
//! # What none of the three can do
//!
//! [`bind`]'s wrapper pushes this domain, calls the callback, pops — same
//! push/call/pop as `run`, and the same limitation: a throw INSIDE the
//! callback is an ordinary throw a native cannot intercept, so it propagates
//! uncaught rather than reaching `'error'`, and the pushed frame is left
//! behind (see `mod.rs`'s doc: real Node's own `run`/`bind` leave the same
//! leak). [`intercept`] sidesteps that for its OWN documented case (an
//! error-shaped first argument, checked before the wrapped callback is ever
//! called — no unwind needed) but inherits the same gap for a throw from
//! INSIDE the callback it does end up calling. [`add`]'s routed `emit`
//! reaches `'error'` events specifically; every other event still goes to
//! the real original `emit`, unaffected.

use super::{STACK, refresh_active_namespace};
use rts_core::entry;
use std::collections::HashMap;
use std::sync::Mutex;

/// `domain instance -> its members`, kept natively rather than solely as a
/// JS array: [`add`]/[`remove`] are called from inside
/// [`entry::with_runtime`] (their own `this`/`emitter` arguments are already
/// tagged values, nothing to read ambiently for), and rebuilding the
/// `members` JS array from this table after each change — the same
/// re-set-a-data-property pattern [`crate::cluster`]'s `workers` property
/// uses — avoids ever mixing an ambient array helper with a borrow already
/// held.
static MEMBERS: Mutex<Option<HashMap<u64, Vec<u64>>>> = Mutex::new(None);

fn with_members<T>(body: impl FnOnce(&mut HashMap<u64, Vec<u64>>) -> T) -> T {
    let mut guard = MEMBERS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

/// `emitter -> its REAL `emit`, captured once`, the first time any domain
/// [`add`]s it. Kept so the routing wrapper installed on the emitter (see
/// [`add`]'s doc) can still reach it — without this, overriding `.emit` would
/// have nothing left to call for every event that is not `'error'`.
static EMIT_ORIGINALS: Mutex<Option<HashMap<u64, u64>>> = Mutex::new(None);

fn with_originals<T>(body: impl FnOnce(&mut HashMap<u64, u64>) -> T) -> T {
    let mut guard = EMIT_ORIGINALS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

/// `emitter -> the domain that currently owns it`. Separate from [`MEMBERS`]
/// (which answers "this domain's emitters") because the routing wrapper needs
/// the inverse question ("this emitter's domain") on every `emit` call, and
/// scanning every domain's list for one emitter on every event a program fires
/// is the cost a second table avoids.
static OWNER: Mutex<Option<HashMap<u64, u64>>> = Mutex::new(None);

fn with_owner<T>(body: impl FnOnce(&mut HashMap<u64, u64>) -> T) -> T {
    let mut guard = OWNER.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

// `MEMBERS`, `EMIT_ORIGINALS` and `OWNER` all hold live JS values (emitters,
// domains, an `emit` function) in a Rust table the collector's root scan
// cannot see — the same class of exposure `crate::async_hooks`'s module doc
// names for its own frame stacks (`docs/engine/lost-roots.md`). Reported
// rather than worked around, for the same reason that doc gives: a shadow
// copy kept reachable through some other object would be a second answer to
// what these tables already are.

fn rebuild_members(context: &mut entry::Context, this: u64) {
    let values = with_members(|table| table.get(&this).cloned()).unwrap_or_default();
    let members = entry::make_array_in(context, values);
    entry::put_member(context, this, "members", members);
}

/// `domain.add(emitter)` — bookkeeping (an emitter belongs to at most one
/// domain: added here, it is first dropped from every other domain's list),
/// AND, the first time any domain ever claims this emitter, a real `emit`
/// override.
///
/// [`ensure_routed`] is what makes the override real rather than a second
/// no-op: it captures the emitter's genuine `emit` — own or INHERITED, which
/// [`entry::get_member`]'s ordinary property read already walks the
/// prototype chain to find — once, in [`EMIT_ORIGINALS`], then installs a
/// wrapper as an OWN property of the emitter. An own property shadows the
/// prototype's `emit` for THIS instance only, so every other `EventEmitter`
/// on the chain is unaffected, and the wrapper still reaches the captured
/// original for every event that is not `'error'`.
pub(super) extern "C" fn add(_e: u64, this: u64, emitter: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    // Read BEFORE the tables below are mutated: the one domain (if any)
    // whose JS `.members` array is about to go stale. The sweep just below
    // drops `emitter` from every domain's [`MEMBERS`] entry — that Rust
    // table is always correct — but only `this`'s array gets REBUILT from
    // it afterward. A program reads `d.members`, the JS array, so the
    // previous owner's array was left listing an emitter already gone.
    let previous_owner = with_owner(|table| table.get(&emitter).copied());
    with_members(|table| {
        for members in table.values_mut() {
            members.retain(|&held| held != emitter);
        }
        table.entry(this).or_default().push(emitter);
    });
    with_owner(|table| {
        table.insert(emitter, this);
    });
    ensure_routed(emitter);
    entry::with_runtime(|context| {
        if let Some(previous) = previous_owner
            && previous != this
        {
            rebuild_members(context, previous);
        }
        rebuild_members(context, this);
    });
    entry::undefined_value()
}

/// `domain.remove(emitter)` — the inverse of [`add`]'s bookkeeping, by
/// identity. The `emit` override [`add`] installed is left in place rather
/// than restored: with no [`OWNER`] entry left for this emitter, [`routed_emit`]
/// finds none and falls straight through to the real original on every event,
/// which is a harmless passthrough — restoring `emit` would additionally have
/// to prove nothing reassigned it in between, which nothing here tracks.
pub(super) extern "C" fn remove(_e: u64, this: u64, emitter: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    with_members(|table| {
        if let Some(members) = table.get_mut(&this) {
            members.retain(|&held| held != emitter);
        }
    });
    with_owner(|table| {
        if table.get(&emitter) == Some(&this) {
            table.remove(&emitter);
        }
    });
    entry::with_runtime(|context| rebuild_members(context, this));
    entry::undefined_value()
}

/// Installs [`routed_emit`] on `emitter`, once — a second [`add`] of the same
/// emitter, by this domain or another, must not wrap an already-wrapped
/// `emit` a second time, or `'error'` routing would have to fall through N
/// layers of "not an error, call the inner one" to reach the real listener
/// dispatch.
fn ensure_routed(emitter: u64) {
    let already = with_originals(|table| table.contains_key(&emitter));
    if already {
        return;
    }
    let original = entry::with_runtime(|context| entry::get_member(context, emitter, "emit"));
    if !entry::with_runtime(|context| entry::is_callable_in(context, original)) {
        // Not an `EventEmitter` (or nothing ever wired `emit` on it) — there is
        // nothing to route through, and nothing to route TO either.
        return;
    }
    with_originals(|table| {
        table.insert(emitter, original);
    });
    // `emitter` itself IS the environment: it is already the exact tagged
    // value [`routed_emit`] needs to look itself up in [`OWNER`]/
    // [`EMIT_ORIGINALS`], so there is nothing to wrap it in.
    let wrapper = entry::closure_new(routed_emit as *const () as usize as i64, emitter);
    entry::with_runtime(|context| entry::put_member(context, emitter, "emit", wrapper));
}

/// The `emit` override [`ensure_routed`] installs. `'error'` with a live
/// [`OWNER`] entry is routed to the owning domain instead of reaching the
/// real listener dispatch (matching Node: the domain intercepts it, the
/// emitter's own `'error'` listeners — if any — are not additionally called);
/// every other event, and `'error'` on an emitter nobody currently owns,
/// reaches the real original unchanged.
extern "C" fn routed_emit(environment: u64, call_this: u64, event: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    let emitter = environment;
    let is_error = entry::with_runtime(|context| entry::string_in(context, event)).as_deref() == Some("error");
    if is_error && let Some(domain) = with_owner(|table| table.get(&emitter).copied()) {
        emit_error(domain, a0);
        return entry::boolean_value(true);
    }
    let original = with_originals(|table| table.get(&emitter).copied()).unwrap_or(entry::undefined_value());
    entry::call(original, call_this, event, a0, a1, a2)
}

/// `domain.emit('error', err)` on `domain` — the routing primitive
/// [`intercept`] and [`routed_emit`] both end in. Reads `emit` back off the
/// domain itself rather than assuming a fixed one: `Domain.prototype`
/// chains onto the real `EventEmitter.prototype` (`mod.rs::namespace`), so
/// an ordinary property read finds it, own or inherited, the same way
/// [`ensure_routed`] finds an emitter's.
fn emit_error(domain: u64, err: u64) {
    let emit = entry::with_runtime(|context| {
        let candidate = entry::get_member(context, domain, "emit");
        entry::is_callable_in(context, candidate).then_some(candidate)
    });
    match emit {
        Some(emit) => {
            let label = entry::with_runtime(|context| entry::make_string(context, "error"));
            let undefined = entry::undefined_value();
            entry::call(emit, domain, label, err, undefined, undefined);
        }
        None => eprintln!(
            "node:domain: an error reached a domain whose own `emit` is not callable — nothing was routed (Domain.prototype should chain onto the real EventEmitter.prototype; see the module doc)"
        ),
    }
}

/// `domain.bind(callback)` — hands back a wrapper with `callback`'s own
/// signature that runs it with THIS domain active (`enter`/exit around the
/// call, same as `run`). See the module doc for what it does not do: catch
/// a throw from inside `callback`.
pub(super) extern "C" fn bind(_e: u64, this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if !entry::with_runtime(|context| entry::is_callable_in(context, callback)) {
        return entry::undefined_value();
    }
    let environment = entry::make_array(vec![this, callback]);
    entry::closure_new(bound as *const () as usize as i64, environment)
}

/// The wrapper [`bind`] hands back.
extern "C" fn bound(environment: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let domain = entry::element_at(environment, entry::make_number(0.0));
    let callback = entry::element_at(environment, entry::make_number(1.0));
    STACK.with(|stack| stack.borrow_mut().push(domain));
    refresh_active_namespace();
    let result = entry::call(callback, this, a0, a1, a2, a3);
    STACK.with(|stack| {
        stack.borrow_mut().pop();
    });
    refresh_active_namespace();
    result
}

/// `domain.intercept(callback)` — hands back a `(err, ...rest) => R` wrapper.
/// A non-null/non-undefined `err` is routed to `'error'` DIRECTLY —
/// `callback` is never called in that case, so this half needs no
/// throw-catching at all: the decision is made by reading `err`, before
/// anything runs. Otherwise runs exactly like [`bind`]'s wrapper, with the
/// same limitation for a throw from inside `callback`.
pub(super) extern "C" fn intercept(_e: u64, this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if !entry::with_runtime(|context| entry::is_callable_in(context, callback)) {
        return entry::undefined_value();
    }
    let environment = entry::make_array(vec![this, callback]);
    entry::closure_new(intercepted as *const () as usize as i64, environment)
}

/// The wrapper [`intercept`] hands back. `err` costs one of the four argument
/// slots, so only three of `callback`'s own are left to forward — the same
/// four-slot trade the reference doc's own type (`Parameters<F>` minus one)
/// already implies.
extern "C" fn intercepted(environment: u64, this: u64, err: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    let domain = entry::element_at(environment, entry::make_number(0.0));
    let undefined = entry::undefined_value();
    if err != undefined && err != entry::null_value() {
        emit_error(domain, err);
        return undefined;
    }
    let callback = entry::element_at(environment, entry::make_number(1.0));
    STACK.with(|stack| stack.borrow_mut().push(domain));
    refresh_active_namespace();
    let result = entry::call(callback, this, a0, a1, a2, undefined);
    STACK.with(|stack| {
        stack.borrow_mut().pop();
    });
    refresh_active_namespace();
    result
}
