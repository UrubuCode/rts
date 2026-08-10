//! The three namespaces the bare `rts` specifier carries beside `num`/`hint`:
//! `time` (the clock), `atomic` (integer/bool/float cells with no data race to
//! guard against), and `sync` (a mutex over the same kind of cell).
//!
//! # Why `atomic` and `sync` are here rather than gone with `ptr`/`mem`/`gc`
//!
//! `CLAUDE.md` names them explicitly as surface this engine KEEPS, unlike the
//! pointer-and-memory family that left with the old engine. The distinction is
//! availability, same as everywhere else in this crate: a cell that holds one
//! `i64`/`bool`/`f64` and a mutex over one is a fact about scheduling, not about
//! an address space, so it has a shape on every target this engine will ever
//! have — including one with a second thread, someday.
//!
//! See `atomic.rs` and `sync.rs` for what each answers today, and the module
//! doc on each for why single-threaded is a real state to be honest about
//! rather than a stand-in for a lock this engine does not yet need.

mod atomic;
mod sync;
mod time;

use rts_core_rwk::entry::Context;

/// The three namespaces.
pub fn install(context: &mut Context, surface: u64) {
    time::install(context, surface);
    atomic::install(context, surface);
    sync::install(context, surface);
}
