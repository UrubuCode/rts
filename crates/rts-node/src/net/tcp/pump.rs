//! node:net — the event PUMP: the JS-thread side of delivery.
//!
//! The accept/read/connect threads only ever queue plain data. The event loop
//! calls this through the engine's generic pump registry
//! ([`rts_engine::loop_sources`], which knows nothing about net): it drains every
//! server's and socket's queue ON THE JS THREAD, builds the JS values (a `Socket`
//! for `'connection'`, a `Buffer`/string for `'data'`, an `Error`) and invokes
//! the listeners.
//!
//! Registration is LAZY (first listen/connect) because a class `register()` runs
//! at codegen time — the same process only for the JIT.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rts_engine::heap::handles::{alloc_entry, Entry};
use rts_engine::heap::poly::POLY_NULL;
use rts_engine::heap::shapes::{alloc_shaped_object, handle_word_auto, string_word};

use super::opts;
use super::state::{self, ServerEvent, ServerState, SockEvent, SocketState};
use crate::emitter::{self, Listener};
use crate::values::byte_array;

use rts_engine::gc_surface::__RTS_FN_RT_INVOKE_AUTO;

unsafe extern "C" {
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

static REGISTERED: AtomicBool = AtomicBool::new(false);

/// Make sure the event loop knows to drain us. Cheap + idempotent.
pub fn ensure_registered() {
    if REGISTERED.swap(true, Ordering::AcqRel) {
        return;
    }
    rts_engine::loop_sources::register_pump(__RTS_FN_NODE_NET_PUMP);
}

/// Drain every server's and socket's pending events on this (the JS) thread.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_NET_PUMP() -> usize {
    let mut delivered = 0usize;
    for (this, st) in state::server_snapshot() {
        let events: Vec<ServerEvent> = {
            let mut q = st.events.lock().unwrap();
            std::mem::take(&mut *q).into()
        };
        for ev in events {
            delivered += 1;
            deliver_server(this, &st, ev);
        }
    }
    for (this, st) in state::socket_snapshot() {
        let events: Vec<SockEvent> = {
            let mut q = st.events.lock().unwrap();
            std::mem::take(&mut *q).into()
        };
        for ev in events {
            delivered += 1;
            deliver_socket(this, &st, ev);
        }
    }
    delivered
}

fn deliver_server(this: u64, st: &ServerState, ev: ServerEvent) {
    match ev {
        ServerEvent::Listening => emit(this, "listening", Vec::new()),
        ServerEvent::Connection(stream) => {
            // The accepted socket becomes a JS `Socket` HERE, on the JS thread.
            let child = Arc::new(SocketState::new(st.opts));
            let handle = super::socket::register(child.clone());
            super::socket::established(handle, &child, stream);
            emit_owned(this, "connection", vec![handle_word_auto(handle)]);
        }
        ServerEvent::Drop { local, remote } => {
            let arg = alloc_shaped_object(
                &[
                    "localAddress",
                    "localPort",
                    "localFamily",
                    "remoteAddress",
                    "remotePort",
                    "remoteFamily",
                ],
                &[
                    string_word(local.ip().to_string().as_bytes()) as i64,
                    f64::from(local.port()).to_bits() as i64,
                    string_word(super::super::family_name(&local).as_bytes()) as i64,
                    string_word(remote.ip().to_string().as_bytes()) as i64,
                    f64::from(remote.port()).to_bits() as i64,
                    string_word(super::super::family_name(&remote).as_bytes()) as i64,
                ],
            );
            emit_owned(this, "drop", vec![handle_word_auto(arg)]);
        }
        ServerEvent::Error(code, message) => {
            // A Server's 'error' does NOT auto-close it (unlike a Socket).
            if !emitter::has(this, "error") {
                throw(&code, &message);
                return;
            }
            emit_owned(this, "error", vec![opts::error_value(&code, &message)]);
        }
        ServerEvent::Callback { cb, err } => match err {
            Some((code, message)) => {
                let args = vec![opts::error_value(&code, &message)];
                pinned(&args, || invoke(cb, this, args.clone()));
            }
            None => invoke(cb, this, Vec::new()),
        },
        ServerEvent::Connections { cb, count } => {
            invoke(cb, this, vec![POLY_NULL, f64::from(count as u32).to_bits()]);
        }
        ServerEvent::Custom(event, args) => {
            emit(this, &event, args.clone());
            for w in args {
                emitter::unpin_word(w);
            }
        }
        ServerEvent::Close => {
            emit(this, "close", Vec::new());
            super::server::finalize(this);
        }
    }
}

fn deliver_socket(this: u64, st: &SocketState, ev: SockEvent) {
    match ev {
        SockEvent::Connect => {
            emit(this, "connect", Vec::new());
            // Node fires 'ready' immediately after 'connect'.
            emit(this, "ready", Vec::new());
        }
        SockEvent::Data(bytes) => {
            let enc = st.encoding.lock().unwrap().clone();
            let arg = match enc {
                // setEncoding() makes 'data' deliver strings.
                Some(enc) => string_word(super::decode(&bytes, &enc).as_bytes()),
                None => handle_word_auto(byte_array(&bytes)),
            };
            emit_owned(this, "data", vec![arg]);
        }
        SockEvent::End => {
            emit(this, "end", Vec::new());
            // allowHalfOpen false (the default): the peer's FIN ends our writable
            // side too, and the socket is done.
            if !st.opts.lock().unwrap().allow_half_open {
                if let Some(state) = state::socket(this) {
                    super::socket::destroy(this, &state, false);
                }
            }
        }
        SockEvent::Drain => emit(this, "drain", Vec::new()),
        SockEvent::Timeout => emit(this, "timeout", Vec::new()),
        SockEvent::Lookup { err, address, family, host } => {
            let e = match &err {
                Some(m) => opts::error_value("ENOTFOUND", m),
                None => POLY_NULL,
            };
            let args = vec![
                e,
                string_word(address.as_bytes()),
                f64::from(family as u32).to_bits(),
                string_word(host.as_bytes()),
            ];
            emit_owned(this, "lookup", args);
        }
        SockEvent::Attempt { ip, port, family, err } => {
            let event = match &err {
                Some(_) => "connectionAttemptFailed",
                None => "connectionAttempt",
            };
            let mut args = vec![
                string_word(ip.as_bytes()),
                f64::from(port).to_bits(),
                f64::from(family as u32).to_bits(),
            ];
            if let Some(m) = &err {
                args.push(opts::error_value("ECONNREFUSED", m));
            }
            emit_owned(this, event, args);
        }
        SockEvent::Error(code, message) => {
            if !emitter::has(this, "error") {
                throw(&code, &message);
                return;
            }
            emit_owned(this, "error", vec![opts::error_value(&code, &message)]);
        }
        SockEvent::Callback { cb, err } => match err {
            Some((code, message)) => {
                let args = vec![opts::error_value(&code, &message)];
                pinned(&args, || invoke(cb, this, args.clone()));
            }
            None => invoke(cb, this, vec![POLY_NULL]),
        },
        SockEvent::Custom(event, args) => {
            emit(this, &event, args.clone());
            for w in args {
                emitter::unpin_word(w);
            }
        }
        SockEvent::Close(had_error) => {
            let flag = if had_error {
                rts_engine::heap::poly::POLY_TRUE
            } else {
                rts_engine::heap::poly::POLY_FALSE
            };
            emit(this, "close", vec![flag]);
            super::socket::finalize(this);
        }
    }
}

/// Run `body` with `args` GC-pinned. The words the pump builds are reachable
/// only from a Rust `Vec` — which the conservative stack scanner does not see —
/// so they are pinned across the window between building and invoking. Only
/// FRESH words: a pin is set membership, not a count.
fn pinned<R>(args: &[u64], body: impl FnOnce() -> R) -> R {
    for &w in args {
        emitter::pin_word(w);
    }
    let out = body();
    for &w in args {
        emitter::unpin_word(w);
    }
    out
}

/// [`emit`] for arguments the pump itself allocated.
fn emit_owned(this: u64, event: &str, args: Vec<u64>) {
    pinned(&args, || emit(this, event, args.clone()));
}

/// Run every listener of `event`, in registration order.
fn emit(this: u64, event: &str, args: Vec<u64>) {
    let listeners: Vec<Listener> = emitter::take_for(this, event);
    for l in listeners {
        invoke(l.cb, this, args.clone());
    }
}

/// Invoke one listener with the receiver as `this`.
///
/// GOTCHA: `INVOKE_AUTO` takes a Function HANDLE (what `Val::Func` normalizes
/// to) plus an `Entry::Vec` of argument WORDS — `__rtsadp_fn_invoke` is the one
/// that wants a boxed word.
fn invoke(cb: u64, this: u64, args: Vec<u64>) {
    if cb == 0 {
        return;
    }
    let words: Vec<i64> = args.into_iter().map(|w| w as i64).collect();
    let argv = alloc_entry(Entry::Vec(Box::new(words)));
    unsafe {
        __RTS_FN_RT_INVOKE_AUTO(cb as i64, handle_word_auto(this) as i64, argv);
    }
}

fn throw(code: &str, message: &str) {
    let msg = format!("{code}: {message}");
    unsafe {
        __rtsadp_throw_js_error(code.as_ptr(), code.len() as i64, msg.as_ptr(), msg.len() as i64);
    }
}
