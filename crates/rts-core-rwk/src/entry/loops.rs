//! What keeps a program running after its last statement.
//!
//! # The duplication this removes, counted
//!
//! Six modules in `rts-node-rwk` had grown their own version of one idea — a
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

/// Asks every source to deliver, and answers how long the caller may wait.
///
/// `None` when nothing is outstanding — the program can finish. `Some(d)` when
/// something is, and `d` is the shortest any source asked for.
///
/// # Why the borrow is released before a source runs
///
/// A source delivers by CALLING a listener, and that listener is user code that
/// will call back into the runtime. Holding the context across it is the nested
/// borrow this crate aborts on, so the list is copied out first — it is a
/// handful of `fn` pointers, and copying them is cheaper than the bug.
pub fn pump_sources() -> Option<Duration> {
    let sources: Vec<Source> = super::current::with_current(|context| {
        context
            .loop_sources
            .iter()
            .map(|(_, source)| *source)
            .collect()
    });
    let mut soonest: Option<Duration> = None;
    for source in sources {
        match source() {
            Pending::Idle | Pending::Blocked => {}
            Pending::In(wait) => {
                soonest = Some(match soonest {
                    Some(held) => held.min(wait),
                    None => wait,
                });
            }
        }
    }
    soonest
}

/// How a host makes time pass.
///
/// # Why this is handed down rather than done here
///
/// `std::thread::sleep` is not on every target, and this crate's membership rule
/// is availability — the same rule that keeps `pump_sources` sleepless and puts
/// the waiting in `rts-host-rwk`'s loop. But `await` needs to wait from INSIDE a
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
