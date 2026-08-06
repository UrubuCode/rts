//! Shared plumbing every class in this module needs: reading/writing a plain
//! data property, JS-array helpers, and the "chain onto `EventEmitter`"
//! recipe `fs/watch.rs` already worked out.
//!
//! Every property this module tracks (`readableEnded`, `writableLength`, …)
//! is an ordinary **data** property, kept in sync by hand as state changes —
//! not a computed accessor. `rts_core_rwk::entry::accessor` exists but is
//! wired for `#[rtse::class]`-declared classes with compile-time-interned
//! keys; a hand-written native module here follows the same plain-property
//! convention `events.rs` and `fs/watch.rs` already use.

use rts_core_rwk::entry::{self, Context, Provided};

/// An interned string, for use as a property key from outside a borrow.
pub(super) fn key(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

/// A JS array's elements, read out through `.length` and indexed access —
/// the same recipe `events.rs::collect_array` uses.
pub(super) fn collect_array(array: u64) -> Vec<u64> {
    let absent = entry::undefined_value();
    if array == absent {
        return Vec::new();
    }
    let length_value = entry::get_indexed(array, key("length"));
    let length = entry::number_of(length_value).map(|value| value as usize).unwrap_or(0);
    (0..length).map(|index| entry::get_indexed(array, entry::make_number(index as f64))).collect()
}

/// A plain data property, read as a `bool` (`undefined`/absent reads `false`).
pub(super) fn get_bool(this: u64, name: &str) -> bool {
    entry::to_boolean(entry::get_indexed(this, key(name)))
}

/// A plain data property, written as a `bool`.
pub(super) fn set_bool(context: &mut Context, this: u64, name: &str, value: bool) {
    let held = entry::boolean_value(value);
    entry::put_member(context, this, name, held);
}

/// A plain data property, read as an `f64` (`undefined`/non-number reads `0`).
pub(super) fn get_num(this: u64, name: &str) -> f64 {
    entry::number_of(entry::get_indexed(this, key(name))).unwrap_or(0.0)
}

/// A plain data property, written as an `f64`.
pub(super) fn set_num(context: &mut Context, this: u64, name: &str, value: f64) {
    let held = entry::make_number(value);
    entry::put_member(context, this, name, held);
}

/// A plain data property, written as any value.
pub(super) fn set_value(context: &mut Context, this: u64, name: &str, value: u64) {
    entry::put_member(context, this, name, value);
}

/// A plain data property, read back unchanged.
pub(super) fn get_value(this: u64, name: &str) -> u64 {
    entry::get_indexed(this, key(name))
}

/// `this.<name>` as an array's elements — `[]` when the property is absent.
pub(super) fn get_array(this: u64, name: &str) -> Vec<u64> {
    collect_array(get_value(this, name))
}

/// Replaces `this.<name>` with a fresh array holding `values`.
pub(super) fn set_array(context: &mut Context, this: u64, name: &str, values: Vec<u64>) {
    let array = entry::make_array_in(context, values);
    entry::put_member(context, this, name, array);
}

/// Calls `this.emit(event, a0, a1, a2)` — looked up fresh each time rather
/// than cached, so a program that replaces `emit` on an instance is honored.
/// Never called while a runtime borrow is held; see the module doc.
pub(super) fn emit(this: u64, event: &str, a0: u64, a1: u64, a2: u64) {
    let emit_fn = entry::with_runtime(|context| entry::get_member(context, this, "emit"));
    let absent = entry::undefined_value();
    if emit_fn == absent {
        return;
    }
    let event_key = key(event);
    entry::call(emit_fn, this, event_key, a0, a1, a2);
}

/// Builds a fresh, empty-bodied prototype chained onto `EventEmitter`'s
/// SHARED prototype (`events::namespace` always runs first — see
/// `lib.rs::install`), then chains `methods` onto that. The same two-step
/// recipe `fs/watch.rs::chained_prototype` uses, generalised to take an
/// explicit parent name instead of always `"EventEmitter"` — `Duplex` chains
/// onto `Readable`, not directly onto `Stream`.
pub(super) fn chained_prototype(context: &mut Context, parent: &'static str, name: &'static str, methods: &[(&str, Provided)]) -> u64 {
    let parent_prototype = entry::make_prototype(context, parent, &[]);
    let prototype = entry::make_prototype(context, name, methods);
    entry::set_prototype_in(context, prototype, parent_prototype);
    prototype
}

/// Installs the `__events__` bookkeeping every `EventEmitter` needs, the
/// same shape `events::make_emitter` builds — duplicated rather than called
/// (that constructor is private to `events.rs`) because an event-carrying
/// object here is built once per instance, inline in each class's own `new`.
pub(super) fn init_emitter(context: &mut Context, instance: u64) {
    let events = entry::make_object(context);
    entry::put_member(context, instance, "__events__", events);
}

/// `this` if it is already an object (a `new` over a subclass hands one in),
/// else a fresh instance of `prototype` — the same "plain call also works"
/// shape `events::make_emitter` documents.
pub(super) fn self_or_new(context: &mut Context, this: u64, prototype: u64) -> u64 {
    match entry::is_object(context, this) {
        true => this,
        false => entry::make_instance(context, prototype),
    }
}
