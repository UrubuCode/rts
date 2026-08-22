//! **Experiment 5 — what claiming the heap costs at startup.**
//!
//! # The question
//!
//! `rts run empty.ts` takes 19.9 ms. Process load is 12.8 ms and `RTS_TIMING`
//! accounts for 5.1 ms of the rest, which leaves about 2 ms between the process
//! being loaded and `run` starting that nothing times.
//!
//! One thing that happens in there is the heap being claimed.
//! `crates/rts-host/src/run.rs:1095` asks for `1 << 16` cells and
//! `crates/rts-core/src/heap/region/mod.rs:311` claims them:
//!
//! ```ignore
//! let reserved = cells.saturating_mul(GROWTH_CEILING);   // 8x -> 524 288 cells
//! let mut words = Vec::new();
//! words.reserve_exact(words_for(reserved));              // 64 MiB of address space
//! words.resize(words_for(cells), 0);                     // 8 MiB, WRITTEN
//! ```
//!
//! The reservation is deliberate and well argued — the base of a region is an
//! immediate in compiled code, so it may never move, and that file rejects
//! `realloc`, a second region and an explicit `VirtualAlloc` each with a reason.
//! None of that is in question here.
//!
//! **The `resize` is.** `Vec::resize` with a zero fill is a `memset` over 8 MiB
//! of memory the operating system has just handed over, and an operating system
//! hands over zeroed pages: it must, or one process could read another's. So the
//! write may be paying to make already-zero memory zero.
//!
//! "May" is the whole point of measuring. On Windows the default Rust allocator
//! is `HeapAlloc`, and for a block this size it goes to `VirtualAlloc` — but a
//! smaller block would come out of a heap segment that has been used before, and
//! that memory is *not* zero. Which of the two happens is not something to
//! reason about from a doc comment.
//!
//! # What is being compared
//!
//! Four ways to end up with the same thing — a `Vec<u64>` whose capacity is the
//! full reservation, whose length is the starting bound, and whose contents are
//! zero:
//!
//! 1. **reserve then resize** — what the engine does.
//! 2. **`vec![0; reserved]` then `truncate`** — `vec!` of a zero is specialised
//!    to `alloc_zeroed`, which on a block this size asks the OS for demand-zero
//!    pages and never writes them. `truncate` lowers the length without moving
//!    or freeing anything, so the capacity — and therefore the base — is
//!    unchanged.
//! 3. **reserve then `resize` only the first cell's worth** — the shape a lazy
//!    region would have, to show how much of row 1 is proportional to the 8 MiB
//!    rather than fixed.
//! 4. **reserve and never fill** — not a candidate (the length would be wrong
//!    and every read out of bounds); the floor, to say what the reservation
//!    itself costs.
//!
//! The two smaller allocations the same constructor makes are measured beside
//! them, because "the region costs X" should account for everything it claims:
//! `spanned_interior: vec![false; cells]` is another 64 KiB.
//!
//! # What this cannot say
//!
//! Whether the saving survives in the engine. A first-touch cost deferred is not
//! a cost removed — if the program then writes those pages, the page faults move
//! rather than disappear. What decides it is how much of the 8 MiB an ordinary
//! program actually touches, and for `empty.ts` the answer is nearly none. So
//! this experiment prices the best case and the engine measurement decides the
//! real one.

use rts_isolated::{measure, opaque, report};

/// `1 << 16`, from `crates/rts-host/src/run.rs:1095`.
const CELLS: usize = 1 << 16;

/// One header word and fifteen field words — `INLINE_SLOTS` is 15.
const CELL_WORDS: usize = 16;

/// `GROWTH_CEILING`, from `crates/rts-core/src/heap/region/growth.rs`: eight
/// times the starting size, so 64 MiB of address space against 8 MiB written.
const GROWTH_CEILING: usize = 8;

fn words_for(cells: usize) -> usize {
    cells * CELL_WORDS
}

fn main() {
    let start = words_for(CELLS);
    let reserved = words_for(CELLS * GROWTH_CEILING);

    let rows = vec![
        // ------------------------------------------------------------ 1
        measure("1. reserve_exact + resize(start, 0)  (engine)", |n| {
            let mut acc = 0u64;
            for _ in 0..n {
                let mut words: Vec<u64> = Vec::new();
                words.reserve_exact(opaque(reserved));
                words.resize(opaque(start), 0);
                acc = acc.wrapping_add(words[opaque(start - 1)]).wrapping_add(words.len() as u64);
            }
            acc
        }),
        // ------------------------------------------------------------ 2
        measure("2. vec![0; reserved] + truncate(start)", |n| {
            let mut acc = 0u64;
            for _ in 0..n {
                let mut words: Vec<u64> = vec![0; opaque(reserved)];
                words.truncate(opaque(start));
                acc = acc.wrapping_add(words[opaque(start - 1)]).wrapping_add(words.len() as u64);
            }
            acc
        }),
        // ------------------------------------------------------------ 3
        measure("3. reserve_exact + resize(one cell, 0)", |n| {
            let mut acc = 0u64;
            for _ in 0..n {
                let mut words: Vec<u64> = Vec::new();
                words.reserve_exact(opaque(reserved));
                words.resize(opaque(CELL_WORDS), 0);
                acc = acc.wrapping_add(words[opaque(CELL_WORDS - 1)]).wrapping_add(words.len() as u64);
            }
            acc
        }),
        // ------------------------------------------------------------ 4
        measure("4. reserve_exact alone (the floor)", |n| {
            let mut acc = 0u64;
            for _ in 0..n {
                let mut words: Vec<u64> = Vec::new();
                words.reserve_exact(opaque(reserved));
                acc = acc.wrapping_add(words.capacity() as u64);
            }
            acc
        }),
        // ------------------------------------------------------------ 5
        measure("5. spanned_interior: vec![false; cells]", |n| {
            let mut acc = 0u64;
            for _ in 0..n {
                let flags: Vec<bool> = vec![false; opaque(CELLS)];
                acc = acc.wrapping_add(flags[opaque(CELLS - 1)] as u64);
            }
            acc
        }),
    ];

    report("Experiment 5 - claiming the region at startup", &rows);
    println!();
    println!("These are per-STARTUP costs, not per-operation: the harness divides by");
    println!("its iteration count, so each row reads directly as milliseconds-per-");
    println!("thousand-startups / nanoseconds-per-startup. Row 1 is what `rts run`");
    println!("pays today before it has compiled anything.");
}
