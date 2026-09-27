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

use rts_core::entry::{Context, add, alloc, make_string, number_to_string, string_of, template_join, with_context, with_runtime};
use rts_core::coerce::number_to_string as number_to_string_str;
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
        // Small enough that the allocating rows stay inside the region: this
        // probe has no compiled frames for the collector to scan, so nothing it
        // makes is ever freed. It was 60 000 with five such rows; the layers of
        // `intern_value` and `template_join` below made it fourteen.
        let each = 25_000;

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

        // The layers under `make_string`, each on its own inside one borrow.
        let at = Instant::now();
        let mut sink = 0u64;
        for _ in 0..each {
            let made = with_runtime(|context| context.intern_value(Str::from_latin1(b"abcd-")).bits());
            sink = sink.wrapping_add(made & 0xff);
        }
        report("intern_value(5)", at, each, sink);

        // The same steps by hand, to see whether the sum is the whole.
        let ty = text_type as u32;
        let at = Instant::now();
        let mut sink = 0u64;
        for _ in 0..each {
            let made = with_runtime(|context| {
                let cell = context.region.alloc(rts_core::heap::STRIDE, ty).unwrap_or(0);
                let slot = context.cells.insert(Str::from_latin1(b"abcd-")).slot();
                context.region.set_field(cell, 0, u64::from(slot.0));
                context.region.set_field(cell, 1, Value::from_f64(5.0).bits());
                cell
            });
            sink = sink.wrapping_add(u64::from(made) & 0xff);
        }
        report("alloc+insert+2 set_field", at, each, sink);

        let at = Instant::now();
        let mut sink = 0u64;
        for _ in 0..each {
            let made = with_runtime(|context| {
                let cell = context.region.alloc(rts_core::heap::STRIDE, ty).unwrap_or(0);
                context.region.set_field(cell, 0, 7);
                context.region.set_field(cell, 1, Value::from_f64(5.0).bits());
                cell
            });
            sink = sink.wrapping_add(u64::from(made) & 0xff);
        }
        report("alloc+2 set_field", at, each, sink);

        let at = Instant::now();
        let mut sink = 0u64;
        for i in 0..each {
            let slot = with_runtime(|context| context.cells.insert(Str::from_latin1(b"abcd-")).slot());
            sink = sink.wrapping_add(u64::from(slot.0) & 0xff).wrapping_add(i & 1);
        }
        report("cells.insert only", at, each, sink);

        let mut plain: Vec<[u64; 6]> = Vec::new();
        let at = Instant::now();
        let mut sink = 0u64;
        for i in 0..each {
            plain.push([i, 1, 2, 3, 4, 5]);
            sink = sink.wrapping_add(plain.len() as u64 & 1);
        }
        report("Vec<[u64;6]>::push", at, each, sink);
        std::hint::black_box(&plain);

        let mut own: Vec<Str> = Vec::new();
        let at = Instant::now();
        let mut sink = 0u64;
        for i in 0..each {
            own.push(Str::from_latin1(b"abcd-"));
            sink = sink.wrapping_add(own.len() as u64 & 1).wrapping_add(i & 1);
        }
        report("Vec<Str>::push", at, each, sink);
        std::hint::black_box(&own);

        let ty = text_type as u32;
        let at = Instant::now();
        let mut sink = 0u64;
        for _ in 0..each {
            let cell = with_runtime(|context| context.region.alloc(rts_core::heap::STRIDE, ty).unwrap_or(0));
            sink = sink.wrapping_add(u64::from(cell) & 0xff);
        }
        report("region.alloc (borrowed)", at, each, sink);

        let at = Instant::now();
        let mut sink = 0u64;
        for i in 0..each {
            let text = number_to_string_str(123456.0 + (i & 1) as f64);
            sink = sink.wrapping_add(text.len() as u64);
        }
        report("number_to_string Str", at, each, sink);

        // What `Rooted::new` costs: a boxed Vec and a thread-local push and pop.
        let at = Instant::now();
        let mut sink = 0u64;
        for i in 0..each {
            let mut held: Box<Vec<u64>> = Box::new(Vec::new());
            held.push(i);
            sink = sink.wrapping_add(held[0] & 1);
        }
        report("Box<Vec> + push", at, each, sink);

        let at = Instant::now();
        let mut sink = 0u64;
        for i in 0..each {
            let mut bytes: Vec<u8> = Vec::with_capacity(12);
            bytes.extend_from_slice(b"abcd-");
            bytes.push((i & 0x7f) as u8);
            sink = sink.wrapping_add(bytes.len() as u64);
        }
        report("Vec<u8>::with_capacity", at, each, sink);

        // A template site `a${x}b` registered by hand, then joined with one number.
        let site = with_runtime(|context| {
            let a = make_string(context, "a");
            let b = make_string(context, "b");
            let first = context.literals.len() as u32;
            context.literals.push(a);
            context.literals.push(b);
            context.templates.push((vec![first, first + 1], None));
            (context.templates.len() - 1) as i64
        });
        let absent = with_runtime(|context| rts_core::entry::undefined_in(context));
        let at = Instant::now();
        let mut sink = 0u64;
        for _ in 0..each {
            sink = sink.wrapping_add(template_join(site, 1, number, absent, absent, absent, absent, absent) & 0xff);
        }
        report("template_join(a${n}b)", at, each, sink);

        let at = Instant::now();
        let mut sink = 0u64;
        for _ in 0..each {
            sink = sink.wrapping_add(template_join(site, 1, left, absent, absent, absent, absent, absent) & 0xff);
        }
        report("template_join(a${s}b)", at, each, sink);

        let at = Instant::now();
        let mut sink = 0u64;
        for _ in 0..each {
            sink = sink.wrapping_add(string_of(number) & 0xff);
        }
        report("string_of(num)", at, each, sink);
    });
}

fn report(what: &str, at: Instant, times: u64, sink: u64) {
    let nanos = at.elapsed().as_nanos() as f64 / times as f64;
    println!("{what:<22} {nanos:>8.2} ns/op   (checksum {sink})");
}
