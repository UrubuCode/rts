//! What a program does when the heap runs out.
//!
//! # Why this needs a subprocess
//!
//! Because the answer is `exit(1)`, and a test that observed it in-process would
//! take the whole test binary with it. So the test re-executes itself with a
//! marker in the environment, and the child is the program that runs out.
//!
//! That is more machinery than a test usually earns, and it is earned here: what
//! is being pinned is that the failure is **loud**. The previous behaviour was
//! silent, and silence is exactly what a test cannot observe by accident — the
//! program answered `undefined` from the allocation, computed `NaN`, and
//! returned successfully. Every assertion anyone would think to write passed.
//!
//! # How it was found
//!
//! Not by reading. A benchmark in this repository allocated forty thousand
//! objects in a region that holds sixty-five thousand cells, ran out, and
//! reported a beautiful two-threads-for-free number — for a program whose answer
//! was the canonical `NaN` and whose timing was of the failure path. It looked
//! exactly like a measurement.

use std::process::Command;

/// The environment variable that tells a child which fixture to be.
const ROLE: &str = "RTS_EXHAUSTION_ROLE";

/// A program that keeps more objects alive than the region can ever hold.
///
/// # Why they are KEPT, and what that replaced
///
/// This fixture used to allocate a quarter of a million short-lived objects and
/// hand each to a function, on the premise that a region of 65 536 cells could
/// not survive them. That premise is gone: the region grows when a collection
/// does not make room, so a program whose garbage is collectable now finishes
/// — which is the whole point of the change, and it made this fixture return a
/// perfectly correct sum instead of running out.
///
/// What is still true, and what this pins, is that the growth has a **ceiling**
/// — the reservation, which cannot be exceeded because the base of it is an
/// immediate in the compiled code — and that reaching it is loud. So the
/// objects go into an array and stay reachable: the collector cannot free one,
/// growth runs out, and the program ends saying so.
///
/// The array also keeps the allocation real against scalar replacement, which
/// is what the function call was doing before: an object nothing outside the
/// loop can observe is removed entirely, and the fixture would measure
/// arithmetic.
const TOO_MANY: &str = "let keep = []; \
     for (let i = 0; i < 5000000; i = i + 1) { keep.push({a: i}); } \
     return keep.length;";

/// How many cells the region reserves, which is where growth stops.
///
/// Computed rather than written, so that a change to either half moves this
/// test with it instead of leaving it asserting a number nothing produces.
fn reserved_cells() -> u32 {
    (1u32 << 16) * rts_core::heap::GROWTH_CEILING
}

#[test]
fn a_full_heap_ends_the_program_and_says_so() {
    // The child half. Reached only in the re-executed process, which runs the
    // fixture and is expected never to return from it.
    if std::env::var(ROLE).is_ok() {
        let mut program = rts_host::compile(TOO_MANY).expect("compiles");
        let produced = program.run();
        // Reached only if the allocation answered a value instead of ending the
        // program — the behaviour this test exists to forbid. Printed rather
        // than asserted, so the parent's message can say what came back.
        println!("returned {produced:#x} instead of ending");
        return;
    }

    let child = Command::new(std::env::current_exe().expect("this test binary"))
        .arg("a_full_heap_ends_the_program_and_says_so")
        .arg("--exact")
        .arg("--nocapture")
        .env(ROLE, "child")
        .output()
        .expect("re-executing this test binary");

    let out = String::from_utf8_lossy(&child.stdout);
    let err = String::from_utf8_lossy(&child.stderr);

    assert!(
        !out.contains("instead of ending"),
        "the allocation answered a value on a full heap: {out}"
    );
    assert!(
        err.contains("heap exhausted"),
        "a full heap ended the program without saying why.\nstderr: {err}\nstdout: {out}"
    );
    // The number of cells, so that the diagnostic says what ran out rather than
    // that something did — and the RESERVATION rather than the starting bound,
    // because the starting bound is not where a growing region stops.
    let reserved = reserved_cells().to_string();
    assert!(
        err.contains(&reserved),
        "the report did not say how large the region could have grown: {err}"
    );
}

#[test]
fn more_live_objects_than_the_region_starts_with_is_not_the_end_of_the_program() {
    // The limitation this file used to encode as normal: the region held 65 536
    // cells and a program with more LIVE objects than that died, however well
    // the collector worked — nothing here was reclaimable, so a collection
    // could not help. It grows instead, and the answer is the count rather than
    // a report on stderr.
    //
    // A hundred thousand rather than a round million: it is comfortably past
    // the starting bound, which is what is being pinned, and a test that also
    // measured the growth of a large heap would take seconds to say the same
    // thing.
    let mut program = rts_host::compile(
        "let keep = []; \
         for (let i = 0; i < 100000; i = i + 1) { keep.push({a: i}); } \
         return keep.length;",
    )
    .expect("compiles");
    let produced = program.run();
    assert_eq!(rts_cranelift::tags::decode_double(produced), 100000.0);
}

#[test]
fn a_program_that_fits_is_unaffected() {
    // The other half of the pair, and the one that would catch a fix applied too
    // eagerly: an allocation that succeeds must still answer a value.
    let mut program = rts_host::compile(
        "let t = 0; \
         for (let i = 0; i < 1000; i = i + 1) { let o = {a: i}; t = t + o.a; } \
         return t;",
    )
    .expect("compiles");
    let produced = program.run();
    let expected: f64 = (0..1000).map(f64::from).sum();
    assert_eq!(rts_cranelift::tags::decode_double(produced), expected);
}
