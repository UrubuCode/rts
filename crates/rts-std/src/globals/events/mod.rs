//! `Event`, `EventTarget`, `CustomEvent`, `AbortController`, `AbortSignal` —
//! the DOM-shaped event globals every non-browser JavaScript host provides.
//!
//! # Why these are globals and not a module
//!
//! The reason [`crate::console`] gives for itself: a program writes
//! `new AbortController()` with no import line, so the value has to be reachable
//! by NAME. All five names are already in `rts-codegen`'s `PROVIDED` list, whose
//! own comment states the rule this module exists to satisfy — *"every name
//! below has a real value in this workspace's host"* — because a listed name
//! with no value turns a refusal at compile time into an `undefined` at run
//! time, which makes a compile-rate measurement count a file that cannot work.
//!
//! # Reuse-check: what was searched, and what was found
//!
//! - **`rts-cranelift`** — `src/sched/` (`SchedulerId`, `Delivery`,
//!   `ContinuationId`) is the machine's answer to "what runs in what order", and
//!   it does **not** answer this. It owns the ordering the compiler lowers
//!   `await` onto; a listener list is a *value*, which the machine layer does not
//!   know exists, and `dispatchEvent` is synchronous — nothing is scheduled at
//!   all. `node:timers`' own reuse-check reached the identical conclusion.
//! - **`rts-core::entry`** (`modules.rs`, read in full) — `make_prototype`,
//!   `make_instance`, `make_callable`, `set_prototype_in` and `declare_global`
//!   ARE the answer for how a class is built from outside that crate, and every
//!   one of them is called rather than reproduced. `declare_loop_source` is the
//!   answer for "this must happen later"; see `AbortSignal.timeout` below.
//! - **`rts-node/src/events.rs`** — `EventEmitter`, read in full. Its
//!   *storage shape* is reused deliberately (below); its *implementation* is
//!   not, because the two are different contracts rather than two spellings of
//!   one. An `EventEmitter` listener is called with the emitted arguments
//!   spread; an `EventTarget` listener is called with exactly one `Event`
//!   object. `once` is a separate registration method there and an option here.
//!   `emit` answers whether anything listened; `dispatchEvent` answers whether
//!   `preventDefault` was *not* called. A shared implementation would have to
//!   branch on which contract it was serving at every one of those four points,
//!   which is two implementations wearing one name.
//! - **`rts-std/src/globals/event_target/` and `.../abort/`** — the OLD engine's
//!   version of this exact surface. Not reachable from here at all: it is
//!   written against `rts_engine::heap::handles` and `__rtsadp_*` link-resolved
//!   externs, which is the value encoding `rts_cranelift::tags` replaced. It is
//!   named here so a reader does not conclude this was written without looking.
//!
//! # What IS shared with `node:events`: the shape, not the key
//!
//! Listeners live in an ordinary own property holding one array per type, each
//! element a small record object — `node:events`' `__events__` shape exactly,
//! because it is right and because it needs nothing this crate would have to
//! invent. The property is named `__listeners__` rather than `__events__`, and
//! that difference is the point: an object carrying both would have one store
//! read by two callers that disagree about how a listener is invoked, and
//! `emit('x', 1, 2)` would call an `EventTarget` listener with `(1, 2)` where it
//! expects `(event)`. Distinct keys make that unrepresentable instead of
//! merely unlikely.
//!
//! # `AbortSignal.timeout` is implemented, not refused
//!
//! The obvious reading is that a timeout needs `node:timers`, which this crate
//! does not depend on and must not. That reading is wrong, and checking it is
//! what `rts-host`'s rule 3 asks for: `rts-core::entry::loops` exists
//! precisely so a module owns its own deadline table and registers a `fn`
//! pointer the host pumps — six modules in `rts-node` do it, `node:timers`
//! among them. Using that contract is reuse; depending on `node:timers` would
//! have been the duplication.
//!
//! A pending timeout answers [`Pending::Blocked`](rts_core::entry::Pending),
//! so it does **not** hold the program open. That is faithful rather than
//! convenient — Node unrefs the timer behind `AbortSignal.timeout`, so a signal
//! does not keep its loop alive there either — and it is the same call
//! `entry::loops` already made for this engine, which states that an interval
//! and a listening socket both end a program with nothing else to do. A program
//! whose only outstanding work is the timeout therefore exits without aborting.
//!
//! # What a program can see that Node hides
//!
//! `__listeners__` and `__stopImmediate__` are ordinary own properties, so
//! `Object.keys(target)` lists them where Node's internal slots are invisible.
//! Every property this module writes is a writable data property, not an
//! accessor: `event.type = "x"` and `signal.aborted = true` are assignments that
//! stick, where the specification makes them read-only. Both are consequences of
//! building a class from outside `rts-core`, where `#[rtse::class]` and its
//! getters are not available — stated rather than discovered.
//!
//! # Not implemented, by name
//!
//! `Event`: `composedPath()`, `initEvent()`, `cancelBubble`, `returnValue`,
//! `srcElement`, `timeStamp`. `timeStamp` in particular is refused rather than
//! approximated — Node's is milliseconds since a process-relative time origin,
//! and nothing here has that origin, so a wall-clock `Date.now()` would be a
//! number of a different magnitude wearing the same name. `new Event()` with no
//! argument does not raise the `TypeError` the specification requires: nothing
//! in this engine can raise one where a handler could catch it (`rts-codegen`
//! refuses to compile a `try` whose body contains a call), so the missing type
//! coerces as `String(undefined)` does. A `type` argument that is an object
//! coerces to `""` rather than running its `toString`, which is the boundary
//! every coercion on this surface stops at.
//!
//! `EventTarget`: the `signal` and `passive` entries of `AddEventListenerOptions`
//! — `passive` is a hint Node does not enforce either, and `signal`
//! (remove-this-listener-when-that-aborts) needs a per-registration reaction a
//! native callable cannot carry; see `AbortSignal.any` below for the same wall.
//! `capture` is honored only as part of a registration's identity, which is all
//! Node uses it for — there is no capture phase here and none there. No
//! `InvalidStateError` for redispatching an in-flight event. A listener that
//! throws ends the program rather than being forwarded to `process.on('error')`.
//!
//! `AbortSignal.any(signals)` — refused by name: the composite has to register a
//! reaction on every input signal, and `entry::make_callable` hands back a fixed
//! function pointer with no environment slot to put the composite in, so nothing
//! reachable from an input's listener list could name it. The identical gap
//! `node:events` records for its once-wrapper `rawListeners`.
//!
//! `signal.reason`'s default is a real `DOMException` — `globals/dom_exception.rs`
//! is the class, and this module's prose used to say the name had no value at
//! all. `signal.throwIfAborted()` ends the program
//! instead of throwing; the `abort` submodule states why that is the faithful
//! answer here rather than a shortcut. `signal.onabort` fires only from this
//! module's own abort path, and always after the registered listeners.

mod abort;
mod event;
mod target;

use rts_core::entry::{self, Context, Provided};

/// Installs all five classes as globals.
///
/// One function rather than five the caller sequences, because the classes are
/// not independent: `AbortSignal`'s prototype links to `EventTarget`'s and its
/// `'abort'` event is an `Event`, so an installer that ran them in the wrong
/// order would build a signal that cannot listen. The order lives here, where
/// the dependency is visible, rather than in a caller that would have to know
/// it.
pub fn install(context: &mut Context) {
    let event = event::install(context);
    entry::declare_global(context, "Event", event);
    let custom = event::install_custom(context);
    entry::declare_global(context, "CustomEvent", custom);
    let target = target::install(context);
    entry::declare_global(context, "EventTarget", target);
    let (controller, signal) = abort::install(context);
    entry::declare_global(context, "AbortController", controller);
    entry::declare_global(context, "AbortSignal", signal);
}

/// A constructor value carrying the prototype `new` links its instances to.
///
/// `node:url`'s `class_ctor` verbatim, and copied rather than reached for: that
/// one is `pub(super)` inside another crate's module tree. Returning an instance
/// from the constructor is NOT enough on its own — `new` over a native keeps the
/// object it allocated, so a constructor with no `prototype` property hands back
/// something with none of the methods on it.
fn class_ctor(context: &mut Context, construct: Provided, prototype: u64) -> u64 {
    let ctor = entry::make_callable(context, construct);
    entry::put_member(context, ctor, "prototype", prototype);
    ctor
}

/// `this` when `new` supplied one, else a fresh instance.
///
/// What makes `Event("x")` work as well as `new Event("x")`: a plain call hands
/// over `undefined`, and answering an instance anyway is ordinary JavaScript
/// semantics (a constructor returning an object wins over the implicit `this`)
/// rather than a special case.
fn self_or_new(context: &mut Context, this: u64, prototype: u64) -> u64 {
    match entry::is_object(context, this) {
        true => this,
        false => entry::make_instance(context, prototype),
    }
}

/// `undefined`, which is also what an argument the caller did not write arrives
/// as — the convention pads every call to four slots.
fn absent() -> u64 {
    entry::undefined_value()
}

/// An interned string, from outside a borrow.
fn string(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

/// An argument as text, `None` for an absent one.
///
/// `None` also for an object, whose `toString` is user code no entry point can
/// run — the two are one answer here because every caller acts the same on them.
fn text(value: u64) -> Option<String> {
    match value == absent() {
        true => None,
        false => entry::text_of(value),
    }
}

/// One property of an object, by name, from outside a borrow.
fn get(object: u64, name: &str) -> u64 {
    entry::get_indexed(object, string(name))
}

/// Writes one property of an object, by name, from outside a borrow.
fn put(object: u64, name: &str, value: u64) {
    entry::with_runtime(|context| entry::put_member(context, object, name, value));
}

/// `ToBoolean` of one property of an object.
fn flag(object: u64, name: &str) -> bool {
    entry::to_boolean(get(object, name))
}

/// An options argument, when it is an object this can read entries from.
///
/// `None` covers both a missing bag and the legacy boolean form
/// `addEventListener(type, fn, true)`, which is not a bag at all — the caller
/// decides what that means, because the two callers disagree: a flag lookup
/// answers `false` for it and a capture lookup answers `ToBoolean` of it.
fn options_bag(options: u64) -> Option<u64> {
    entry::with_runtime(|context| entry::is_object(context, options).then_some(options))
}

/// One boolean entry of an options bag, `false` when there is no bag.
fn option_flag(options: u64, name: &str) -> bool {
    options_bag(options).is_some_and(|bag| flag(bag, name))
}
