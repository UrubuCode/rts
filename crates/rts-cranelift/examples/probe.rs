//! Runs the probe and prints what each primitive costs.
//!
//! An example rather than a test, on purpose: a benchmark that runs in the
//! ordinary test loop either slows the loop down or is too short to mean
//! anything, and both make people stop reading it.
//!
//! ```text
//! cargo run --release --example probe -p rts-cranelift
//! ```
//!
//! Without `--release` it still runs, and says so. A debug number is not a
//! number, and the output repeats that rather than trusting anyone to remember.

use rts_cranelift::probe::{Profile, all, measure};

fn main() {
    let iterations: u64 = std::env::args()
        .nth(1)
        .and_then(|argument| argument.parse().ok())
        .unwrap_or(2_000_000);

    let profile = Profile::current();
    println!("rts-cranelift probe — {iterations} iterations, {profile:?} build");
    if !profile.is_meaningful() {
        println!("  !! these numbers are from an unoptimized build and are not numbers");
        println!("  !! rerun with: cargo run --release --example probe -p rts-cranelift");
    }
    println!();

    let fixtures = all();
    let width = fixtures
        .iter()
        .map(|fixture| fixture.name.len())
        .max()
        .unwrap_or(0);

    // # Why every fixture is measured more than once, and the MINIMUM is kept
    //
    // This printed one sample per fixture and took the FIRST fixture's number
    // as the floor, once, before anything else ran. Measured 2026-08-20 at 50
    // million iterations, three runs: the three primitives came out stable to
    // within 0.05 ns of each other — and the floor read 3.08, 3.07 and then
    // 4.13, because the third run sampled it while the processor was still
    // ramping. Every delta inherited that one bad sample, and the report said
    // a field read costs MINUS 0.70 ns over the floor.
    //
    // A negative cost is not a small error. It is the instrument saying its
    // own subtrahend is untrustworthy, and no number it prints can be believed
    // until that is fixed. More iterations does not fix it: what is wrong is
    // WHEN the floor is sampled, not for how long.
    //
    // So: a discarded warm-up pass over every fixture, then `ROUNDS` passes
    // kept, and the minimum per fixture. The minimum is the right statistic
    // here rather than the mean — every perturbation this measurement can
    // suffer (frequency ramp, a scheduler steal, a cache eviction from
    // something else on the machine) makes a run SLOWER and none makes it
    // faster, so the fastest observed run is the closest to the primitive and
    // the spread is noise by construction.
    //
    // The floor is re-measured on every round alongside the others rather than
    // pinned from the first, which is what makes drift across a round cancel
    // instead of accumulate into the deltas.
    const ROUNDS: usize = 3;

    // Discarded, and that is the point: this is the pass the old report was
    // accidentally publishing.
    for fixture in &fixtures {
        std::hint::black_box(measure(fixture, iterations / 10));
    }

    let mut best: Vec<f64> = vec![f64::MAX; fixtures.len()];
    let mut worst: Vec<f64> = vec![0.0; fixtures.len()];
    let mut checksums: Vec<i64> = vec![0; fixtures.len()];
    for _ in 0..ROUNDS {
        for (index, fixture) in fixtures.iter().enumerate() {
            let measurement = measure(fixture, iterations);
            let per_op = measurement.nanos_per_op();
            best[index] = best[index].min(per_op);
            worst[index] = worst[index].max(per_op);
            checksums[index] = measurement.checksum;
        }
    }

    // The first fixture is the floor: reaching any of them costs a call through
    // a function pointer, and that cost is in every number. What a primitive
    // costs is its distance from the floor, so both are printed and neither is
    // left for a reader to work out.
    let floor = best[0];

    // What this run can and cannot tell apart, printed rather than left for a
    // reader to infer. The floor's own spread across the kept rounds is the
    // resolution: a delta smaller than the amount the SAME fixture moved
    // between rounds is not a measurement of anything, and saying so is the
    // difference between an instrument and a number generator.
    let resolution = (worst[0] - best[0]) * 1000.0;

    for (index, fixture) in fixtures.iter().enumerate() {
        let per_op = best[index];
        let picos = (per_op - floor) * 1000.0;
        let verdict = match index {
            0 => String::new(),
            _ if picos.abs() < resolution => "  (under this run's resolution)".to_string(),
            _ => String::new(),
        };
        println!(
            "{:width$}  {:>8.2} ns/op  {:>+8.0} ps   {}{}",
            fixture.name,
            per_op,
            picos,
            fixture.about,
            verdict,
        );

        // Printed so that nothing here can be optimized into an empty loop
        // without the number changing in a way a reader notices.
        if checksums[index] == i64::MIN {
            println!("  (checksum {})", checksums[index]);
        }
    }

    println!();
    println!(
        "  best of {ROUNDS} rounds after a discarded warm-up. A primitive's cost is its \
         DISTANCE from the floor,"
    );
    println!(
        "  and the floor — a call through a function pointer at ~{floor:.2} ns — dominates \
         every row above."
    );
    println!(
        "  This run resolves about {resolution:.0} ps: the floor itself moved that much between \
         rounds, so a"
    );
    println!(
        "  smaller delta is noise. Raise the iteration count (argv[1]) to resolve finer."
    );
}
