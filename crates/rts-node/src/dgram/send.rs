//! node:dgram — `socket.send(...)`, every documented overload.
//!
//! ```text
//! send(msg[, port][, address][, callback])
//! send(msg, offset, length[, port][, address][, callback])   // byte forms only
//! ```
//!
//! `msg` is a `Buffer`/`TypedArray`/`DataView`, a `string` (UTF-8 encoded), or an
//! array of those (scattered, concatenated here into one datagram). `offset`/
//! `length` select a sub-range in BYTES and are valid only for the byte forms.
//! Node resolves these overloads in JS by inspecting the arguments; RTS does the
//! same natively (the Registry keys overloads by arity, so each arity is one
//! member taking `PolyValue`s).
//!
//! The `sendto` syscall runs inline on the JS thread (fast, non-blocking in the
//! common case) and the callback is queued for a later turn, per Node's contract
//! that the callback is "the only reliable way to know when a datagram was sent".
//! A hostname destination is resolved off-thread — resolution blocks, and Node
//! guarantees only that such a send is delayed by at least a tick.

use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use socket2::SockAddr;

use super::errors;
use super::lifecycle::{default_peer, ensure_bound, open, resolve};
use super::pump;
use super::state::{SockEvent, SocketState};
use crate::values::{read_bytes, val, Val};

/// The `send` argument list after Node's normalization.
struct SendArgs {
    payload: Vec<u8>,
    port: Option<u16>,
    address: Option<String>,
    cb: u64,
}

/// A `msg` argument, decoded. The kind matters because `offset`/`length` are
/// valid ONLY for a byte source — never for a string or a scatter list.
enum Msg {
    /// `Buffer`/`TypedArray`/`DataView` — the only form `offset`/`length` accept.
    Bytes(Vec<u8>),
    /// A string (UTF-8 encoded) or an array of parts, already flattened.
    Whole(Vec<u8>),
}

impl Msg {
    fn bytes(self) -> Vec<u8> {
        match self {
            Msg::Bytes(b) | Msg::Whole(b) => b,
        }
    }
}

/// Read the `msg` argument. `None` when the value is not a legal `msg` (Node
/// throws `ERR_INVALID_ARG_TYPE`).
fn msg_of(word: u64) -> Option<Msg> {
    match val(word) {
        Val::Str(s) => Some(Msg::Whole(s.into_bytes())),
        Val::Obj(h) => match scatter_parts(h) {
            // An array of parts concatenates into ONE datagram.
            Some(parts) => {
                let mut out = Vec::new();
                for p in parts {
                    out.extend_from_slice(&p?.bytes());
                }
                Some(Msg::Whole(out))
            }
            None => Some(Msg::Bytes(read_bytes(h))),
        },
        _ => None,
    }
}

/// If `h` is an ARRAY of message parts (`send([buf1, 'str', buf2])`), its parts.
/// A `Buffer`/`Uint8Array` is an `Entry::Vec` too, so the two are told apart by
/// their elements: a byte array holds only numbers, a scatter list does not.
fn scatter_parts(h: u64) -> Option<Vec<Option<Msg>>> {
    use rts_engine::heap::handles::{with_entry, Entry};
    let words: Vec<u64> = with_entry(h, |e| match e {
        Some(Entry::Vec(v)) => Some(v.iter().map(|&w| w as u64).collect()),
        _ => None,
    })?;
    if words.is_empty() || words.iter().all(|&w| matches!(val(w), Val::Num(_))) {
        return None;
    }
    Some(words.into_iter().map(msg_of).collect())
}

/// Normalize `send`'s polymorphic tail (`[offset, length][, port][, address][,
/// callback]`). Node's own rule: numbers are positional (offset+length first,
/// when the 3-number form is used, then port), a string is the address, a
/// function is the callback.
fn send_args(words: &[u64]) -> Result<SendArgs, (&'static str, String)> {
    let Some(&msg) = words.first() else {
        return Err(("ERR_INVALID_ARG_TYPE", "The \"msg\" argument must be specified".into()));
    };
    let msg = msg_of(msg).ok_or((
        "ERR_INVALID_ARG_TYPE",
        "The \"msg\" argument must be of type string or an instance of Buffer, TypedArray, or DataView".to_string(),
    ))?;

    let tail = &words[1..];
    let numbers: Vec<f64> = tail.iter().filter_map(|&w| val(w).as_num()).collect();
    // Two or more numbers means the `(msg, offset, length[, port])` form: one
    // number alone is the port.
    let has_offset_len = numbers.len() >= 2;
    if has_offset_len && !matches!(msg, Msg::Bytes(_)) {
        return Err((
            "ERR_INVALID_ARG_TYPE",
            "The \"offset\" and \"length\" arguments are invalid with a string or array message"
                .into(),
        ));
    }
    let (slice, port_from) = if has_offset_len {
        (Some((numbers[0], numbers[1])), numbers.get(2).copied())
    } else {
        (None, numbers.first().copied())
    };

    let payload = match slice {
        Some((o, l)) => slice_payload(&msg.bytes(), o, l)?,
        None => msg.bytes(),
    };

    let port = match port_from {
        Some(n) => Some(port_of(n)?),
        None => None,
    };
    let mut address = None;
    let mut cb = 0u64;
    for &w in tail {
        match val(w) {
            Val::Str(s) => address = Some(s),
            Val::Func(f) => cb = f,
            _ => {}
        }
    }
    Ok(SendArgs { payload, port, address, cb })
}

/// Apply `offset`/`length`, bounds-checked in BYTES against the message.
fn slice_payload(bytes: &[u8], offset: f64, length: f64) -> Result<Vec<u8>, (&'static str, String)> {
    let bad = |what: &str, v: f64| {
        (
            "ERR_OUT_OF_RANGE",
            format!("The value of \"{what}\" is out of range. Received {v}"),
        )
    };
    if !offset.is_finite() || offset < 0.0 || offset > bytes.len() as f64 {
        return Err(bad("offset", offset));
    }
    if !length.is_finite() || length < 0.0 || offset + length > bytes.len() as f64 {
        return Err(bad("length", length));
    }
    let (o, l) = (offset as usize, length as usize);
    Ok(bytes[o..o + l].to_vec())
}

fn port_of(n: f64) -> Result<u16, (&'static str, String)> {
    if !n.is_finite() || n.fract() != 0.0 || !(1.0..=65535.0).contains(&n) {
        return Err((
            errors::BAD_PORT,
            format!("Port should be > 0 and < 65536. Received {n}."),
        ));
    }
    Ok(n as u16)
}

/// The shared `send` implementation for every arity.
fn send_impl(this: u64, words: &[u64]) {
    let Some(st) = open(this) else { return };
    let args = match send_args(words) {
        Ok(a) => a,
        Err((code, msg)) => {
            errors::throw(code, &msg);
            return;
        }
    };
    let peer = st.peer_addr();
    if peer.is_some() && (args.port.is_some() || args.address.is_some()) {
        // Node: a connected socket rejects a per-send destination.
        errors::throw(
            errors::IS_CONNECTED,
            "Already connected — send() takes no port/address on a connected socket",
        );
        return;
    }
    // An unbound socket auto-binds to a random port before its first send.
    if let Err(e) = ensure_bound(this, &st) {
        let (code, msg) = errors::message_for(&e, "bind");
        st.push_err(args.cb, true, &code, &msg);
        pump::ensure_registered();
        return;
    }
    pump::ensure_registered();

    if peer.is_some() {
        // Connected: sendto with no destination.
        dispatch(&st, args.payload, None, args.cb);
        return;
    }
    let Some(port) = args.port else {
        errors::throw(
            errors::BAD_PORT,
            "Port should be > 0 and < 65536. Received undefined.",
        );
        return;
    };
    let host = args.address.unwrap_or_else(|| default_peer(st.v6).to_string());
    match host.parse::<std::net::IpAddr>() {
        // A literal address sends inline — no resolution needed.
        Ok(ip) => dispatch(&st, args.payload, Some(SocketAddr::new(ip, port)), args.cb),
        // A hostname must be resolved first; that call blocks, so it (and the
        // send it feeds) runs off the JS thread. The datagram is counted as
        // queued until it leaves — which is exactly what getSendQueueSize
        // reports.
        Err(_) => spawn_resolving_send(st.clone(), args.payload, host, port, args.cb),
    }
}

/// `sendto` on the calling thread + the callback queued for a later turn.
fn dispatch(st: &Arc<SocketState>, payload: Vec<u8>, to: Option<SocketAddr>, cb: u64) {
    let result = match to {
        Some(addr) => st.sock.send_to(&payload, &SockAddr::from(addr)),
        None => st.sock.send(&payload),
    };
    complete(st, result, cb);
}

/// Turn a `sendto` result into the callback/`'error'` Node delivers.
fn complete(st: &Arc<SocketState>, result: std::io::Result<usize>, cb: u64) {
    match result {
        Ok(_) => {
            if cb != 0 {
                // Node ≥ v6: the success callback's first argument is `null`.
                st.push(SockEvent::Callback { cb, err: None, err_first: true });
            }
        }
        Err(e) => {
            let (code, msg) = errors::message_for(&e, "send");
            st.push_err(cb, true, &code, &msg);
        }
    }
}

/// Resolve a hostname and send, off the JS thread.
fn spawn_resolving_send(st: Arc<SocketState>, payload: Vec<u8>, host: String, port: u16, cb: u64) {
    st.queue_bytes.fetch_add(payload.len() as i64, Ordering::AcqRel);
    st.queue_count.fetch_add(1, Ordering::AcqRel);
    let bytes = payload.len() as i64;
    let spawned = std::thread::Builder::new()
        .name("rts-dgram-send".to_string())
        .spawn(move || {
            let result = resolve(&host, port, st.v6)
                .and_then(|addr| st.sock.send_to(&payload, &SockAddr::from(addr)));
            st.queue_bytes.fetch_sub(bytes, Ordering::AcqRel);
            st.queue_count.fetch_sub(1, Ordering::AcqRel);
            complete(&st, result, cb);
        });
    if spawned.is_err() {
        errors::throw("ERR_SOCKET_DGRAM_NOT_RUNNING", "could not start the resolver thread");
    }
}

/// `socket.send(msg)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_SEND1(this: u64, a0: u64) {
    send_impl(this, &[a0]);
}

/// `socket.send(msg, portOrCallback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_SEND2(this: u64, a0: u64, a1: u64) {
    send_impl(this, &[a0, a1]);
}

/// `socket.send(msg, port, addressOrCallback)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_SEND3(this: u64, a0: u64, a1: u64, a2: u64) {
    send_impl(this, &[a0, a1, a2]);
}

/// `socket.send(msg, port, address, callback)` / `send(msg, offset, length, port)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_SEND4(this: u64, a0: u64, a1: u64, a2: u64, a3: u64) {
    send_impl(this, &[a0, a1, a2, a3]);
}

/// `socket.send(msg, offset, length, port, address)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_SEND5(this: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64) {
    send_impl(this, &[a0, a1, a2, a3, a4]);
}

/// `socket.send(msg, offset, length, port, address, callback)`.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_SEND6(
    this: u64,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    a4: u64,
    a5: u64,
) {
    send_impl(this, &[a0, a1, a2, a3, a4, a5]);
}
