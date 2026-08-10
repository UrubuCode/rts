//! The two namespaces the bare `rts` specifier carries beside `num`/`hint`:
//! `time` (the clock) and `atomic` (integer/bool/float cells with no data race
//! to guard against).
//!
//! # Why `atomic` is here rather than gone with `ptr`/`mem`/`gc`/`sync`
//!
//! Availability, same as everywhere else in this crate: a cell that holds one
//! `i64`/`bool`/`f64` is a fact about scheduling, not about an address space,
//! so it has a shape on every target this engine will ever have.
//!
//! `sync` — a mutex over the same kind of cell — left with `thread`: this
//! engine runs JavaScript on one OS thread (see `atomic.rs`'s module doc for
//! the evidence), and the owner decided a mutex that can never contend is not
//! surface worth keeping rather than kept as a lock nothing exercises under
//! contention.
//!
//! See `atomic.rs` for what it answers today, and its module doc for why
//! single-threaded is a real state to be honest about rather than a stand-in
//! for a lock this engine does not have.

mod atomic;
mod time;

use rts_core_rwk::entry::Context;

/// The two namespaces.
pub fn install(context: &mut Context, surface: u64) {
    time::install(context, surface);
    atomic::install(context, surface);
}
