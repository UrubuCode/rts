//! Compiled code running on more than one thread, each with a heap of its own.
//!
//! # What this is testing, and what it is not
//!
//! That N threads can run the same program at once, that each allocates in its
//! **own** region, and that two of them never hand out the same reference. That
//! is the whole of what the multi-region heap buys today.
//!
//! It is deliberately not testing that objects can be *shared* between threads,
//! because they cannot. `Region::Local` versus `Shared`, the write barrier's
//! runtime half and the collector are all still absent, so publishing a value to
//! another thread is not expressible and nothing would protect a program that
//! found a way. A test asserting otherwise would be asserting a property nothing
//! implements.
//!
//! # Why the region count is a compile-time argument
//!
//! The number of regions decides the width of the selector, and the selector is
//! **in every address computation** the program was compiled with. So it is
//! `compile_for(source, regions)` rather than something `run_on` could choose —
//! and asking for more threads than regions is a panic rather than a silent
//! wrap, because a wrapped selector would read another thread's heap.

use rts_cranelift::tags;
use rts_host::{compile, compile_for};

/// A program that allocates and reads back, so that a heap it did not own would
/// show up as a wrong answer rather than as a crash.
const ALLOCATES: &str = "let t = 0; \
     for (let i = 0; i < 200; i = i + 1) { let o = {a: i, b: i + 1}; t = t + o.a + o.b; } \
     return t;";

#[test]
fn one_program_runs_on_several_threads_at_once() {
    let mut program = compile_for(ALLOCATES, 4).expect("compiles for four regions");
    let answers = program.run_on(4);

    assert_eq!(answers.len(), 4, "one answer per thread");
    // Every thread ran the same program over its own heap, so every thread must
    // have computed the same number. A thread reading another's cells would
    // most likely still answer *something*, which is why the fixture sums what
    // it wrote rather than merely allocating.
    let expected: f64 = (0..200).map(|i| f64::from(i) + f64::from(i + 1)).sum();
    for (index, answer) in answers.iter().enumerate() {
        assert_eq!(
            tags::decode_double(*answer),
            expected,
            "thread {index} disagreed"
        );
    }
}

#[test]
fn a_single_thread_run_is_unchanged_by_the_region_count() {
    // The one-region path must not start paying for a table load, and must not
    // change what it answers. Compiled two ways, run one way, compared.
    let mut one = compile(ALLOCATES).expect("compiles");
    let mut many = compile_for(ALLOCATES, 4).expect("compiles for four regions");

    let single = one.run();
    let sharded = many.run_on(1);

    assert_eq!(sharded.len(), 1);
    assert_eq!(
        tags::decode_double(single),
        tags::decode_double(sharded[0]),
        "the sharded addressing changed what the program computes"
    );
}

#[test]
fn each_thread_allocates_in_its_own_region() {
    // The property the selector exists for: a reference carries which region it
    // came from in its low bits, so two threads cannot name one cell.
    //
    // The program answers its first object rather than a number, so the test can
    // look at the reference itself.
    let mut program = compile_for("let o = {a: 1}; return o;", 4)
        .expect("compiles for four regions");
    let answers = program.run_on(4);

    let mut seen: Vec<u64> = answers
        .iter()
        .map(|answer| tags::payload_of(*answer))
        .collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(
        seen.len(),
        4,
        "four threads allocated their first object and produced {} distinct \
         references — two of them named the same cell",
        seen.len()
    );

    // And the low bits are the region, so the four are exactly the four regions
    // rather than four cells that happened to differ.
    let mut regions: Vec<u64> = seen.iter().map(|reference| reference & 0b11).collect();
    regions.sort_unstable();
    assert_eq!(regions, vec![0, 1, 2, 3], "one object per region");
}

#[test]
fn asking_for_more_threads_than_regions_is_refused() {
    // A wrapped selector would read another thread's heap, so this is a panic
    // rather than a clamp: the width is in the code and a run cannot widen it.
    let mut program = compile_for("return 1;", 2).expect("compiles for two regions");
    let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        program.run_on(4);
    }));
    assert!(refused.is_err(), "four threads over two regions was allowed");
}

#[test]
fn a_thread_sees_the_string_literals_of_the_program() {
    // Each thread seeds its own context: the key registry and the literal table
    // are per-context, so a thread that skipped either would read a property as
    // absent or a literal as `undefined`. Cheaper to check than to discover.
    let mut program = compile_for(
        "let o = {}; o[\"alpha\"] = \"text\"; return o.alpha === \"text\";",
        2,
    )
    .expect("compiles for two regions");
    for answer in program.run_on(2) {
        assert_eq!(tags::tag_of(answer), tags::TAG_BOOL);
        assert_eq!(tags::payload_of(answer), tags::BOOL_TRUE);
    }
}
