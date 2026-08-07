//! The same small property/emit/prototype helpers `net/common.rs` carries,
//! duplicated for the reason its own doc gives: `net`'s `common` module is
//! `pub(super)` to `net` alone, so a sibling crate module gets nothing from
//! it except the prototype names it registers under, which
//! [`rts_core_rwk::entry::make_prototype`] hands back by name regardless of
//! which module asks.

use rts_core_rwk::entry::{self, Context, Provided};

pub(super) fn key(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

pub(super) fn set_bool(context: &mut Context, this: u64, name: &str, value: bool) {
    let held = entry::boolean_value(value);
    entry::put_member(context, this, name, held);
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

/// Calls `this.emit(event, a0, a1, a2)` — looked up fresh, never while a
/// runtime borrow is held; see `net/common.rs`'s own doc for why.
pub(super) fn emit(this: u64, event: &str, a0: u64, a1: u64, a2: u64) {
    let emit_fn = entry::with_runtime(|context| entry::get_member(context, this, "emit"));
    let absent = entry::undefined_value();
    if emit_fn == absent {
        return;
    }
    let event_key = key(event);
    entry::call(emit_fn, this, event_key, a0, a1, a2);
}

pub(super) fn init_emitter(context: &mut Context, instance: u64) {
    let events = entry::make_object(context);
    entry::put_member(context, instance, "__events__", events);
}

/// Chains `methods` onto a fresh prototype whose parent is found — or, if
/// nothing has built it yet, registered empty — by name. `parent` is
/// `"Socket"`/`"Server"` (`node:net`) here; `net`'s own module doc explains
/// why this always resolves to the real, already-built one: `lib.rs`'s
/// install order builds every registered module's namespace before any
/// program-written JS runs.
pub(super) fn chained_prototype(context: &mut Context, parent: &'static str, name: &'static str, methods: &[(&str, Provided)]) -> u64 {
    let parent_prototype = entry::make_prototype(context, parent, &[]);
    let prototype = entry::make_prototype(context, name, methods);
    entry::set_prototype_in(context, prototype, parent_prototype);
    prototype
}

/// A string option field, read from a context already in hand.
pub(super) fn option_text(context: &mut Context, options: u64, name: &str) -> Option<String> {
    let absent = entry::undefined_in(context);
    if options == absent {
        return None;
    }
    let value = entry::get_member(context, options, name);
    entry::text_in(context, value)
}

/// A numeric option field, read from a context already in hand.
pub(super) fn option_num(context: &mut Context, options: u64, name: &str) -> Option<f64> {
    let absent = entry::undefined_in(context);
    if options == absent {
        return None;
    }
    let value = entry::get_member(context, options, name);
    entry::number_of(value)
}

/// A value option field's raw handle, read from a context already in hand —
/// `None` for `undefined`.
pub(super) fn option_value(context: &mut Context, options: u64, name: &str) -> Option<u64> {
    let absent = entry::undefined_in(context);
    if options == absent {
        return None;
    }
    let value = entry::get_member(context, options, name);
    (value != absent).then_some(value)
}
