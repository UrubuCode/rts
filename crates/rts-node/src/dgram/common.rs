//! Small plumbing every other module here needs — property helpers, the
//! `emit`/`once` pair, and the options-bag readers. The same shape
//! `net/common.rs` keeps for the same reason, named there: duplicating a
//! handful of one-line accessors is cheaper, and safer, than widening a
//! private module's visibility for one caller.

use rts_core::entry::{self, Context};
use std::net::UdpSocket;

pub(super) fn key(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

pub(super) fn get_value(this: u64, name: &str) -> u64 {
    entry::get_indexed(this, key(name))
}

pub(super) fn get_bool(this: u64, name: &str) -> bool {
    entry::to_boolean(get_value(this, name))
}

pub(super) fn set_bool(context: &mut Context, this: u64, name: &str, value: bool) {
    let held = entry::boolean_value(value);
    entry::put_member(context, this, name, held);
}

pub(super) fn set_num(context: &mut Context, this: u64, name: &str, value: f64) {
    let held = entry::make_number(value);
    entry::put_member(context, this, name, held);
}

pub(super) fn set_value(context: &mut Context, this: u64, name: &str, value: u64) {
    entry::put_member(context, this, name, value);
}

pub(super) fn socket_id(this: u64) -> Option<u64> {
    entry::number_of(get_value(this, "__socketId")).map(|value| value as u64)
}

pub(super) fn with_socket(this: u64, body: impl FnOnce(&UdpSocket) -> std::io::Result<()>) {
    let Some(id) = socket_id(this) else { return };
    super::registry::with_sockets(|table| {
        if let Some(entry) = table.get(&id)
            && let Some(socket) = &entry.socket
        {
            let _ = body(socket);
        }
    });
}

/// Calls `this.emit(event, a0, a1, a2)` — looked up fresh, never while a
/// runtime borrow is held, the same recipe `net/common.rs::emit` uses and for
/// the same reason.
pub(super) fn emit(this: u64, event: &str, a0: u64, a1: u64, a2: u64) {
    let emit_fn = entry::with_runtime(|context| entry::get_member(context, this, "emit"));
    let absent = entry::undefined_value();
    if emit_fn == absent {
        return;
    }
    let event_key = key(event);
    entry::call(emit_fn, this, event_key, a0, a1, a2);
}

pub(super) fn once(this: u64, event: &str, listener: u64) {
    let once_fn = entry::with_runtime(|context| entry::get_member(context, this, "once"));
    let absent = entry::undefined_value();
    if once_fn != absent {
        let event = key(event);
        entry::call(once_fn, this, event, listener, absent, absent);
    }
}

/// Reads one member from an options object, from a context already in
/// hand — `entry::get_member` (context-taking), never `get_indexed` (the
/// ambient form, which is a nested borrow and therefore an abort when called,
/// as every option read here is, from inside [`entry::with_runtime`]).
pub(super) fn option_value(context: &mut Context, options: u64, name: &str) -> u64 {
    let absent = entry::undefined_in(context);
    if options == absent { absent } else { entry::get_member(context, options, name) }
}

pub(super) fn option_text(context: &mut Context, options: u64, name: &str) -> Option<String> {
    let value = option_value(context, options, name);
    // `string_in`, which TESTS, rather than `text_in`, which converts: an option
    // the caller left out is `undefined`, and converting that answers the literal
    // text "undefined". That is what sent `bind(0)` to the resolver looking for a
    // host by that name.
    entry::string_in(context, value)
}

pub(super) fn option_num(context: &mut Context, options: u64, name: &str) -> Option<f64> {
    entry::number_of(option_value(context, options, name))
}
