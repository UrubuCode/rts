//! `AsyncLocalStorage` and the `RunScope` `withScope` hands back.
//!
//! # Identity, not a slot number
//!
//! An `AsyncLocalStorage` instance is a heap object, and its own tagged value
//! (the `this` bits an instance method receives) IS its identity — two calls
//! with the same instance carry the same bits, and `rts_core::entry` mints
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
//!
//! # `bind`/`snapshot` — the premise that stood as "not implemented" was stale
//!
//! [`super`]'s module doc used to say a native cannot mint a closure at all.
//! [`rts_core::entry::closure_new`] says otherwise — `perf_hooks::timerify` and
//! `util::promisify` were already using it — and the shape both statics need
//! ("capture something now, run a function against it later") is exactly what
//! it is for. [`static_bind`] and [`static_snapshot`] are built on it.
//!
//! The one thing capture must not do is copy [`CLEARED`]'s literal `u64::MAX`
//! into a value a real JS array holds: that word is a Rust-only sentinel, never
//! a valid tagged value, and an array's elements ARE scanned as roots — handing
//! the collector one would be exactly the class of defect
//! `docs/engine/lost-roots.md` names. [`encode_frames`] substitutes
//! `undefined_value()` instead, which is safe to do because [`get_store`] reads
//! `CLEARED` and a real stored `undefined` down the same branch anyway (`Some(store) => store`,
//! and `Some(CLEARED) => undefined_value()` is that same value written a second
//! way) — the substitution changes no OBSERVABLE answer.

use rts_core::entry::{Context, Provided};
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU64, Ordering};

/// A sentinel store value meaning "cleared", pushed by [`exit`] — distinct from
/// a real store so `getStore` inside an `exit` body answers `undefined` even
/// when the instance has a configured `defaultValue`, matching Node.
const CLEARED: u64 = u64::MAX;

/// One entered store: which instance entered it, the value, and the token that
/// names this frame for [`dispose`].
///
/// `Copy`, so [`snapshot_frames`] can clone the whole stack with an iterator
/// rather than a manual field-by-field rebuild.
#[derive(Clone, Copy)]
struct Frame {
    instance: u64,
    store: u64,
    token: u64,
}

thread_local! {
    /// Every live `run`/`exit`/`enterWith`/`withScope` frame, oldest first,
    /// shared across every `AsyncLocalStorage` instance on this thread.
    static STACK: RefCell<Vec<Frame>> = const { RefCell::new(Vec::new()) };

    /// `AsyncLocalStorage.prototype`, minted once by [`install`].
    ///
    /// `construct` used to call `make_prototype(context, "AsyncLocalStorage",
    /// METHODS)` itself on every `new AsyncLocalStorage()` — a second, direct
    /// call from THIS file, while [`install`] had already registered the same
    /// name through `super::attach` (whose call site, `mod.rs`, is a different
    /// file by the guard's own `#[track_caller]` reckoning). That read as two
    /// modules racing for one name and panicked on the very first construction,
    /// exactly the class `resource.rs`'s own `PROTOTYPE` cell already exists to
    /// avoid — this module just never received that fix. Holding the object
    /// instead of re-deriving it is also the stronger invariant: one prototype,
    /// so `instanceof` compares every instance against the same object.
    static PROTOTYPE: Cell<u64> = const { Cell::new(0) };
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
    let prototype = super::attach(context, namespace, "AsyncLocalStorage", METHODS, 0);
    PROTOTYPE.with(|held| held.set(prototype));
    rts_core::entry::make_prototype(context, "RunScope", SCOPE_METHODS);
    // `bind`/`snapshot` are STATIC — Node hangs them on the constructor, not
    // the prototype, because they read the CALLER's context rather than one
    // instance's. `get_member`/`put_member` rather than a third `METHODS`-style
    // table: there is no per-instance dispatch here to share.
    let constructor = rts_core::entry::get_member(context, namespace, "AsyncLocalStorage");
    let bind_fn = rts_core::entry::make_callable(context, static_bind);
    rts_core::entry::put_member(context, constructor, "bind", bind_fn);
    let snapshot_fn = rts_core::entry::make_callable(context, static_snapshot);
    rts_core::entry::put_member(context, constructor, "snapshot", snapshot_fn);
}

/// Every live frame, oldest first, cloned out from under the borrow.
///
/// `Frame` is `Copy`, so this is one allocation and no per-field rebuilding.
fn snapshot_frames() -> Vec<Frame> {
    STACK.with(|stack| stack.borrow().clone())
}

/// Swaps `frames` in as the live stack and answers what was there before —
/// one call does both halves of "enter this snapshot", which is what keeps
/// [`reenter`] from ever observing a half-swapped `STACK`.
fn install_frames(frames: Vec<Frame>) -> Vec<Frame> {
    STACK.with(|stack| stack.replace(frames))
}

/// A captured stack, flattened to `[instance0, store0, instance1, store1, …]`
/// — pairs rather than a `Frame` per array slot, because the token exists only
/// to let [`dispose`] find ONE frame again and a restored copy is never the
/// target of a `dispose()` call (its `RunScope`, if any, still holds the
/// ORIGINAL token and looks it up on the REAL stack, not this swapped-in one).
/// See the module doc for why [`CLEARED`] itself never reaches this array.
fn encode_frames(frames: &[Frame]) -> u64 {
    let mut flat = Vec::with_capacity(frames.len() * 2);
    for frame in frames {
        flat.push(frame.instance);
        let safe_store = match frame.store {
            CLEARED => rts_core::entry::undefined_value(),
            other => other,
        };
        flat.push(safe_store);
    }
    rts_core::entry::make_array(flat)
}

/// The inverse of [`encode_frames`]. Restored frames carry token `0`, which
/// [`TOKENS`] never hands out (it starts at 1) — so a `dispose()` racing a
/// restored copy can never mistake it for a live one.
fn decode_frames(encoded: u64) -> Vec<Frame> {
    let length = rts_core::entry::array_length(encoded) as usize;
    let mut frames = Vec::with_capacity(length / 2);
    let mut at = 0usize;
    while at + 1 < length {
        let instance = rts_core::entry::element_at(encoded, rts_core::entry::make_number(at as f64));
        let store =
            rts_core::entry::element_at(encoded, rts_core::entry::make_number((at + 1) as f64));
        frames.push(Frame {
            instance,
            store,
            token: 0,
        });
        at += 2;
    }
    frames
}

/// Swaps `encoded` in, calls `target`, swaps the CALLER's own stack back out —
/// by value, so it makes no difference whether `target` itself pushed or
/// popped frames of its own before returning: whatever was current right
/// before this call is exactly what is current right after.
fn reenter(encoded: u64, target: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let frames = decode_frames(encoded);
    let previous = install_frames(frames);
    let result = rts_core::entry::call(target, this, a0, a1, a2, a3);
    install_frames(previous);
    result
}

/// `AsyncLocalStorage.bind(fn)` (static) — captures the context now and hands
/// back a function with `fn`'s own signature that re-enters it on every call.
///
/// A non-callable `fn` answers `undefined` — the reference doc's `TypeError`
/// is one this surface cannot raise, the same trade every member of this
/// crate already makes.
extern "C" fn static_bind(_e: u64, _this: u64, target: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    if !rts_core::entry::with_runtime(|context| rts_core::entry::is_callable_in(context, target)) {
        return rts_core::entry::undefined_value();
    }
    let encoded = encode_frames(&snapshot_frames());
    let environment = rts_core::entry::make_array(vec![target, encoded]);
    rts_core::entry::closure_new(bound_runner as *const () as usize as i64, environment)
}

/// The wrapper [`static_bind`] hands back — same arity and `this` as an
/// ordinary call, because the target was already named at bind time and costs
/// no argument slot here.
extern "C" fn bound_runner(environment: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let target = rts_core::entry::element_at(environment, rts_core::entry::make_number(0.0));
    let encoded = rts_core::entry::element_at(environment, rts_core::entry::make_number(1.0));
    reenter(encoded, target, this, a0, a1, a2, a3)
}

/// `AsyncLocalStorage.snapshot()` (static) — captures the context now and
/// hands back a runner `(fn, ...args) => R` that invokes `fn` inside it,
/// whenever and wherever it is called.
extern "C" fn static_snapshot(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let encoded = encode_frames(&snapshot_frames());
    rts_core::entry::closure_new(snapshot_runner as *const () as usize as i64, encoded)
}

/// The runner [`static_snapshot`] hands back. `fn` is the FIRST real argument
/// at call time here (unlike [`bound_runner`], nothing named it earlier), so
/// only three of it are left to forward — the same four-slot trade every
/// variadic member of this crate makes, stated rather than silently dropped.
extern "C" fn snapshot_runner(
    environment: u64,
    call_this: u64,
    target: u64,
    a0: u64,
    a1: u64,
    a2: u64,
) -> u64 {
    reenter(environment, target, call_this, a0, a1, a2, rts_core::entry::undefined_value())
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
    rts_core::entry::with_runtime(|context| {
        let prototype = PROTOTYPE.with(Cell::get);
        let instance = match rts_core::entry::is_object(context, this) {
            true => this,
            false => rts_core::entry::make_instance(context, prototype),
        };
        let default = rts_core::entry::get_member(context, options, "defaultValue");
        rts_core::entry::put_member(context, instance, "__default__", default);
        let named = rts_core::entry::get_member(context, options, "name");
        if let Some(text) = rts_core::entry::string_in(context, named) {
            let value = rts_core::entry::make_string(context, &text);
            rts_core::entry::put_member(context, instance, "name", value);
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
    let undefined = rts_core::entry::undefined_value();
    let result = rts_core::entry::call(callback, undefined, a, b, undefined, undefined);
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
    rts_core::entry::undefined_value()
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
    rts_core::entry::with_runtime(|context| {
        let prototype = rts_core::entry::make_prototype(context, "RunScope", SCOPE_METHODS);
        let scope = rts_core::entry::make_instance(context, prototype);
        let held = rts_core::entry::make_number(token as f64);
        rts_core::entry::put_member(context, scope, "__token__", held);
        scope
    })
}

/// `scope.dispose()` — idempotent, because the second call finds no frame with
/// that token rather than because a flag says it already ran.
extern "C" fn dispose(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let token = rts_core::entry::with_runtime(|context| {
        let held = rts_core::entry::get_member(context, this, "__token__");
        rts_core::entry::number_of(held)
    });
    if let Some(token) = token {
        drop_token(token as u64);
    }
    rts_core::entry::undefined_value()
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
        Some(CLEARED) => rts_core::entry::undefined_value(),
        Some(store) => store,
        None => rts_core::entry::with_runtime(|context| {
            rts_core::entry::get_member(context, this, "__default__")
        }),
    }
}

/// `als.disable()` — drops every frame this instance ever entered, from
/// anywhere in the stack, not just the top: an `enterWith` frame has no pairing
/// pop, so a plain "pop mine off the top" would leave one behind the moment a
/// nested `run` from another instance sits above it.
extern "C" fn disable(_e: u64, this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    STACK.with(|stack| stack.borrow_mut().retain(|frame| frame.instance != this));
    rts_core::entry::undefined_value()
}
