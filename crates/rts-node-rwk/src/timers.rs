//! `node:timers` — `setTimeout`/`setInterval`/`setImmediate` and their
//! `clear*` counterparts, over a native queue this crate owns itself.
//!
//! # WHEN a scheduled callback runs — read this before relying on a timer
//!
//! **This engine has no event loop.** Nothing pumps a queue after the
//! program's last statement, and nothing here spawns a background thread to
//! call into JS later — a native thread calling a JS function aborts unless
//! it is the one thread already holding the runtime borrow
//! (`rts_core_rwk::entry::with_runtime`'s own doc), which rules out a
//! `std::thread::sleep`-then-call design outright.
//!
//! `crates/rts-node-rwk/src/fs/watch.rs`'s module doc faced the identical
//! wall for a file watcher and chose: queue the event as plain native data on
//! whichever thread produced it, and DELIVER it on the JS thread, synchronously,
//! at the start of the next call this module's own natives make. This module
//! makes the **same** choice, for the same reason — it is the "queue and
//! deliver" branch of the two named in this crate's own task list, not
//! "refuse entirely": **every extern in this file calls [`pump`] first**, so
//! a due callback runs synchronously, just before the NEXT `setTimeout`/
//! `setInterval`/`setImmediate`/`clearTimeout`/`clearInterval`/
//! `clearImmediate` call the program issues — in practice, whichever of those
//! six the program happens to call next, on whichever thread issues it.
//!
//! **A timer scheduled with nothing after it DOES fire**, and that used to be
//! the paragraph saying it never could. The host calls [`drain`] at the end of
//! the turn, which pumps and then SLEEPS to the nearest deadline and pumps
//! again — the waiting an event loop does, narrowed to what a host without one
//! can honestly provide. A `setTimeout(cb, 0)` is clamped to `1`ms exactly as
//! Node clamps it, so a single pump could never have found it due; that, and
//! not "nothing pumps", was the whole of the defect.
//!
//! An INTERVAL does not hold a program open, and that is a divergence from Node
//! stated rather than discovered: `drain` waits only on non-periodic timers,
//! because the alternative is every fixture with a stray interval hanging.
//!
//! # Reuse-check
//!
//! `rts-cranelift`'s `src/sched/` (`SchedulerId`, `Delivery`,
//! `ContinuationId`) is the nearest thing the machine layer has to "run order"
//! — read, and it does not answer this: it is about promises/continuations
//! the compiler itself lowers `await` onto, not an externally-triggered
//! callback queue a `node:` module owns. `rts-core-rwk::entry::promise`
//! (`drain_microtasks`/`settled`) is a queue for an ALREADY-CREATED promise,
//! not a place to register a brand-new deferred callback from a host module —
//! nothing there is reused because nothing there does what a `Timeout` needs.
//! `fs/watch.rs`'s `WATCHERS`/`pump` shape IS reused, deliberately: same
//! problem (host-native queue, JS-thread-only delivery), same shape.
//!
//! # `Timeout`/`Immediate` — a number, not an object
//!
//! Real Node returns a `Timeout`/`Immediate` instance with `.ref()`/
//! `.unref()`/`.refresh()`/`[Symbol.dispose]`/`[Symbol.toPrimitive]`. None of
//! those are implemented — with no event loop, "keep the process alive"
//! and "refresh the deadline" have nothing to act on that isn't already
//! either the no-op or the deferred-forever case above — so `setTimeout`/
//! `setInterval`/`setImmediate` return the numeric id directly, which is
//! already the shape `clearTimeout`/`clearInterval`/`clearImmediate` need and
//! is exactly what real Node's own `Timeout[Symbol.toPrimitive]()` coerces
//! down to for cross-thread use; a program calling `clearTimeout(id)` with
//! the returned value works unchanged.
//!
//! # Not implemented, by name
//!
//! `timers/promises` (`setTimeout`/`setImmediate`/`setInterval`/
//! `scheduler.wait`/`scheduler.yield`) — every one of them needs to construct
//! a fresh `Promise` from Rust, and this crate's entry surface has no
//! `Promise` constructor (the same gap `events.rs`'s module doc names for
//! `events.on`/`events.once`). `.ref()`/`.unref()`/`.hasRef()`/`.refresh()`/
//! `[Symbol.dispose]`/`[Symbol.toPrimitive]` — no `Timeout`/`Immediate`
//! object exists to hang them on (see above). Trailing `...args` forwarding
//! beyond one value — this module's four-slot calling convention leaves one
//! argument slot once the callback and delay are read; `setTimeout(cb, 10, a,
//! b, c)` forwards only `a`. Delay clamping/`NaN` handling beyond a floor of
//! `1` — an out-of-range or non-numeric delay reads as `1`ms rather than
//! being separately validated against the documented `2147483647` ceiling.

use rts_core_rwk::entry;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// One pending timer. `period` is what distinguishes the three JS-visible
/// kinds: `Some` is a `setInterval`, `None` with a future deadline is a
/// `setTimeout`, `None` with a due-now deadline is a `setImmediate` — no
/// separate tag is kept because nothing here ever needs to ask "which kind is
/// this" independent of those two fields.
struct Timer {
    callback: u64,
    arg: u64,
    deadline: Instant,
    period: Option<Duration>,
}

thread_local! {
    /// This thread's timers.
    ///
    /// # Why per thread and not one table
    ///
    /// It WAS one table behind a `Mutex`, and a timer holds two things — a
    /// callback and an argument — that are cells in the region of the thread
    /// that scheduled them. So a shared table lets one thread's [`pump`] fire
    /// another thread's callback, with the wrong context installed and handles
    /// that name cells in a region this thread does not have.
    ///
    /// That is not hypothetical: two `#[test]`s scheduling timers run on two
    /// threads of one process, and each was firing the other's. It became
    /// visible only when [`drain`] made the loop long enough for the two to
    /// overlap; before that each pumped once and usually missed. A worker thread
    /// is the same shape with no test harness to notice.
    ///
    /// The context is thread-local for exactly this reason, and anything holding
    /// values has to follow it.
    static TIMERS: RefCell<HashMap<u64, Timer>> = RefCell::new(HashMap::new());
}

/// Ids stay process-wide, which is deliberate: a handle a program prints or
/// compares should not repeat across threads, and nothing indexes by it except
/// the table that issued it.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_timers<T>(body: impl FnOnce(&mut HashMap<u64, Timer>) -> T) -> T {
    TIMERS.with(|table| body(&mut table.borrow_mut()))
}

/// Delivers every currently-DUE timer, oldest-registered-id first.
///
/// Public because the host calls it too, and because [`drain`] is built on it:
/// this fires what is already due and never waits, which is why one call could
/// not run a `setTimeout(f, 0)` and `drain` can.
///
/// Releases [`TIMERS`]'s borrow before calling anything. A callback that
/// schedules or clears a timer is ordinary and expected, and it would otherwise
/// panic on a borrow this function still holds — which in an `extern "C"` frame
/// is an abort.
pub fn pump() {
    let now = Instant::now();
    let due: Vec<(u64, u64, u64)> = with_timers(|table| {
        let mut ready: Vec<u64> = table
            .iter()
            .filter(|(_, timer)| timer.deadline <= now)
            .map(|(&id, _)| id)
            .collect();
        ready.sort_unstable();
        ready
            .into_iter()
            .filter_map(|id| {
                let timer = table.get_mut(&id)?;
                let fire = (id, timer.callback, timer.arg);
                match timer.period {
                    Some(period) => timer.deadline = now + period,
                    None => {
                        table.remove(&id);
                    }
                }
                Some(fire)
            })
            .collect()
    });
    let absent = entry::undefined_value();
    for (_id, callback, arg) in due {
        entry::call(callback, absent, arg, absent, absent, absent);
    }
}

/// `delay` clamped to a floor of `1`ms — see the module doc for what is not
/// separately validated.
fn clamp_delay(delay: u64, _a1: u64) -> u64 {
    let millis = entry::number_of(delay).map(|value| value as i64).unwrap_or(1);
    millis.max(1) as u64
}

const PRESENT: fn(u64) -> bool = |value| value != entry::undefined_value();

/// The namespace `node:timers` is.
pub fn namespace(context: &mut entry::Context) -> u64 {
    let members: &[(&str, entry::Provided)] = &[
        ("setTimeout", set_timeout),
        ("clearTimeout", clear_timeout),
        ("setInterval", set_interval),
        ("clearInterval", clear_interval),
        ("setImmediate", set_immediate),
        ("clearImmediate", clear_immediate),
    ];
    entry::make_namespace(context, members)
}

/// `setTimeout(callback, delay?, arg?)`.
extern "C" fn set_timeout(_e: u64, _this: u64, callback: u64, delay: u64, arg: u64, _a3: u64) -> u64 {
    pump();
    schedule(callback, arg, clamp_delay(delay, 0), None)
}

/// `clearTimeout(id)`.
extern "C" fn clear_timeout(_e: u64, _this: u64, id: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    pump();
    cancel(id);
    entry::undefined_value()
}

/// `setInterval(callback, delay?, arg?)`.
extern "C" fn set_interval(_e: u64, _this: u64, callback: u64, delay: u64, arg: u64, _a3: u64) -> u64 {
    pump();
    let period = Duration::from_millis(clamp_delay(delay, 0));
    schedule(callback, arg, period.as_millis() as u64, Some(period))
}

/// `clearInterval(id)`.
extern "C" fn clear_interval(_e: u64, _this: u64, id: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    pump();
    cancel(id);
    entry::undefined_value()
}

/// `setImmediate(callback, arg?)` — due at the next [`pump`], not after any
/// delay.
extern "C" fn set_immediate(_e: u64, _this: u64, callback: u64, arg: u64, _a2: u64, _a3: u64) -> u64 {
    pump();
    if !PRESENT(callback) {
        return entry::undefined_value();
    }
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    with_timers(|table| {
        table.insert(id, Timer { callback, arg, deadline: Instant::now(), period: None });
    });
    entry::make_number(id as f64)
}

/// `clearImmediate(id)`.
extern "C" fn clear_immediate(_e: u64, _this: u64, id: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    pump();
    cancel(id);
    entry::undefined_value()
}

fn schedule(callback: u64, arg: u64, delay_ms: u64, period: Option<Duration>) -> u64 {
    if !PRESENT(callback) {
        return entry::undefined_value();
    }
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_millis(delay_ms);
    with_timers(|table| {
        table.insert(id, Timer { callback, arg, deadline, period });
    });
    entry::make_number(id as f64)
}

/// Removes a timer by its numeric id — a no-op for an unknown/foreign/
/// already-cleared id, matching real Node's silent tolerance.
fn cancel(id: u64) {
    let Some(number) = entry::number_of(id) else {
        return;
    };
    let key = number as u64;
    with_timers(|table| {
        table.remove(&key);
    });
}

/// Runs every pending TIMEOUT to its deadline, sleeping between passes.
///
/// # The defect this removes, and why it was never mysterious once measured
///
/// `setTimeout(f, 0)` alone did not fire, and it was pinned as a defect with the
/// note that "nothing pumps" was not the cause — which was right and stopped one
/// step early. A delay of `0` is clamped to `1`ms, exactly as Node clamps it, and
/// the host's single end-of-turn [`pump`] runs microseconds after the schedule.
/// The timer is not due yet. Pumping once can only ever fire a timer whose
/// deadline has already passed.
///
/// What was missing is the waiting an event loop does. This is that, narrowed to
/// what a host without one can honestly provide: pump, sleep, repeat, until no
/// non-periodic timer is left.
///
/// # Why an interval does NOT hold a program open
///
/// It would never finish. In Node a live `setInterval` does keep a process
/// alive, and the answers to that — `unref()`, `clearInterval` — assume an event
/// loop and a program written to end itself. A suite whose every fixture hangs
/// on a stray interval is worse than the divergence, so an interval fires
/// whenever this passes and stops the program from ending never. Stated here
/// rather than discovered.
pub fn drain() {
    loop {
        pump();
        let waiting = with_timers(|table| {
            table
                .values()
                .filter(|timer| timer.period.is_none())
                .map(|timer| timer.deadline)
                .min()
        });
        let Some(deadline) = waiting else {
            return;
        };
        // Sleeping until the nearest deadline rather than spinning: the whole
        // point of the loop is that time has to pass, and a busy wait makes it
        // pass on a core the callback might want.
        let now = Instant::now();
        if deadline > now {
            std::thread::sleep(deadline - now);
        }
    }
}
