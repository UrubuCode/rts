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

    // The first fixture is the floor: reaching any of them costs a call through
    // a function pointer, and that cost is in every number. What a primitive
    // costs is its distance from the floor, so both are printed and neither is
    // left for a reader to work out.
    let mut floor = None;

    for fixture in &fixtures {
        let measurement = measure(fixture, iterations);
        let per_op = measurement.nanos_per_op();
        let floor = *floor.get_or_insert(per_op);

        println!(
            "{:width$}  {:>8.2} ns/op  {:>+8.2} over the floor   {}",
            measurement.name,
            per_op,
            per_op - floor,
            fixture.about,
        );

        // Printed so that nothing here can be optimized into an empty loop
        // without the number changing in a way a reader notices.
        if measurement.checksum == i64::MIN {
            println!("  (checksum {})", measurement.checksum);
        }
    }
}
