//! What reaching a cell costs, with no `Context` and no entry point present.
//!
//! # The question this answers
//!
//! `Context::key_of_text_cell` reads two words of ONE cell — the header type and
//! slot zero — and reaches them through `Region::type_of` followed by
//! `Region::field`. Read together (`region/mod.rs:632-640, 671-674`) that is
//! **three `decompose` calls and two loads of the same header word**: `field`
//! bounds itself by `width_of`, which decomposes and loads the header, and then
//! decomposes a second time on its own account.
//!
//! Whether that redundancy costs anything is not obvious, and this exists to be
//! able to say it rather than assume it. `decompose` is a mask, a compare and a
//! shift over a value already in a register, and LLVM may well collapse all
//! three within one inlined call chain.
//!
//! # The falsifier
//!
//! **If `field` alone costs about what `type_of` alone costs, the redundancy is
//! already gone** — the second decompose and the second header load were
//! eliminated, what remains is the load nobody can remove, and fusing the two
//! accessors by hand would buy nothing. The premise dies and the change is not
//! made.
//!
//! What it does NOT say: nothing here measures `key_of_text_cell`, a property
//! read, or anything a compiled program does. It measures two accessors of one
//! data structure, which is the only claim that can be attributed to this layer.
//!
//! The reference goes through `black_box` at every iteration. Without it the
//! whole chain is loop-invariant, LLVM hoists it out, and all four rows report
//! 0.000 ns — which is what the first run of this file actually did.
//!
//! Run with `cargo run --release --example cell_access -p rts-core`. A debug
//! number is not a number, and this says so rather than assuming a reader
//! checked.

use std::hint::black_box;
use std::time::Instant;

use rts_core::heap::Region;

fn main() {
    if cfg!(debug_assertions) {
        println!("DEBUG BUILD — these are not numbers\n");
    }

    let mut region = Region::with_capacity(1024);
    let cell = region
        .alloc(16, 7)
        .expect("a region with room has a cell to give");
    region
        .set_field(cell, 0, 0x1234_5678)
        .expect("slot zero of a fresh cell is writable");

    let each = 20_000_000u64;

    // The floor: the loop with the work removed, so a reader can tell a
    // one-nanosecond accessor from a one-nanosecond harness.
    let at = Instant::now();
    let mut sink = 0u64;
    for i in 0..each {
        sink = sink.wrapping_add(black_box(i));
    }
    report("empty loop", at, each, sink);

    // One decompose, one header load.
    let at = Instant::now();
    let mut sink = 0u64;
    for _ in 0..each {
        sink = sink.wrapping_add(u64::from(region.type_of(black_box(cell)).unwrap_or(0)));
    }
    report("type_of", at, each, sink);

    // As written: `width_of` (decompose + header load), a second decompose, a
    // bounds check, and the slot load. Against `type_of` above, the difference
    // is what the redundancy costs — or is not, which is the falsifier.
    let at = Instant::now();
    let mut sink = 0u64;
    for _ in 0..each {
        sink = sink.wrapping_add(region.field(black_box(cell), 0).unwrap_or(0));
    }
    report("field(_, 0)", at, each, sink);

    // The pair, in the order `key_of_text_cell` asks them. This is the number a
    // fused single-decompose form has to beat.
    let at = Instant::now();
    let mut sink = 0u64;
    for _ in 0..each {
        let reference = black_box(cell);
        let ty = u64::from(region.type_of(reference).unwrap_or(0));
        let slot = region.field(reference, 0).unwrap_or(0);
        sink = sink.wrapping_add(ty ^ slot);
    }
    report("type_of + field", at, each, sink);
}

/// One line, with the checksum, so nothing measured here can be optimised away
/// unobserved.
fn report(what: &str, at: Instant, times: u64, sink: u64) {
    let nanos = at.elapsed().as_nanos() as f64 / times as f64;
    println!("{what:<20} {nanos:>8.3} ns/op   (checksum {sink})");
}
