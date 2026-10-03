//! What keeps a program running after its last statement.
//!
//! # The duplication this removes, counted
//!
//! Six modules in `rts-node` had grown their own version of one idea — a
//! background thread that cannot call into JavaScript, a table of native
//! records, and a `pump` that turns those into calls on the program's thread:
//! `fs/watch`, `net`, `dgram`, `child_process`, `timers` and `worker_threads`.
//! The recipe is right. Six copies of it are not, and the copies had already
//! diverged in the way that matters: the host named **two** of them by hand, so
//! the other four only ever delivered if the program happened to call into that
//! same module again.
//!
//! A module that starts a thread and is never called again therefore queued
//! events nothing read. That is not a limitation a reader could find — it looks
//! finished from inside each module.
//!
//! # What this is, and the one rule it keeps
//!
//! A source registers a `fn` pointer. The host asks every registered source to
//! deliver what is due and to say when it wants to be asked again, and it does
//! that on the program's own thread — which is the rule none of the six may
//! break, because a value belongs to the region of the thread that made it.
//!
//! # Why this crate and not `rts-cranelift`
//!
//! `rts-cranelift` owns the concurrency substrate — `src/sched/` holds promises,
//! continuations and the ORDER they run in, and that ordering is a property of
//! the machine. A deadline in milliseconds is not: it needs a wall clock and a
//! way to wait, neither of which every target has, and a source's callback is a
//! *value*, which the machine layer does not know exists.
//!
//! So the ordering stays there and the waiting stays out here — and out of this
//! module too. [`pump_sources`] never sleeps; it answers how long the host
//! should. A sleep would put `std::thread::sleep` in the crate whose membership
//! rule is "every target, including wasm".
//!
//! # What holds a program open, and what does not
//!
//! Only a source that answers [`Pending::In`]. A source that answers
//! [`Pending::Blocked`] is still pumped on every pass but does not keep the loop
//! alive — a listening server would otherwise run forever, which is what Node
//! does and what a test suite cannot.
//!
//! Stated here rather than discovered: an interval and a listening socket both
//! end a program that has nothing else to do.

use std::time::Duration;

use super::Context;

/// What a source has left to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pending {
    /// Nothing outstanding.
    Idle,
    /// Something outstanding with a known deadline: ask again within this long.
    ///
    /// This is what keeps a program running past its last statement.
    In(Duration),
    /// Something outstanding with no deadline — waiting on the outside world.
    ///
    /// Pumped on every pass and does NOT hold the program open; see the module
    /// doc for why that is the choice rather than an omission.
    Blocked,
}

/// Delivers what is due for one source, and says when it wants to be asked
/// again.
///
/// A `fn` pointer rather than a closure: a source's state is a table it owns,
/// and a closure would put that state here — where it would be one table for a
/// process, which is the bug the thread-local timer table already was.
pub type Source = fn() -> Pending;

/// Registers a source with this thread's context.
///
/// By the module, at install time, and never by the host — the host naming them
/// is exactly what left four of the six unpumped.
pub fn declare_loop_source(context: &mut Context, name: &'static str, source: Source) {
    // Idempotent by name: `install` runs once per context, but a module that
    // builds its namespace twice (`fs` and `fs/promises` did) would otherwise
    // register twice and pump twice per pass.
    if context.loop_sources.iter().any(|(held, _)| *held == name) {
        return;
    }
    context.loop_sources.push((name, source));
}

/// The longest a caller may wait while some source answered [`Pending::Blocked`].
///
/// `Blocked` is documented as "pumped on every pass", and that sentence was
/// false: the wait was the minimum over the `In` answers alone, so a `Blocked`
/// source whose only company was a `setTimeout(f, 15_000)` was pumped twice —
/// once before the sleep and once after, by which time the timer had already run
/// and called `process.exit`. A socket connected in the first millisecond and
/// its `'connect'` was never delivered.
///
/// A cap rather than a wake-up: nothing here can be notified by a background
/// thread, because waiting at all is the host's (see [`Rest`]) and this crate
/// must exist on targets with no threads to be notified from.
const BLOCKED_CAP: Duration = Duration::from_millis(1);

/// Asks every source to deliver, and answers how long the caller may wait.
///
/// `None` when nothing is outstanding — the program can finish. `Some(d)` when
/// something is, and `d` is the shortest any source asked for, capped at
/// [`BLOCKED_CAP`] when any source is `Blocked`.
///
/// # Why the borrow is released before a source runs
///
/// A source delivers by CALLING a listener, and that listener is user code that
/// will call back into the runtime. Holding the context across it is the nested
/// borrow this crate aborts on, so the list is copied out first — it is a
/// handful of `fn` pointers, and copying them is cheaper than the bug.
pub fn pump_sources() -> Option<Duration> {
    match one_pass() {
        Some(wait) => Some(wait),
        // A pass that answers "nothing outstanding" may have CREATED some during
        // itself, for a source it had ALREADY asked — and whether it did is
        // decided by the order the sources happen to be registered in, which no
        // module chooses and none can see. Measured: `node:http` reports a refused
        // connection by scheduling `setTimeout(fn, 0)` from inside `node:net`'s
        // delivery of the socket's `'error'`; `node:timers` is asked before
        // `node:net`, so that timer was never seen, this answered `None`, the host
        // ended the program, and `req.on('error', …)` ran for nothing. Node 22
        // reports `ECONNREFUSED` on the same program.
        //
        // One extra pass and not a loop: what it buys is independence from
        // registration order, because after it every source has been asked AFTER
        // every delivery of the first pass. Work deferred through a second layer
        // of sources needs a deadline of its own to be correct anyway, and
        // spinning here until two passes agree would hide one module queueing work
        // the next one re-queues forever. Costs nothing in the steady state: the
        // second pass only happens on the pass that was about to end the program.
        None => one_pass(),
    }
}

/// One round of asking every source, and the wait its answers imply.
fn one_pass() -> Option<Duration> {
    let sources: Vec<Source> = super::current::with_current(|context| {
        context
            .loop_sources
            .iter()
            .map(|(_, source)| *source)
            .collect()
    });
    let mut soonest: Option<Duration> = None;
    let mut blocked = false;
    for source in sources {
        match source() {
            Pending::Idle => {}
            Pending::Blocked => blocked = true,
            Pending::In(wait) => {
                soonest = Some(match soonest {
                    Some(held) => held.min(wait),
                    None => wait,
                });
            }
        }
    }
    wait_for(soonest, blocked)
}

/// The two answers combined into the one duration a caller waits.
///
/// Split out from [`pump_sources`] so the rule can be asserted without a context
/// and a background thread — the defect it fixes is invisible in any answer a
/// test can read, and shows up only as an event delivered too late.
///
/// A `Blocked` source still does not hold the program open: `None` when nothing
/// answered `In` is what ends a program whose last act was to start a listener.
/// What the cap does is bound the wait of a program that IS open, so that
/// "pumped on every pass" means passes soon enough to matter.
fn wait_for(soonest: Option<Duration>, blocked: bool) -> Option<Duration> {
    match (soonest, blocked) {
        (Some(wait), true) => Some(wait.min(BLOCKED_CAP)),
        (soonest, _) => soonest,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blocked_source_is_not_starved_by_a_distant_deadline() {
        // The measured defect: a socket's source answered `Blocked` while a
        // `setTimeout(f, 15_000)` was the only other work, so the host slept
        // fifteen seconds between pumps and the timer ran first on waking.
        let waited = wait_for(Some(Duration::from_secs(15)), true);
        assert_eq!(waited, Some(BLOCKED_CAP));
    }

    #[test]
    fn a_nearer_deadline_than_the_cap_is_kept() {
        let soon = Duration::from_micros(200);
        assert_eq!(wait_for(Some(soon), true), Some(soon));
    }

    #[test]
    fn blocked_alone_still_ends_the_program() {
        // The divergence `node:net`'s server and `node:stream` rely on: a source
        // with no deadline does not keep a program running by itself.
        assert_eq!(wait_for(None, true), None);
    }

    #[test]
    fn an_unblocked_deadline_is_untouched() {
        let far = Duration::from_secs(15);
        assert_eq!(wait_for(Some(far), false), Some(far));
    }
}

/// How a host makes time pass.
///
/// # Why this is handed down rather than done here
///
/// `std::thread::sleep` is not on every target, and this crate's membership rule
/// is availability — the same rule that keeps `pump_sources` sleepless and puts
/// the waiting in `rts-host`'s loop. But `await` needs to wait from INSIDE a
/// call, where there is no host loop to return to, so the capability has to come
/// down the way the evaluator does.
///
/// `None` until a host installs one, and a caller that finds none must say so
/// rather than spin: a promise only time can settle, waited on by a runtime that
/// cannot let time pass, is a deadlock and reporting it beats burning a core.
pub type Rest = fn(Duration);

/// Installs the host's waiter.
pub fn declare_rest(context: &mut Context, rest: Rest) {
    context.rest = Some(rest);
}

/// Waits, and says whether anything could.
pub fn rest_for(wait: Duration) -> bool {
    let Some(rest) = super::current::with_current(|context| context.rest) else {
        return false;
    };
    // OUTSIDE the borrow: resting is the host's, it takes real time, and holding
    // the runtime across it would stop every other entry point for its duration.
    rest(wait);
    true
}
