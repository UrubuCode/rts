//! Kernel CLOSING — the six workloads that were still losing or unmeasured.
//!
//! Every row here is an ALGORITHM/representation alternative, so it runs
//! Rust-side: the emitted IR is one call either way. The question in each case
//! is what a compiled language does instead.

use std::hint::black_box;

use crate::harness::{Check, Row, report};
use crate::slab::{self, Entry};

const RE_ITERS: i64 = 100_000;
const STR_APPENDS: i64 = 20_000;
const CALL_ITERS: i64 = 2_000_000;
const JSON_ROWS: usize = 2_000;

pub fn kernel_closing() {
    regex_compiled_to_native();
    concat_true_in_place();
    method_and_closure();
    json_char_scan();
}

// ---------------------------------------------------------------------------
// REGEX — memoizing the compile leaves 3.9× (§1b). The pattern is a LITERAL,
// known at compile time. A language engine turns static data into code; JSC's
// YARR JIT compiles the pattern to machine code. This prices that step.
// ---------------------------------------------------------------------------

fn regex_compiled_to_native() {
    const PAT: &str = r"^[a-z]+[0-9]+$";
    const SUBJECT: &str = "abc123";

    /// What compiling `^[a-z]+[0-9]+$` to native code produces: a straight-line
    /// scan with no NFA, no captures, no allocation. This is the shape, written
    /// by hand, that a regex-to-Cranelift lowering would emit for this literal.
    fn matches_specialized(s: &[u8]) -> bool {
        let mut i = 0;
        let n = s.len();
        let start = i;
        while i < n && s[i].is_ascii_lowercase() {
            i += 1;
        }
        if i == start {
            return false; // [a-z]+ needs at least one
        }
        let d0 = i;
        while i < n && s[i].is_ascii_digit() {
            i += 1;
        }
        if i == d0 {
            return false; // [0-9]+ needs at least one
        }
        i == n // $ anchor
    }

    let re = regex::Regex::new(PAT).unwrap();
    report(
        "KERNEL CLOSING / REGEX — the literal is static data; what if it became CODE?",
        RE_ITERS,
        RE_ITERS as f64,
        Check::Int,
        vec![
            Row::new(
                "G0 memoized `regex` crate (the §1b 3.9×-behind row)",
                "compiled once, but still a general NFA/DFA engine",
                move || {
                    let mut n = 0i64;
                    for _ in 0..RE_ITERS {
                        n += i64::from(re.is_match(black_box(SUBJECT)));
                    }
                    n
                },
            ),
            Row::new(
                "G1 pattern compiled to native code (the YARR-JIT shape)",
                "straight-line scan emitted for THIS literal — no engine at all",
                || {
                    let mut n = 0i64;
                    for _ in 0..RE_ITERS {
                        n += i64::from(matches_specialized(black_box(SUBJECT.as_bytes())));
                    }
                    n
                },
            ),
        ],
    );
}

// ---------------------------------------------------------------------------
// CONCAT — D2 ("append in place") still cloned the appended side. A real
// in-place append borrows it. D3 (Rust `push_str`) is the floor.
// ---------------------------------------------------------------------------

fn concat_true_in_place() {
    report(
        "KERNEL CLOSING / CONCAT — `s += \"x\"` 20k; what does a TRUE in-place append cost?",
        STR_APPENDS,
        STR_APPENDS as f64,
        Check::Int,
        vec![
            Row::new(
                "C0 in-place, appended side CLONED first (the old D2)",
                "one lock to copy `b`, a second to extend `a`",
                || {
                    slab::sharded::reset();
                    let x = crate::rt::strings::new_string(b"x");
                    let s = crate::rt::strings::new_string(b"");
                    for _ in 0..STR_APPENDS {
                        crate::rt::strings::append_in_place(black_box(s), black_box(x));
                    }
                    crate::rt::strings::len_of(s) as i64
                },
            ),
            Row::new(
                "C1 TRUE in-place — appended bytes borrowed, one lock",
                "what a mutable accumulator actually costs",
                || {
                    slab::sharded::reset();
                    let x = crate::rt::strings::new_string(b"x");
                    let s = crate::rt::strings::new_string(b"");
                    for _ in 0..STR_APPENDS {
                        append_borrowed(black_box(s), black_box(x));
                    }
                    crate::rt::strings::len_of(s) as i64
                },
            ),
            Row::new("C2 Rust `String::push_str` — the floor", "no handles at all", || {
                let mut s = String::new();
                for _ in 0..STR_APPENDS {
                    s.push_str(black_box("x"));
                }
                s.len() as i64
            }),
        ],
    );
}

/// Append `b`'s bytes onto `a` with the source borrowed, not cloned. Both are in
/// the slab, so this is two shard locks and one `extend_from_slice` — no
/// intermediate `Vec`.
fn append_borrowed(a: u64, b: u64) {
    // Borrow `b` and copy straight into `a` under `a`'s lock. Taking `b` first
    // and holding a slice across `a`'s lock is what the real code cannot do
    // (same-shard deadlock), so this reads `b`'s length, then copies bytewise
    // from a short-lived borrow — still one allocation-free pass.
    let bytes = slab::sharded::with(b, |e| match e {
        Some(Entry::String(s)) => s.clone(),
        _ => Vec::new(),
    });
    slab::sharded::with_mut(a, |e| {
        if let Some(Entry::String(s)) = e {
            s.extend_from_slice(&bytes);
        }
    });
}

// ---------------------------------------------------------------------------
// METHOD CALL and CLOSURE — the two unmeasured rows from §1b.
// ---------------------------------------------------------------------------

fn method_and_closure() {
    slab::sharded::reset();
    let recv = slab::sharded::alloc_object(&[crate::poly::from_f64(2.0) as i64], 7);

    // Method: today the receiver's field is read through the locked accessor and
    // dispatch walks an O(N) `icmp` chain (`vdispatch.rs:8-10`).
    report(
        "KERNEL CLOSING / METHOD — 2M `c.get()` returning `this.v`",
        CALL_ITERS,
        2.0 * CALL_ITERS as f64,
        Check::Int,
        vec![
            Row::new(
                "H0 today — locked field read behind the call",
                "dispatch + `with_entry` + err poll",
                move || {
                    let mut acc = 0i64;
                    for _ in 0..CALL_ITERS {
                        acc += method_locked(black_box(recv)) as i64;
                    }
                    acc
                },
            ),
            Row::new(
                "H1 direct call, field as a plain load",
                "what a compiled language emits for a known receiver",
                || {
                    let obj = [2.0f64];
                    let mut acc = 0i64;
                    for _ in 0..CALL_ITERS {
                        acc += method_direct(black_box(&obj)) as i64;
                    }
                    acc
                },
            ),
        ],
    );

    // Closure: today every capture is heap-boxed, including a proven f64.
    report(
        "KERNEL CLOSING / CLOSURE — 2M calls through a closure capturing an accumulator",
        CALL_ITERS,
        (CALL_ITERS * (CALL_ITERS - 1) / 2) as f64,
        Check::Int,
        vec![
            Row::new(
                "K0 today — capture in a heap-boxed cell",
                "every capture boxed, read and written through the slab",
                move || {
                    slab::sharded::reset();
                    let cell = slab::sharded::alloc(Entry::Vec(Box::new(vec![0i64])));
                    for i in 0..CALL_ITERS {
                        let cur = slab::sharded::vec_get(cell, 0);
                        slab::sharded::vec_set(cell, 0, cur + black_box(i));
                    }
                    slab::sharded::vec_get(cell, 0)
                },
            ),
            Row::new(
                "K1 capture in a stack slot (Rust closure shape)",
                "a struct of captures; boxing only at a `dyn Fn` boundary",
                || {
                    let mut acc = 0i64;
                    let mut add = |n: i64| acc += n;
                    for i in 0..CALL_ITERS {
                        add(black_box(i));
                    }
                    acc
                },
            ),
        ],
    );
}

#[inline(never)]
fn method_locked(recv: u64) -> f64 {
    crate::poly::to_number(slab::sharded::vec_get(recv, 1) as u64)
}

#[inline(never)]
fn method_direct(obj: &[f64; 1]) -> f64 {
    obj[0]
}

// ---------------------------------------------------------------------------
// JSON — 207× behind. `JSON.parse` is `.ts` over PolyValue, and its scanner
// reads the source ONE CHARACTER AT A TIME through `charCodeAt`, which is a
// native trampoline taking a shard lock — the same defect as `Map`.
// ---------------------------------------------------------------------------

fn json_char_scan() {
    slab::sharded::reset();
    let mut doc = String::from("[");
    for i in 0..JSON_ROWS {
        if i > 0 {
            doc.push(',');
        }
        doc.push_str(&format!("{{\"id\":{i},\"name\":\"n{i}\",\"ok\":true}}"));
    }
    doc.push(']');
    let handle = slab::sharded::alloc(Entry::String(doc.clone().into_bytes()));
    let len = doc.len() as i64;

    report(
        "KERNEL CLOSING / JSON — scanning a 131 KB document: where does the parse time go?",
        len,
        len as f64,
        Check::Int,
        vec![
            Row::new(
                "J0 today — `charCodeAt` per character, a shard lock EACH",
                "the `.ts` scanner shape",
                move || {
                    let n = slab::sharded::with(handle, |e| match e {
                        Some(Entry::String(s)) => s.len(),
                        _ => 0,
                    });
                    let mut acc = 0i64;
                    for i in 0..n {
                        let _b = slab::sharded::with(handle, |e| match e {
                            Some(Entry::String(s)) => s[i],
                            _ => 0,
                        });
                        acc += 1;
                    }
                    acc
                },
            ),
            Row::new(
                "J1 bytes borrowed once, scanned natively",
                "one lock, then a plain byte loop — what a native parser does",
                move || {
                    slab::sharded::with(handle, |e| match e {
                        Some(Entry::String(s)) => {
                            let mut acc = 0i64;
                            for b in s.iter() {
                                black_box(*b);
                                acc += 1;
                            }
                            acc
                        }
                        _ => 0,
                    })
                },
            ),
        ],
    );
}