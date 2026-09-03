//! Small plumbing this module needs and cannot reach from `crate::stream`:
//! that module's own `common.rs` is `pub(super)`, scoped to `stream` alone
//! (every submodule there is private — `mod readable;` not `pub mod`), so a
//! sibling module gets nothing from it beyond the prototype names it
//! registers under (`"Readable"`, `"Duplex"`, …), which
//! [`rts_core::entry::make_prototype`] happily hands back by name. This
//! file is the same handful of one-line property helpers `stream::common`
//! and `events.rs` each already have, kept small on purpose: duplicating
//! four property accessors is cheaper, and safer, than reaching into another
//! crate module's private state through a hole punched for one caller.

use rts_core::entry::{self, Context, Provided};

pub(super) fn key(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

pub(super) fn set_bool(context: &mut Context, this: u64, name: &str, value: bool) {
    let held = entry::boolean_value(value);
    entry::put_member(context, this, name, held);
}

pub(super) fn get_num(this: u64, name: &str) -> f64 {
    entry::number_of(entry::get_indexed(this, key(name))).unwrap_or(0.0)
}

pub(super) fn set_num(context: &mut Context, this: u64, name: &str, value: f64) {
    let held = entry::make_number(value);
    entry::put_member(context, this, name, held);
}

pub(super) fn get_value(this: u64, name: &str) -> u64 {
    entry::get_indexed(this, key(name))
}

pub(super) fn set_value(context: &mut Context, this: u64, name: &str, value: u64) {
    entry::put_member(context, this, name, value);
}

pub(super) fn set_array(context: &mut Context, this: u64, name: &str, values: Vec<u64>) {
    let array = entry::make_array_in(context, values);
    entry::put_member(context, this, name, array);
}

/// Calls `this.emit(event, a0, a1, a2)` — looked up fresh, never while a
/// runtime borrow is held. The same recipe `stream::common::emit` and
/// `events.rs`'s own `emit` use, for the same reason: an `extern "C"` frame
/// cannot unwind, so a nested borrow here is an abort, not an error.
pub(super) fn emit(this: u64, event: &str, a0: u64, a1: u64, a2: u64) {
    let emit_fn = entry::with_runtime(|context| entry::get_member(context, this, "emit"));
    let absent = entry::undefined_value();
    if emit_fn == absent {
        return;
    }
    let event_key = key(event);
    entry::call(emit_fn, this, event_key, a0, a1, a2);
}

/// Installs the `__events__`/`__eventNames__` bookkeeping every
/// `EventEmitter` needs — the same shape
/// `events::make_emitter`/`stream::common::init_emitter` build, duplicated
/// for the reason this file's own doc gives. Both, not `__events__` alone:
/// `events.rs`'s `eventNames()`/no-argument `removeAllListeners()` read
/// `__eventNames__` specifically, and a missing one reads as an
/// always-empty list forever, silently.
pub(super) fn init_emitter(context: &mut Context, instance: u64) {
    let events = entry::make_object(context);
    entry::put_member(context, instance, "__events__", events);
    let event_names = entry::make_array_in(context, Vec::new());
    entry::put_member(context, instance, "__eventNames__", event_names);
}

/// `this` if it is already an object (a `new` over a subclass hands one in),
/// else a fresh instance of `prototype`.
pub(super) fn self_or_new(context: &mut Context, this: u64, prototype: u64) -> u64 {
    match entry::is_object(context, this) {
        true => this,
        false => entry::make_instance(context, prototype),
    }
}

/// Builds a fresh, empty-bodied prototype chained onto `parent` (found — or
/// registered, if this is the first read anywhere in the process — by name)
/// then chains `methods` onto that. `parent` is usually `"EventEmitter"` or
/// `"Duplex"`, both built by the `node:events`/`node:stream` modules, which
/// `lib.rs::install` always runs before any JS the program wrote can reach
/// `node:net` — so by the time this runs, the name already resolves to the
/// SHARED prototype those modules built, never a fresh empty one.
pub(super) fn chained_prototype(context: &mut Context, parent: &'static str, name: &'static str, methods: &[(&str, Provided)]) -> u64 {
    let parent_prototype = entry::make_prototype(context, parent, &[]);
    let prototype = entry::make_prototype(context, name, methods);
    entry::set_prototype_in(context, prototype, parent_prototype);
    prototype
}

/// A string argument, read from a context already in hand — the pair
/// `stream::mod.rs::option_member` documents the same reason for: reading an
/// argument object's fields inside `with_runtime` where the ambient form
/// would be a nested borrow.
pub(super) fn option_text(context: &mut Context, options: u64, name: &str) -> Option<String> {
    let absent = entry::undefined_in(context);
    if options == absent {
        return None;
    }
    let value = entry::get_member(context, options, name);
    // The type TEST, not `ToString`. An option the caller left out reads as
    // `undefined`, and converting that answers the literal text `"undefined"` —
    // so `bind({ port: 0 })` bound to a host by that name, failed to resolve, and
    // emitted an `'error'` nothing handled. `None` is what an absent option is.
    entry::string_in(context, value)
}

/// A numeric option, read from a context already in hand.
pub(super) fn option_num(context: &mut Context, options: u64, name: &str) -> Option<f64> {
    let absent = entry::undefined_in(context);
    if options == absent {
        return None;
    }
    let value = entry::get_member(context, options, name);
    entry::number_of(value)
}
