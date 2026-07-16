//! node:dgram — the event PUMP: the JS-thread side of delivery.
//!
//! The reader thread and the sender threads only ever queue plain data. This
//! module is what the event loop calls (through the engine's generic
//! [`rts_engine::loop_sources`] registry, which knows nothing about dgram): it
//! drains every socket's queue ON THE JS THREAD, builds the argument words
//! (`Buffer` + `rinfo`, an `Error`, …) and invokes the listeners.
//!
//! Registration is LAZY — on the first bind/send — because the Registry's
//! `register()` runs at codegen time, which is the same process only for the JIT;
//! doing it from a live socket covers `rts run` and an AOT binary alike.

use std::sync::atomic::{AtomicBool, Ordering};

use rts_engine::heap::handles::{alloc_entry, Entry};
use rts_engine::heap::shapes::{alloc_shaped_object, handle_word_auto, string_word};

use super::emitter;
use super::lifecycle;
use super::state::{self, Datagram, Listener, SockEvent, SocketState};
use crate::values::byte_array;

unsafe extern "C" {
    fn __RTS_FN_RT_INVOKE_AUTO(callee: i64, this_arg: i64, args_handle: u64) -> i64;
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

static REGISTERED: AtomicBool = AtomicBool::new(false);

/// Make sure the event loop knows to drain us. Cheap + idempotent.
pub fn ensure_registered() {
    if REGISTERED.swap(true, Ordering::AcqRel) {
        return;
    }
    rts_engine::loop_sources::register_pump(__RTS_FN_NODE_DGRAM_PUMP);
}

/// Drain every socket's pending events, delivering each on this (the JS) thread.
/// Returns how many events were delivered — the loop uses it as its activity
/// signal.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_PUMP() -> usize {
    let mut delivered = 0usize;
    for (this, st) in state::snapshot() {
        // Snapshot out of the lock: a listener may close its socket or emit.
        let events: Vec<SockEvent> = {
            let mut q = st.events.lock().unwrap();
            std::mem::take(&mut *q).into()
        };
        for ev in events {
            delivered += 1;
            deliver(this, &st, ev);
        }
    }
    delivered
}

fn deliver(this: u64, st: &SocketState, ev: SockEvent) {
    match ev {
        SockEvent::Listening => emit(this, st, "listening", Vec::new()),
        SockEvent::Connect => emit(this, st, "connect", Vec::new()),
        SockEvent::Message(dg) => emit_owned(this, st, "message", message_args(&dg)),
        SockEvent::Custom(event, args) => {
            // Pinned since `emit_words` queued them.
            emit(this, st, &event, args.clone());
            for w in args {
                emitter::unpin_word(w);
            }
        }
        SockEvent::Error(code, message) => {
            if st.listeners.lock().unwrap().get("error").is_empty() {
                // Node: an 'error' with no listener is fatal — an EventEmitter has
                // no default handler. Surface it as a real thrown error rather
                // than swallowing it.
                throw(&code, &message);
                return;
            }
            emit_owned(this, st, "error", vec![error_value(&code, &message)]);
        }
        SockEvent::Callback { cb, err, err_first } => match (&err, err_first) {
            (Some((code, message)), _) => {
                let args = vec![error_value(code, message)];
                pinned(&args, || invoke(cb, this, args.clone()));
            }
            (None, true) => invoke(cb, this, vec![rts_engine::heap::poly::POLY_NULL]),
            (None, false) => invoke(cb, this, Vec::new()),
        },
        SockEvent::Close => {
            emit(this, st, "close", Vec::new());
            // Everything is delivered — release the state, the listener pins and
            // the object pin.
            lifecycle::finalize(this);
        }
    }
}

/// Run `body` with `args` GC-pinned.
///
/// The words the pump builds (the `Buffer`, the `rinfo`, an `Error`) are
/// reachable only from a Rust `Vec` — which the conservative stack scanner does
/// not see, since its buffer lives in the malloc heap, not on the stack. Pinning
/// covers the window between building them and the invoke that puts them on the
/// JS side. Only FRESH words are pinned this way: a pin is a set membership, not
/// a count, so unpinning a word something else pinned (a registered listener a
/// user passed to `emit`) would drop that pin too.
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
fn emit_owned(this: u64, st: &SocketState, event: &str, args: Vec<u64>) {
    pinned(&args, || emit(this, st, event, args.clone()));
}

/// `(msg: Buffer, rinfo: { address, family, port, size })`.
fn message_args(dg: &Datagram) -> Vec<u64> {
    let buf = handle_word_auto(byte_array(&dg.bytes));
    let family = if dg.from.is_ipv6() { "IPv6" } else { "IPv4" };
    let rinfo = alloc_shaped_object(
        &["address", "family", "port", "size"],
        &[
            string_word(dg.from.ip().to_string().as_bytes()) as i64,
            string_word(family.as_bytes()) as i64,
            f64::from(dg.from.port()).to_bits() as i64,
            (dg.bytes.len() as f64).to_bits() as i64,
        ],
    );
    vec![buf, handle_word_auto(rinfo)]
}

/// The `Error` value a listener/callback receives — the same real Error-family
/// instance the synchronous throws produce, built through the one authority
/// (`errors::value`) so the two paths cannot drift.
fn error_value(code: &str, message: &str) -> u64 {
    super::errors::value(code, message)
}

fn throw(code: &str, message: &str) {
    let msg = format!("{code}: {message}");
    unsafe {
        __rtsadp_throw_js_error(code.as_ptr(), code.len() as i64, msg.as_ptr(), msg.len() as i64);
    }
}

/// Run every listener of `event` with `args`, in registration order.
fn emit(this: u64, st: &SocketState, event: &str, args: Vec<u64>) {
    let listeners: Vec<Listener> = emitter::take_for(st, event);
    for l in listeners {
        invoke(l.cb, this, args.clone());
    }
}

/// Invoke one listener with the socket as `this`.
///
/// GOTCHA: `INVOKE_AUTO` takes a Function HANDLE (what `Val::Func` normalizes
/// to) plus an `Entry::Vec` of argument WORDS — `__rtsadp_fn_invoke` is the one
/// that wants a boxed word. Mixing the two yields "not a function".
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
