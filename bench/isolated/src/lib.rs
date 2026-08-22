//! The timing harness every isolated experiment shares.
//!
//! # Why a harness rather than `Instant::now()` at each site
//!
//! Two reasons, and both were paid for elsewhere in this repository before they
//! were written down here.
//!
//! The clock is the first. `Instant` on Windows is `QueryPerformanceCounter`,
//! whose tick is ~100 ns. An operation that costs 2 ns measured over 100
//! iterations is measuring the clock, and the number it produces looks like a
//! measurement. So the iteration count is **calibrated**: a case is grown until
//! it takes at least [`TARGET_MS`], the same rule `bench/analytic.ts` uses, so
//! that a 0.5 ns case and a 5 us case can appear in one table without either
//! being a lie.
//!
//! The optimiser is the second. A micro-benchmark whose result is discarded is
//! a micro-benchmark whose body was deleted, and `opt-level = 3` will do that.
//! Every case here returns a `u64` derived from its work and the harness
//! accumulates it into a sink it prints, so a removed body shows up as a number
//! too good to be true rather than as a fast one — again the rule
//! `bench/analytic.ts` states.
//!
//! # What a number from here does and does not mean
//!
//! It means: *this shape of Rust costs that much, compiled this way, on this
//! machine, with its operands in cache.* It does **not** mean the engine will
//! move by the same amount — the engine calls this shape from compiled code
//! across an `extern "C"` boundary, with different register pressure and a cold
//! cache. An experiment here is a **gate**, not a forecast: a change that does
//! not win in isolation will not win in the engine, so it is refused here and
//! the engine is never touched. A change that does win here still has to be
//! measured in the engine afterwards, per file, against a kept baseline binary.

use std::time::Instant;

/// How long a case must run before its number is believed.
///
/// Forty milliseconds against a ~100 ns clock tick is a relative resolution of
/// about 2.5 parts per million, which is far below the run-to-run noise this
/// machine actually has. The bound that matters is the other one: a case grown
/// past this takes seconds, and an experiment nobody runs answers nothing.
pub const TARGET_MS: f64 = 40.0;

/// Where every case's result goes, so that none of them can be deleted.
static mut SINK: u64 = 0;

/// Adds to the sink. Not atomic and not synchronised: an experiment is one
/// thread by construction, and making this atomic would put a locked
/// instruction inside the thing being measured.
///
/// # Safety
///
/// Called only from the single thread running the experiment.
fn sink(value: u64) {
    unsafe {
        SINK = SINK.wrapping_add(value);
    }
}

/// What the sink holds, for the line an experiment prints at the end.
pub fn checksum() -> u64 {
    unsafe { SINK }
}

/// One row of an experiment's table.
pub struct Row {
    /// What was measured.
    pub name: &'static str,
    /// Nanoseconds per iteration.
    pub nanos: f64,
    /// How many iterations produced that number.
    pub iterations: u64,
}

/// Times `body` per iteration, growing the count until the run is long enough
/// to believe, then taking the best of three.
///
/// The **best** and not the mean, because the distribution is one-sided: a
/// scheduler preemption, an interrupt or a migration can only make a run
/// slower, never faster, so the minimum is the closest thing to the cost of the
/// code and the mean is a measurement of the machine's other work.
pub fn measure(name: &'static str, mut body: impl FnMut(u64) -> u64) -> Row {
    let mut n: u64 = 1024;
    let mut ms = run_once(&mut body, n);
    while ms < TARGET_MS && n < (1 << 28) {
        n *= 4;
        ms = run_once(&mut body, n);
    }
    // One warm-up at the final count: the growth loop's last run was at this
    // count too, but the branch predictor and the caches are in a different
    // state after the growth check than after a settled run.
    let _ = run_once(&mut body, n);
    let mut best = f64::MAX;
    for _ in 0..3 {
        let ms = run_once(&mut body, n);
        if ms < best {
            best = ms;
        }
    }
    Row {
        name,
        nanos: (best * 1e6) / n as f64,
        iterations: n,
    }
}

fn run_once(body: &mut impl FnMut(u64) -> u64, n: u64) -> f64 {
    let start = Instant::now();
    let out = body(n);
    let elapsed = start.elapsed();
    sink(out);
    elapsed.as_secs_f64() * 1e3
}

/// Prints a table, with every row expressed against the first one.
///
/// Against the **first** rather than against the fastest, because the first row
/// of every experiment here is the shape the engine has today. What the reader
/// needs is "how much would changing it buy", and that is a ratio to the
/// current design, not to the winner.
pub fn report(title: &str, rows: &[Row]) {
    println!();
    println!("{title}");
    println!("{}", "=".repeat(title.len()));
    println!(
        "{:<44} {:>10} {:>10} {:>14}",
        "shape", "ns/op", "vs first", "iterations"
    );
    println!("{}", "-".repeat(82));
    let base = rows.first().map(|r| r.nanos).unwrap_or(1.0);
    for row in rows {
        let ratio = if base > 0.0 { row.nanos / base } else { 0.0 };
        println!(
            "{:<44} {:>10.3} {:>9.2}x {:>14}",
            row.name, row.nanos, ratio, row.iterations
        );
    }
    println!("{}", "-".repeat(82));
    println!("checksum {}", checksum());
}

/// Stops the optimiser from seeing through a value.
///
/// `std::hint::black_box` is the right tool and this forwards to it. It is
/// wrapped rather than used directly so that an experiment reads as one vocabulary,
/// and so that the one place it is called is the one place to change if a future
/// toolchain needs something stronger.
#[inline(always)]
pub fn opaque<T>(value: T) -> T {
    std::hint::black_box(value)
}
