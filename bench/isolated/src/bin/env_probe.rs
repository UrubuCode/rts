//! **Experiment 7 — what a debug switch costs when it is read on a hot path.**
//!
//! # The question
//!
//! `crates/rts-core/src/entry/objects.rs:544`, inside `put` — the by-name
//! property write:
//!
//! ```ignore
//! if std::env::var_os("RTS_CACHE_WHY").is_some() && context.resolves % 20_000 <= 1 {
//!     eprintln!(...);
//! }
//! ```
//!
//! `&&` evaluates left to right, so **the environment is read first**, on every
//! by-name property write, and the cheap modulo that would have short-circuited
//! it is second. The same shape appears at `entry/cache.rs:93` (`RTS_CACHE_DEBUG`,
//! though there it is guarded by a counter test first) and
//! `crates/rts-codegen/src/emit/escape.rs:257` (`RTS_ESCAPE_STATS`, at compile
//! time rather than run time).
//!
//! On Windows `std::env::var_os` is not a memory read. It is
//! `GetEnvironmentVariableW` — a UTF-16 re-encode of the name, a call into
//! `kernel32`, and a walk of the process environment block. **How expensive is
//! it, and does the answer change what to do?**
//!
//! # Why this needs measuring rather than assuming
//!
//! Because the obvious estimate contradicts the table. `alloc add prop after`
//! costs 54.12 ns end to end in `bench/analytic.ts`, and if a `var_os` in `put`
//! cost the several hundred nanoseconds it intuitively should, that row could
//! not be 54. Either the read is far cheaper than it looks, or that row does not
//! reach this line. Both are worth knowing, and guessing between them is exactly
//! what this tree's first rule forbids.
//!
//! # What is being compared
//!
//! 1. `var_os` for a name that is **absent**, which is the case in every run
//!    where nobody set the switch — the case that matters.
//! 2. `var_os` for a name that is **present**, since a miss may walk the whole
//!    block while a hit stops early.
//! 3. The same behind a `OnceLock<bool>`, which is how
//!    `crates/rts-cranelift/src/probe/phase.rs:48` already does it:
//!    `*ASKED.get_or_init(|| std::env::var_os("RTS_TIMING").is_some())`.
//! 4. A plain `bool` in a `static`, the floor.
//! 5. The modulo that should have been the left operand.

use rts_isolated::{measure, opaque, report};
use std::sync::OnceLock;

static ASKED: OnceLock<bool> = OnceLock::new();

#[inline(always)]
fn asked() -> bool {
    *ASKED.get_or_init(|| std::env::var_os("RTS_ISOLATED_ABSENT_SWITCH").is_some())
}

fn main() {
    // Present for row 2. Set from inside the process so the experiment does not
    // depend on how it was launched.
    unsafe {
        std::env::set_var("RTS_ISOLATED_PRESENT_SWITCH", "1");
    }

    let mut resolves: u64 = 0;

    let rows = vec![
        measure("1. env::var_os, name ABSENT   (engine)", |n| {
            let mut acc = 0u64;
            for _ in 0..n {
                if std::env::var_os(opaque("RTS_ISOLATED_ABSENT_SWITCH")).is_some() {
                    acc = acc.wrapping_add(1);
                }
            }
            acc
        }),
        measure("2. env::var_os, name PRESENT", |n| {
            let mut acc = 0u64;
            for _ in 0..n {
                if std::env::var_os(opaque("RTS_ISOLATED_PRESENT_SWITCH")).is_some() {
                    acc = acc.wrapping_add(1);
                }
            }
            acc
        }),
        measure("3. OnceLock<bool>  (probe::phase's shape)", |n| {
            let mut acc = 0u64;
            for _ in 0..n {
                if opaque(asked()) {
                    acc = acc.wrapping_add(1);
                }
            }
            acc
        }),
        measure("4. a plain bool    (the floor)", |n| {
            let mut acc = 0u64;
            for _ in 0..n {
                if opaque(false) {
                    acc = acc.wrapping_add(1);
                }
            }
            acc
        }),
        measure("5. the counter test that should be first", |n| {
            let mut acc = 0u64;
            for _ in 0..n {
                resolves = resolves.wrapping_add(1);
                if opaque(resolves) % 20_000 <= 1 {
                    acc = acc.wrapping_add(1);
                }
            }
            acc
        }),
    ];

    report("Experiment 7 - a debug switch on a hot path", &rows);
    println!();
    println!("Row 1 is what every by-name property write pays today. Row 3 is what");
    println!("the same switch costs when the answer is remembered, which is the shape");
    println!("`probe::phase` already uses for RTS_TIMING.");
}
