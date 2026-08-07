//! `AsyncLocalStorage` and the `RunScope` `withScope` hands back.
//!
//! # Identity, not a slot number
//!
//! An `AsyncLocalStorage` instance is a heap object, and its own tagged value
//! (the `this` bits an instance method receives) IS its identity — two calls
//! with the same instance carry the same bits, and `rts_core_rwk::entry` mints
//! no separate "ALS slot id" for this module to duplicate.
//!
//! [`STACK`] is ONE shared stack of frames rather than one per instance, which
//! is what lets `getStore` on one instance see past a frame pushed by a
//! different one — exactly Node's nesting rule for independent
//! `AsyncLocalStorage`s.
//!
//! # Why a frame carries a token
//!
//! `run` and `exit` pop what they pushed, so they need no name for it. `disable`
//! drops every frame of one instance, so it needs only the instance. `dispose`
//! on a `RunScope` needs to drop ONE frame that may no longer be on top (a
//! `run` nested inside the scope has not returned yet is the ordinary case), and
//! neither of the other two keys can find it. The token is that key, minted from
//! a counter here because no registry on the host surface hands out one.

use rts_core_rwk::entry::{Context, Provided};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

/// A sentinel store value meaning "cleared", pushed by [`exit`] — distinct from
/// a real store so `getStore` inside an `exit` body answers `undefined` even
/// when the instance has a configured `defaultValue`, matching Node.
const CLEARED: u64 = u64::MAX;

/// One entered store: which instance entered it, the value, and the token that
/// names this frame for [`dispose`].
struct Frame {
    instance: u64,
    store: u64,
    token: u64,
}

thread_local! {
    /// Every live `run`/`exit`/`enterWith`/`withScope` frame, oldest first,
    /// shared across every `AsyncLocalStorage` instance on this thread.
    static STACK: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };
}

/// The source of frame tokens. Process-wide rather than per thread so a token
/// read back on another thread cannot collide with a local one.
static TOKENS: AtomicU64 = AtomicU64::new(1);

/// The instance methods every `AsyncLocalStorage` shares through one prototype.
const METHODS: &[(&str, Provided)] = &[
    ("run", run),
    ("exit", exit),
    ("enterWith", enter_with),
    ("getStore", get_store),
    ("disable", disable),
    ("withScope", with_scope),
];

/// What a `RunScope` can do — see the module doc for why `[Symbol.dispose]` is
/// not beside it.
const SCOPE_METHODS: &[(&str, Provided)] = &[("dispose", dispose)];

/// Links both classes' prototypes to their constructors.
///
/// `RunScope` has no constructor on the namespace, and that is Node's shape
/// too: it is returned by `withScope` and never constructed by a program.
pub(super) fn install(context: &mut Context, namespace: u64) {
    super::attach(context, namespace, "AsyncLocalStorage", METHODS);
    rts_core_rwk::entry::make_prototype(context, "RunScope", SCOPE_METHODS);
}

/// `new AsyncLocalStorage(options?)` — keeps `options.defaultValue` and
/// `options.name` as own properties.
///
/// `name` is read with `string_in` rather than `text_in`: the question is
/// whether a name was GIVEN, and a coercion answers `"42"` for a number and
/// `"undefined"` for the common no-options call, which is the defect class this
/// crate has already paid for three times. Absent means the property is never
/// set, so `als.name` is `undefined` — Node's own answer when no name was
/// passed.
pub(super) extern "C" fn construct(_e: u64, this: u64, options: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    rts_core_rwk::entry::with_runtime(|context| {
        let prototype =
            rts_core_rwk::entry::make_prototype(context, "AsyncLocalStorage", METHODS);
        let instance = match rts_core_rwk::entry::is_object(context, this) {
            true => this,
            false => rts_core_rwk::entry::make_instance(context, prototype),
        };
        let default = rts_core_rwk::entry::get_member(context, options, "defaultValue");
        rts_core_rwk::entry::put_member(context, instance, "__default__", default);
        let named = rts_core_rwk::entry::get_member(context, options, "name");
        if let Some(text) = rts_core_rwk::entry::string_in(context, named) {
            let value = rts_core_rwk::entry::make_string(context, &text);
            rts_core_rwk::entry::put_member(context, instance, "name", value);
        }
        instance
    })
}

/// Pushes a frame and answers its token.
fn push(instance: u64, store: u64) -> u64 {
    let token = TOKENS.fetch_add(1, Ordering::Relaxed);
    STACK.with(|stack| {
        stack.borrow_mut().push(Frame {
            instance,
            store,
            token,
        })
    });
    token
}

/// Drops the frame with this token, wherever it sits.
fn drop_token(token: u64) {
    STACK.with(|stack| stack.borrow_mut().retain(|frame| frame.token != token));
}

/// Enters a store, runs a callback, leaves — the body `run` and `exit` share.
///
/// The pop is by token rather than by `Vec::pop` because `callback` may have
/// called `enterWith`, which leaves a frame behind on purpose: popping the top
/// would remove that one and leave this scope's frame entered forever.
fn scoped(instance: u64, store: u64, callback: u64, a: u64, b: u64) -> u64 {
    let token = push(instance, store);
    let undefined = rts_core_rwk::entry::undefined_value();
    let result = rts_core_rwk::entry::call(callback, undefined, a, b, undefined, undefined);
    drop_token(token);
    result
}

/// `als.run(store, callback, ...args)` — the first two extra arguments are
/// forwarded, the rest are not; see the module doc's refusal list.
extern "C" fn run(_e: u64, this: u64, store: u64, callback: u64, a: u64, b: u64) -> u64 {
    scoped(this, store, callback, a, b)
}

/// `als.exit(callback, ...args)` — the mirror of [`run`]: enters [`CLEARED`] so
/// `getStore` answers `undefined` for the duration, regardless of a configured
/// `defaultValue`.
extern "C" fn exit(_e: u64, this: u64, callback: u64, a: u64, b: u64, _d: u64) -> u64 {
    scoped(this, CLEARED, callback, a, b)
}

/// `als.enterWith(store)` — pushed and never popped by this module, which is
/// exactly Node's "for the rest of this execution" persistence; only [`disable`]
/// removes it.
extern "C" fn enter_with(_e: u64, this: u64, store: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    push(this, store);
    rts_core_rwk::entry::undefined_value()
}

/// `als.withScope(store)` — enters `store` and answers a `RunScope` whose
/// `dispose()` leaves it again.
///
/// The token is carried on the returned object as a number property rather than
/// in a Rust-side table keyed by the object: a table would have to be told when
/// the object dies, and nothing here is told that.
extern "C" fn with_scope(
    _e: u64,
    this: u64,
    store: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let token = push(this, store);
    rts_core_rwk::entry::with_runtime(|context| {
        let prototype = rts_core_rwk::entry::make_prototype(context, "RunScope", SCOPE_METHODS);
        let scope = rts_core_rwk::entry::make_instance(context, prototype);
        let held = rts_core_rwk::entry::make_number(token as f64);
        rts_core_rwk::entry::put_member(context, scope, "__token__", held);
        scope
    })
}

/// `scope.dispose()` — idempotent, because the second call finds no frame with
/// that token rather than because a flag says it already ran.
extern "C" fn dispose(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let token = rts_core_rwk::entry::with_runtime(|context| {
        let held = rts_core_rwk::entry::get_member(context, this, "__token__");
        rts_core_rwk::entry::number_of(held)
    });
    if let Some(token) = token {
        drop_token(token as u64);
    }
    rts_core_rwk::entry::undefined_value()
}

/// `als.getStore()` — the most recent frame entered for THIS instance, or the
/// instance's own `defaultValue` if none is on the stack. [`CLEARED`] is not a
/// store a program can observe: it answers `undefined` in its place.
extern "C" fn get_store(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let found = STACK.with(|stack| {
        stack
            .borrow()
            .iter()
            .rev()
            .find(|frame| frame.instance == this)
            .map(|frame| frame.store)
    });
    match found {
        Some(CLEARED) => rts_core_rwk::entry::undefined_value(),
        Some(store) => store,
        None => rts_core_rwk::entry::with_runtime(|context| {
            rts_core_rwk::entry::get_member(context, this, "__default__")
        }),
    }
}

/// `als.disable()` — drops every frame this instance ever entered, from
/// anywhere in the stack, not just the top: an `enterWith` frame has no pairing
/// pop, so a plain "pop mine off the top" would leave one behind the moment a
/// nested `run` from another instance sits above it.
extern "C" fn disable(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    STACK.with(|stack| stack.borrow_mut().retain(|frame| frame.instance != this));
    rts_core_rwk::entry::undefined_value()
}
