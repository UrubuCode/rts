//! `node:diagnostics_channel` — named publish/subscribe channels, with the
//! one contract that matters: `channel("x") === channel("x")`.
//!
//! # Reuse-check
//!
//! Searched, per `.claude/skills/reuse-check/SKILL.md`:
//!
//! - `rts-cranelift` `src/shape/`, `src/sched/`, `src/symbols/` — nothing
//!   resembling a named registry or a publish/subscribe primitive. A channel
//!   table is runtime bookkeeping, not a machine capability.
//! - `rts_core::entry::Context` (`entry/mod.rs`) — holds no channel table
//!   and no store stack; `entry/modules.rs` is the whole host surface and
//!   offers no named-object registry (`class_support` is private to that
//!   crate, which is why `make_prototype` exists there and not here).
//! - This crate — `fs/watch.rs`'s `WATCHERS: Mutex<HashMap<..>>` is the
//!   established shape for "a process-global table holding a JS object across
//!   calls", and [`CHANNELS`] is that shape for the same reason. `events.rs`
//!   already owns listener storage on the emitter object, and the subscriber
//!   array here follows it rather than inventing a second mechanism.
//! - **`node:async_hooks` now exists in this crate** (`async_hooks.rs`, with a
//!   real `AsyncLocalStorage`), which is what unblocked `bindStore`/
//!   `unbindStore`/`runStores` — see [`stores`]. The previous version of this
//!   doc said `AsyncLocalStorage` "does not exist anywhere in this crate";
//!   that had become false, and a stale refusal is the doc drift this
//!   repository treats as a defect.
//! - `Function.prototype.bind` (`entry/function_proto.rs`) already mints a
//!   fresh callable that carries data beside its cell, which is exactly the
//!   per-call identity [`tracing`]'s promise handlers need — so no second
//!   "callable with an environment" mechanism is built here.
//!
//! # Why the registry is keyed by TEXT and not `u64`
//!
//! `channel(name)` must answer the same `Channel` object for the same name
//! across the whole run, and a name is exactly what a caller already has —
//! keying by anything else would mean minting a second identity for one name,
//! which is the numbering bug `reuse-check` calls out.
//!
//! # Function identity: the gap that CLOSED
//!
//! `docs/reference/node/diagnostics_channel.md` records "unsubscribe
//! removal-by-ref is a no-op (engine re-reifies fn values → `f === f` false)".
//! That was the OLD engine. In this one a function is an ordinary heap cell
//! (`entry::functions::closure_new` allocates once, at the function
//! definition) and `entry::strict_equals` compares slot bits for objects, so
//! reading the same variable twice yields the same bits and
//! [`remove_subscriber`] really removes. The reference doc's line is stale for
//! the new engine; it is not restated here as a limitation.
//!
//! # `hasSubscribers` — a data property, not a getter
//!
//! Real Node's is a live getter. `rts_core::entry` exposes
//! `define_getter`, but it takes an already-INTERNED property key (`i64`)
//! minted from compiled code's literal table, and no host-side "define a
//! getter under this string name" exists — so a live accessor is not buildable
//! from here. What is built instead: an ordinary data property, rewritten
//! every time a subscriber is added or removed anywhere in this module
//! ([`add_subscriber`]/[`remove_subscriber`], which also refresh every
//! `TracingChannel`'s aggregate). There is no point at which it is wrong for a
//! program that changes subscriptions through this module's own API.
//!
//! # Not implemented, by name
//!
//! - **Symbol-named channels.** [`string_arg`] is a string TEST, not a
//!   coercion, and nothing on the host surface hands back a symbol's identity,
//!   so a symbol name answers `undefined` rather than being stringified into a
//!   name that would collide with the same text as a string key.
//! - **Argument-type `TypeError`s** (`ERR_INVALID_ARG_TYPE`). A non-string
//!   name or a non-function `onMessage` answers `undefined`/`false` instead of
//!   throwing: `entry::throw` takes a handler tag a native cannot mint.
//! - **`TracingChannel.traceCallback`.** Its signature is
//!   `traceCallback(fn, position, context, thisArg, ...args)` — the four
//!   argument slots of the native convention are already spent before the
//!   first of `...args`, and the callback it must wrap lives *inside* `...args`.
//!   There is no subset of it that is not a wrong answer, so it is not
//!   installed at all: `tc.traceCallback` reads `undefined`.
//! - **Subscriber-exception isolation.** A subscriber that throws stops the
//!   siblings of that same `publish` — Node isolates each subscriber's own
//!   exception and reports it async; this crate has nothing to report to, so
//!   the throw is left to propagate like any other rather than swallowed and
//!   reported nowhere.
//!
//!   `TracingChannel.traceSync`'s error path is NOT on this list any more —
//!   see `tracing::trace_sync`'s own doc. The wall `assert.rs` names (a native
//!   cannot resume ITS OWN JS-facing logic after a callee's throw) is real,
//!   but "ask whether the callee left a throw behind before looking at the
//!   answer" (`rts-core/README.md` rule 8) is a different, available move:
//!   `entry::thrown()`/`entry::take_thrown()` answer exactly that, and
//!   `entry::throw_value()` re-raises what was taken. None of that resumes
//!   `fn` — it all runs strictly AFTER `fn` has already returned or thrown.
//! - **Variadic tails.** `runStores(context, fn, thisArg, ...args)`,
//!   `traceSync(fn, context, thisArg, ...args)` and `tracePromise`'s likewise:
//!   the four argument slots of the native convention are spent before
//!   `...args` begins, so `fn` is called with none. Dropped by name here
//!   rather than silently.
//! - **`tracePromise`'s non-thenable warning.** Node emits a process warning
//!   when `fn` returns a non-thenable; nothing here can raise one, so the value
//!   is returned untraced and silently.
//! - **`WeakRef`-based channel GC.** [`CHANNELS`] holds strong references — the
//!   deviation `docs/reference/node/diagnostics_channel.md` §7 already chose,
//!   since RTS's `WeakRef` is interim-strong (issue #217).

pub mod stores;
pub mod tracing;

use rts_core::entry::{self, Context, Provided};
use std::collections::HashMap;
use std::sync::Mutex;

/// Every channel ever asked for, by name — the identity `channel(name)` keeps.
static CHANNELS: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);

/// The registry, created on first use.
fn with_channels<T>(body: impl FnOnce(&mut HashMap<String, u64>) -> T) -> T {
    let mut guard = CHANNELS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

/// What every `Channel` inherits.
const CHANNEL_METHODS: &[(&str, Provided)] = &[
    ("publish", publish),
    ("subscribe", instance_subscribe),
    ("unsubscribe", instance_unsubscribe),
    ("bindStore", stores::bind_store),
    ("unbindStore", stores::unbind_store),
    ("runStores", stores::run_stores),
];

/// The `Channel` object for `name`, creating it (with an empty subscriber
/// array) the first time. Same object on every later call — the whole contract
/// this module exists to keep.
pub(crate) fn channel_object(context: &mut Context, name: &str) -> u64 {
    if let Some(existing) = with_channels(|table| table.get(name).copied()) {
        return existing;
    }
    let prototype = entry::make_prototype(context, "Channel", CHANNEL_METHODS);
    let instance = entry::make_instance(context, prototype);
    let name_value = entry::make_string(context, name);
    entry::put_member(context, instance, "name", name_value);
    let subscribers = entry::make_array_in(context, Vec::new());
    entry::put_member(context, instance, "__subscribers__", subscribers);
    let has_subscribers = entry::boolean_value(false);
    entry::put_member(context, instance, "hasSubscribers", has_subscribers);
    with_channels(|table| table.insert(name.to_owned(), instance));
    instance
}

/// The namespace `node:diagnostics_channel` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("channel", channel_fn),
        ("hasSubscribers", has_subscribers_fn),
        ("subscribe", subscribe_fn),
        ("unsubscribe", unsubscribe_fn),
        ("tracingChannel", tracing::tracing_channel_fn),
    ];
    let namespace = entry::make_namespace(context, members);
    let channel_ctor = channel_constructor(context);
    entry::put_member(context, namespace, "Channel", channel_ctor);
    let tracing_ctor = tracing::tracing_channel_constructor(context);
    entry::put_member(context, namespace, "TracingChannel", tracing_ctor);
    namespace
}

/// `diagnostics_channel.Channel` — never constructs anything for real
/// ([`channel_object`] is the only maker); exported so `instanceof` has a
/// value to check `ch instanceof Channel` against.
///
/// # Why this passes the REAL method table, not an empty one
///
/// `entry::make_prototype`'s own doc names the trap: two DIFFERENT files each
/// registering a non-empty table under one name panics, but the SAME file
/// doing it twice is exactly the "idempotent by name" case it declares safe —
/// whichever of this function and [`channel_object`] runs first installs the
/// table, the other gets the same cell back. Both are in THIS file, and both
/// pass [`CHANNEL_METHODS`], so which one runs first cannot matter and the
/// prototype `instanceof` walks is the literal one every `Channel` instance
/// already has, not a chained stand-in (contrast `fs/streams.rs::constructors`,
/// which chains for the opposite reason: its real prototype is owned by a
/// DIFFERENT module that has not necessarily run yet).
fn channel_constructor(context: &mut Context) -> u64 {
    let prototype = entry::make_prototype(context, "Channel", CHANNEL_METHODS);
    let ctor = entry::make_callable(context, channel_construct);
    entry::put_member(context, ctor, "prototype", prototype);
    // Back-link — see `crate::stream::class_ctor`'s doc. Every `Channel`
    // handed out by `channel(name)` shares this SAME prototype, so this one
    // call fixes `.constructor.name` for all of them.
    entry::declare_host_class(context, ctor, prototype, "Channel", 1);
    ctor
}

extern "C" fn channel_construct(_e: u64, _this: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    entry::undefined_value()
}

/// `diagnostics_channel.channel(name)`.
extern "C" fn channel_fn(_e: u64, _this: u64, name: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(text) = string_arg(name) else {
        return entry::undefined_value();
    };
    entry::with_runtime(|context| channel_object(context, &text))
}

/// `diagnostics_channel.hasSubscribers(name)` — a registry lookup that does
/// NOT create the channel, unlike `channel(name).hasSubscribers`.
extern "C" fn has_subscribers_fn(_e: u64, _this: u64, name: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(text) = string_arg(name) else {
        return entry::boolean_value(false);
    };
    let existing = with_channels(|table| table.get(&text).copied());
    match existing {
        Some(instance) => entry::boolean_value(subscriber_count(instance) > 0),
        None => entry::boolean_value(false),
    }
}

/// `diagnostics_channel.subscribe(name, onMessage)`.
extern "C" fn subscribe_fn(_e: u64, _this: u64, name: u64, on_message: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(text) = string_arg(name) else {
        return entry::undefined_value();
    };
    let instance = entry::with_runtime(|context| channel_object(context, &text));
    add_subscriber(instance, on_message);
    entry::undefined_value()
}

/// `diagnostics_channel.unsubscribe(name, onMessage)`.
extern "C" fn unsubscribe_fn(_e: u64, _this: u64, name: u64, on_message: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(text) = string_arg(name) else {
        return entry::boolean_value(false);
    };
    let existing = with_channels(|table| table.get(&text).copied());
    match existing {
        Some(instance) => entry::boolean_value(remove_subscriber(instance, on_message)),
        None => entry::boolean_value(false),
    }
}

/// `channel.subscribe(onMessage)` — a non-function is not recorded, because a
/// subscriber list holding something `publish` cannot call is a list that
/// reports subscribers nobody will ever hear from.
extern "C" fn instance_subscribe(_e: u64, this: u64, on_message: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    add_subscriber(this, on_message);
    entry::undefined_value()
}

/// `channel.unsubscribe(onMessage)`.
extern "C" fn instance_unsubscribe(_e: u64, this: u64, on_message: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::boolean_value(remove_subscriber(this, on_message))
}

/// `channel.publish(message)`.
extern "C" fn publish(_e: u64, this: u64, message: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    do_publish(this, message);
    entry::undefined_value()
}

/// The number of subscribers a channel currently holds.
pub(crate) fn subscriber_count(channel: u64) -> usize {
    collect_subscribers(channel).len()
}

/// The subscriber array's elements, copied out.
///
/// Ambient — it takes its own borrows, so it must NOT be called from inside
/// one. Every caller here is an `extern "C"` body or a helper reached from one.
pub(crate) fn collect_subscribers(channel: u64) -> Vec<u64> {
    elements_under(channel, "__subscribers__")
}

/// The elements of an array held under one of an object's own names.
///
/// Ambient, for the reason [`collect_subscribers`] gives. Shared with
/// [`stores`], which keeps its bound stores the same way — one reader rather
/// than two that could come to disagree about what an empty slot means.
pub(crate) fn elements_under(object: u64, name: &str) -> Vec<u64> {
    let array = member(object, name);
    let absent = entry::undefined_value();
    if array == absent {
        return Vec::new();
    }
    let length_value = entry::get_indexed(array, string_key("length"));
    let length = entry::number_of(length_value).map(|value| value as usize).unwrap_or(0);
    (0..length)
        .map(|index| entry::get_indexed(array, entry::make_number(index as f64)))
        .collect()
}

/// Replaces the array held under one of an object's own names.
pub(crate) fn set_elements_under(object: u64, name: &str, values: Vec<u64>) {
    entry::with_runtime(|context| {
        let array = entry::make_array_in(context, values);
        entry::put_member(context, object, name, array);
    });
}

/// Records a subscriber, if it is something `publish` could call.
pub(crate) fn add_subscriber(channel: u64, on_message: u64) {
    if !is_callable(on_message) {
        return;
    }
    let mut subscribers = collect_subscribers(channel);
    subscribers.push(on_message);
    store_subscribers(channel, subscribers);
}

/// Removes a subscriber by identity, answering whether one went.
pub(crate) fn remove_subscriber(channel: u64, on_message: u64) -> bool {
    let mut subscribers = collect_subscribers(channel);
    let Some(at) = subscribers.iter().position(|&held| entry::strict_equals(held, on_message)) else {
        return false;
    };
    subscribers.remove(at);
    store_subscribers(channel, subscribers);
    true
}

/// Writes the subscriber array back and re-derives every `hasSubscribers` that
/// depends on it — the channel's own, and every `TracingChannel` aggregate.
fn store_subscribers(channel: u64, subscribers: Vec<u64>) {
    entry::with_runtime(|context| {
        let array = entry::make_array_in(context, subscribers.clone());
        entry::put_member(context, channel, "__subscribers__", array);
        let flag = entry::boolean_value(!subscribers.is_empty());
        entry::put_member(context, channel, "hasSubscribers", flag);
    });
    tracing::refresh_aggregates();
}

/// Every subscriber, called `(message, name)` in order.
///
/// Copied out and the borrow dropped BEFORE any call — the rule
/// [`crate::events`] documents and the one this crate's whole calling
/// convention exists to protect: calling into JS from inside a held borrow
/// aborts the process.
pub(crate) fn do_publish(channel: u64, message: u64) {
    let subscribers = collect_subscribers(channel);
    if subscribers.is_empty() {
        return;
    }
    let name = member(channel, "name");
    let absent = entry::undefined_value();
    for subscriber in subscribers {
        entry::call(subscriber, absent, message, name, absent, absent);
    }
}

/// One property of an object, from outside a borrow.
pub(crate) fn member(object: u64, name: &str) -> u64 {
    entry::with_runtime(|context| entry::get_member(context, object, name))
}

/// One property written, from outside a borrow.
pub(crate) fn set_member(object: u64, name: &str, value: u64) {
    entry::with_runtime(|context| entry::put_member(context, object, name, value));
}

/// Whether a value can be called, from outside a borrow.
pub(crate) fn is_callable(value: u64) -> bool {
    entry::with_runtime(|context| entry::is_callable_in(context, value))
}

/// An interned string, for use as a property key from outside a borrow.
pub(crate) fn string_key(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

/// An argument's text **only when it really is a string**.
///
/// `string_in` rather than `text_of`: `text_of` is `ToString`, so it answers
/// `"42"` for a number and would silently mint a channel named `"42"` for
/// `channel(42)` — a coercion used as a type test, which is the defect class
/// this repository has already been bitten by three times.
pub(crate) fn string_arg(value: u64) -> Option<String> {
    entry::with_runtime(|context| entry::string_in(context, value))
}
