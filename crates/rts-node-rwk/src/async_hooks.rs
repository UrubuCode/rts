//! `node:async_hooks` — `AsyncLocalStorage` built properly, the id-tracking
//! half refused by name.
//!
//! # Reuse-check
//!
//! `.claude/skills/reuse-check/SKILL.md`'s search over `rts-cranelift`'s
//! `src/sched/`, `src/frame/` found nothing that threads a context value
//! through a suspension point — `SchedulerId`/`Delivery` are about *when* a
//! continuation runs, not about carrying a value across it, which is a
//! different concern. `rts_core_rwk::entry::Context` (checked against
//! `entry/mod.rs`) holds nothing shaped like a store stack, so this module's
//! [`STACK`] is new state, and it is new for the reason `AsyncResource`'s own
//! spec names: it belongs to whichever crate calls into user code across a
//! suspension, and `rts-node-rwk` is the only one that does that here.
//!
//! # Why `run` is real and the rest of the id half is not
//!
//! `run(store, callback)` needs exactly three things: somewhere to hold a
//! store while `callback` executes, a call into `callback` with this
//! module's own borrow already released (so a nested call from inside
//! `callback` back into an ambient helper is not a nested `RefCell` borrow —
//! see the module `run`'s own doc for where that borrow ends), and a pop
//! after. All three exist today: [`STACK`] is a thread-local outside any
//! runtime borrow, [`rts_core_rwk::entry::call`] is the ambient call form and
//! manages its own borrow, and the pop is Rust. Nothing about `run` waits on
//! a mechanism that is not built.
//!
//! What DOES wait: reporting *when* an async operation begins —
//! `executionAsyncId`, `triggerAsyncId`, `createHook`'s `init`/`before`/
//! `after`/`destroy`/`promiseResolve`. Real Node fires those from inside
//! every timer tick, every promise settle, every socket callback — call
//! sites this crate does not own (they live in the promise/timer machinery,
//! which has no hook point today). `AsyncResource` is refused for the same
//! reason one layer up: `runInAsyncScope` could push/pop an id the way `run`
//! pushes/pops a store, but the id it would report is meaningless without
//! `executionAsyncId` beside it, and that is the thing waiting on the hook.
//!
//! # Identity, not a slot number
//!
//! An `AsyncLocalStorage` instance is a heap object, and its own tagged
//! value (the `this` bits an instance method receives) IS its identity — two
//! calls with the same instance carry the same bits, and
//! [`rts_core_rwk::entry`] mints no separate "ALS slot id" for this module to
//! duplicate. [`STACK`] is one shared stack of `(instance, store)` frames
//! rather than one stack per instance, which is what lets `getStore` on one
//! instance see past a frame pushed by a different one — exactly Node's
//! nesting rule for independent `AsyncLocalStorage`s.
//!
//! # Not implemented, by name
//!
//! `createHook`, `AsyncHook`, `executionAsyncId`, `triggerAsyncId`,
//! `executionAsyncResource`, `asyncWrapProviders`, `AsyncResource` — all
//! need the engine to report when an async operation begins; nothing does,
//! for the reason above. `AsyncLocalStorage.bind`/`.snapshot` (static) —
//! each needs to hand back a NEW callable that carries a captured snapshot,
//! and [`rts_core_rwk::entry::make_callable`] returns a fixed function
//! pointer with no environment slot to hold one — the same gap
//! `node:events`' `rawListeners` doc names. `withScope`/`RunScope` — needs
//! `using`/`Symbol.dispose` language support to be worth building as
//! anything but a plain object with a `.dispose()` method; not attempted
//! here since confirming that prerequisite is outside this crate.
//! `AsyncLocalStorage#exit` reads as "return `undefined` inside `callback`
//! regardless of `defaultValue`" and IS built ([`exit`]) — listed here only
//! to say it is the one Experimental method built alongside the Stable ones,
//! because the mechanism it needs is identical to `run`'s.

use rts_core_rwk::entry::Provided;
use std::cell::RefCell;

/// A sentinel store value meaning "cleared", pushed by [`exit`] — distinct
/// from a real store so `getStore` inside an `exit` body answers `undefined`
/// even when the instance has a configured `defaultValue`, matching Node.
const CLEARED: u64 = u64::MAX;

thread_local! {
    /// Every live `run`/`exit` frame, oldest first, shared across every
    /// `AsyncLocalStorage` instance on this thread — see the module doc for
    /// why one stack rather than one per instance.
    static STACK: RefCell<Vec<(u64, u64)>> = const { RefCell::new(Vec::new()) };
}

/// The instance methods every `AsyncLocalStorage` shares through one
/// prototype.
const METHODS: &[(&str, Provided)] = &[
    ("run", run),
    ("exit", exit),
    ("enterWith", enter_with),
    ("getStore", get_store),
    ("disable", disable),
];

/// The namespace `node:async_hooks` is.
pub fn namespace(context: &mut rts_core_rwk::entry::Context) -> u64 {
    let members: &[(&str, Provided)] = &[("AsyncLocalStorage", construct)];
    let namespace = rts_core_rwk::entry::make_namespace(context, members);
    let prototype = rts_core_rwk::entry::make_prototype(context, "AsyncLocalStorage", METHODS);
    let constructor = rts_core_rwk::entry::get_member(context, namespace, "AsyncLocalStorage");
    rts_core_rwk::entry::put_member(context, constructor, "prototype", prototype);
    namespace
}

/// `new AsyncLocalStorage(options?)` — `options.defaultValue` is kept as an
/// own property; `options.name` is not (nothing here reads it back, and
/// carrying a value nobody consults is the same "invented completeness" the
/// module doc refuses elsewhere).
extern "C" fn construct(_e: u64, this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    rts_core_rwk::entry::with_runtime(|context| {
        let prototype = rts_core_rwk::entry::make_prototype(context, "AsyncLocalStorage", METHODS);
        let instance = match rts_core_rwk::entry::is_object(context, this) {
            true => this,
            false => rts_core_rwk::entry::make_instance(context, prototype),
        };
        let default = rts_core_rwk::entry::get_member(context, options, "defaultValue");
        rts_core_rwk::entry::put_member(context, instance, "__default__", default);
        instance
    })
}

/// `als.run(store, callback)` — no extra arguments forwarded to `callback`:
/// the four-slot convention every member here carries is already spent on
/// `store` and `callback`, so `run`'s own variadic tail (Node allows
/// `...args`) is not reachable from a native this shape and is silently not
/// forwarded, which is the same trade `path.join`'s doc names for its own
/// four slots.
extern "C" fn run(_e: u64, this: u64, store: u64, callback: u64, _c: u64, _d: u64) -> u64 {
    STACK.with(|stack| stack.borrow_mut().push((this, store)));
    let undefined = rts_core_rwk::entry::undefined_value();
    let result = rts_core_rwk::entry::call(callback, undefined, undefined, undefined, undefined, undefined);
    STACK.with(|stack| {
        stack.borrow_mut().pop();
    });
    result
}

/// `als.exit(callback)` — the mirror of [`run`]: pushes [`CLEARED`] so
/// `getStore` answers `undefined` for the duration, regardless of a
/// configured `defaultValue`.
extern "C" fn exit(_e: u64, this: u64, callback: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    STACK.with(|stack| stack.borrow_mut().push((this, CLEARED)));
    let undefined = rts_core_rwk::entry::undefined_value();
    let result = rts_core_rwk::entry::call(callback, undefined, undefined, undefined, undefined, undefined);
    STACK.with(|stack| {
        stack.borrow_mut().pop();
    });
    result
}

/// `als.enterWith(store)` — pushed and never popped by this module, which is
/// exactly Node's "for the rest of this execution" persistence; only
/// [`disable`] removes it.
extern "C" fn enter_with(_e: u64, this: u64, store: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    STACK.with(|stack| stack.borrow_mut().push((this, store)));
    rts_core_rwk::entry::undefined_value()
}

/// `als.getStore()` — the most recent frame pushed for THIS instance, or the
/// instance's own `defaultValue` if none is on the stack. [`CLEARED`] is not
/// a store a program can observe: it answers `undefined` in its place.
extern "C" fn get_store(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let found = STACK.with(|stack| {
        stack
            .borrow()
            .iter()
            .rev()
            .find(|&&(instance, _)| instance == this)
            .map(|&(_, store)| store)
    });
    match found {
        Some(CLEARED) => rts_core_rwk::entry::undefined_value(),
        Some(store) => store,
        None => rts_core_rwk::entry::with_runtime(|context| {
            rts_core_rwk::entry::get_member(context, this, "__default__")
        }),
    }
}

/// `als.disable()` — drops every frame this instance ever pushed, from
/// anywhere in the stack, not just the top: an `enterWith` frame has no
/// pairing pop, so a plain "pop mine off the top" would leave one behind
/// the moment a nested `run` from another instance sits above it.
extern "C" fn disable(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    STACK.with(|stack| stack.borrow_mut().retain(|&(instance, _)| instance != this));
    rts_core_rwk::entry::undefined_value()
}
