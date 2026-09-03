//! `node:domain` — event routing and the active-domain stack now work;
//! catching a throw crossing a native call boundary does not, which matches
//! a real gap in Node's own `Domain.prototype.run` (see below).
//!
//! # Why this is a folder and not one file
//!
//! Fixing `domain.active` and `Domain`'s missing `EventEmitter` state (both
//! below) pushed this past the workspace's 500-line ceiling. Split by what
//! each half needs from the other: this file is construction and the
//! active-domain stack (`namespace`, `create`, `fresh`,
//! `run`/`enter`/`exit`); [`routing`] is `add`/`remove`'s membership
//! bookkeeping and the `'error'`-routing `emit` override (`bind`/`intercept`
//! too, since they push the same stack this file owns).
//!
//! # Reuse-check
//!
//! `rts-cranelift` has nothing shaped like a context-carrying stack across a
//! call (checked `src/sched/`, `src/frame/` — the same search
//! [`crate::async_hooks`]'s own module doc already ran). `Context` holds no
//! domain table. [`crate::async_hooks`]'s `STACK` shape — pushed by `run`,
//! popped after, walked from the top — is what `domain.active` needs (Node's
//! own docs call `domain` `AsyncLocalStorage`'s ancestor), so this module
//! reuses the SHAPE, not the table: an `AsyncLocalStorage` frame and a
//! `Domain` frame answer different questions, so sharing the table would let
//! one instance's `run()` be found by the other's lookup.
//!
//! # A `Domain` instance was never actually an `EventEmitter` — FIXED
//!
//! [`fresh`] chained `Domain.prototype` onto a same-named "EventEmitter"
//! prototype for the METHODS (`on`, `emit`, …) but never ran the real `new
//! EventEmitter()` constructor (`crate::events::make_emitter`), which is
//! what builds an instance's OWN `__events__`/`__eventNames__` — the state
//! every method in `crate::events` reads and writes with no other path. So
//! `d.on(...)` wrote into `get_indexed(undefined, …)` and `d.emit` never
//! found a listener, for ANY event, `'error'` included — this module's
//! entire reason to exist. [`fresh`] now builds both properties itself; see
//! its own doc for why inline rather than by calling `make_emitter`.
//!
//! # `domain.active` was frozen — FIXED
//!
//! `refresh_active` used to run only from [`fresh`] (`create()`/`new
//! Domain()`), which never touches `STACK` — so `active` answered whatever
//! domain object was built last, never the one a program was actually
//! inside a `run()`/`enter()`/`bind()`/`intercept()` for. Every site that
//! pushes or pops the stack now refreshes it, matching real Node's own
//! `enter()`/`exit()`, which write `exports.active` themselves on every
//! push and pop — checked against Node v20.19.5, including its `null`-
//! before-anything-runs vs. `undefined`-after-any-`exit()` asymmetry, and
//! its own leak: a domain whose `run()` callback throws stays on the stack
//! forever in Node too (`exit()` runs sequentially after `fn(...)`, no
//! try/finally), so a caller that chains a throwing `run()` before reading
//! `active` should not expect a clean `null`/`undefined` afterward — Node
//! does not answer that either.
//!
//! # What `run` still cannot do
//!
//! [`run`] pushes, calls `fn`, pops — real for a normally-returning `fn`.
//! What it does NOT do is what `domain.run` exists for: catching a throw
//! from `fn` and routing it to `'error'` instead of letting it propagate. A
//! native cannot catch a JS throw crossing back through it —
//! [`crate::assert`]'s doc names the same wall — so a throwing `fn` leaves
//! `'error'` unfired and the pushed frame unpopped, which (see above) is not
//! a divergence from Node either. `bind`/`intercept` inherit the same gap
//! for a throw from inside the wrapped callback.
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
//!   `undefined` instead — the same trade every member of this crate makes:
//!   nothing here can raise a catchable exception.

mod routing;

use rts_core::entry::{self, Provided};
use std::cell::Cell;
use std::cell::RefCell;

thread_local! {
    /// Every live `run`/`enter` frame, oldest first — see the module doc for
    /// why this is a stack independent of `crate::async_hooks::STACK` despite
    /// the identical shape. `pub(super)`: [`routing`]'s `bind`/`intercept`
    /// wrappers push and pop it too.
    pub(super) static STACK: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };

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

fn active() -> Option<u64> {
    STACK.with(|stack| stack.borrow().last().copied())
}

const METHODS: &[(&str, Provided)] = &[
    ("run", run),
    ("add", routing::add),
    ("remove", routing::remove),
    ("enter", enter),
    ("exit", exit),
    ("bind", routing::bind),
    ("intercept", routing::intercept),
];

/// The namespace `node:domain` is.
pub fn namespace(context: &mut entry::Context) -> u64 {
    // ONE native for `create`. Node's own `exports.createDomain =
    // exports.create = function create(...)` is one function under two
    // names (`domain.createDomain === domain.create` there), but listing
    // `("createDomain", create)` as a SECOND table entry would not
    // reproduce that: `install` mints a fresh callable per `(name,
    // Provided)` pair even when two share a function pointer. Aliased below
    // instead, onto the exact object `"create"` built.
    let members: &[(&str, Provided)] = &[("create", create), ("Domain", domain_class)];
    let namespace = entry::make_namespace(context, members);
    let create_fn = entry::get_member(context, namespace, "create");
    entry::put_member(context, namespace, "createDomain", create_fn);
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
    // Real Node's module-level default (`exports.active = null;`), written
    // straight rather than through [`refresh_active`]: that fallback is
    // `undefined`, matching `exit()`'s `stack.length === 0 ? undefined : …`
    // — the answer once a domain has entered and exited, not the answer
    // before the first one. Checked against real Node v20.19.5: `null`
    // before anything runs, `undefined` (not `null` again) from the first
    // `enter`/`exit` pair onward.
    entry::put_member(context, namespace, "active", entry::null_in(context));
    namespace
}

/// Re-reads [`active`] and writes it onto the namespace's `active` property —
/// what real Node's `Domain.prototype.enter`/`.exit` do on every push and
/// pop. The empty-stack fallback is `undefined`, not `null`; [`namespace`]
/// is where the one-time `null` default is written instead.
fn refresh_active(context: &mut entry::Context, namespace: u64) {
    let active_domain = active().unwrap_or_else(|| entry::undefined_in(context));
    entry::put_member(context, namespace, "active", active_domain);
}

/// [`refresh_active`] from outside a borrow, called from every site that
/// pushes or pops [`STACK`]: [`run`]/[`enter`]/[`exit`] here, and
/// [`routing`]'s `bind`/`intercept` wrappers (`pub(super)` for exactly that).
///
/// It used to be called from nowhere but [`fresh`] (`create()`/`new
/// Domain()`), which never touches [`STACK`] — so `domain.active` answered
/// whatever the LAST domain object happened to be built, frozen at that
/// moment, never the domain a `run()`/`enter()` was actually inside.
pub(super) fn refresh_active_namespace() {
    let namespace = NAMESPACE.with(Cell::get);
    if namespace != 0 {
        entry::with_runtime(|context| refresh_active(context, namespace));
    }
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
        // Chaining onto "EventEmitter" (in [`namespace`]) only gets an
        // instance the METHODS by prototype walk; every one of them reads or
        // writes the instance's OWN `__events__`/`__eventNames__` —
        // `crate::events::make_emitter`'s state, and nothing else in this
        // crate ever ran that constructor for a `Domain`. Before this fix
        // `d.on(...)` wrote into `get_indexed(undefined, …)`
        // (`d.hasOwnProperty("__events__")` was `false`) and `d.emit` found
        // zero listeners for every event, `'error'` included — this
        // module's entire reason to exist. Built inline rather than by
        // calling `events::make_emitter`: that native's fixed `Provided`
        // signature has no slot for an instance that already exists.
        let events = entry::make_object(context);
        entry::put_member(context, instance, "__events__", events);
        let event_names = entry::make_array_in(context, Vec::new());
        entry::put_member(context, instance, "__eventNames__", event_names);
        let members = entry::make_array_in(context, Vec::new());
        entry::put_member(context, instance, "members", members);
        // NOT `refresh_active_namespace()` here: real Node's
        // `create()`/`new Domain()` do not touch `exports.active` (checked
        // directly — unchanged before/after a bare `domain.create()`); only
        // `enter`/`exit`/`run`/`bind`/`intercept` do. Calling it here was
        // this module's `domain.active` bug in a second way: every
        // CONSTRUCTION overwrote the answer with whatever the stack held.
        instance
    })
}

/// `domain.run(fn, ...args)` — see the module doc: the push/call/pop works,
/// error-routing does not. No extra arguments are forwarded to `fn`, the same
/// four-slot trade [`crate::async_hooks::run`]'s own doc names.
extern "C" fn run(_e: u64, this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    STACK.with(|stack| stack.borrow_mut().push(this));
    refresh_active_namespace();
    let undefined = entry::undefined_value();
    let result = entry::call(callback, undefined, undefined, undefined, undefined, undefined);
    // Reached only when `callback` returns normally: a throw crossing back
    // through `entry::call` unwinds this native frame too ([`crate::assert`]
    // names the same wall), so the pop and refresh below never run for an
    // escaped exception — matching real Node's own `run`, whose `exit()`
    // call is sequential after `fn(...)` with no try/finally either (checked
    // directly: a throwing `run()` leaves `domain.active` on the throwing
    // domain in v20.19.5 too).
    STACK.with(|stack| {
        stack.borrow_mut().pop();
    });
    refresh_active_namespace();
    result
}

/// `domain.enter()` — pushes, without a paired `run`/`exit` call already
/// scheduled; idempotent-callable, per the spec (nests again if called on a
/// domain already on the stack).
extern "C" fn enter(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    STACK.with(|stack| stack.borrow_mut().push(this));
    refresh_active_namespace();
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
    refresh_active_namespace();
    entry::undefined_value()
}
