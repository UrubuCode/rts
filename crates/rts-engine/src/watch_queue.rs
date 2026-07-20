//! Cross-crate queue for filesystem-watch events (`node:fs` `watch`/`watchFile`).
//!
//! `node:fs` lives in a crate ABOVE the event loop (`rts-std`), and a watcher's
//! OS-notification thread must NOT touch the JS heap or invoke a JS callback
//! itself. So the watcher thread pushes PLAIN DATA here (a listener fn handle +
//! the event kind + the path — no GC handles), the event loop drains it on the
//! JS thread, builds the argument words, and invokes the listener. An
//! `active`-watcher counter keeps the loop alive while any watcher is open
//! (Node's `fs.watch` keeps the process running until every watcher is closed).
//!
//! This lives in `rts-engine` (the lowest shared crate) so both `rts-node` (the
//! producer) and `rts-std`'s event loop (the consumer) reach it without a
//! layering cycle.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

/// One pending watch notification. `kind`: 0 = `rename`, 1 = `change`
/// (`fs.watch`); 2 = `watchFile` change. `path` is the affected filename (for
/// `fs.watch`) or the watched path (for `watchFile`).
pub struct WatchEvent {
    pub listener: u64,
    pub kind: u8,
    pub path: String,
}

fn queue() -> &'static Mutex<Vec<WatchEvent>> {
    static Q: OnceLock<Mutex<Vec<WatchEvent>>> = OnceLock::new();
    Q.get_or_init(|| Mutex::new(Vec::new()))
}

static ACTIVE: AtomicUsize = AtomicUsize::new(0);

/// Push a watch event (called from a watcher's OS-notification thread — plain
/// data only, never a GC handle).
pub fn push(listener: u64, kind: u8, path: String) {
    queue().lock().unwrap().push(WatchEvent {
        listener,
        kind,
        path,
    });
}

/// Take all pending events (called by the event loop on the JS thread).
pub fn drain() -> Vec<WatchEvent> {
    std::mem::take(&mut *queue().lock().unwrap())
}

/// Register a newly-opened watcher — keeps the event loop alive.
pub fn inc_active() {
    ACTIVE.fetch_add(1, Ordering::AcqRel);
}

/// A watcher closed — the loop exits once this reaches zero.
pub fn dec_active() {
    // saturating: a double-close must not underflow the counter.
    let mut cur = ACTIVE.load(Ordering::Acquire);
    while cur > 0 {
        match ACTIVE.compare_exchange_weak(cur, cur - 1, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => break,
            Err(actual) => cur = actual,
        }
    }
}

/// Whether any watcher is still open (the event loop keeps draining while true).
pub fn active_count() -> usize {
    ACTIVE.load(Ordering::Acquire)
}
