//! What one allocation costs, and how much of it is the collector.
//!
//! # The question
//!
//! `bench/analytic.ts` puts `new Callee()` at 90.89 ns. `RTS_GC_DEBUG=1` over a
//! 200 000-iteration loop of it prints
//!
//! ```text
//! rts-gc roots 171 live 1794 freed 63669   (three times)
//! ```
//!
//! — the region holds 65 536 cells (`rts-host/src/run.rs:1095`), 1 794 of them
//! are the built-in world, and every cycle reclaims the rest. So **every
//! allocation past the first fill of the region is paid for by exactly one
//! [`collect_cycle::release`]**: 22 `Aside::remove` calls, two full linear scans
//! (`weak::clear_freed`, `finalize::queue_freed`), a `type_of`, and a
//! `Region::free` — plus its share of the `each_live` walk that found it.
//!
//! Nothing has measured that. This does, in the engine rather than in a model,
//! by running the same `entry::alloc` in two heaps that differ **only** in
//! whether a collection has to happen:
//!
//! | | what it measures |
//! |---|---|
//! | `sweeping` | a 65 536-cell region, allocations far past its size, so the collector runs and every allocation carries one `release` |
//! | `pre-freed` | a region of the same shape whose free list was built by `Region::free` directly, so `alloc` takes exactly the same free-list path with **no walk and no release** |
//!
//! Both regions are **fully faulted in before the clock starts** — the pre-freed
//! one by allocating every cell and giving them all back, the sweeping one by
//! the warm-up loop. That is the whole reason the comparison is written this way
//! rather than as "a big region against a small one": at 128 bytes a cell a
//! fresh region takes one soft page fault per 32 allocations, which is tens of
//! nanoseconds an allocation and would be the entire difference between two
//! heaps of different sizes.
//!
//! `sweeping − pre-freed` is therefore the collector's bookkeeping, per
//! allocation, with the allocation itself, the zero fill and the page faults
//! held identical.
//!
//! # Run it
//!
//! ```bash
//! cargo run --release -p rts-core --example alloc_cost
//! ```
//!
//! Builds `rts-core` alone, not the CLI. A debug build says so and its numbers
//! are not numbers.

use std::time::Instant;

use rts_core::entry::{Context, alloc, with_context};
use rts_core::heap::{Region, STRIDE};
use rts_core::value::{Kinds, Singletons};

/// What `rts-host/src/run.rs:1095` gives a running program.
const CELLS: u32 = 1 << 16;
/// Enough to cross the region several times, so the steady state — not the
/// first fill — is what is timed.
const EACH: u64 = 400_000;

fn singletons() -> Singletons {
    Singletons {
        undefined: 0,
        null: 1,
        hole: 2,
    }
}

fn kinds() -> Kinds {
    Kinds {
        symbol: 4,
        bigint: 5,
    }
}

/// A context over a region of `cells`, with the stack bound installed.
///
/// Without the bound `collect_cycle::collect` refuses to run at all — see its
/// own documentation for why a cycle that cannot see the stack half of the root
/// set declines rather than running smaller — and the region would simply fill
/// and the program would exit.
fn context_over(cells: u32, stack_high: usize) -> Context {
    let mut context = Context::over(singletons(), kinds(), Region::with_capacity(cells));
    context.stack_high = Some(stack_high);
    context
}

/// A type number to allocate against. Any layout will do: what is being timed
/// is the heap, not the shape.
fn a_type(context: &mut Context) -> u32 {
    context
        .types
        .declare(&[rts_cranelift::repr::Repr::I64])
        .index() as u32
}

fn report(name: &str, at: Instant, each: u64, sink: u64) -> f64 {
    let nanos = at.elapsed().as_secs_f64() * 1e9 / each as f64;
    println!("{name:<38} {nanos:>9.2} ns/op   sink {sink}");
    nanos
}

fn main() {
    if cfg!(debug_assertions) {
        println!("DEBUG BUILD — these are not numbers\n");
    }
    let anchor = 0u64;
    let stack_high = &anchor as *const u64 as usize + 4096;

    // ---------------------------------------------------------------- sweeping
    let mut context = context_over(CELLS, stack_high);
    let ty = a_type(&mut context);
    let (context_back, sweeping) = with_context(context, || {
        // Cross the region twice before the clock starts, so what is timed is
        // the steady state and not the first, bump-allocated fill.
        let mut sink = 0u64;
        for _ in 0..(CELLS as u64 * 2) {
            sink = sink.wrapping_add(alloc(STRIDE as i64, ty as i64));
        }
        let at = Instant::now();
        for _ in 0..EACH {
            sink = sink.wrapping_add(alloc(STRIDE as i64, ty as i64));
        }
        report("sweeping (a release per alloc)", at, EACH, sink)
    });
    drop(context_back);

    // --------------------------------------------------------------- pre-freed
    //
    // A region big enough that `EACH` allocations all come out of a free list
    // built by hand, so `alloc` runs its free-list path — the link read, the
    // header store, the fifteen zero words — with no `each_live` walk and no
    // `release` in front of it.
    let cells = (EACH as u32) + 1024;
    let mut context = context_over(cells, stack_high);
    let ty = a_type(&mut context);
    // Every cell touched and given back, so the pages are faulted in and the
    // free list is as long as the loop below is.
    let mut taken: Vec<u32> = Vec::with_capacity(cells as usize);
    while let Some(cell) = context.region.alloc(STRIDE, ty) {
        taken.push(cell);
    }
    for cell in taken {
        context.region.free(cell);
    }
    let (context_back, prefreed) = with_context(context, || {
        let mut sink = 0u64;
        let at = Instant::now();
        for _ in 0..EACH {
            sink = sink.wrapping_add(alloc(STRIDE as i64, ty as i64));
        }
        report("pre-freed (no walk, no release)", at, EACH, sink)
    });
    drop(context_back);

    // ------------------------------------------- what a linear scan per cell costs
    //
    // `collect_cycle::release` calls two things that are NOT keyed by cell:
    // `weak::clear_freed` walks the whole of `Context::weak`, and
    // `finalize::queue_freed` walks the whole of `Context::deaths` — once per
    // FREED CELL. So a cycle is O(freed × registrations), and 63 669 freed cells
    // against a thousand registrations is 63 million comparisons per cycle.
    //
    // `weak::watch` is the one of the two an example can reach — `finalize` has
    // no `_current` wrapper — and the two scans are the same shape, so what this
    // prices for one is what the other costs. A watch is never removed by a
    // collection either: `clear_freed` blanks the value and leaves the entry, so
    // the table only ever grows.
    for registrations in [100u32, 1000u32] {
        let mut context = context_over(CELLS, stack_high);
        let ty = a_type(&mut context);
        let (context_back, scanned) = with_context(context, || {
            let mut sink = 0u64;
            for _ in 0..registrations {
                let cell = alloc(STRIDE as i64, ty as i64);
                let value = rts_core::value::Value::from_slot(cell as u32).bits();
                let _watch = rts_core::entry::weak_watch(value);
                sink = sink.wrapping_add(cell);
            }
            for _ in 0..(CELLS as u64 * 2) {
                sink = sink.wrapping_add(alloc(STRIDE as i64, ty as i64));
            }
            let at = Instant::now();
            for _ in 0..EACH {
                sink = sink.wrapping_add(alloc(STRIDE as i64, ty as i64));
            }
            report(
                &format!("sweeping, {registrations} weak watches"),
                at,
                EACH,
                sink,
            )
        });
        drop(context_back);
        println!(
            "    ... {:.2} ns/alloc more than the same heap with none",
            scanned - sweeping
        );
    }

    println!();
    println!(
        "the collector's bookkeeping, per allocation: {:.2} ns",
        sweeping - prefreed
    );
    // The row this is a share OF, from `docs/codegen/measurements.md`. It was
    // 90.89 and that table was re-measured on 2026-08-26 at `673b9c0c`: the row
    // is 74.55 now, so the share printed here was reading low against a number
    // that had moved. A constant copied out of a table is a second source of it,
    // and this is the comment that says which table and when — re-read it before
    // trusting the percentage.
    const ALLOC_CLASS_INSTANCE: f64 = 74.55;
    println!(
        "against the {ALLOC_CLASS_INSTANCE} ns `bench/analytic.ts` reports for `new Callee()`: {:.1}%",
        (sweeping - prefreed) / ALLOC_CLASS_INSTANCE * 100.0
    );
}
