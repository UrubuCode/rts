//! What an entry point costs before compiled code is anywhere near it.
//!
//! # The question this answers
//!
//! `bench/analytic.ts` (2026-08-11, release) puts `typeof x` at 250 ns and a
//! property read at 225 ns — two operations with nothing in common, at the same
//! price, which is what a fixed per-operation cost looks like. Three candidates
//! were named for it: reaching the thread's context, the throw check compiled
//! code performs after every operation, and the call itself.
//!
//! The second was measured from the other side and is ~9% of an operation
//! (`RTS_NO_THROW_CHECK`). This measures the first, with no compiled code in
//! the picture at all: whatever a Rust caller sees here is what remains after
//! the call and the check are both taken away.
//!
//! Run with `cargo run --release --example entry_cost -p rts-core`. A debug
//! number is not a number, and this says so rather than assuming a reader
//! checked.

use std::time::Instant;

use rts_core::entry::{Context, type_of, with_context};
use rts_core::value::{Kinds, Singletons, Value};

fn main() {
    if cfg!(debug_assertions) {
        println!("DEBUG BUILD — these are not numbers\n");
    }

    let context = Context::new(
        Singletons {
            undefined: 0,
            null: 1,
            hole: 2,
        },
        Kinds {
            symbol: 4,
            bigint: 5,
        },
    );

    // An integer, so `type_of` takes its shortest path: a tag switch, a cache
    // hit, and a return. Anything slower than this in a compiled program is the
    // call and the check around it, not the operation.
    let number = Value::from_f64(1.0).bits();

    let (_context, ()) = with_context(context, || {
        let each = 2_000_000;

        // The floor: entering and leaving the loop, with the work removed. Kept
        // so that a reader can tell a 3 ns operation from a 3 ns harness.
        let at = Instant::now();
        let mut sink = 0u64;
        for i in 0..each {
            sink = sink.wrapping_add(i);
        }
        report("empty loop", at, each, sink);

        let at = Instant::now();
        let mut sink = 0u64;
        for _ in 0..each {
            sink = sink.wrapping_add(type_of(number));
        }
        report("type_of, cached", at, each, sink);
    });
}

/// One line, with the checksum, so nothing measured here can be optimised away
/// unobserved.
fn report(what: &str, at: Instant, times: u64, sink: u64) {
    let nanos = at.elapsed().as_nanos() as f64 / times as f64;
    println!("{what:<20} {nanos:>8.2} ns/op   (checksum {sink})");
}
