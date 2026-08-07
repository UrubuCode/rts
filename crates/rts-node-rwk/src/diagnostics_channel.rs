//! `node:diagnostics_channel` — named publish/subscribe channels, with the
//! one contract that matters: `channel("x") === channel("x")`.
//!
//! # Reuse-check
//!
//! `rts-cranelift` answers nothing here — a named-channel registry is
//! runtime bookkeeping, not a machine capability (checked `src/shape/`,
//! `src/sched/` for anything resembling a publish/subscribe primitive or a
//! named-registry pattern; neither exists). `rts-core-rwk::entry::Context`
//! holds no channel table either. The nearest existing shape is
//! `fs/watch.rs`'s `WATCHERS: Mutex<HashMap<u64, WatcherEntry>>` — a
//! process-global registry keyed by a native id, holding a JS object across
//! calls — and this module follows the SAME shape for the SAME reason: a
//! channel, like a watcher, needs one identity a second call must find again,
//! and nothing in this crate's own `entry::modules` surface offers a named
//! object registry (that mechanism, `super::class_support`, is a private
//! submodule `rts-node-rwk` cannot reach — see `entry::make_prototype`'s own
//! doc for why it exists there and not here).
//!
//! # Why the registry is keyed by TEXT and not `u64`
//!
//! `channel(name)` must answer the same `Channel` object for the same name
//! across the whole run, and a name is exactly what a caller already has —
//! keying by anything else would mean minting a second identity for one
//! name, which is the numbering bug `reuse-check`'s own doc calls out.
//! Symbol-named channels are not implemented (see below): they read as
//! `undefined`, no second-numbering apparatus is added just for a case with
//! no implementation.
//!
//! # `Channel.hasSubscribers` — a data property, not a getter
//!
//! Real Node's is a live getter. `rts_core_rwk::entry` exposes
//! `define_getter`/`define_setter`, but those take an already-INTERNED
//! property key (`i64`) minted from compiled code's own literal table —
//! there is no host-side "define a getter under this string name" entry
//! point, so a live accessor is not buildable from here. What is built
//! instead: an ordinary data property, re-set to the correct boolean every
//! time [`subscribe`]/[`unsubscribe`] change the subscriber count. A read
//! between those two points sees a stale-but-correct value; there is no
//! point where it is wrong.
//!
//! # Not implemented, by name
//!
//! Symbol-named channels — `text_of` is this crate's only way to read a
//! channel name, so only strings work (the same limit `events.rs`'s event
//! names have). `bindStore`/`unbindStore` — `AsyncLocalStorage` does not
//! exist anywhere in this crate; both are refused (return `undefined`/
//! `false`) rather than silently doing nothing under a name that implies
//! they did something. `runStores` still publishes and calls `fn` (the part
//! that does not need a store), just never enters one. `tracePromise`/
//! `traceCallback` — both need to construct a fresh `Promise`/wrap a callback
//! argument from Rust, and this crate's entry surface has no `Promise`
//! constructor (the same gap `events.rs`'s module doc names for
//! `events.on`/`events.once`). `TracingChannel.traceSync`'s error path — a
//! native entry point can neither catch a JS throw crossing back through it
//! nor resume after one (`crates/rts-node-rwk/src/assert.rs`'s module doc
//! names the same wall), so `traceSync` instruments only the path where `fn`
//! returns: `start` publishes, `fn` runs, `end` publishes with
//! `context.result` set. A throwing `fn` propagates the throw as an ordinary
//! throw would from any call site — `error` never fires, `end` never fires,
//! which is a real divergence from the spec's `error`-then-`end`-then-rethrow
//! contract, named rather than silently half-built. Automatic
//! `WeakRef`-based channel GC — the registry holds strong references
//! (documented as the chosen deviation in `docs/reference/node/
//! diagnostics_channel.md` §7, since RTS's own `WeakRef` is interim-strong
//! per issue #217); channel count is small and bounded in practice.

use rts_core_rwk::entry::{self, Context, Provided};
use std::collections::HashMap;
use std::sync::Mutex;

static CHANNELS: Mutex<Option<HashMap<String, u64>>> = Mutex::new(None);

fn with_channels<T>(body: impl FnOnce(&mut HashMap<String, u64>) -> T) -> T {
    let mut guard = CHANNELS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

const CHANNEL_METHODS: &[(&str, Provided)] = &[
    ("publish", publish),
    ("subscribe", instance_subscribe),
    ("unsubscribe", instance_unsubscribe),
    ("hasSubscribers", has_subscribers_method),
    ("bindStore", bind_store),
    ("unbindStore", unbind_store),
    ("runStores", run_stores),
];

/// The `Channel` object for `name`, creating it (with an empty subscriber
/// array) the first time. Same object on every later call — the whole
/// contract this module exists to keep.
fn channel_object(context: &mut Context, name: &str) -> u64 {
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
        ("tracingChannel", tracing_channel_fn),
    ];
    entry::make_namespace(context, members)
}

/// `diagnostics_channel.channel(name)`.
extern "C" fn channel_fn(_e: u64, _this: u64, name: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(text) = text(name) else {
        return entry::undefined_value();
    };
    entry::with_runtime(|context| channel_object(context, &text))
}

/// `diagnostics_channel.hasSubscribers(name)` — a registry lookup that does
/// NOT create the channel, unlike `channel(name).hasSubscribers`.
extern "C" fn has_subscribers_fn(_e: u64, _this: u64, name: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(text) = text(name) else {
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
    let Some(text) = text(name) else {
        return entry::undefined_value();
    };
    let instance = entry::with_runtime(|context| channel_object(context, &text));
    add_subscriber(instance, on_message);
    entry::undefined_value()
}

/// `diagnostics_channel.unsubscribe(name, onMessage)`.
extern "C" fn unsubscribe_fn(_e: u64, _this: u64, name: u64, on_message: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(text) = text(name) else {
        return entry::boolean_value(false);
    };
    let existing = with_channels(|table| table.get(&text).copied());
    match existing {
        Some(instance) => entry::boolean_value(remove_subscriber(instance, on_message)),
        None => entry::boolean_value(false),
    }
}

/// `channel.subscribe(onMessage)`.
extern "C" fn instance_subscribe(_e: u64, this: u64, on_message: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    add_subscriber(this, on_message);
    entry::undefined_value()
}

/// `channel.unsubscribe(onMessage)`.
extern "C" fn instance_unsubscribe(_e: u64, this: u64, on_message: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::boolean_value(remove_subscriber(this, on_message))
}

/// `channel.hasSubscribers` is a data property (see the module doc); this
/// method exists only so `tracingChannel`'s aggregate check has something
/// uniform to call across all five sub-channels.
extern "C" fn has_subscribers_method(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::boolean_value(subscriber_count(this) > 0)
}

/// `channel.publish(message)`.
extern "C" fn publish(_e: u64, this: u64, message: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    do_publish(this, message);
    entry::undefined_value()
}

/// `channel.bindStore(store, transform?)` — refused; see the module doc.
extern "C" fn bind_store(_e: u64, _this: u64, _store: u64, _transform: u64, _a2: u64, _a3: u64) -> u64 {
    entry::undefined_value()
}

/// `channel.unbindStore(store)` — refused; see the module doc.
extern "C" fn unbind_store(_e: u64, _this: u64, _store: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::boolean_value(false)
}

/// `channel.runStores(context, fn, thisArg?)` — publishes and calls `fn`;
/// enters no store (see the module doc). Only one trailing argument is
/// forwarded to `fn`, the same four-slot limit `events.rs`'s
/// `static_set_max_listeners` names.
extern "C" fn run_stores(_e: u64, this: u64, context_val: u64, callback: u64, this_arg: u64, _a3: u64) -> u64 {
    do_publish(this, context_val);
    let absent = entry::undefined_value();
    entry::call(callback, this_arg, absent, absent, absent, absent)
}

/// The number of subscribers a channel currently holds.
fn subscriber_count(channel: u64) -> usize {
    collect_subscribers(channel).len()
}

/// The subscriber array's elements, copied out.
fn collect_subscribers(channel: u64) -> Vec<u64> {
    let array = entry::get_indexed(channel, string_key("__subscribers__"));
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

fn add_subscriber(channel: u64, on_message: u64) {
    let mut subscribers = collect_subscribers(channel);
    subscribers.push(on_message);
    store_subscribers(channel, subscribers);
}

fn remove_subscriber(channel: u64, on_message: u64) -> bool {
    let mut subscribers = collect_subscribers(channel);
    let Some(at) = subscribers.iter().position(|&held| entry::strict_equals(held, on_message)) else {
        return false;
    };
    subscribers.remove(at);
    store_subscribers(channel, subscribers);
    true
}

fn store_subscribers(channel: u64, subscribers: Vec<u64>) {
    entry::with_runtime(|context| {
        let array = entry::make_array_in(context, subscribers.clone());
        entry::put_member(context, channel, "__subscribers__", array);
        let flag = entry::boolean_value(!subscribers.is_empty());
        entry::put_member(context, channel, "hasSubscribers", flag);
    });
}

/// Every subscriber, called `(message, name)` in order. Copied out and the
/// borrow dropped BEFORE any call — the same rule [`crate::events::emit`]
/// documents and the one thing this crate's whole calling convention exists
/// to protect: calling into JS from inside a held borrow aborts the process.
fn do_publish(channel: u64, message: u64) {
    let subscribers = collect_subscribers(channel);
    if subscribers.is_empty() {
        return;
    }
    let name = entry::get_indexed(channel, string_key("name"));
    let absent = entry::undefined_value();
    for subscriber in subscribers {
        entry::call(subscriber, absent, message, name, absent, absent);
    }
}

// ---------------------------------------------------------------- tracing --

const TRACING_METHODS: &[(&str, Provided)] = &[
    ("subscribe", tracing_subscribe),
    ("unsubscribe", tracing_unsubscribe),
    ("traceSync", trace_sync),
];

/// `diagnostics_channel.tracingChannel(nameOrChannels)`.
///
/// Only the `string` form is implemented — a pre-built
/// `TracingChannelCollection` object argument is not (it would need to read
/// five arbitrary `Channel`s back out and re-key the registry by object
/// identity rather than name, which nothing here needs yet); passing one
/// reads as `undefined` for `nameOrChannels`, so the five channels are named
/// from the STRINGIFIED argument instead of failing silently on a
/// misinterpreted name.
extern "C" fn tracing_channel_fn(_e: u64, _this: u64, name_or_channels: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(name) = text(name_or_channels) else {
        return entry::undefined_value();
    };
    entry::with_runtime(|context| {
        let start = channel_object(context, &format!("tracing:{name}:start"));
        let end = channel_object(context, &format!("tracing:{name}:end"));
        let async_start = channel_object(context, &format!("tracing:{name}:asyncStart"));
        let async_end = channel_object(context, &format!("tracing:{name}:asyncEnd"));
        let error = channel_object(context, &format!("tracing:{name}:error"));
        let prototype = entry::make_prototype(context, "TracingChannel", TRACING_METHODS);
        let instance = entry::make_instance(context, prototype);
        entry::put_member(context, instance, "start", start);
        entry::put_member(context, instance, "end", end);
        entry::put_member(context, instance, "asyncStart", async_start);
        entry::put_member(context, instance, "asyncEnd", async_end);
        entry::put_member(context, instance, "error", error);
        instance
    })
}

/// `tracingChannel.subscribe({ start?, end?, asyncStart?, asyncEnd?, error? })`.
extern "C" fn tracing_subscribe(_e: u64, this: u64, subscribers: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    for key in ["start", "end", "asyncStart", "asyncEnd", "error"] {
        let handler = entry::with_runtime(|context| entry::get_member(context, subscribers, key));
        let absent = entry::undefined_value();
        if handler == absent {
            continue;
        }
        let sub_channel = entry::with_runtime(|context| entry::get_member(context, this, key));
        add_subscriber(sub_channel, handler);
    }
    entry::undefined_value()
}

/// `tracingChannel.unsubscribe({ ... })` — `true` only if every provided
/// handler was found and removed.
extern "C" fn tracing_unsubscribe(_e: u64, this: u64, subscribers: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let mut all_found = true;
    for key in ["start", "end", "asyncStart", "asyncEnd", "error"] {
        let handler = entry::with_runtime(|context| entry::get_member(context, subscribers, key));
        let absent = entry::undefined_value();
        if handler == absent {
            continue;
        }
        let sub_channel = entry::with_runtime(|context| entry::get_member(context, this, key));
        all_found &= remove_subscriber(sub_channel, handler);
    }
    entry::boolean_value(all_found)
}

/// `tracingChannel.traceSync(fn, context?, thisArg?)` — the success path
/// only; see the module doc for the throwing path.
extern "C" fn trace_sync(_e: u64, this: u64, callback: u64, context_val: u64, this_arg: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let context_obj = match context_val == absent {
        true => entry::with_runtime(entry::make_object),
        false => context_val,
    };
    let start = entry::with_runtime(|context| entry::get_member(context, this, "start"));
    do_publish(start, context_obj);
    let result = entry::call(callback, this_arg, absent, absent, absent, absent);
    entry::with_runtime(|context| entry::put_member(context, context_obj, "result", result));
    let end = entry::with_runtime(|context| entry::get_member(context, this, "end"));
    do_publish(end, context_obj);
    result
}

/// An interned string, for use as a property key from outside a borrow.
fn string_key(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

/// An argument as text, `None` for `undefined`/a non-string (including a
/// symbol — see the module doc).
fn text(value: u64) -> Option<String> {
    let absent = entry::undefined_value();
    match value == absent {
        true => None,
        false => entry::text_of(value),
    }
}
