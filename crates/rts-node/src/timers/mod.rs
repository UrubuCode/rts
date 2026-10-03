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
//! # `Timeout`/`Immediate` — an OBJECT, and the paragraph saying a number was
//! right about its reason and wrong about its conclusion
//!
//! It said: Node returns a `Timeout`/`Immediate` with `.ref()`/`.unref()`/
//! `.refresh()`/`[Symbol.dispose]`/`[Symbol.toPrimitive]`, and none of those are
//! implemented, because "with no event loop, *keep the process alive* and
//! *refresh the deadline* have nothing to act on". That was true when written.
//! Both halves have since stopped being: there IS a loop — `entry::loops`, which
//! this module registers as a [`source`] — and the deadline is a field in
//! [`TIMERS`]. So `unref()` means exactly *stop answering
//! [`entry::Pending::In`]*, which is the one thing that holds a program open,
//! and `refresh()` means *write the deadline again*.
//!
//! The handle is therefore a [`handle::timeout`]/[`handle::immediate`] object,
//! and the number is still reachable: `Timeout[Symbol.toPrimitive]` answers the
//! same id, and [`cancel`] takes either — so a program calling
//! `clearTimeout(id)` with a number it kept from before works unchanged, which
//! is what kept the old shape defensible and is why nothing had to be broken to
//! leave it.
//!
//! What forced the change is a program rather than a tidiness argument:
//! `@whiskeysockets/baileys` dies with `(intermediate value).unref is not a
//! function`, and `.unref()` is the most common thing published JavaScript
//! writes about a timer handle. [`handle`] holds the design, the measured Node
//! contract and the divergences that remain.
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
//! `.ref()`/`.unref()`/`.hasRef()`/`.refresh()`/`.close()`/`[Symbol.dispose]`/
//! `[Symbol.toPrimitive]` were all listed here, for the reason the section above
//! records; all seven exist now and [`handle`] is where. What is still absent on
//! that side is named in that module rather than here, so the list is in one
//! place: `Timeout.prototype[Symbol.toPrimitive].name`'s spelling, a
//! `t.constructor` that cannot build a scheduled timer, and Node's
//! underscore-prefixed internals.
//!
//! `setInterval` still does not hold the program open, and `unref()` does not
//! change that in either direction — see [`source`]. Trailing `...args` forwarding
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

mod handle;
pub mod promises;
mod surface;

pub use surface::namespace;

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
    /// Whether this timer may hold the program open — `ref()`/`unref()`.
    ///
    /// Read by [`source`] and nowhere else, which is the whole of what `unref`
    /// means here: an unrefed timer is still pumped, still fires while anything
    /// else keeps the loop turning, and contributes no [`entry::Pending::In`].
    ///
    /// The FLAG a program reads back with `hasRef()` is not this field, and that
    /// is not duplication — see [`handle`]'s module doc. Node answers `hasRef()`
    /// after the timer has fired, when this entry no longer exists.
    refed: bool,
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
        // A microtask CHECKPOINT between two timers, and it is what makes the
        // boundary observable at all: one pump finds EVERY timer whose deadline
        // has passed, so two `setTimeout(f, 0)` arrive together and the second
        // used to run before anything the first queued. That answered
        // `timer1, timer2, t1-micro` where every runtime answers
        // `timer1, t1-micro, timer2` — the queue is drained completely between
        // macrotasks, and a macrotask is one callback rather than one pump.
        //
        // Here rather than in the host loop, which is where it would have to be
        // to stay wrong: the loop can only see a pump boundary, and the boundary
        // the language defines is the callback's.
        entry::drain_microtasks();
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
///
/// Here rather than in [`surface`]: `promises` normalises its own delay through
/// it too, so it is the QUEUE's rule about what a deadline may be and not one
/// surface's reading of an argument.
pub(super) fn clamp_delay(delay: u64, _a1: u64) -> u64 {
    let millis = entry::number_of(delay).map(|value| value as i64).unwrap_or(1);
    millis.max(1) as u64
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
            refed: true,
        });
    });
    id
}

/// Writes a timer's ref flag, if it is still scheduled.
///
/// Silent about an id the table does not hold, because the common case for that
/// is a handle whose timer already fired — and [`handle`] keeps the flag a
/// program reads back, so there is nothing lost by this answering nothing.
fn set_refed(id: u64, refed: bool) {
    with_timers(|table| {
        if let Some(timer) = table.get_mut(&id) {
            timer.refed = refed;
        }
    });
}

/// Schedules `id` again from now — `Timeout.prototype.refresh()`.
///
/// Under the id it already had, replacing whatever entry is there: Node keeps
/// `Number(t)` stable across a refresh, so a program that kept the primitive to
/// cancel with still names this timer afterwards. Minting a fresh id would leave
/// that number pointing at nothing.
fn rearm(id: u64, callback: u64, arg: u64, delay_ms: u64, periodic: bool, refed: bool) {
    let period = Duration::from_millis(delay_ms);
    with_timers(|table| {
        table.insert(id, Timer {
            deliver: Deliver::Call { callback, arg },
            deadline: Instant::now() + period,
            period: periodic.then_some(period),
            immediate: false,
            refed,
        });
    });
}

/// Removes a timer by its raw id — the half of [`surface`]'s `cancel` that has a number
/// already, and what [`promises`] cancels an outstanding tick with.
fn forget(id: u64) {
    with_timers(|table| {
        table.remove(&id);
    });
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
/// live interval keeps a process alive — a suite where one stray interval hangs
/// every fixture is worse, and the divergence is the conservative direction.
///
/// An UNREFED timer answers the same way, and that is not a divergence but the
/// definition: `Timer::refed` is read here and nowhere else, so `unref()` removes
/// a timer from the `In` set while leaving it pumped. A program whose only
/// outstanding work is an unrefed timer therefore ENDS, which is what Node does
/// and what `scripts`-level measurement of this change had to show before the
/// method could ship.
pub fn source() -> entry::Pending {
    pump();
    let now = Instant::now();
    let (soonest, periodic) = with_timers(|table| {
        let soonest = table
            .values()
            .filter(|timer| timer.period.is_none() && timer.refed)
            .map(|timer| timer.deadline)
            .min();
        // `Blocked` and not `Idle` for an unrefed timer, which is the difference
        // between "does not hold the program open" and "will never fire": the
        // first is what `unref` means, and `Blocked` is pumped on every pass
        // while contributing no deadline — exactly an interval's answer, for
        // exactly the same reason.
        let waiting = table
            .values()
            .any(|timer| timer.period.is_some() || !timer.refed);
        (soonest, waiting)
    });
    match (soonest, periodic) {
        (Some(deadline), _) => entry::Pending::In(deadline.saturating_duration_since(now)),
        (None, true) => entry::Pending::Blocked,
        (None, false) => entry::Pending::Idle,
    }
}

/// Every timer/immediate this thread's table currently holds, as the Node
/// handle-class name a program would see in `getActiveResourcesInfo()`.
///
/// Real state, read at call time — not a fixed list: `TIMERS` is exactly what
/// [`pump`] and [`source`] already read, so this answers whatever they would
/// find due or not-yet-due right now. [`Timer::immediate`] is the one field
/// that decides which of the two names applies, because it is the one field
/// that already distinguishes a `setImmediate` from a `setTimeout`/
/// `setInterval` for every other purpose in this module (see its own doc).
pub(crate) fn active_handles() -> Vec<&'static str> {
    with_timers(|table| {
        table
            .values()
            .map(|timer| if timer.immediate { "Immediate" } else { "Timeout" })
            .collect()
    })
}
