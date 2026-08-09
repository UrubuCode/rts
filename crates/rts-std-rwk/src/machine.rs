//! `rts`'s `time` and `gc` — the clock, and what the heap has handed out.
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
//! It blocks the thread. There is no other honest reading of a SYNCHRONOUS
//! sleep: a program that writes `time.sleep_ms(50)` outside an `async` function
//! is asking for the next statement not to run for 50 ms, and nothing else can
//! deliver that. What it therefore does NOT do is let timers fire — a
//! `setTimeout` due during the sleep runs after it, not inside it.
//!
//! `await new Promise(r => setTimeout(r, n))` is the form that keeps the loop
//! turning, and it is what a program should reach for. This exists because 14
//! files in the suite call it and because a blocking sleep is a real thing to
//! want; it is not the better spelling.
//!
//! # `gc.live_count` counts cells, and `gc.collect` collects nothing
//!
//! There is no collector. `live_count` answers how many cells the region has
//! handed out, which is what it can say truthfully — that number only ever goes
//! up, and a program watching it fall is watching for something this engine does
//! not do yet. `collect` answers that count rather than pretending to have freed
//! anything: a function that reported a number of reclaimed objects would be
//! inventing one.

use rts_core_rwk::entry::{self, Context, Provided};

/// The two namespaces.
pub fn install(context: &mut Context, surface: u64) {
    let clock = entry::make_namespace(context, TIME);
    entry::put_member(context, surface, "time", clock);
    let heap = entry::make_namespace(context, GC);
    entry::put_member(context, surface, "gc", heap);
}

/// `time` — the wall clock, in milliseconds.
const TIME: &[(&str, Provided)] = &[
    ("now_ms", now_ms),
    ("sleep_ms", sleep_ms),
];

/// `gc` — what the heap has handed out.
const GC: &[(&str, Provided)] = &[
    ("live_count", live_count),
    ("collect", collect),
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

/// `time.sleep_ms(n)` — blocks for `n` milliseconds.
///
/// See the module note: this stops the thread, so nothing else runs during it.
extern "C" fn sleep_ms(_e: u64, _this: u64, a: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let millis = entry::number_of(a).unwrap_or(0.0);
    // A negative or absent argument sleeps for nothing rather than saturating
    // into a very long wait, which is what `as u64` on a negative double would
    // produce and the worst possible reading of a typo.
    if millis > 0.0 {
        std::thread::sleep(std::time::Duration::from_millis(millis as u64));
    }
    entry::undefined_value()
}

/// `gc.live_count()` — cells handed out by the region.
extern "C" fn live_count(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::with_runtime(|context| entry::make_number(f64::from(context.region.used())))
}

/// `gc.collect()` — the same count, because nothing is collected.
extern "C" fn collect(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    live_count(0, 0, 0, 0, 0, 0)
}
