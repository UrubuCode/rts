//! `TracingChannel` — five channels and the helpers that publish around a
//! call.
//!
//! # Why the promise handlers are `bind`-ed natives
//!
//! `tracePromise` has to attach two continuations to a promise it was handed,
//! and each one must know WHICH tracing channel and WHICH context object it
//! belongs to. A native cannot answer either question about itself: its
//! environment slot is `undefined` by construction and it is handed no
//! identity of its own.
//!
//! `Function.prototype.bind` already solves exactly that, and it is reachable
//! from here — a callable inherits from `Function.prototype`, so
//! `get_member(native, "bind")` finds it, and `entry/function_proto.rs`'s
//! `bound` remembers the partial arguments beside the *new* cell it mints. So
//! the correlation travels as the first two arguments of the handler, and no
//! second "callable with an environment" mechanism is invented here. The
//! alternative considered and rejected: a thread-local map keyed by the
//! handler's identity — which is the identity a native cannot read, so it
//! would have had to be a queue, and promise settlement order is not call
//! order.
//!
//! # Why `end` fires before the promise settles
//!
//! Node's split: `start`/`end` bracket the SYNCHRONOUS call to `fn`, and
//! `asyncStart`/`asyncEnd` bracket the settlement. Publishing `end` after the
//! settlement instead would be the intuitive reading and the wrong one — it
//! would make `end` mean something different for `tracePromise` than for
//! `traceSync`.
//!
//! # Not implemented, by name
//!
//! `traceCallback` and `traceSync`'s error path — see
//! [`crate::diagnostics_channel`]'s doc for why neither is expressible. The
//! `hasSubscribers` aggregate is a data property refreshed by
//! [`refresh_aggregates`] rather than a live getter, for the reason that doc
//! also gives; a program that reaches past this module and mutates a
//! channel's `__subscribers__` array by hand sees a stale one. [`TRACERS`]
//! holds strong references and never drops one — the same deviation
//! the channel registry takes, and for the same reason: RTS's `WeakRef` is
//! interim-strong (issue #217).

use super::{
    add_subscriber, channel_object, do_publish, is_callable, member, remove_subscriber, set_member,
    string_arg, subscriber_count,
};
use rts_core_rwk::entry::{self, Provided};
use std::sync::Mutex;

/// The five channels a `TracingChannel` is made of, in the order Node names
/// them. One list rather than five spellings: the constructor, the bulk
/// subscribe, the bulk unsubscribe and the aggregate all walk the same names,
/// and a sixth copy is how one of them comes to miss a channel.
const SUB_CHANNELS: [&str; 5] = ["start", "end", "asyncStart", "asyncEnd", "error"];

/// What every `TracingChannel` inherits. `traceCallback` is deliberately
/// absent — see the module doc.
const TRACING_METHODS: &[(&str, Provided)] = &[
    ("subscribe", tracing_subscribe),
    ("unsubscribe", tracing_unsubscribe),
    ("traceSync", trace_sync),
    ("tracePromise", trace_promise),
];

/// Every `TracingChannel` handed out, so [`refresh_aggregates`] can re-derive
/// their `hasSubscribers` when any channel's subscriber list changes — including
/// a change made through a sub-channel directly, which the aggregate would
/// otherwise never hear about.
static TRACERS: Mutex<Vec<u64>> = Mutex::new(Vec::new());

/// Re-derives every `TracingChannel`'s aggregate `hasSubscribers`.
///
/// Ambient: it reads properties and therefore takes its own borrows, so it is
/// called after the caller's has been released.
pub(super) fn refresh_aggregates() {
    let tracers = {
        let guard = TRACERS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.clone()
    };
    for tracer in tracers {
        let any = SUB_CHANNELS
            .iter()
            .any(|name| subscriber_count(member(tracer, name)) > 0);
        set_member(tracer, "hasSubscribers", entry::boolean_value(any));
    }
}

/// `diagnostics_channel.tracingChannel(nameOrChannels)`.
///
/// Both documented forms. A string derives the five `tracing:${name}:*`
/// channels; an object is read as a `TracingChannelCollection` and its five
/// channels are used as they are — which is the point of that form, since a
/// caller passing it wants its own names traced rather than derived ones.
/// Anything else answers `undefined` rather than being stringified into a name
/// nobody asked for.
pub(super) extern "C" fn tracing_channel_fn(_e: u64, _this: u64, name_or_channels: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let channels: [u64; 5] = match string_arg(name_or_channels) {
        Some(name) => entry::with_runtime(|context| {
            SUB_CHANNELS.map(|part| channel_object(context, &format!("tracing:{name}:{part}")))
        }),
        None => {
            let given = SUB_CHANNELS.map(|part| member(name_or_channels, part));
            let absent = entry::undefined_value();
            if given.iter().any(|&channel| channel == absent) {
                return absent;
            }
            given
        }
    };
    let instance = entry::with_runtime(|context| {
        let prototype = entry::make_prototype(context, "TracingChannel", TRACING_METHODS);
        let instance = entry::make_instance(context, prototype);
        for (name, channel) in SUB_CHANNELS.iter().zip(channels) {
            entry::put_member(context, instance, name, channel);
        }
        let none = entry::boolean_value(false);
        entry::put_member(context, instance, "hasSubscribers", none);
        instance
    });
    {
        let mut guard = TRACERS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.push(instance);
    }
    refresh_aggregates();
    instance
}

/// `tracingChannel.subscribe({ start?, end?, asyncStart?, asyncEnd?, error? })`.
extern "C" fn tracing_subscribe(_e: u64, this: u64, subscribers: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    for name in SUB_CHANNELS {
        let handler = member(subscribers, name);
        if handler == absent {
            continue;
        }
        add_subscriber(member(this, name), handler);
    }
    entry::undefined_value()
}

/// `tracingChannel.unsubscribe({ ... })` — `true` only if every provided
/// handler was found and removed.
extern "C" fn tracing_unsubscribe(_e: u64, this: u64, subscribers: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let mut all_found = true;
    for name in SUB_CHANNELS {
        let handler = member(subscribers, name);
        if handler == absent {
            continue;
        }
        all_found &= remove_subscriber(member(this, name), handler);
    }
    entry::boolean_value(all_found)
}

/// `tracingChannel.traceSync(fn, context?, thisArg?)` — the returning path
/// only; see the module doc for the throwing one.
extern "C" fn trace_sync(_e: u64, this: u64, callback: u64, context_val: u64, this_arg: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let context_obj = trace_context(context_val);
    do_publish(member(this, "start"), context_obj);
    let result = entry::call(callback, this_arg, absent, absent, absent, absent);
    set_member(context_obj, "result", result);
    do_publish(member(this, "end"), context_obj);
    result
}

/// `tracingChannel.tracePromise(fn, context?, thisArg?)`.
///
/// A non-thenable return is handed straight back, untraced past `end` — Node
/// warns there and nothing here can raise a process warning.
extern "C" fn trace_promise(_e: u64, this: u64, callback: u64, context_val: u64, this_arg: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let context_obj = trace_context(context_val);
    do_publish(member(this, "start"), context_obj);
    let result = entry::call(callback, this_arg, absent, absent, absent, absent);
    do_publish(member(this, "end"), context_obj);
    let then = member(result, "then");
    if !is_callable(then) {
        return result;
    }
    let (Some(on_ok), Some(on_error)) = (
        correlated(settle_ok, this, context_obj),
        correlated(settle_error, this, context_obj),
    ) else {
        return result;
    };
    // Two attachments rather than `then(onOk, onError)`, and the second's own
    // promise is discarded. With both handlers on one `then`, the rejection is
    // CONSUMED — the caller's promise would fulfil with whatever the handler
    // returned, turning a failure into a success. The first attachment is what
    // the caller gets back, and it rejects with the original reason because it
    // was given no rejection handler; the second only observes.
    let chained = entry::call(then, result, on_ok, absent, absent, absent);
    entry::call(then, result, absent, on_error, absent, absent);
    chained
}

/// The context object a trace publishes on — the caller's, or a fresh one.
fn trace_context(context_val: u64) -> u64 {
    match context_val == entry::undefined_value() {
        true => entry::with_runtime(entry::make_object),
        false => context_val,
    }
}

/// A callable that carries which tracing channel and which context it belongs
/// to, as its first two arguments. `None` when `bind` is not reachable, which
/// is the one thing that would make this silently trace the wrong call.
fn correlated(code: Provided, tracer: u64, context_obj: u64) -> Option<u64> {
    let made = entry::with_runtime(|context| entry::make_callable(context, code));
    let bind = member(made, "bind");
    if !is_callable(bind) {
        return None;
    }
    let absent = entry::undefined_value();
    Some(entry::call(bind, made, absent, tracer, context_obj, absent))
}

/// The fulfilment side of [`trace_promise`], reached through [`correlated`].
///
/// It answers the value it was given so the promise the caller holds fulfils
/// with what `fn`'s promise did rather than with `undefined`.
extern "C" fn settle_ok(_e: u64, _this: u64, tracer: u64, context_obj: u64, value: u64, _d: u64) -> u64 {
    set_member(context_obj, "result", value);
    do_publish(member(tracer, "asyncStart"), context_obj);
    do_publish(member(tracer, "asyncEnd"), context_obj);
    value
}

/// The rejection side. What it returns is discarded — the promise the caller
/// holds is the other attachment's, and that one still rejects.
extern "C" fn settle_error(_e: u64, _this: u64, tracer: u64, context_obj: u64, reason: u64, _d: u64) -> u64 {
    set_member(context_obj, "error", reason);
    do_publish(member(tracer, "error"), context_obj);
    do_publish(member(tracer, "asyncStart"), context_obj);
    do_publish(member(tracer, "asyncEnd"), context_obj);
    entry::undefined_value()
}
