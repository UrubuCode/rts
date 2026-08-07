//! The same handful of one-line property/prototype helpers `http::common`,
//! `net::common` and `tls::common` each already have. Duplicated for the
//! same reason `http::common`'s own doc gives, restated for this crate's
//! third layer: `http`'s and `tls`'s submodules are private (`mod server;`,
//! not `pub mod`), so this module cannot reach either crate module's helper
//! file directly — only the prototype NAMES they register under
//! (`"EventEmitter"`, `"Writable"`, `"Server"`, `"Socket"`), which
//! [`rts_core_rwk::entry::make_prototype`] hands back by name by design, and
//! the JS-level surface (`get_member`/`call`/`emit`) both modules already
//! expose. Reaching into another crate module's private state through a
//! hole punched for one caller is the worse alternative — the same call
//! `http::common`'s own doc makes.

use rts_core_rwk::entry::{self, Context};

pub(super) fn key(text: &str) -> u64 {
    entry::with_runtime(|context| entry::make_string(context, text))
}

pub(super) fn get_value(this: u64, name: &str) -> u64 {
    entry::get_indexed(this, key(name))
}

pub(super) fn set_text(context: &mut Context, this: u64, name: &str, value: &str) {
    let held = entry::make_string(context, value);
    entry::put_member(context, this, name, held);
}

pub(super) fn set_value(context: &mut Context, this: u64, name: &str, value: u64) {
    entry::put_member(context, this, name, value);
}

pub(super) fn set_bool(context: &mut Context, this: u64, name: &str, value: bool) {
    let held = entry::boolean_value(value);
    entry::put_member(context, this, name, held);
}

/// Installs the `__events__` bookkeeping every `EventEmitter` needs — the
/// same one-liner `http::common::init_emitter` and `tls::common`'s own copy
/// are, private to their own crate modules.
pub(super) fn init_emitter(context: &mut Context, instance: u64) {
    let events = entry::make_object(context);
    entry::put_member(context, instance, "__events__", events);
}

/// Calls `this.emit(event, a0, a1, a2)`, looked up fresh — never while a
/// runtime borrow is held (an `extern "C"` frame cannot unwind, so a nested
/// borrow aborts). The mechanism this whole module leans on: `tls.Server`'s
/// `'secureConnection'` is relayed onto `http.Server`'s own held
/// `net.Server` through exactly this, which is what lets the real
/// `on_connection`/parser/`IncomingMessage`/`ServerResponse` machinery
/// already registered as ITS listener run unmodified — see `server.rs`'s
/// own doc.
pub(super) fn emit(this: u64, event: &str, a0: u64, a1: u64, a2: u64) {
    let emit_fn = entry::with_runtime(|context| entry::get_member(context, this, "emit"));
    let absent = entry::undefined_value();
    if emit_fn == absent {
        return;
    }
    entry::call(emit_fn, this, key(event), a0, a1, a2);
}

/// A method looked up by name and called with up to three arguments.
pub(super) fn call_method(this: u64, name: &str, a: u64, b: u64, c: u64) -> u64 {
    let method = entry::with_runtime(|context| entry::get_member(context, this, name));
    let absent = entry::undefined_value();
    if method == absent {
        return absent;
    }
    entry::call(method, this, a, b, c, absent)
}

pub(super) fn is_callable(value: u64) -> bool {
    let absent = entry::undefined_value();
    value != absent && entry::with_runtime(|context| entry::get_member(context, value, "call")) != absent
}

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

/// One member off `http`'s namespace object — its constructors and
/// prototypes are ordinary properties on it, so reaching one is a property
/// read like any other; `Provided` methods installed on a class's
/// prototype are reachable the same way regardless of which crate module
/// built them, since a JS property has no Rust-side privacy.
pub(super) fn http_member(context: &mut Context, name: &str) -> u64 {
    let http_ns = crate::http::namespace(context);
    entry::get_member(context, http_ns, name)
}

pub(super) fn tls_member(context: &mut Context, name: &str) -> u64 {
    let tls_ns = crate::tls::namespace(context);
    entry::get_member(context, tls_ns, name)
}
