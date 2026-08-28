//! Runs the machine probe and prints the table.
//!
//! ```text
//! cargo run --release -p rts-cranelift --example probe_run
//! ```
//!
//! # Why this exists
//!
//! `src/probe/` has been in this crate since the crate existed and its own
//! module doc says the numbers are a CONTRACT: a regression in them is a
//! regression in the machine layer, and a slow program whose probe numbers are
//! unchanged has its problem somewhere else.
//!
//! Nothing ran them. A contract nobody can read binds nothing.
//!
//! # Why an example and not a test
//!
//! A test that fails because a machine was busy fails on someone else's laptop,
//! in shared CI, on a Tuesday morning. The probe is a ruler for whoever is
//! measuring, not a gate. `.claude/skills/perf-claim/SKILL.md` names this
//! command for that reason — it named `cargo test -p rts-cranelift --test probe`
//! until 2026-08-28, and that target has never existed.
//!
//! # How to read it
//!
//! Against `loop_floor`, never in absolute terms. Every fixture runs its
//! primitive inside a counted loop — that is what gives the instrument the
//! resolution to tell these rows apart at all, see `probe/fixtures.rs` — so
//! every row carries one compare, one branch and one increment that belong to
//! the instrument. The floor is that and nothing else, and what a primitive
//! costs is the distance from it.

fn main() {
    let profile = rts_cranelift::probe::Profile::current();
    println!("machine probe — profile: {profile:?}");
    if !profile.is_meaningful() {
        println!();
        println!("  WARNING: this is a DEBUG build and the numbers are worth nothing.");
        println!("  Run with --release. (`CLAUDE.md`: a debug number is not a number.)");
        println!();
    }
    println!();
    println!(
        "  {:<14} {:>9}  {:>11}   {}",
        "fixture", "ns/op", "above floor", "what it is"
    );
    println!("  {:-<14} {:->9}  {:->11}   {:-<46}", "", "", "", "");

    // Enough inner iterations that the one call into the fixture disappears
    // into them: at this count it contributes under a femtosecond per
    // operation, which is the whole reason the loop is inside the compiled
    // program rather than around it.
    const TRIPS: u64 = 200_000_000;

    let mut floor = 0.0_f64;
    for fixture in rts_cranelift::probe::all() {
        let measurement = rts_cranelift::probe::measure(&fixture, TRIPS);
        let ns = measurement.nanos_per_op();
        if fixture.name == "loop_floor" {
            floor = ns;
        }
        // The DISTANCE from the floor is the number. Reporting the totals alone
        // would let a reader conclude that every primitive costs half a
        // nanosecond, which is a statement about a counted loop and not about
        // this layer at all.
        let above = match fixture.name == "loop_floor" {
            true => "-".to_owned(),
            false => format!("{:+.2} ns", ns - floor),
        };
        println!(
            "  {:<14} {:>6.2} ns  {:>11}   {}",
            fixture.name, ns, above, fixture.about
        );
        // Printed so that an optimizer which deleted the work shows up as a
        // number too good to be true rather than as a fast one.
        debug_assert!(measurement.checksum != i64::MIN, "the fixture computed nothing");
    }

    println!();
    println!("  A row at the floor is a primitive this machine emits for free —");
    println!("  it fits in the slack of a loop the processor is already running.");
    println!("  `call_direct` is the one to watch: it is the cheapest call this");
    println!("  layer can emit, so it is the floor under every call above it, and");
    println!("  the distance between it and what a built-in costs belongs to");
    println!("  somebody else. See docs/codegen/native-call-floor.md.");
}
