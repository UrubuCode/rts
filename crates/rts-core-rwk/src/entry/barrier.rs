//! Telling the collector a reference was written.
//!
//! # Why this exists before there is a collector
//!
//! Because the code that calls it exists. A cached store emits a barrier for
//! the reason the machine's lowering states, and the direction of the error is
//! chosen there rather than here:
//!
//! > A barrier that was not needed costs a call. A barrier that was needed and
//! > skipped produces a reference the collector never learns about, which
//! > becomes a use-after-free that reproduces rarely and explains nothing. The
//! > first is a bill; the second is a bug nobody can find.
//!
//! So compiled code calls this on every reference store. What it does today is
//! count, because there is nothing to tell — and counting rather than doing
//! nothing at all means the day a collector arrives, the call site does not have
//! to be found again.
//!
//! # Why it was implemented before `cached_set` and not after
//!
//! The host answered `RtEntry::WriteBarrier` with a null pointer while nothing
//! emitted one. A store that emitted a barrier before this existed would call
//! that null pointer, which is a process dying with no diagnostic — so the order
//! is not a preference.

use super::with_current;

/// Records that `written` was stored into `object`.
///
/// Both are values rather than references: the store that called this may have
/// written a number, and a barrier that only ran for references would need the
/// caller to have decided which it was — which is the decision the barrier
/// exists to not depend on.
#[rtse::entry("rts_write_barrier")]
pub fn write_barrier(object: u64, written: u64) {
    with_current(|context| {
        let _ = (object, written);
        context.barriers += 1;
    })
}
