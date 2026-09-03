//! `events.addAbortListener(signal, listener)`.

use rts_core::entry;

/// `events.addAbortListener(signal, listener)`.
///
/// The callback is registered on the engine's global `AbortSignal` through its
/// ordinary EventTarget API, but carries an internal protected flag. EventTarget
/// delivers that registration after `stopImmediatePropagation`, which is the
/// security property Node's helper promises. A fixed native callable stores the
/// user's listener as its closure state, so disposal needs no runtime cache.
pub(super) extern "C" fn add_abort_listener(_e: u64, _this: u64, signal: u64, listener: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    if !is_abort_signal(signal) {
        entry::invalid_arg_instance("signal", "AbortSignal", signal);
        return absent;
    }
    let callable = entry::with_runtime(|context| entry::is_callable_in(context, listener));
    if !callable {
        entry::invalid_arg_type("listener", "function", listener);
        return absent;
    }

    let callback = entry::closure_new(abort_listener_callback as *const () as usize as i64, listener);
    let options = entry::with_runtime(|context| {
        let options = entry::make_object(context);
        entry::put_member(context, options, "once", entry::boolean_value(true));
        entry::put_member(
            context,
            options,
            "__nodeProtectedAbortListener__",
            entry::boolean_value(true),
        );
        options
    });
    let add = entry::get_indexed(signal, super::string_key("addEventListener"));
    entry::call(add, signal, super::string_key("abort"), callback, options, absent);
    if entry::thrown() != 0 {
        return absent;
    }

    let aborted = entry::to_boolean(entry::get_indexed(signal, super::string_key("aborted")));
    if aborted {
        let event = entry::get_indexed(signal, super::string_key("__nodeAbortEvent__"));
        entry::call(listener, signal, event, absent, absent, absent);
    }
    disposable(signal, callback)
}

/// The fixed callable stored by a protected EventTarget registration.
extern "C" fn abort_listener_callback(listener: u64, this: u64, event: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let absent = entry::undefined_value();
    entry::call(listener, this, event, absent, absent, absent)
}

/// Build the object returned by `addAbortListener`.
fn disposable(signal: u64, callback: u64) -> u64 {
    let (object, dispose_key, dispose_fn) = entry::with_runtime(|context| {
        let object = entry::make_object(context);
        entry::put_member(context, object, "__abortSignal__", signal);
        entry::put_member(context, object, "__abortCallback__", callback);
        let dispose_key = entry::well_known_symbol(context, "dispose");
        let dispose_fn = entry::make_callable(context, dispose_abort_listener);
        (object, dispose_key, dispose_fn)
    });
    let object_root = entry::hold_current(object);
    let dispose_root = entry::hold_current(dispose_fn);
    entry::set_indexed(object, dispose_key, dispose_fn, 0 /* strict: quem escreve a partir do host reporta a recusa */);
    entry::release_current(dispose_root);
    entry::release_current(object_root);
    object
}

/// `[Symbol.dispose]()` removes the protected registration. Repeated disposal
/// is idempotent because EventTarget removal is idempotent for a missing record.
extern "C" fn dispose_abort_listener(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let absent = entry::undefined_value();
    let signal = entry::get_indexed(this, super::string_key("__abortSignal__"));
    let callback = entry::get_indexed(this, super::string_key("__abortCallback__"));
    if signal == absent || callback == absent {
        return absent;
    }
    let remove = entry::get_indexed(signal, super::string_key("removeEventListener"));
    entry::call(remove, signal, super::string_key("abort"), callback, absent, absent);
    entry::set_indexed(this, super::string_key("__abortSignal__"), absent, 0 /* strict: quem escreve a partir do host reporta a recusa */);
    entry::set_indexed(this, super::string_key("__abortCallback__"), absent, 0 /* strict: quem escreve a partir do host reporta a recusa */);
    absent
}

/// Whether `value` is an instance of the global `AbortSignal` class.
///
/// `pub(super)` rather than local: [`super::once_promise`] reuses this exact
/// check for `options.signal` rather than the looser "carries `aborted`" rule
/// `node:timers/promises` uses for ITS signal option. That module can afford
/// the loose rule because it only ever READS `.aborted`/`.reason`, polled at
/// its own next call; `events.once` has no such next call to poll from and
/// must SUBSCRIBE — `signal.addEventListener('abort', …)` — so a value with no
/// real `AbortSignal` behind it would silently never reject. Rejecting it up
/// front with `ERR_INVALID_ARG_TYPE`, which is what real Node does too, is the
/// honest answer.
pub(super) fn is_abort_signal(value: u64) -> bool {
    let constructor = entry::with_runtime(|context| {
        let global = entry::global_object(context);
        entry::get_member(context, global, "AbortSignal")
    });
    constructor != entry::undefined_value() && entry::instance_of(value, constructor)
}
