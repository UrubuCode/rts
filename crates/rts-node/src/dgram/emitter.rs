//! node:dgram — the `Socket`'s `emit`.
//!
//! Everything else the `EventEmitter` surface needs (`on`/`once`/`off`/
//! `prepend*`/`removeAllListeners`/`listeners`/`eventNames`/max-listeners, and
//! the listener table itself) is the crate-shared [`crate::emitter`] — its
//! externs take the receiver handle, so one implementation serves every
//! `rts-node` class that is an emitter. `mod.rs` installs that surface on the
//! `Socket` class.
//!
//! `emit` stays here because it is the one member that touches dgram's own
//! event QUEUE: a user `socket.emit(ev, …)` goes through the same queue the OS
//! events do, so its ordering against `'message'`/`'listening'` is preserved.

use super::state::{self, SockEvent};
use crate::emitter::pin_word;

/// Queue a user `socket.emit(event, ...args)`. Returns whether the event had
/// listeners at the time of the call (Node's `emit` return).
fn emit_words(this: u64, ep: *const u8, el: i64, args: Vec<u64>) -> i64 {
    let Some(st) = state::get(this) else {
        return 0;
    };
    let event = crate::values::read(ep, el);
    let had = crate::emitter::has(this, &event);
    for &a in &args {
        // The arg words are reachable only from the queue until the pump runs.
        pin_word(a);
    }
    st.push(SockEvent::Custom(event, args));
    i64::from(had)
}

/// `socket.emit(event)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_EMIT0(this: u64, ep: *const u8, el: i64) -> i64 {
    emit_words(this, ep, el, Vec::new())
}

/// `socket.emit(event, a0)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_EMIT1(this: u64, ep: *const u8, el: i64, a0: u64) -> i64 {
    emit_words(this, ep, el, vec![a0])
}

/// `socket.emit(event, a0, a1)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DGRAM_EMIT2(this: u64, ep: *const u8, el: i64, a0: u64, a1: u64) -> i64 {
    emit_words(this, ep, el, vec![a0, a1])
}
