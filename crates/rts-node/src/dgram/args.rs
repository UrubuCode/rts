//! What `dgram.createSocket` and `socket.send` accept, and what they refuse.
//!
//! # Why this is its own module
//!
//! `mod.rs` is over this workspace's 500-line ceiling already, and CLAUDE.md's
//! rule for that case is explicit: new code lands in a small focused module,
//! never appended to something oversized. The two functions here are also the
//! only ones in `node:dgram` that read an argument in order to REFUSE it,
//! which is a different job from the rest of that file — every other native
//! there reads an argument in order to act on it.
//!
//! # Every check runs OUTSIDE a runtime borrow
//!
//! `crate::errors::*` builds an `Error` and throws it, taking its own borrow;
//! raising from inside `entry::with_runtime` is a nested `RefCell` borrow that
//! an `extern "C"` frame cannot unwind past, so it aborts the process rather
//! than failing a call. Each predicate below therefore opens its own short
//! borrow and closes it before answering, and the raising happens between
//! them. That is more borrows than one big closure would take, on a path that
//! ends in a throw — a cost paid where nothing measures it.
//!
//! # What Node refuses here that this does not
//!
//! - **A `msg` that is a bare `ArrayBuffer`.** Node refuses it for `send`
//!   (only string/`Buffer`/`TypedArray`/`DataView` are listed) and so does
//!   this — but for the wrong reason: `entry::bytes_of` answers only for a
//!   view, so this cannot tell an `ArrayBuffer` from a plain object either
//!   way. The right answer for the wrong reason is still stated.
//! - **`sendto`'s own signature.** It is not on the prototype at all, so a
//!   call to it is `undefined is not a function` — a `TypeError`, which is
//!   what `test-dgram-send-invalid-msg-type.js` asserts, but not because
//!   anything here validated it.

use rts_core::entry;

/// What Node's `ERR_INVALID_ARG_TYPE` says a datagram may be, word for word.
///
/// Shorter than `zlib`'s list by `ArrayBuffer`, and that difference is Node's
/// own: `test-dgram-send-bad-arguments.js` compares the whole sentence, so the
/// two surfaces cannot share one constant even though they share a raiser.
const INPUT_TYPES: &str = "string or an instance of Buffer, TypedArray, or DataView";

/// One resolved `socket.send` call — after the overload has been picked apart
/// and every argument accepted.
pub(super) struct SendCall {
    /// The datagram, already narrowed to the `offset`/`length` window when the
    /// call gave one.
    pub(super) bytes: Vec<u8>,
    /// The destination port, absent when the socket is expected to be
    /// connected.
    pub(super) port: Option<u16>,
    /// The destination host, absent when the caller left it to the family
    /// default.
    pub(super) address: Option<String>,
    /// The completion callback, `undefined` when there is none.
    pub(super) callback: u64,
}

/// The socket type a `createSocket` argument asks for, or `None` for one Node
/// refuses.
///
/// The valid set is exactly `'udp4'`, `'udp6'`, `{ type: 'udp4' }` and
/// `{ type: 'udp6' }`. `entry::string_in` — which TESTS — and not
/// `entry::text_in`, which converts: `text_in` is what let the number `1` and
/// the array `['udp4']` through as the text `"1"` and `"udp4"`, the first
/// silently becoming a `udp4` socket and the second one that actually worked.
/// `test-dgram-createSocket-type.js` refuses both, and `new String('udp4')`
/// with them — which falls out for free, since a wrapper object is not a
/// string and carries no `type`.
pub(super) fn socket_kind(options: u64) -> Option<String> {
    let kind = entry::with_runtime(|context| {
        if let Some(text) = entry::string_in(context, options) {
            return Some(text);
        }
        if !entry::is_object(context, options) || entry::is_array_in(context, options) {
            return None;
        }
        let value = entry::get_member(context, options, "type");
        entry::string_in(context, value)
    })?;
    matches!(kind.as_str(), "udp4" | "udp6").then_some(kind)
}

/// Node's `send(msg, [offset, length,] [port,] [address,] [callback])`,
/// resolved and checked. `None` means the call was refused — **the error has
/// already been raised**, so a caller answers `undefined` and does nothing
/// else.
///
/// # How the overload is decided
///
/// By asking whether the second AND third arguments are both numbers, which is
/// Node's own test. `send(buf, 6, 0)` is therefore an offset/length pair on a
/// connected socket and `send(buf, 12345, host)` is a port and a host, with no
/// ambiguity left: a port is a number and a host is not.
///
/// # Why the arguments come from `arguments_at`
///
/// A native here has four value slots, and this signature has six. Reading the
/// slots alone made `send(buf, 1, 1, -1, host)` arrive as
/// `(buf, port: 1, address: 1, callback: -1)` — a valid-looking call that sent
/// the datagram to port 1 instead of refusing port `-1`. `rts-core`'s
/// `arguments_at` is the one place that reads the compiler's spilled vector
/// back, and `node:path` already reaches for it for the same reason.
pub(super) fn send_call(connected: bool, a: u64, b: u64, c: u64, d: u64) -> Option<SendCall> {
    let absent = entry::undefined_value();
    let given = entry::with_runtime(|context| entry::arguments_at(context, 0, [a, b, c, d]));
    let at = |index: usize| given.get(index).copied().unwrap_or(absent);
    let window = match (entry::number_of(at(1)), entry::number_of(at(2))) {
        (Some(offset), Some(length)) => Some((offset, length)),
        _ => None,
    };
    let (port, address, callback) = match window.is_some() {
        true => (at(3), at(4), at(5)),
        false => (at(1), at(2), at(3)),
    };
    // The trailing callback slides forward when the arguments before it are
    // left out — `send(buf, cb)` on a connected socket, `send(buf, port, cb)`
    // with the host defaulted. Node performs the same two shifts, and without
    // them the function would be read as a destination and refused as a port
    // or a host: a legal call turned into a throw, which is the failure mode a
    // validation change is most likely to introduce.
    let (port, address, callback) = match (callable(port), callable(address)) {
        (true, _) => (absent, absent, port),
        (_, true) => (port, absent, address),
        _ => (port, address, callback),
    };

    let message = at(0);
    let bytes = datagram_bytes(message)?;
    // BEFORE the bounds and the port: a connected socket refuses a destination
    // outright, and `test-dgram-send-bad-arguments.js` asserts
    // `ERR_SOCKET_DGRAM_IS_CONNECTED` for a port of `-1` that would otherwise
    // be a `RangeError`. Which refusal comes first is observable, so it is
    // ordered here rather than left to the first check that happens to fail.
    if connected && port != absent {
        crate::errors::socket_dgram_is_connected();
        return None;
    }
    let bytes = match window {
        Some((offset, length)) => narrow(bytes, offset, length)?,
        None => bytes,
    };
    let port = match port == absent {
        true => None,
        false => Some(checked_port(port)?),
    };
    let address = entry::with_runtime(|context| entry::string_in(context, address));
    Some(SendCall { bytes, port, address, callback })
}

/// The datagram's bytes — one input, or a list of them concatenated.
///
/// The list arm reports the whole ARRAY and not the element that was wrong,
/// under the argument name `"buffer list arguments"`. That reads backwards
/// until you see the assertion it matches: Node says *"Received an instance of
/// Array"* for `send([buf, 23], …)`, because the list is what it validated.
fn datagram_bytes(message: u64) -> Option<Vec<u8>> {
    if entry::with_runtime(|context| entry::is_array_in(context, message)) {
        let count = entry::number_of(member(message, "length")).unwrap_or(0.0) as usize;
        let mut all = Vec::new();
        for index in 0..count {
            let element = entry::get_indexed(message, entry::make_number(index as f64));
            let Some(mut bytes) = input_bytes(element) else {
                crate::errors::invalid_arg_type("buffer list arguments", INPUT_TYPES, message);
                return None;
            };
            all.append(&mut bytes);
        }
        return Some(all);
    }
    match input_bytes(message) {
        Some(bytes) => Some(bytes),
        None => {
            crate::errors::invalid_arg_type("buffer", INPUT_TYPES, message);
            None
        }
    }
}

/// A `Buffer`/`TypedArray`/`DataView`'s bytes, or a `string`'s UTF-8.
///
/// `entry::string_in` rather than `entry::text_of`, which is `ToString` and is
/// what sent the number `23` as the two bytes `"23"` instead of refusing it.
fn input_bytes(value: u64) -> Option<Vec<u8>> {
    entry::with_runtime(|context| match entry::bytes_of(context, value) {
        Some(bytes) => Some(bytes),
        None => entry::string_in(context, value).map(String::into_bytes),
    })
}

/// `bytes[offset..offset + length]`, or `None` after raising the refusal.
///
/// Two names over one code: Node reports `ERR_BUFFER_OUT_OF_BOUNDS` for both
/// an offset past the end and a length that runs off it, and the quoted name
/// in the message is the only thing telling them apart.
fn narrow(bytes: Vec<u8>, offset: f64, length: f64) -> Option<Vec<u8>> {
    if offset < 0.0 || offset > bytes.len() as f64 {
        crate::errors::buffer_out_of_bounds("offset");
        return None;
    }
    if length < 0.0 || offset + length > bytes.len() as f64 {
        crate::errors::buffer_out_of_bounds("length");
        return None;
    }
    let start = offset as usize;
    Some(bytes[start..start + length as usize].to_vec())
}

/// A destination port, or `None` after raising `ERR_SOCKET_BAD_PORT`.
///
/// A numeric STRING is accepted, because Node's `validatePort` accepts one —
/// `send(buf, '12345', host)` is a working call there, and refusing it here
/// would be a stricter surface wearing a compatibility fix's clothes. Zero is
/// not a port a datagram can be sent to, which is why the range starts at 1.
fn checked_port(port: u64) -> Option<u16> {
    let number = entry::number_of(port).or_else(|| {
        entry::with_runtime(|context| entry::string_in(context, port))
            .and_then(|text| text.trim().parse::<f64>().ok())
    });
    match number {
        Some(number) if (1.0..=65535.0).contains(&number) && number.fract() == 0.0 => {
            Some(number as u16)
        }
        _ => {
            // `socket_bad_port` and not `bad_port`: the two carry one code and
            // two ranges, and `dgram`'s is the one that refuses port `0` —
            // that module's own doc records the pair as a decision.
            crate::errors::socket_bad_port("Port", port);
            None
        }
    }
}

/// Whether a value is a function — the question the overload shift asks, and
/// the reason `entry::is_callable_in` exists (`fs.watch` read its listener as
/// an options object for as long as nothing could ask).
fn callable(value: u64) -> bool {
    entry::with_runtime(|context| entry::is_callable_in(context, value))
}

/// A named member, read through the ambient accessor — every caller here is
/// outside a borrow, which is what makes that legal (see the module doc).
fn member(object: u64, name: &str) -> u64 {
    let key = entry::with_runtime(|context| entry::make_string(context, name));
    entry::get_indexed(object, key)
}
