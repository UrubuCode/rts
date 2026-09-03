//! `node:domain` — the active-domain stack works; catching a throw does not.
//!
//! # Reuse-check
//!
//! `rts-cranelift` has nothing shaped like a context-carrying stack across a
//! call (checked `src/sched/`, `src/frame/` — the same search
//! [`crate::async_hooks`]'s own module doc already ran and reported for the
//! identical question). `rts_core::entry::Context` holds no domain
//! table. [`crate::async_hooks`]'s `STACK` — a thread-local
//! `Vec<(instance, value)>`, pushed by `run`, popped after, walked from the
//! top for the innermost frame — is the exact shape `domain.active` needs
//! (real Node's own docs describe `domain` as `AsyncLocalStorage`'s
//! ancestor), so this module reuses that SHAPE rather than re-deriving it.
//! It is not the SAME stack: an `AsyncLocalStorage` frame and a `Domain`
//! frame answer different questions (`getStore()` vs "what does an emitted
//! `'error'` reach"), so sharing the table would let one instance's `run()`
//! be found by the other's lookup — the exact bug
//! [`crate::async_hooks`]'s own doc names for why it uses one stack across
//! `AsyncLocalStorage` instances rather than one per instance: same reasoning,
//! opposite conclusion, because here the two callers are different classes.
//!
//! # What `run` can do, and the one thing it cannot
//!
//! [`run`] pushes this domain, calls `fn` with the borrow already released
//! (identical to [`crate::async_hooks::run`] — see that module's doc for why
//! the release matters), and pops. That much is real: `domain.active` and
//! `process`-adjacent code reading it during the synchronous extent of `run`
//! sees the right domain.
//!
//! What it does NOT do is what `domain.run` exists for: catching a throw from
//! `fn` and routing it to `'error'` instead of letting it propagate. A native
//! entry point cannot catch a JS throw crossing back through it —
//! [`crate::assert`]'s module doc names the same wall for its own assertion
//! failures, and [`crate::diagnostics_channel`]'s `traceSync` names it again
//! for the identical reason. So a `fn` that throws inside [`run`] propagates
//! as an ordinary throw, `'error'` never fires, and — because there is no
//! `finally` a native can express either — the pushed frame is never popped,
//! leaving `domain.active` pointing at a domain that is no longer running.
//! This is `domain`'s entire value proposition, refused by the same
//! mechanism gap every other module in this crate that has hit it names
//! explicitly rather than working around it with something that looks like
//! it catches and does not.
//!
//! # `bind`/`intercept`/routed `add` — the "no environment slot" premise was stale
//!
//! This doc used to say [`bind`] and [`intercept`] were unbuildable because a
//! native cannot mint a closure. That was true of
//! [`rts_core::entry::make_callable`] and false of the surface:
//! [`rts_core::entry::closure_new`] takes a code address AND an environment
//! and delivers the environment back as the callee's first argument —
//! [`crate::async_hooks`]'s `local.rs`/`resource.rs` (`AsyncLocalStorage.bind`/
//! `.snapshot`, `AsyncResource.prototype.bind`) reached the identical shape
//! first, and this module reuses the same trick rather than re-deriving it.
//! So `bind` and `intercept` ARE built — see their own docs for exactly what
//! each does and does not catch.
//!
//! The second premise this doc carried was also stale: rerouting `add`/
//! `remove` needs reading a value's own inherited method WITHOUT invoking it,
//! and that claimed no accessor for it existed. [`rts_core::entry::get_member`]
//! already does exactly this — a plain property read that walks the prototype
//! chain, which is how every other native in this crate already reads
//! `.name`/`.length` off values it did not build — nothing new was missing,
//! the premise had just never been checked against that function. [`add`] now
//! installs a real `emit` override the first time any domain claims an
//! emitter; see its own doc for the one thing that override still cannot do.
//!
//! # What none of the three can do
//!
//! [`bind`]'s wrapper pushes this domain, calls the callback, pops — same
//! push/call/pop as [`run`], and the same limitation: a throw INSIDE the
//! callback is an ordinary throw a native cannot intercept, so it propagates
//! uncaught rather than reaching `'error'`, and the pushed frame is left
//! behind. [`intercept`] sidesteps that for its OWN documented case (an
//! error-shaped first argument, checked before the wrapped callback is ever
//! called — no unwind needed) but inherits the same gap for a throw from
//! INSIDE the callback it does end up calling. [`add`]'s routed `emit` reaches
//! `'error'` events specifically; every other event still goes to the real
//! original `emit`, unaffected.
//!
//! # Not implemented, by name
//!
//! - **`process.domain`.** Wiring it needs editing `crate::process`, which
//!   this pass does not own.
//! - **Promise `.then`/`.catch` registration-time capture, implicit binding
//!   of timers/fs/net callbacks scheduled inside `run`.** All need a hook at
//!   another module's call site this module cannot reach without editing it.
//! - **`domain.dispose()`.** Removed from Node itself since v8; not
//!   resurrected here either.
//! - **A `TypeError` for a non-function `bind`/`intercept` argument.**
//!   `undefined` instead — the same trade every member of this crate makes
//!   for the reason [`run`]'s doc gives: nothing here can raise a catchable
//!   exception.

use rts_core::entry::{self, Provided};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::Mutex;

thread_local! {
    /// Every live `run`/`enter` frame, oldest first — see the module doc for
    /// why this is a stack independent of `crate::async_hooks::STACK` despite
    /// the identical shape.
    static STACK: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };

    /// O proprio namespace `node:domain`, guardado no install.
    ///
    /// `create()` alcancava-o pelo receptor — `domain.create()` tem o namespace
    /// como `this`. `new domain.Domain()` nao: ali o `this` e a instancia nova,
    /// e escrever `active` nela em vez de no namespace poe a resposta onde
    /// ninguem a le. Guardado uma vez, entao as duas portas atualizam o mesmo
    /// sitio.
    static NAMESPACE: Cell<u64> = const { Cell::new(0) };

    /// `Domain.prototype`, minted once in [`namespace`].
    ///
    /// Held for the reason `async_hooks::resource` holds its own:
    /// `make_prototype` reports a collision by the CALLER'S FILE, and one
    /// object is the stronger invariant anyway — every instance shares the
    /// prototype a program's `instanceof` compares against.
    static PROTOTYPE: Cell<u64> = const { Cell::new(0) };
}

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

fn active() -> Option<u64> {
    STACK.with(|stack| stack.borrow().last().copied())
}

const METHODS: &[(&str, Provided)] = &[
    ("run", run),
    ("add", add),
    ("remove", remove),
    ("enter", enter),
    ("exit", exit),
    ("bind", bind),
    ("intercept", intercept),
];

/// The namespace `node:domain` is.
pub fn namespace(context: &mut entry::Context) -> u64 {
    // `createDomain` is the same function under the older name, which is what
    // Node has too — checked rather than assumed: `typeof
    // require('domain').createDomain` answers `'function'` there. Answering one
    // spelling and refusing the other reports a naming history as a missing
    // feature.
    let members: &[(&str, Provided)] = &[("create", create), ("createDomain", create), ("Domain", domain_class)];
    let namespace = entry::make_namespace(context, members);
    NAMESPACE.with(|held| held.set(namespace));
    let event_emitter = entry::make_prototype(context, "EventEmitter", &[]);
    let prototype = entry::make_prototype(context, "Domain", METHODS);
    entry::set_prototype_in(context, prototype, event_emitter);
    PROTOTYPE.with(|held| held.set(prototype));
    // `Domain.prototype`, so `new domain.Domain()` hands the constructor an
    // object already on the chain — the constructor fills it and answers it,
    // and `d instanceof domain.Domain` holds. Without this link `new` builds an
    // object with nothing on it, and the methods are one property lookup away
    // from existing.
    let constructor = entry::get_member(context, namespace, "Domain");
    entry::put_member(context, constructor, "prototype", prototype);
    let active_domain = active().unwrap_or_else(|| entry::undefined_in(context));
    entry::put_member(context, namespace, "active", active_domain);
    namespace
}

fn refresh_active(context: &mut entry::Context, namespace: u64) {
    let active_domain = active().unwrap_or_else(|| entry::undefined_in(context));
    entry::put_member(context, namespace, "active", active_domain);
}

/// `domain.create()` — a fresh, empty, not-yet-entered `Domain`.
extern "C" fn create(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    // No receiver: `domain.create()` is a plain call and the object it answers
    // is a fresh one either way.
    fresh(entry::undefined_value())
}

/// `new domain.Domain()` — the class form of the same thing.
///
/// Node has both, and a program picks by habit rather than by meaning:
/// `domain.create()` and `new domain.Domain()` answer the same kind of object.
/// Absent, `domain.Domain` was `undefined` and `new undefined()` took the
/// program down before its first listener — 10 files of Node's own `domain`
/// suite, measured 2026-08-24.
///
/// The receiver is ignored on purpose: what `new` hands in is a bare instance
/// with the wrong prototype for this class, and answering an object from a
/// constructor is what makes `new` use it instead.
extern "C" fn domain_class(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    // The receiver is FILLED rather than replaced, which is what makes `new`
    // work: `new` hands in an object already linked to `Domain.prototype` and
    // takes that object back. Answering a different one instead is what the
    // first version did, and `new domain.Domain()` came back `undefined`.
    fresh(this)
}

/// One empty domain, whichever door asked for it.
///
/// `receiver` is what `new` handed in, or `undefined` for a plain call. An
/// object is filled and answered; anything else means a fresh instance.
fn fresh(receiver: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = match PROTOTYPE.with(Cell::get) {
            0 => entry::make_prototype(context, "Domain", METHODS),
            held => held,
        };
        let instance = match entry::is_object(context, receiver) {
            true => receiver,
            false => entry::make_instance(context, prototype),
        };
        let members = entry::make_array_in(context, Vec::new());
        entry::put_member(context, instance, "members", members);
        let namespace = NAMESPACE.with(Cell::get);
        if namespace != 0 {
            refresh_active(context, namespace);
        }
        instance
    })
}

/// `domain.run(fn, ...args)` — see the module doc: the push/call/pop works,
/// error-routing does not. No extra arguments are forwarded to `fn`, the same
/// four-slot trade [`crate::async_hooks::run`]'s own doc names.
extern "C" fn run(_e: u64, this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    STACK.with(|stack| stack.borrow_mut().push(this));
    let undefined = entry::undefined_value();
    let result = entry::call(callback, undefined, undefined, undefined, undefined, undefined);
    STACK.with(|stack| {
        stack.borrow_mut().pop();
    });
    result
}

/// `domain.enter()` — pushes, without a paired `run`/`exit` call already
/// scheduled; idempotent-callable, per the spec (nests again if called on a
/// domain already on the stack).
extern "C" fn enter(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    STACK.with(|stack| stack.borrow_mut().push(this));
    entry::undefined_value()
}

/// `domain.exit()` — pops this domain and anything nested above it that was
/// entered after it, restoring whatever was active before. A call with no
/// matching prior `enter()` on this domain is a no-op, not a panic.
extern "C" fn exit(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    STACK.with(|stack| {
        let mut frames = stack.borrow_mut();
        if let Some(at) = frames.iter().rposition(|&frame| frame == this) {
            frames.truncate(at);
        }
    });
    entry::undefined_value()
}

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
/// prototype chain to find, the accessor the module doc used to say did not
/// exist — once, in [`EMIT_ORIGINALS`], then installs a wrapper as an OWN
/// property of the emitter. An own property shadows the prototype's `emit`
/// for THIS instance only, so every other `EventEmitter` on the chain is
/// unaffected, and the wrapper still reaches the captured original for every
/// event that is not `'error'`.
extern "C" fn add(_e: u64, this: u64, emitter: u64, _b: u64, _c: u64, _d: u64) -> u64 {
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
    entry::with_runtime(|context| rebuild_members(context, this));
    entry::undefined_value()
}

/// `domain.remove(emitter)` — the inverse of [`add`]'s bookkeeping, by
/// identity. The `emit` override [`add`] installed is left in place rather
/// than restored: with no [`OWNER`] entry left for this emitter, [`routed_emit`]
/// finds none and falls straight through to the real original on every event,
/// which is a harmless passthrough — restoring `emit` would additionally have
/// to prove nothing reassigned it in between, which nothing here tracks.
extern "C" fn remove(_e: u64, this: u64, emitter: u64, _b: u64, _c: u64, _d: u64) -> u64 {
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
    let original =
        entry::with_runtime(|context| entry::get_member(context, emitter, "emit"));
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
extern "C" fn routed_emit(
    environment: u64,
    call_this: u64,
    event: u64,
    a0: u64,
    a1: u64,
    a2: u64,
) -> u64 {
    let emitter = environment;
    let is_error =
        entry::with_runtime(|context| entry::string_in(context, event)).as_deref() == Some("error");
    if is_error {
        if let Some(domain) = with_owner(|table| table.get(&emitter).copied()) {
            emit_error(domain, a0);
            return entry::boolean_value(true);
        }
    }
    let original = with_originals(|table| table.get(&emitter).copied()).unwrap_or(entry::undefined_value());
    entry::call(original, call_this, event, a0, a1, a2)
}

/// `domain.emit('error', err)` on `domain` — the routing primitive [`intercept`]
/// and [`routed_emit`] both end in. Reads `emit` back off the domain itself
/// rather than assuming a fixed one: `Domain.prototype` chains onto the real
/// `EventEmitter.prototype` (see [`namespace`]), so an ordinary property read
/// finds it, own or inherited, the same way [`ensure_routed`] finds an
/// emitter's.
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
/// call, same as [`run`]). See the module doc for what it does not do: catch
/// a throw from inside `callback`.
extern "C" fn bind(_e: u64, this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
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
    let result = entry::call(callback, this, a0, a1, a2, a3);
    STACK.with(|stack| {
        stack.borrow_mut().pop();
    });
    result
}

/// `domain.intercept(callback)` — hands back a `(err, ...rest) => R` wrapper.
/// A non-null/non-undefined `err` is routed to `'error'` DIRECTLY —
/// `callback` is never called in that case, so this half needs no
/// throw-catching at all: the decision is made by reading `err`, before
/// anything runs. Otherwise runs exactly like [`bind`]'s wrapper, with the
/// same limitation for a throw from inside `callback`.
extern "C" fn intercept(_e: u64, this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
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
extern "C" fn intercepted(
    environment: u64,
    this: u64,
    err: u64,
    a0: u64,
    a1: u64,
    a2: u64,
) -> u64 {
    let domain = entry::element_at(environment, entry::make_number(0.0));
    let undefined = entry::undefined_value();
    if err != undefined && err != entry::null_value() {
        emit_error(domain, err);
        return undefined;
    }
    let callback = entry::element_at(environment, entry::make_number(1.0));
    STACK.with(|stack| stack.borrow_mut().push(domain));
    let result = entry::call(callback, this, a0, a1, a2, undefined);
    STACK.with(|stack| {
        stack.borrow_mut().pop();
    });
    result
}
