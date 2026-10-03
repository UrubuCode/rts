//! What each runtime entry point costs, with no compiled code and no harness.
//!
//! # Why a second instrument beside `entry_cost.rs`
//!
//! That one answers ONE question — what reaching the thread's context costs —
//! and answers it well. This answers a different question, and it is a question
//! about the *measurement* rather than about the engine:
//!
//! > **How much of a row's number is the operation, and how much is the harness
//! > it was measured through?**
//!
//! It is not hypothetical. Measured 2026-08-21, the same release binary run six
//! times over `bench/analytic.ts` reported `string index []` between 104.44 and
//! 135.85 ns — a **30.1 % spread with nothing changing but the run** — and
//! `string slice 16` between 179.99 and 224.09 (24.5 %). On that corpus a 4 %
//! difference between two builds is unreadable, and a first pass over one A/B
//! did in fact report four "regressions" that were the instrument rather than
//! the code.
//!
//! `bench/analytic.ts` reaches an operation through a compiled JavaScript
//! program, a closure called per case, `performance.now()`, a heap already
//! filled by every case that ran before it, and a collector whose cost depends
//! on what that heap holds. Every one of those is a real cost of running
//! JavaScript and every one belongs in that file. **None of them belongs in a
//! number about `instance_of`.**
//!
//! So this calls the entry points DIRECTLY, from Rust:
//!
//! - no compiled code, so no calling convention, no throw check, no inline cache
//! - no `performance.now()`, no `console`, no formatting inside a timed region
//! - every subject built ONCE, outside the loop that measures it
//! - several rounds, and the **spread between them printed beside the minimum**,
//!   because a number whose own noise is not stated cannot be compared to
//!   another number
//!
//! # What it deliberately does not say
//!
//! Anything about a program. These are the same tight loops with their operands
//! already in hand that `bench/analytic.ts`'s own header warns about: the best
//! case for caches and the worst for representativeness. Nor does it measure
//! what compiled code adds — the call, the throw check and the cache are
//! exactly what it removes, so a row here is a FLOOR for the same row there,
//! never a prediction of it.
//!
//! Run with `cargo run --release --example entry_probe -p rts-core`.
//! A debug number is not a number, and this says so rather than assuming a
//! reader checked.

use std::time::Instant;

use rts_core::entry::{
    Context, add, array_new, closure_new, declare_literals, get_property, instance_of, key_number,
    object_new, set_property, set_prototype, string_const, type_of, with_context,
};
use rts_core::value::{Kinds, Singletons, Value};

/// Iterations inside one round, for a case that allocates nothing.
const EACH: u64 = 200_000;

/// Iterations for a case that allocates, and why it is not [`EACH`].
///
/// The region starts at 65 536 cells and grows to a reservation of 524 288
/// (`heap::region`, `GROWTH_CEILING`). At [`EACH`] × [`ROUNDS`] this file's
/// first draft asked for 1.4 million cells and died with `heap exhausted` —
/// every allocation unreachable, and none of them reclaimed.
///
/// **That is a fact about this harness, not about the engine**, and it is worth
/// stating rather than quietly tuning around: `roots::scan_stack` is
/// conservative, so any word anywhere on the machine stack that looks like an
/// encoded reference pins the cell it names. A Rust loop that keeps summing
/// freshly allocated references into an accumulator leaves exactly that kind of
/// word behind. Compiled JavaScript running the same shape of loop does not,
/// which is why `bench/analytic.ts`'s allocation rows run for billions of
/// iterations and this cannot.
///
/// So the allocating cases are bounded below what the region holds even if
/// NOTHING is reclaimed — `closure_new` takes two cells per call, which is the
/// binding constraint — and a number from them is a number about the operation
/// with no collection in it. That is a different question from the one
/// `bench/analytic.ts` answers, where 39-73 % of an allocation row is the
/// collector, and neither number substitutes for the other.
///
/// The bound is on the WHOLE program and not on one case, because the region is
/// never emptied between them: three allocating cases at this count take about
/// 56 000 of the 65 536 the region starts with, so no case here reaches a
/// collection at all. At 20 000 they did, and the tell was in the output rather
/// than in the failure — `object_new` reported a **751 % spread** between
/// rounds, which is one round paying for a cycle the others did not.
const ALLOC_EACH: u64 = 2_000;

/// Why the allocating rows are capped here, and why raising it does not work.
///
/// `roots::scan_stack` is conservative: any word anywhere on the machine stack
/// that looks like an encoded reference pins the cell it names. These rows
/// allocate in a tight Rust loop, which leaves such words behind in spill
/// slots, so the cells stay live, nothing is reclaimed, and the count has to
/// stay far below what the region holds.
///
/// **Tried on 2026-10-02 and it does not help**: folding each answer to a byte
/// before the checksum, so no full-width reference reaches the sink. At 200 000
/// the region still died with *"all of them are in use even after a
/// collection"*. The loop pins what it allocates whatever is done with the
/// RESULT, so the cap is a property of conservative scanning rather than of how
/// this file is written, and precise roots
/// (`docs/engine/the-unwired-keystone.md`, plan 6.5) is what lifts it.
///
/// What follows for anyone reading these rows, and it is the reason this note
/// is here rather than in a commit message: an allocating row measures
/// allocation **with no collection in it**, and it cannot be made to measure
/// anything else from here. For the cost a program actually pays — where
/// `bench/analytic.ts` records 39-73 % of an allocation row being the collector
/// — the instrument is a compiled JavaScript loop, because compiled code does
/// not leave those words on the stack. Two numbers, two questions, and neither
/// substitutes for the other.


/// Rounds, so the spread between them can be reported.
const ROUNDS: usize = 7;

/// Only the rows whose name contains this run, when one is given.
///
/// # Why a filter rather than running the whole table
///
/// The allocating rows share one region that is never emptied between them, so
/// adding a row CHANGES THE OTHER ROWS: three `array_new` rows were added on
/// 2026-10-02 and `array_new(4)` went from a 36 % spread to **527 %**, because
/// the extra six thousand cells pushed later rounds into collections the
/// earlier ones did not pay. The minimum is still the least-interfered round,
/// but a row whose rounds differ by five times is not a number to compare
/// anything against.
///
/// So an allocating row is measured in a process of its own — `entry_probe
/// array_new(4)` — which is the same discipline this tree's bench files already
/// state for allocating rows. Without an argument everything runs, which is
/// what the non-allocating rows want and what makes the shared floor visible
/// in the same run as the kinds above it.
fn wanted(what: &str) -> bool {
    match std::env::args().nth(1) {
        Some(filter) => what.contains(&filter),
        None => true,
    }
}

fn main() {
    if cfg!(debug_assertions) {
        println!("DEBUG BUILD — these are not numbers\n");
    }

    let mut context = Context::new(
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
    // Seeded before the context is installed, because `declare_literals` takes
    // the context directly — it is what a host does once per program, and it is
    // the only public door to a string VALUE, which `key_number` then turns into
    // the number a property is filed under.
    let names: Vec<Vec<u16>> = ["prototype", "x"]
        .iter()
        .map(|name| name.encode_utf16().collect())
        .collect();
    declare_literals(&mut context, &names);

    let (_context, ()) = with_context(context, || {
        let undefined = Value::from_singleton(0).bits();
        let number = Value::from_f64(1.0).bits();
        let other = Value::from_f64(2.0).bits();

        // Asked rather than hard-coded: a probe carrying a literal key number
        // would keep measuring after the numbering changed and report a MISS as
        // a cost, which is the shape of defect this whole file exists to catch.
        let prototype_key = key_number(string_const(0));
        let x_key = key_number(string_const(1));

        // Built ONCE, outside every timed region. A case that allocates its own
        // subject per iteration measures the allocation too — `docs/engine/`
        // records a whole campaign whose A/B credited flattening with the
        // allocator's cost for exactly that reason.
        let ctor = closure_new(0, undefined);
        // The code address is never called here: `instance_of` reads a property
        // and walks prototype links. Zero is a legal stand-in for that, and
        // would not be anywhere a call can happen.
        let ctor_prototype = get_property(ctor, prototype_key);
        let derived = object_new(2);
        set_prototype(derived, ctor_prototype);
        // So the row measures the answer `true` at depth one, which is what
        // `bench/analytic.ts`'s own `instanceof` row measures — a `false` walks
        // the chain to its end instead, and would be a different number.
        let victim = object_new(4);
        set_property(victim, x_key, number,
                0 /* strict: uma sonda mede o caminho que o programa usa */);

        // `black_box` on the operand, because without it the floor came out at
        // 0.00 ns/op: summing `i` over a constant range has a closed form and
        // the compiler used it. That is the "too good to be true rather than
        // fast" signal this harness is supposed to produce, and it produced it
        // on its own first row.
        report("floor: empty loop", EACH, |sink, i| {
            sink.wrapping_add(std::hint::black_box(i))
        });
        report("type_of(double)", EACH, |sink, _| {
            sink.wrapping_add(type_of(number))
        });
        report("add(1.0, 2.0)", EACH, |sink, _| {
            sink.wrapping_add(add(number, other))
        });
        report("set_property existing", EACH, move |sink, i| {
            sink.wrapping_add(set_property(victim, x_key, Value::from_f64(i as f64).bits(),
                0 /* strict: uma sonda mede o caminho que o programa usa */))
        });
        report("get_property", EACH, move |sink, _| {
            sink.wrapping_add(get_property(victim, x_key))
        });
        report("instance_of", EACH, move |sink, _| {
            sink.wrapping_add(u64::from(instance_of(derived, ctor)))
        });
        report("object_new(2)", ALLOC_EACH, |sink, _| {
            sink.wrapping_add(object_new(2))
        });
        report("array_new(4)", ALLOC_EACH, |sink, _| {
            sink.wrapping_add(array_new(4))
        });
        // ------------------------------------- where the array's +41 ns LIVES
        //
        // `array_new(4)` measures 60 against `object_new(2)` at 19, and the
        // gap has three candidates: the `Vec` the `Slab` holds (a malloc, plus
        // `wanted` writes to fill it with holes), the `array_elements` side
        // table, and `fresh_length`. Only the FIRST of the three grows with
        // the element count.
        //
        // So the split says which to attack: if 0 and 14 land together the
        // cost is fixed — the side table and the length — and inline elements
        // (plan 14.3) buy nothing by removing a malloc that was not the cost.
        // If the slope is steep it is the `Vec`, and inline is the answer.
        // Measuring this before building it, because this session has already
        // spent a release build on an attribution the rows refuted.
        report("array_new(0)", ALLOC_EACH, |sink, _| {
            sink.wrapping_add(array_new(0))
        });
        report("array_new(14)", ALLOC_EACH, |sink, _| {
            sink.wrapping_add(array_new(14))
        });
        report("array_new(64)", ALLOC_EACH, |sink, _| {
            sink.wrapping_add(array_new(64))
        });
        report("closure_new", ALLOC_EACH, |sink, _| {
            sink.wrapping_add(closure_new(0, undefined))
        });

        // ------------------------------------------- what every kind shares
        //
        // The rows above each measure one kind of value, and the question they
        // cannot answer on their own is whether they have five problems or one
        // problem five times. These two are the shared floor:
        //
        // `alloc(cell)` is the entry point every kind reaches — the allocation
        // with the collector's slice and the growth policy in it. `region.alloc`
        // is the same cell with neither, which is the bump-or-pop and the
        // zeroing alone.
        //
        // Read the kinds against these. On 2026-10-02 the answer was one
        // problem: `object_new(2)` was 21.15 against `alloc(cell)` at 19.5, so
        // a two-field object IS the cell plus a nanosecond and a half, and the
        // same holds for a string of five characters (30.7, of which 19.5) and
        // for an array (57.75, the cell plus its element `Slab`). Lowering the
        // floor lowers all of them at once, which is why plan items 6.4, 6.6
        // and 11.2 outrank anything written per kind.
        // Any declared layout will do: what is being timed is the heap, not the
        // shape, and this is the one the runtime declares first.
        let a_type = rts_core::entry::with_runtime(|context| i64::from(context.text_type_index()));
        report("alloc(cell) — the shared floor", ALLOC_EACH, move |sink, _| {
            sink.wrapping_add(rts_core::entry::alloc(crate_stride(), a_type))
        });
        report("region.alloc — no collector", ALLOC_EACH, move |sink, _| {
            let cell = rts_core::entry::with_runtime(|context| {
                context
                    .region
                    .alloc(crate_stride() as u32, a_type as u32)
                    .unwrap_or(0)
            });
            sink.wrapping_add(u64::from(cell))
        });
    });
}

/// A cell's stride, as the allocation entry takes it.
fn crate_stride() -> i64 {
    i64::from(rts_core::heap::STRIDE)
}

/// Runs one case for [`ROUNDS`] rounds and prints the minimum with its spread.
///
/// **The minimum, not the mean**, for the reason every harness in this
/// repository states: the fastest round is the one least interfered with. The
/// spread beside it is what says whether that minimum was typical, and it is
/// printed rather than swallowed — a number without its own noise cannot be
/// compared against another number, which is the whole lesson this file was
/// written after.
///
/// The checksum is carried through the loop and printed, so that nothing
/// measured here can be optimised away unobserved: a case whose body the
/// compiler removes shows up as a number too good to be true rather than as a
/// fast one.
fn report(what: &str, each: u64, mut body: impl FnMut(u64, u64) -> u64) {
    if !wanted(what) {
        return;
    }
    let mut best = f64::INFINITY;
    let mut worst: f64 = 0.0;
    let mut sink = 0u64;
    for _ in 0..ROUNDS {
        let at = Instant::now();
        for i in 0..each {
            sink = body(sink, i);
        }
        let nanos = at.elapsed().as_nanos() as f64 / each as f64;
        best = best.min(nanos);
        worst = worst.max(nanos);
    }
    let spread = if best > 0.0 {
        100.0 * (worst - best) / best
    } else {
        0.0
    };
    println!("{what:<22} {best:>9.2} ns/op   spread {spread:>5.1}%   (checksum {sink})");
}
