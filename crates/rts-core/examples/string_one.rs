//! One string layer per process, because `string_cost` cannot answer this one.
//!
//! # Why this exists beside `string_cost`
//!
//! `string_cost` measures fourteen layers in one process, and every one of them
//! that allocates keeps what it made: the probe has no compiled frames for the
//! collector to scan, so nothing it allocates is ever freed. So each row runs
//! with a fuller region than the row above it, and a row's number includes
//! however much collection work that fill state causes.
//!
//! That is tolerable for reading the layers against each other once. It is not
//! tolerable for comparing a row between two builds, and 2026-10-01 is where
//! that was measured rather than assumed: a change that touched neither
//! `cells.insert` nor the hand-rolled cell moved `alloc+insert+2 set_field`
//! from 58.77 to 29.90 and `cells.insert only` from 10.14 to 20.50 — because it
//! made the rows ABOVE them allocate differently, so those two met a different
//! region. Both rows were honest about the program they ran and said nothing
//! about the change.
//!
//! So: one row, one process, a context that has done nothing else.
//!
//! ```text
//! cargo run --release --example string_one -p rts-core -- intern 5
//! ```
//!
//! The second argument is the string's length in characters, which is the axis
//! that decides the placement: at or below `text::INLINE` the `Str` owns no
//! buffer and lives in the string's own cell, above it the buffer and the slab
//! come back. A pair of runs either side of that boundary is the measurement.

use std::time::Instant;

use rts_core::entry::{Context, alloc, make_string, with_context, with_runtime};
use rts_core::text::Str;
use rts_core::value::{Kinds, Singletons};

fn main() {
    if cfg!(debug_assertions) {
        println!("DEBUG BUILD — these are not numbers");
        return;
    }
    let mut args = std::env::args().skip(1);
    let row = args.next().unwrap_or_else(|| "intern".to_owned());
    let length: usize = args
        .next()
        .and_then(|given| given.parse().ok())
        .unwrap_or(5);
    // Enough to resolve a twenty-nanosecond operation and few enough that the
    // region does not fill: the rows here allocate and never free, which is
    // the whole reason the process is one row long.
    let each: u64 = args
        .next()
        .and_then(|given| given.parse().ok())
        .unwrap_or(20_000);

    let text: String = "a".repeat(length);
    let bytes = text.clone().into_bytes();

    let context = Context::new(
        Singletons { undefined: 0, null: 1, hole: 2 },
        Kinds { symbol: 4, bigint: 5 },
    );
    let (_context, ()) = with_context(context, || {
        let text_type = with_runtime(|context| i64::from(context.text_type_index()));
        let at = Instant::now();
        let mut sink = 0u64;
        match row.as_str() {
            // What a string costs, whole: the cell, the placement, the length.
            // This is the number `make_string` and every runtime-produced
            // string pays.
            "intern" => {
                for _ in 0..each {
                    let made =
                        with_runtime(|context| context.intern_value(Str::from_latin1(&bytes)).bits());
                    sink = sink.wrapping_add(made & 0xff);
                }
            }
            // The same through the entry point a native reaches for.
            "make" => {
                for _ in 0..each {
                    let made = with_runtime(|context| make_string(context, &text));
                    sink = sink.wrapping_add(made & 0xff);
                }
            }
            // The floor: the cell and two slots written, which is what remains
            // once the text is in the cell and nothing else is allocated.
            "floor" => {
                let ty = text_type as u32;
                for _ in 0..each {
                    let made = with_runtime(|context| {
                        let cell = context.region.alloc(rts_core::heap::STRIDE, ty).unwrap_or(0);
                        context.region.set_field(cell, 0, 7);
                        context.region.set_field(cell, 1, 5);
                        cell
                    });
                    sink = sink.wrapping_add(u64::from(made) & 0xff);
                }
            }
            // The cell alone, through the allocation entry the runtime uses —
            // so with the collector's slice and the growth policy included.
            "cell" => {
                for _ in 0..each {
                    sink = sink.wrapping_add(alloc(128, text_type) & 0xff);
                }
            }
            // Reading the text back out, which is the half a placement can make
            // worse: a cell-held `Str` is one line, a slab-held one is two.
            "read" => {
                let cell = with_runtime(|context| {
                    context.intern_value(Str::from_latin1(&bytes)).bits()
                });
                let reference = rts_core::value::Value(cell).as_slot().expect("a text cell");
                for _ in 0..each {
                    sink = sink.wrapping_add(with_runtime(|context| {
                        context.text_at(reference).map_or(0, |text| text.len() as u64)
                    }));
                }
            }
            other => {
                println!("unknown row {other} — intern, make, floor, cell, read");
                return;
            }
        }
        let nanos = at.elapsed().as_nanos() as f64 / each as f64;
        println!("{row:<8} len {length:<4} {nanos:>8.2} ns/op   (checksum {sink})");
    });
}
