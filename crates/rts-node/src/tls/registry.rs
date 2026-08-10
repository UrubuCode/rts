//! The table from a `TLSSocket`'s own id to its [`super::conn::Driver`] and
//! the underlying `net.Socket` it wraps — the native state a JS value can't
//! hold, the same shape `net/registry.rs` uses for its own sockets.
//!
//! Unlike `net/registry.rs`, nothing here owns a background thread: bytes
//! arrive through the UNDERLYING `net.Socket`'s own `'data'` listener
//! (`socket.rs`'s `on_data`), which already runs on the JS thread — see
//! `mod.rs`'s "WHEN" section for exactly when.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use super::conn::Driver;

pub(super) struct Entry {
    pub(super) driver: Driver,
    /// The `net.Socket` (or server-accepted equivalent) instance this
    /// `TLSSocket` sends ciphertext to and receives it from.
    pub(super) underlying: u64,
    pub(super) tls_instance: u64,
    pub(super) handshake_done: bool,
}

static ENTRIES: Mutex<Option<HashMap<u64, Entry>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub(super) fn next_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

pub(super) fn with_entries<T>(body: impl FnOnce(&mut HashMap<u64, Entry>) -> T) -> T {
    let mut guard = ENTRIES.lock().unwrap_or_else(|p| p.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}
