//! The same handful of one-line property/prototype helpers `net::common`,
//! `stream::common` and `events.rs` each already have. Duplicated for the
//! same reason `net::common`'s own doc gives: every submodule of `stream`
//! and `net` is private (`mod socket;`, not `pub mod`), so this module
//! cannot reach either crate module's helper file — only the prototype
//! NAMES they register under (`"EventEmitter"`, `"Readable"`, `"Writable"`),
//! which [`rts_core::entry::make_prototype`] hands back by name by
//! design (see its own doc). Reaching into another crate module's private
//! state through a hole punched for one caller is the worse alternative.

use rts_core::entry::{self, Context, Provided};

pub(super) fn key(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

pub(super) fn get_value(this: u64, name: &str) -> u64 {
    entry::get_indexed(this, key(name))
}

/// O mesmo, mas DENTRO de um contexto já emprestado.
///
/// `get_value` monta a chave com `key`, que abre o contexto por conta própria —
/// e abrir o contexto dentro de um `with_runtime` é um `borrow_mut` reentrante
/// num `RefCell`, que num quadro `extern "C"` não desenrola: o processo morre
/// com `RefCell already borrowed`. Era o que acontecia em TODO `http.get`, no
/// primeiro cabeçalho que o cliente semeava.
pub(super) fn get_value_in(context: &mut entry::Context, this: u64, name: &str) -> u64 {
    entry::get_member(context, this, name)
}

pub(super) fn get_text(this: u64, name: &str) -> Option<String> {
    entry::text_of(get_value(this, name))
}

pub(super) fn get_num(this: u64, name: &str) -> f64 {
    entry::number_of(get_value(this, name)).unwrap_or(0.0)
}

pub(super) fn set_bool(context: &mut Context, this: u64, name: &str, value: bool) {
    let held = entry::boolean_value(value);
    entry::put_member(context, this, name, held);
}

pub(super) fn set_num(context: &mut Context, this: u64, name: &str, value: f64) {
    let held = entry::make_number(value);
    entry::put_member(context, this, name, held);
}

pub(super) fn set_text(context: &mut Context, this: u64, name: &str, value: &str) {
    let held = entry::make_string(context, value);
    entry::put_member(context, this, name, held);
}

pub(super) fn set_value(context: &mut Context, this: u64, name: &str, value: u64) {
    entry::put_member(context, this, name, value);
}

/// `this` if it is already an object (`new` over a subclass hands one in),
/// else a fresh instance of `prototype` — the same collapse
/// `net::common::self_or_new` and `stream::common`'s own copy make.
pub(super) fn self_or_new(context: &mut Context, this: u64, prototype: u64) -> u64 {
    match entry::is_object(context, this) {
        true => this,
        false => entry::make_instance(context, prototype),
    }
}

/// Installs the `__events__`/`__eventNames__` bookkeeping every
/// `EventEmitter` needs — both, not `__events__` alone: `events.rs`'s
/// `eventNames()`/no-argument `removeAllListeners()` read `__eventNames__`
/// specifically, and a missing one reads as an always-empty list forever,
/// silently, rather than as a crash — the same shape gap this crate's other
/// four copies of this helper had before all five were fixed together.
pub(super) fn init_emitter(context: &mut Context, instance: u64) {
    let events = entry::make_object(context);
    entry::put_member(context, instance, "__events__", events);
    let event_names = entry::make_array_in(context, Vec::new());
    entry::put_member(context, instance, "__eventNames__", event_names);
}

/// Calls `this.emit(event, a0, a1, a2)`, looked up fresh — never while a
/// runtime borrow is held; see [`net::common::emit`](super) for why (an
/// `extern "C"` frame cannot unwind, so a nested borrow aborts).
pub(super) fn emit(this: u64, event: &str, a0: u64, a1: u64, a2: u64) {
    let emit_fn = entry::with_runtime(|context| entry::get_member(context, this, "emit"));
    let absent = entry::undefined_value();
    if emit_fn == absent {
        return;
    }
    entry::call(emit_fn, this, key(event), a0, a1, a2);
}

/// A method looked up by name and called with up to three arguments — the
/// shape every delegation onto an underlying `net.Socket`/`net.Server`
/// instance in this module uses (`socket.write(...)`, `server.listen(...)`).
pub(super) fn call_method(this: u64, name: &str, a: u64, b: u64, c: u64) -> u64 {
    let method = entry::with_runtime(|context| entry::get_member(context, this, name));
    let absent = entry::undefined_value();
    if method == absent {
        return absent;
    }
    entry::call(method, this, a, b, c, absent)
}

/// Builds a fresh, empty-bodied prototype chained onto `parent` (found — or
/// registered, if this is the first read anywhere in the process — by name)
/// then chains `methods` onto that. The same recipe `net::common::chained_prototype`
/// documents; `parent` here is always `"EventEmitter"`, `"Readable"` or
/// `"Writable"`, all built by `node:events`/`node:stream`, which
/// `lib.rs::install` always runs before this module's own registration.
pub(super) fn chained_prototype(context: &mut Context, parent: &'static str, name: &'static str, methods: &[(&str, Provided)]) -> u64 {
    let parent_prototype = entry::make_prototype(context, parent, &[]);
    let prototype = entry::make_prototype(context, name, methods);
    entry::set_prototype_in(context, prototype, parent_prototype);
    prototype
}

/// Whether a value looks callable — `typeof value === "function"` would be
/// the precise test; this crate's convention (see `net::server::is_callable`)
/// is "has a `call` member", which is cheap and matches every callable this
/// crate ever hands out.
pub(super) fn is_callable(value: u64) -> bool {
    let absent = entry::undefined_value();
    value != absent && entry::with_runtime(|context| entry::get_member(context, value, "call")) != absent
}

/// A string option read off an options object, from a context already in
/// hand — reading an argument object's fields inside `with_runtime`, where
/// the ambient form would be a nested borrow and therefore an abort.
pub(super) fn option_text(context: &mut Context, options: u64, name: &str) -> Option<String> {
    let absent = entry::undefined_in(context);
    if options == absent {
        return None;
    }
    let value = entry::get_member(context, options, name);
    entry::text_in(context, value)
}

pub(super) fn option_num(context: &mut Context, options: u64, name: &str) -> Option<f64> {
    let absent = entry::undefined_in(context);
    if options == absent {
        return None;
    }
    let value = entry::get_member(context, options, name);
    entry::number_of(value)
}

pub(super) fn option_member(context: &mut Context, options: u64, name: &str) -> u64 {
    let absent = entry::undefined_in(context);
    if options == absent {
        return absent;
    }
    entry::get_member(context, options, name)
}
