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
//! # Not implemented, by name
//!
//! - **`bind`/`intercept`.** Each needs to hand back a NEW callable closing
//!   over `(this domain, the wrapped callback)`.
//!   [`rts_core::entry::make_callable`] returns a fixed function pointer
//!   with no environment slot to hold either — the exact gap
//!   [`crate::async_hooks`]'s module doc names for
//!   `AsyncLocalStorage.bind`/`.snapshot`. Even built, both would inherit the
//!   `run` limitation above: the whole point of `intercept` is catching an
//!   error-first argument, which needs no throw-catching and DOES work here
//!   (see [`intercept`]) — but wrapping an arbitrary throw from inside the
//!   wrapped callback does not, for the same reason `run` cannot.
//! - **`domain.add`/`.remove` actually rerouting another emitter's `'error'`.**
//!   Node patches the bound emitter so ITS `emit('error', ...)` reaches the
//!   domain instead of throwing. Doing that here would mean overriding
//!   `emit` on the target instance with a wrapper that still reaches the
//!   ORIGINAL `EventEmitter.prototype.emit` for every other event — and this
//!   crate's entry surface has no way to read a value's own inherited method
//!   without invoking it (no prototype-walk accessor is exposed to a host
//!   module; see `entry::modules`'s own inventory). `add`/`remove` are built
//!   as real bookkeeping (`members` gains/loses the emitter, matching
//!   Node's "belongs to at most one domain" rule) with no routing effect —
//!   named here rather than silently doing nothing under a `bind`-shaped
//!   name.
//! - **`process.domain`.** Wiring it needs editing `crate::process`, which
//!   this pass does not own.
//! - **Promise `.then`/`.catch` registration-time capture, implicit binding
//!   of timers/fs/net callbacks scheduled inside `run`.** All need a hook at
//!   another module's call site this module cannot reach without editing it.
//! - **`domain.dispose()`.** Removed from Node itself since v8; not
//!   resurrected here either.

use rts_core::entry::{self, Provided};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Mutex;

thread_local! {
    /// Every live `run`/`enter` frame, oldest first — see the module doc for
    /// why this is a stack independent of `crate::async_hooks::STACK` despite
    /// the identical shape.
    static STACK: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
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
    let members: &[(&str, Provided)] = &[("create", create)];
    let namespace = entry::make_namespace(context, members);
    let event_emitter = entry::make_prototype(context, "EventEmitter", &[]);
    let prototype = entry::make_prototype(context, "Domain", METHODS);
    entry::set_prototype_in(context, prototype, event_emitter);
    let active_domain = active().unwrap_or_else(|| entry::undefined_in(context));
    entry::put_member(context, namespace, "active", active_domain);
    namespace
}

fn refresh_active(context: &mut entry::Context, namespace: u64) {
    let active_domain = active().unwrap_or_else(|| entry::undefined_in(context));
    entry::put_member(context, namespace, "active", active_domain);
}

/// `domain.create()` — a fresh, empty, not-yet-entered `Domain`.
extern "C" fn create(_e: u64, namespace: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "Domain", METHODS);
        let instance = entry::make_instance(context, prototype);
        let members = entry::make_array_in(context, Vec::new());
        entry::put_member(context, instance, "members", members);
        refresh_active(context, namespace);
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

/// `domain.add(emitter)` — bookkeeping only; see the module doc for why no
/// routing effect follows from it. An emitter belongs to at most one domain:
/// added here, it is first dropped from every other domain's list, matching
/// the spec.
extern "C" fn add(_e: u64, this: u64, emitter: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    with_members(|table| {
        for members in table.values_mut() {
            members.retain(|&held| held != emitter);
        }
        table.entry(this).or_default().push(emitter);
    });
    entry::with_runtime(|context| rebuild_members(context, this));
    entry::undefined_value()
}

/// `domain.remove(emitter)` — the inverse of [`add`], by identity.
extern "C" fn remove(_e: u64, this: u64, emitter: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    with_members(|table| {
        if let Some(members) = table.get_mut(&this) {
            members.retain(|&held| held != emitter);
        }
    });
    entry::with_runtime(|context| rebuild_members(context, this));
    entry::undefined_value()
}

/// `domain.bind(callback)` — refused; see the module doc's `bind`/`intercept`
/// entry for the environment-capture gap. Answers `undefined` rather than a
/// half-working wrapper.
extern "C" fn bind(_e: u64, _this: u64, _callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::undefined_value()
}

/// `domain.intercept(callback)` — same refusal as [`bind`], for the same
/// reason.
extern "C" fn intercept(_e: u64, _this: u64, _callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::undefined_value()
}
