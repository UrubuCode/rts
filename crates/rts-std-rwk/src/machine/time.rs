//! `rts`'s `time` — the clock.
//!
//! # Why `time` is here and not in the runtime
//!
//! Availability, the same rule that decides everything else in this crate: a
//! clock is the operating system's, and `rts-core-rwk` exists on targets that
//! have none. `Date.now()` reaches the same clock through a different door, and
//! that is not a duplicate answer — it is one implementation this calls.
//!
//! # What `sleep_ms` costs, said rather than hidden
//!
//! It blocks the thread — there is no other honest reading of a SYNCHRONOUS
//! sleep — but it does not block the LOOP: a program calling it outside `await`
//! has no other way to let a `setTimeout` due during the pause fire, and the
//! suite's own fixtures (`set_timeout_interval.test.ts`) are written against
//! that contract. So the wait is sliced rather than handed to one
//! `std::thread::sleep`, and between slices `entry::pump_sources` delivers
//! whatever timer source is due and `entry::drain_microtasks` runs whatever
//! that queued — the same two steps `rts-host-rwk`'s own loop performs between
//! statements. What still does NOT happen is a callback running *inside* this
//! native's own call frame from a nested borrow — each slice pumps between
//! iterations, on this native's own stack, with no borrow held across it, same
//! as the host loop.
//!
//! `await new Promise(r => setTimeout(r, n))` is the async-native form and nothing
//! here replaces it; this exists because 14 files in the suite call the
//! synchronous one and a blocking-but-loop-turning wait is a real thing to want.
//!
//! # Why there is no `gc` here
//!
//! It was written and removed the same day. `live_count` and `collect` can each
//! be answered truthfully — cells handed out, and that same count, since nothing
//! is collected — but the owner's decision is that this engine does not offer a
//! `gc` surface at all: the old one had it, the programs that used it allocated
//! in order to watch it, and a namespace kept alive for that is a surface to
//! keep in agreement for no reader. The tests that called it were removed with
//! it, so nothing here is a structure without a producer.

use rts_core_rwk::entry::{self, Context, Provided};

/// The clock.
pub fn install(context: &mut Context, surface: u64) {
    let clock = entry::make_namespace(context, TIME);
    entry::put_member(context, surface, "time", clock);
}

/// `time` — the wall clock, in milliseconds.
const TIME: &[(&str, Provided)] = &[
    ("now_ms", now_ms),
    ("sleep_ms", sleep_ms),
];

/// `time.now_ms()` — milliseconds since the epoch, as a whole number.
///
/// The same clock `Date.now()` reads, and whole rather than fractional: this is
/// the spelling a program reaches for to measure something, and a fractional
/// millisecond is a difference nothing here can measure anyway.
extern "C" fn now_ms(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as f64)
        // Before the epoch means a clock somebody set backwards. Zero rather
        // than a negative time, which nothing that reads this would handle.
        .unwrap_or(0.0);
    entry::make_number(since)
}

/// `time.sleep_ms(n)` — blocks for `n` milliseconds, pumping the loop as it
/// goes.
///
/// See the module note: the thread stops between slices, but a timer due
/// during the wait still fires and its reactions still run, on this frame.
extern "C" fn sleep_ms(_e: u64, _this: u64, a: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let millis = entry::number_of(a).unwrap_or(0.0);
    // A negative or absent argument sleeps for nothing rather than saturating
    // into a very long wait, which is what `as u64` on a negative double would
    // produce and the worst possible reading of a typo.
    if millis > 0.0 {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(millis as u64);
        // A short slice: fine-grained enough that a 10ms setTimeout still fires
        // inside a 50ms sleep instead of only at its very end, and coarse enough
        // not to spend the wait on scheduling overhead.
        const SLICE: std::time::Duration = std::time::Duration::from_millis(5);
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            std::thread::sleep(remaining.min(SLICE));
            // Deliver whatever timer is now due, then run whatever that queued —
            // the same pair of steps between two statements of a program, so a
            // callback firing here sees the same world one firing at top level
            // would.
            entry::drain_microtasks();
            entry::pump_sources();
            entry::drain_microtasks();
        }
    } else {
        // Even a zero/negative sleep still yields the loop one turn: a program
        // that calls `sleep_ms(0)` as a "let everything pending run" idiom
        // (several fixtures do) should see that, not nothing.
        entry::drain_microtasks();
        entry::pump_sources();
        entry::drain_microtasks();
    }
    entry::undefined_value()
}

