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

/// A program that asks for far more objects than a region holds.
///
/// A region is 65 536 cells and nothing reclaims them, so a loop of a quarter of
/// a million allocations cannot finish however the engine behaves. What differs
/// is whether it says so.
const TOO_MANY: &str = "let t = 0; \
     for (let i = 0; i < 250000; i = i + 1) { let o = {a: i}; t = t + o.a; } \
     return t;";

#[test]
fn a_full_heap_ends_the_program_and_says_so() {
    // The child half. Reached only in the re-executed process, which runs the
    // fixture and is expected never to return from it.
    if std::env::var(ROLE).is_ok() {
        let mut program = rts_host_rwk::compile(TOO_MANY).expect("compiles");
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
    // that something did.
    assert!(
        err.contains("65536"),
        "the report did not say how large the region was: {err}"
    );
}

#[test]
fn a_program_that_fits_is_unaffected() {
    // The other half of the pair, and the one that would catch a fix applied too
    // eagerly: an allocation that succeeds must still answer a value.
    let mut program = rts_host_rwk::compile(
        "let t = 0; \
         for (let i = 0; i < 1000; i = i + 1) { let o = {a: i}; t = t + o.a; } \
         return t;",
    )
    .expect("compiles");
    let produced = program.run();
    let expected: f64 = (0..1000).map(f64::from).sum();
    assert_eq!(rts_cranelift::tags::decode_double(produced), expected);
}
