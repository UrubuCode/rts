//! What each `node:net` member ACCEPTS, checked before it does any work.
//!
//! # Why a module of its own, and not a check per member
//!
//! Because one rule serves several members and the code is not negotiable. A
//! port is refused identically by `net.connect(port)`, `net.connect({ port })`,
//! `socket.connect(...)` and `server.listen(...)`, and Node's own suite asserts
//! WHICH refusal each value earns:
//!
//! ```js
//! syncFailToConnect(null, { code: 'ERR_INVALID_ARG_TYPE' });
//! syncFailToConnect(65536, { code: 'ERR_SOCKET_BAD_PORT' });
//! ```
//!
//! Four hand-written checks are four chances to pick the wrong one of those two
//! codes. So the DECIDING lives here, once per rule, and the RAISING stays in
//! [`crate::errors`] — the same split `fs::validate` documents from the other
//! side, and the reason this file is that module's shape rather than a new one.
//!
//! # Every function answers `Option`, and `None` means "already raised"
//!
//! A native cannot unwind, so a refusal is a throw REGISTERED plus a return:
//!
//! ```ignore
//! let Some(port) = validate::port("options.port", held) else {
//!     return entry::undefined_value();
//! };
//! ```
//!
//! Returning immediately is not a style preference: [`crate::errors`] has left a
//! pending throw behind, and carrying on would open the socket the refusal
//! exists to prevent, with the throw landing afterwards over a connection that
//! already happened.
//!
//! # Never called from inside `with_runtime`
//!
//! Each function opens its own borrow to READ the value, closes it, and only
//! then raises — raising opens one of its own. A nested borrow in an
//! `extern "C"` frame cannot unwind and aborts the process, which is why the
//! reads below are collected into plain Rust values first.

use rts_core::entry::{self, Context};

/// The three shapes a port argument arrives in, told apart under one borrow.
///
/// A string is a shape of its own rather than something to coerce, because
/// Node accepts `connect('8080')` and refuses `connect(true)` — and `to_number`
/// would answer `1` for the second, turning a `TypeError` into a connection.
enum Given {
    Number(f64),
    Text(String),
    Other,
}

fn given(context: &Context, value: u64) -> Given {
    if let Some(number) = entry::number_of(value) {
        return Given::Number(number);
    }
    match entry::string_in(context, value) {
        Some(text) => Given::Text(text),
        None => Given::Other,
    }
}

/// A port — Node's `validatePort`, whose two refusals are two different codes.
///
/// A value that is neither a number nor a string never had a port in it:
/// `ERR_INVALID_ARG_TYPE`. A value of the right type that does not name a
/// whole number in `0..=65535` is `ERR_SOCKET_BAD_PORT`, including `NaN`,
/// `Infinity` and a string that does not read as a number at all.
pub(super) fn port(name: &str, value: u64) -> Option<u16> {
    let number = match entry::with_runtime(|context| given(context, value)) {
        Given::Other => {
            crate::errors::invalid_arg_type(name, "number or string", value);
            return None;
        }
        Given::Number(number) => Some(number),
        // Refused BEFORE coercion, which is Node's own order and not a
        // shortcut: `+' '` is `0`, so a blank string would otherwise bind port
        // zero — a random OS port — for a program that plainly meant to name
        // one and wrote nothing.
        Given::Text(text) if text.trim().is_empty() => None,
        Given::Text(text) => port_text(&text),
    };
    let whole = number.filter(|number| number.is_finite() && number.fract() == 0.0);
    match whole {
        Some(number) if (0.0..=65535.0).contains(&number) => Some(number as u16),
        _ => {
            crate::errors::bad_port(name, value);
            None
        }
    }
}

/// A string as `Number()` reads it, which is not as `str::parse` reads it.
///
/// The radix prefixes are the difference that matters here: `net`'s own suite
/// connects to `` `0x${port.toString(16)}` `` and expects it to WORK, while
/// expecting `'0x'` and `'-0x1'` to be refused — so a plain float parse would
/// fail the first and a lenient hex parse would accept the third. Rust's own
/// parser answering `inf`/`NaN` for those spellings costs nothing: both are
/// non-finite, and the caller refuses every non-finite port anyway.
///
/// Also the test `connect`'s own overload split needs: a first argument that is
/// a string is a PORT when this answers, and an IPC path when it does not,
/// which is exactly Node's `isPipeName`.
pub(super) fn port_text(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    let radix = |prefix: &str, base: u32| {
        trimmed
            .strip_prefix(prefix)
            .filter(|rest| !rest.is_empty())
            .and_then(|rest| u64::from_str_radix(rest, base).ok())
            .map(|value| value as f64)
    };
    for (prefix, base) in [("0x", 16), ("0X", 16), ("0o", 8), ("0O", 8), ("0b", 2), ("0B", 2)] {
        if trimmed.starts_with(prefix) {
            return radix(prefix, base);
        }
    }
    trimmed.parse::<f64>().ok()
}

/// A delay in milliseconds — Node's `getTimerDuration`, whose two refusals are
/// again two codes: a non-number is `ERR_INVALID_ARG_TYPE`, and a negative or
/// non-finite number is `ERR_OUT_OF_RANGE`.
///
/// `undefined` is NOT defaulted. `socket.setTimeout(undefined)` is in Node's
/// own list of values that throw, because a socket whose idle timeout was
/// meant to be computed and came out missing is a bug in the caller, not a
/// request for no timeout.
pub(super) fn msecs(name: &str, value: u64) -> Option<f64> {
    let Some(number) = entry::number_of(value) else {
        crate::errors::invalid_arg_type(name, "number", value);
        return None;
    };
    if number < 0.0 || !number.is_finite() {
        crate::errors::out_of_range(name, "a non-negative finite number", value);
        return None;
    }
    Some(number)
}

/// An optional callback: absent is fine, anything else must be callable.
///
/// `false` means the throw is already registered and the caller must return.
pub(super) fn optional_callback(name: &str, value: u64) -> bool {
    if value == entry::undefined_value() {
        return true;
    }
    let callable = entry::with_runtime(|context| entry::is_callable_in(context, value));
    if !callable {
        crate::errors::invalid_arg_type(name, "function", value);
    }
    callable
}

/// The `Readable`/`Writable` options a socket cannot honour.
///
/// A socket carries bytes; object mode would have it carry values, and nothing
/// under `registry` can. Node refuses these three by name rather than ignoring
/// them, which is the same rule this crate states everywhere: a surface that
/// cannot do what its name means does not accept the name.
pub(super) fn stream_options(options: u64) -> bool {
    const REFUSED: [&str; 3] = ["objectMode", "readableObjectMode", "writableObjectMode"];
    let found = entry::with_runtime(|context| {
        REFUSED.iter().find_map(|name| {
            let held = entry::get_member(context, options, name);
            entry::to_boolean_in(context, held).then_some((*name, held))
        })
    });
    let Some((name, held)) = found else {
        return true;
    };
    crate::errors::unsupported_property(&format!("options.{name}"), held);
    false
}

/// `options.hints` — a bit set drawn from `dns.ADDRCONFIG|V4MAPPED|ALL`.
///
/// The mask is `crate::dns`'s own numbering (4, 8, 16) and not Node's, because
/// a program reads the constants from the `node:dns` this crate ships; a
/// literal copy of Node's values would refuse the very flags our own module
/// hands out. Anything with a bit outside the mask is
/// `ERR_INVALID_ARG_VALUE` — the type was right and the value was not.
pub(super) fn hints(value: u64) -> bool {
    if value == entry::undefined_value() {
        return true;
    }
    const KNOWN: u32 = 4 | 8 | 16;
    let number = entry::number_of(value).unwrap_or(f64::NAN);
    let known = number.is_finite()
        && number.fract() == 0.0
        && (0.0..=f64::from(u32::MAX)).contains(&number)
        && (number as u32) & !KNOWN == 0;
    if !known {
        crate::errors::invalid_arg_value("hints", value, "is invalid");
    }
    known
}
