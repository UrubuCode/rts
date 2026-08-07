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
//! **A program that schedules a timer and calls no further timer function
//! never observes it fire.** There is no timer thread, no event loop, and no
//! other point this engine hands control back to in a way this crate can
//! hook — unlike `watch.rs`, which gets a second call "for free" because a
//! program that starts a watcher overwhelmingly also keeps calling `fs`, a
//! `setTimeout` is very often the LAST thing a small program does, so this is
//! a real, common-case limitation, not an edge case. It is the honest
//! answer required over a `setTimeout` that silently never fires with no way
//! to find out why: a program that calls `setTimeout(cb, 0)` immediately
//! followed by `setTimeout(cb2, 0)` (or any second timer call) observes `cb`
//! fire; a program that calls `setTimeout(cb, 0)` and then returns does not.
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
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
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

static TIMERS: Mutex<Option<HashMap<u64, Timer>>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn with_timers<T>(body: impl FnOnce(&mut HashMap<u64, Timer>) -> T) -> T {
    let mut guard = TIMERS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    body(guard.get_or_insert_with(HashMap::new))
}

/// Delivers every currently-due timer, oldest-registered-id first, THEN
/// drops the lock — and it is PUBLIC because the host calls it too.
///
/// Where the program's own timer calls pump it as they go, the host pumps once
/// more where it already drains microtasks: at the end of the turn, after the
/// last statement. That is what makes `setTimeout(f, 0)` with nothing after it
/// run `f` at all — the single most common way a timer is written, and the one
/// the program-driven pump alone never reaches, because a timer is often the
/// last thing a program does.
///
/// drops [`TIMERS`]'s lock before calling anything — a callback that itself
/// schedules or clears a timer (an ordinary, expected thing to do) must not
/// deadlock on a lock this function is still holding. See the module doc for
/// WHEN this runs.
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
