//! `node:timers` — `setTimeout`/`setInterval`/`setImmediate` and their
//! `clear*` counterparts, over a native queue this crate owns itself.
//!
//! # WHEN a scheduled callback runs — read this before relying on a timer
//!
//! **This engine has no event loop.** Nothing pumps a queue after the
//! program's last statement, and nothing here spawns a background thread to
//! call into JS later — a native thread calling a JS function aborts unless
//! it is the one thread already holding the runtime borrow
//! (`rts_core::entry::with_runtime`'s own doc), which rules out a
//! `std::thread::sleep`-then-call design outright.
//!
//! `crates/rts-node/src/fs/watch.rs`'s module doc faced the identical
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
//! callback queue a `node:` module owns. `rts-core::entry::promise`
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
//! # `timers/promises` is [`promises`], and the paragraph refusing it was stale
//!
//! It said every member of that module "needs to construct a fresh `Promise`
//! from Rust, and this crate's entry surface has no `Promise` constructor".
//! `entry::promise_new` and `entry::promise_settle` are exported — they are the
//! runtime half of what the machine lowers `await` onto — so the capability was
//! there and the refusal was a note nobody re-read. It is written down rather
//! than quietly deleted, because a "cannot" that outlives its cause is how a
//! module stays unwritten for reasons that stopped being true.
//!
//! What makes the two halves ONE queue rather than two: a promise-shaped timer
//! is a [`Deliver::Settle`] in this same table, so [`pump`] and [`source`] fire
//! it, and the host's waiting — the thing that makes `await sleep(10)` finish —
//! costs nothing extra to reach.
//!
//! # Not implemented, by name
//!
//! `.ref()`/`.unref()`/`.hasRef()`/`.refresh()`/
//! `[Symbol.dispose]`/`[Symbol.toPrimitive]` — no `Timeout`/`Immediate`
//! object exists to hang them on (see above). Trailing `...args` forwarding
//! beyond one value — this module's four-slot calling convention leaves one
//! argument slot once the callback and delay are read; `setTimeout(cb, 10, a,
//! b, c)` forwards only `a`. Delay clamping/`NaN` handling beyond a floor of
//! `1` — an out-of-range or non-numeric delay reads as `1`ms rather than
//! being separately validated against the documented `2147483647` ceiling.

use rts_core::entry;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub mod promises;

/// What a due timer DOES, which is the one thing the callback and the promise
/// forms do not share.
///
/// A second table for [`promises`] was the alternative, and it is the shape this
/// crate's own rules call a duplicate: it would need its own deadline ordering,
/// its own `pump`, and its own [`source`] registration, so a program mixing
/// `setTimeout(cb, 5)` with `await sleep(5)` would have two queues disagreeing
/// about which fires first. One table with two deliveries has one order.
enum Deliver {
    /// A JavaScript function, called with one argument.
    Call { callback: u64, arg: u64 },
    /// A promise, fulfilled with a value when the deadline arrives — or
    /// REJECTED before it, if `signal` is one a program aborted.
    ///
    /// `signal` is `Option` rather than the `undefined` value because the two
    /// are asked at different moments: whether a signal was passed is decided
    /// once, at scheduling time, and comparing against `undefined_value()` on
    /// every pump would take the runtime borrow to answer a question Rust
    /// already knows.
    Settle {
        promise: u64,
        value: u64,
        signal: Option<u64>,
    },
}

/// One pending timer. `period` is what distinguishes the three JS-visible
/// kinds: `Some` is a `setInterval`, `None` with a future deadline is a
/// `setTimeout`, `None` with a due-now deadline is a `setImmediate` — no
/// separate tag is kept because nothing here ever needs to ask "which kind is
/// this" independent of those two fields.
struct Timer {
    deliver: Deliver,
    deadline: Instant,
    period: Option<Duration>,
    /// Whether this came from `setImmediate`.
    ///
    /// An immediate is a PHASE and not a deadline: Node drains every one of
    /// them before it looks at a timer, whatever order they were registered in.
    /// Sorting by id alone made `setTimeout(f, 0); setImmediate(g);` run `f`
    /// first, because `f` was registered first — which is the right rule for
    /// two timers and the wrong one across the two phases.
    immediate: bool,
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
    reject_aborted();
    let now = Instant::now();
    let due: Vec<Deliver> = with_timers(|table| {
        // Every immediate before any timer, and within each phase by
        // registration — which is what the id orders. See [`Timer::immediate`].
        let mut ready: Vec<(bool, u64)> = table
            .iter()
            .filter(|(_, timer)| timer.deadline <= now)
            .map(|(&id, timer)| (!timer.immediate, id))
            .collect();
        ready.sort_unstable();
        let ready: Vec<u64> = ready.into_iter().map(|(_, id)| id).collect();
        ready
            .into_iter()
            .filter_map(|id| {
                let timer = table.get_mut(&id)?;
                let fire = match timer.deliver {
                    Deliver::Call { callback, arg } => Deliver::Call { callback, arg },
                    Deliver::Settle {
                        promise,
                        value,
                        signal,
                    } => Deliver::Settle {
                        promise,
                        value,
                        signal,
                    },
                };
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
    for fire in due {
        match fire {
            Deliver::Call { callback, arg } => {
                entry::call(callback, absent, arg, absent, absent, absent);
            }
            // Fulfilment, not rejection: the `0` is the `rejected` flag, an
            // `i64` because that is what the lowering passes and what
            // `promise_settle` documents. An abort has already been dealt with
            // by `reject_aborted` above, so a timer that reaches its deadline
            // here is one nothing cancelled.
            Deliver::Settle { promise, value, .. } => entry::promise_settle(promise, value, 0),
        }
    }
}

/// Rejects every promise-shaped timer whose `AbortSignal` has fired.
///
/// # Why this is polled and not delivered
///
/// Because an abort has no way to reach this module. `AbortSignal` lives in
/// `rts-std` and records the abort as two ordinary properties — `aborted`
/// and `reason` — with no registry a host crate could subscribe to. The
/// alternative was to add one, which is a mechanism in another crate for one
/// caller; polling reads the property this module can already reach.
///
/// **What the divergence costs, stated:** Node rejects at the moment
/// `controller.abort()` returns. This rejects on the next turn of the loop —
/// the next call into any timer native, or the next `pump_sources` an `await`
/// drives. For a program that aborts and then awaits, the two are
/// indistinguishable; for one that aborts and inspects the promise in the same
/// synchronous run of statements, they are not.
///
/// The price is one property read per outstanding promise-timer per pump, and
/// only for the ones that were given a signal.
fn reject_aborted() {
    let watched: Vec<(u64, u64, u64)> = with_timers(|table| {
        table
            .iter()
            .filter_map(|(&id, timer)| match timer.deliver {
                Deliver::Settle {
                    promise,
                    signal: Some(signal),
                    ..
                } => Some((id, promise, signal)),
                _ => None,
            })
            .collect()
    });
    if watched.is_empty() {
        return;
    }
    // Read every signal inside ONE borrow, settle outside it: `promise_settle`
    // takes the borrow itself, so doing both at once aborts the process.
    let aborted: Vec<(u64, u64, u64)> = entry::with_runtime(|context| {
        let mut fired = Vec::new();
        for (id, promise, signal) in watched {
            if entry::get_member(context, signal, "aborted") == entry::boolean_value(true) {
                fired.push((id, promise, entry::get_member(context, signal, "reason")));
            }
        }
        fired
    });
    for (id, promise, reason) in aborted {
        forget(id);
        entry::promise_settle(promise, reason, 1);
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
    entry::declare_loop_source(context, "node:timers", source);
    entry::make_namespace(context, members)
}

/// `setTimeout(callback, delay?, arg?)`.
///
/// # Why none of these pumps first
///
/// Every extern here used to call [`pump`] before doing anything, which was
/// right when there was no event loop: the only chance a due callback had to
/// run was when the program next touched a timer.
///
/// There is a loop now — `rts-host`'s `run` drains microtasks and then pumps —
/// so pumping here runs a due callback SYNCHRONOUSLY, in the middle of whatever
/// statement happened to mention a timer. Measured: `setImmediate(f);
/// queueMicrotask(g);` alone orders correctly, and adding a `setTimeout` after
/// them inverts it — the `setTimeout` call fires the already-due immediate
/// before the synchronous code that follows.
///
/// [`source`] keeps its own, and that one is not the same thing: it is the loop
/// ASKING what is due, which is the question pumping answers.
extern "C" fn set_timeout(_e: u64, _this: u64, callback: u64, delay: u64, arg: u64, _a3: u64) -> u64 {
    schedule(callback, arg, clamp_delay(delay, 0), None)
}

/// `clearTimeout(id)`.
extern "C" fn clear_timeout(_e: u64, _this: u64, id: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    cancel(id);
    entry::undefined_value()
}

/// `setInterval(callback, delay?, arg?)`.
extern "C" fn set_interval(_e: u64, _this: u64, callback: u64, delay: u64, arg: u64, _a3: u64) -> u64 {
    let period = Duration::from_millis(clamp_delay(delay, 0));
    schedule(callback, arg, period.as_millis() as u64, Some(period))
}

/// `clearInterval(id)`.
extern "C" fn clear_interval(_e: u64, _this: u64, id: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    cancel(id);
    entry::undefined_value()
}

/// `setImmediate(callback, arg?)` — due at the next [`pump`], not after any
/// delay.
extern "C" fn set_immediate(_e: u64, _this: u64, callback: u64, arg: u64, _a2: u64, _a3: u64) -> u64 {
    if !PRESENT(callback) {
        return entry::undefined_value();
    }
    entry::make_number(
        registered(Deliver::Call { callback, arg }, Instant::now(), None, true) as f64,
    )
}

/// `clearImmediate(id)`.
extern "C" fn clear_immediate(_e: u64, _this: u64, id: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    cancel(id);
    entry::undefined_value()
}

fn schedule(callback: u64, arg: u64, delay_ms: u64, period: Option<Duration>) -> u64 {
    if !PRESENT(callback) {
        return entry::undefined_value();
    }
    let deadline = Instant::now() + Duration::from_millis(delay_ms);
    let id = register(Deliver::Call { callback, arg }, deadline, period);
    entry::make_number(id as f64)
}

/// Puts one timer in this thread's table and answers its RAW id.
///
/// Raw — a `u64` key, not the JS number `schedule` hands a program — because
/// [`promises`] never shows an id to a program: it holds one to cancel with, and
/// coercing it out to a number value and back again would be two conversions
/// around a value nothing else reads.
fn register(deliver: Deliver, deadline: Instant, period: Option<Duration>) -> u64 {
    registered(deliver, deadline, period, false)
}

/// The same, saying which phase it belongs to.
fn registered(
    deliver: Deliver,
    deadline: Instant,
    period: Option<Duration>,
    immediate: bool,
) -> u64 {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    with_timers(|table| {
        table.insert(id, Timer {
            deliver,
            deadline,
            period,
            immediate,
        });
    });
    id
}

/// Removes a timer by its raw id — the half of [`cancel`] that has a number
/// already, and what [`promises`] cancels an outstanding tick with.
fn forget(id: u64) {
    with_timers(|table| {
        table.remove(&id);
    });
}

/// Removes a timer by its numeric id — a no-op for an unknown/foreign/
/// already-cleared id, matching real Node's silent tolerance.
fn cancel(id: u64) {
    let Some(number) = entry::number_of(id) else {
        return;
    };
    forget(number as u64);
}


/// This module as a loop source: deliver what is due, then say when to come
/// back.
///
/// # What replaced a `drain` that slept here
///
/// This module briefly owned the waiting itself — pump, sleep to the nearest
/// deadline, repeat. It worked and it was in the wrong place: five other modules
/// have the same problem, the host named two of them by hand, and a sixth copy
/// of one loop is what `entry::loops` exists to stop.
///
/// So the sleeping moved out and this answers a duration instead. A
/// `setTimeout(f, 0)` is still clamped to `1`ms exactly as Node clamps it, and
/// the host waits that millisecond — which is the whole of why a single pump
/// could never fire it.
///
/// An INTERVAL answers `Blocked`, not `In`: it is pumped on every pass and does
/// not hold the program open. That is a stated divergence from Node, where a
/// live interval keeps a process alive — the answers to that (`unref`,
/// `clearInterval`) assume an event loop and a program written to end itself,
/// and a suite where one stray interval hangs every fixture is worse.
pub fn source() -> entry::Pending {
    pump();
    let now = Instant::now();
    let (soonest, periodic) = with_timers(|table| {
        let soonest = table
            .values()
            .filter(|timer| timer.period.is_none())
            .map(|timer| timer.deadline)
            .min();
        (soonest, table.values().any(|timer| timer.period.is_some()))
    });
    match (soonest, periodic) {
        (Some(deadline), _) => entry::Pending::In(deadline.saturating_duration_since(now)),
        (None, true) => entry::Pending::Blocked,
        (None, false) => entry::Pending::Idle,
    }
}
