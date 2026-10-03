//! The six functions `node:timers` exports, and the namespace that holds them.
//!
//! Split from [`super`] rather than appended to it: that file was at 492 lines
//! before the handle objects arrived and the change pushed it past this
//! workspace's 500-line ceiling. The seam chosen is the one the module already
//! had — [`super`] owns the QUEUE (the table, the pump, the loop source) and
//! this owns the JS SURFACE over it — because the two are read for different
//! questions: "when does a callback run" against "what does a program call".
//!
//! Everything binding is in [`super`]'s module doc, which is where a reader of
//! either half starts; nothing is restated here.

use rts_core::entry;
use std::time::{Duration, Instant};

use super::{Deliver, clamp_delay, forget, handle, register, registered, source};

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
    // The two handle classes, registered here because this function is the one
    // thing that runs exactly once per context — see `handle::declare`.
    handle::declare(context);
    entry::declare_loop_source(context, "node:timers", source);
    entry::make_namespace(context, members)
}

/// `setTimeout(callback, delay?, arg?)`.
///
/// # Why none of these pumps first
///
/// Every extern here used to call [`super::pump`] before doing anything, which was
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

/// `setImmediate(callback, arg?)` — due at the next [`super::pump`], not after any
/// delay.
extern "C" fn set_immediate(_e: u64, _this: u64, callback: u64, arg: u64, _a2: u64, _a3: u64) -> u64 {
    if !PRESENT(callback) {
        return entry::undefined_value();
    }
    let id = registered(Deliver::Call { callback, arg }, Instant::now(), None, true);
    entry::with_runtime(|context| handle::immediate(context, id, callback, arg))
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
    entry::with_runtime(|context| {
        handle::timeout(context, id, callback, arg, delay_ms, period.is_some())
    })
}

/// Removes a timer by the value a program passed `clearTimeout`.
///
/// # Why both forms, and why the object form is not merely the number's
///
/// Node accepts a `Timeout` AND the primitive it coerces to, and a program that
/// moved a handle across a boundary as a number is the reason the coercion
/// exists. So a number is cancelled directly and an object is asked for its id —
/// NOT coerced, because `entry::number_of` of an object answers `None` and
/// running `ToPrimitive` here would call user code from a native that then reads
/// the answer, which is `rts-core`'s README rule 8 territory for no gain:
/// `handle::id_of` reads an own property and calls nothing.
///
/// A handle also gets MARKED destroyed, which a raw number cannot do — that is
/// what makes `clearTimeout(t); t.refresh()` refuse while a refresh after the
/// callback ran succeeds. Clearing by the number leaves a handle that still
/// believes it may refresh; Node has the same seam, since the number carries no
/// way back to the object.
///
/// A no-op for an unknown/foreign/already-cleared value, matching real Node's
/// silent tolerance.
///
/// # The third form, absent and named
///
/// `docs/reference/node/timers.md` types the parameter `Timeout | string |
/// number`, and the STRING is not accepted here — `clearTimeout("2")` does
/// nothing. Taking it means `ToNumber` on the argument, which for an object
/// argument is `ToPrimitive`, which calls user code; this function's whole
/// protection against that is reading an own property instead. Narrowing to "a
/// text cell, parsed" needs a `Value`-level text test a host crate cannot reach,
/// so the form lands with the first program that needs it rather than with a
/// coercion that could run a `valueOf` on a handle.
fn cancel(id: u64) {
    if let Some(number) = entry::number_of(id) {
        forget(number as u64);
        return;
    }
    if entry::with_runtime(|context| handle::id_of(context, id)).is_some() {
        handle::destroy(id);
    }
}
