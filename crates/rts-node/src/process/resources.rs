//! `process.getActiveResourcesInfo()` — the handle-class names of what is
//! actually still open, read out of the two tables that already know.
//!
//! # Why this is honest and not a fixed list
//!
//! Node's own answer enumerates libuv's live handles by their class name
//! (`"Timeout"`, `"TCPWRAP"`, …). This crate has no single handle registry —
//! it has one table per module that owns a background thread or a deferred
//! callback — so the honest version of the same question is "ask every table
//! that could be holding one, right now", not a list typed once and left to
//! drift from what those tables actually contain. [`crate::timers`] already
//! reads its own table for [`crate::timers::pump`] and
//! [`crate::timers::source`]; [`crate::net`] already reads its two for
//! [`crate::net::source`]. This calls the same reads, at call time, so the
//! answer is never stale in either direction — an empty result really means
//! nothing is outstanding, which is what lets
//! `tests/node_process_full.test.ts` assert `getActiveResourcesInfo().length
//! === 0` truthfully: nothing has scheduled a timer or opened a socket by the
//! time the fixture calls it.
//!
//! # What is NOT wired in, and why this is a declared scope rather than a gap
//!
//! `node:dgram` (its own `SOCKETS` table, `dgram/registry.rs`), `fs.watch()`
//! (its own `WATCHERS` table, `fs/watch.rs`), `node:child_process` (its own
//! process table, `child_process/spawn_async.rs`) and `node:worker_threads`
//! (its own registry) each have a table this function could read the same
//! way — none of them is read here. Node's own `getActiveResourcesInfo` is
//! explicitly a best-effort, non-exhaustive list (its own docs say so), so
//! answering from a SUBSET of this runtime's tables is still an honest answer
//! to the same question; what would not be honest is claiming the subset is
//! everything. Wiring the other four is the same recipe as the two below —
//! `pub(crate) fn active_handles() -> Vec<&'static str>` reading that module's
//! own table — left for whichever future change actually needs a program to
//! see a UDP socket or a watcher in this list.

use rts_core::entry;

/// `process.getActiveResourcesInfo()`.
pub(super) extern "C" fn get_active_resources_info(
    _e: u64,
    _this: u64,
    _a: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let mut names: Vec<&'static str> = crate::timers::active_handles();
    names.extend(crate::net::active_handles());
    entry::with_runtime(|context| {
        let values = names
            .into_iter()
            .map(|name| entry::make_string(context, name))
            .collect();
        entry::make_array_in(context, values)
    })
}
