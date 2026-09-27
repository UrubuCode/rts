//! What making a string costs, decomposed, with no compiled code in sight.
//!
//! `bench/analytic.ts` (2026-09-26, release) puts `s + "-"` at 147 ns, a
//! template with one number at 328 and `String(i)` at 175, against 1.6, 27 and
//! 24 on Node. Each of those is one runtime call and the number is the whole of
//! it. This asks each layer on its own — the `Str` buffer, the cell, the
//! coercion, the operator's dispatch — so a reader can say WHICH layer is the
//! cost rather than that the total is high.
//!
//! Run with `cargo run --release --example string_cost -p rts-core`. A debug
//! number is not a number.

use std::time::Instant;

use rts_core::entry::{Context, add, alloc, make_string, number_to_string, with_context, with_runtime};
use rts_core::text::Str;
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
    let (_context, ()) = with_context(context, || {
        // Small enough that five rows of one allocation each stay inside the
        // region: this probe has no compiled frames for the collector to scan,
        // so nothing it makes is ever freed.
        let each = 60_000;

        let at = Instant::now();
        let mut sink = 0u64;
        for i in 0..each {
            sink = sink.wrapping_add(i);
        }
        report("empty loop", at, each, sink);

        // The borrow alone: a thread-local and a RefCell, nothing else.
        let at = Instant::now();
        let mut sink = 0u64;
        for i in 0..each {
            sink = sink.wrapping_add(with_runtime(|_| i));
        }
        report("with_runtime(empty)", at, each, sink);

        // The slab alone: a `Str` put in and taken straight back out.
        let at = Instant::now();
        let mut sink = 0u64;
        for _ in 0..each {
            let held = with_runtime(|context| {
                let handle = context.cells.insert(Str::from_latin1(b"abcd-"));
                let slot = handle.slot();
                context.cells.free(slot);
                slot.0 as u64
            });
            sink = sink.wrapping_add(held);
        }
        report("slab insert+free", at, each, sink);

        // The Rust buffer alone: no cell, no context.
        let at = Instant::now();
        let mut sink = 0u64;
        for _ in 0..each {
            let text = Str::from_latin1(b"abcd-");
            sink = sink.wrapping_add(text.len() as u64);
        }
        report("Str::from_latin1(5)", at, each, sink);

        // A bare cell of the text type, through the allocation entry: the region
        // alone, no slab and no text.
        let text_type = with_runtime(|context| i64::from(context.text_type_index()));
        let at = Instant::now();
        let mut sink = 0u64;
        for _ in 0..each {
            sink = sink.wrapping_add(alloc(128, text_type) & 0xff);
        }
        report("alloc(cell)", at, each, sink);

        // The buffer plus the cell: what every new string pays.
        let at = Instant::now();
        let mut sink = 0u64;
        for _ in 0..each {
            let made = with_runtime(|context| make_string(context, "abcd-"));
            sink = sink.wrapping_add(made & 0xff);
        }
        report("make_string(5)", at, each, sink);

        // `"abcd" + "-"` through the operator: dispatch, ToPrimitive twice, concat, cell.
        let (left, right) = with_runtime(|context| (make_string(context, "abcd"), make_string(context, "-")));
        let at = Instant::now();
        let mut sink = 0u64;
        for _ in 0..each {
            sink = sink.wrapping_add(add(left, right) & 0xff);
        }
        report("add(str, str)", at, each, sink);

        // A number to text: the decimal conversion and the cell.
        let number = Value::from_f64(123456.0).bits();
        let at = Instant::now();
        let mut sink = 0u64;
        for _ in 0..each {
            sink = sink.wrapping_add(number_to_string(123456.0) & 0xff);
        }
        report("number_to_string", at, each, sink);

        // `"abcd" + 123456`: the operator with a number on one side.
        let at = Instant::now();
        let mut sink = 0u64;
        for _ in 0..each {
            sink = sink.wrapping_add(add(left, number) & 0xff);
        }
        report("add(str, num)", at, each, sink);
    });
}

fn report(what: &str, at: Instant, times: u64, sink: u64) {
    let nanos = at.elapsed().as_nanos() as f64 / times as f64;
    println!("{what:<22} {nanos:>8.2} ns/op   (checksum {sink})");
}
