//! Kernel COMPLEXITY — is the JSON/Map gap CODEGEN or ALGORITHM?
//!
//! The question that forced this kernel: Cranelift self-reports ~14% behind an
//! LLVM-based compiler. A 207× gap on `JSON` and a 47× gap on `Map` therefore
//! CANNOT be codegen quality. Either each operation does far more work than it
//! looks like, or the operation count grows faster than the input.
//!
//! Reading the two scanners answers it, and this kernel prices what they do:
//!
//! * `json.ts:238` scans with `const c = this.#s[this.#i]` and then compares
//!   `c === " "`. In JS, `s[i]` yields a ONE-CHARACTER STRING. In RTS that is
//!   `intern_utf16_unit` → `String::from_utf16_lossy` → `intern_poly`, and
//!   `abi_adapter.rs:62-67` records that `intern_poly` does NOT intern: it
//!   allocates a fresh string and a fresh handle on every call. So the scanner
//!   pays a heap allocation and a HandleTable slot PER CHARACTER, plus four
//!   string comparisons per whitespace character.
//! * `map_set.ts:323` hashes with `k.charCodeAt(i)` in a `.ts` loop — no
//!   allocation, but a native trampoline per character.
//!
//! Both have an allocation-free alternative expressible in the SAME `.ts`, so
//! this measures the three shapes against each other.

use std::hint::black_box;

use crate::harness::{Check, Row, report};
use crate::slab::{self, Entry};

/// A JSON-document-sized scan, the size `bench/cross_runtime_gap.ts` uses.
const DOC: usize = 131_072;
/// Sizes double, so a flat ns/char means O(1) and a doubling means O(n).
const SIZES: [usize; 4] = [4_096, 16_384, 65_536, 131_072];

pub fn kernel_complexity() {
    char_scan_shapes();
    ascii_cache_thrash();
}

// ---------------------------------------------------------------------------
// The scanner shapes: `s[i]` vs `charCodeAt(i)` vs borrowed bytes.
// ---------------------------------------------------------------------------

fn char_scan_shapes() {
    let doc: String = "a".repeat(DOC);
    slab::sharded::reset();
    let h = slab::sharded::alloc(Entry::String(doc.into_bytes()));

    report(
        "KERNEL COMPLEXITY / SCAN — what `json.ts` pays PER CHARACTER, three ways",
        DOC as i64,
        DOC as f64,
        Check::Int,
        vec![
            Row::new(
                "Y0 `s[i]` — a fresh 1-char STRING per character (json.ts:238)",
                "intern_utf16_unit → from_utf16_lossy → intern_poly (allocates!), then 4 `===`",
                move || {
                    let n = str_len(h);
                    let mut acc = 0i64;
                    for i in 0..n {
                        let c = index_as_string(h, i);
                        // `c === " " || c === "\t" || c === "\n" || c === "\r"`
                        acc += i64::from(
                            str_eq(c, b" ")
                                || str_eq(c, b"\t")
                                || str_eq(c, b"\n")
                                || str_eq(c, b"\r"),
                        );
                    }
                    acc + n as i64
                },
            ),
            Row::new(
                "Y1 `s.charCodeAt(i)` — a NUMBER per character (map_set.ts:323)",
                "same native trampoline and shard lock, but no allocation",
                move || {
                    let n = str_len(h);
                    let mut acc = 0i64;
                    for i in 0..n {
                        let c = char_code_at(h, i);
                        acc += i64::from(c == 32 || c == 9 || c == 10 || c == 13);
                    }
                    acc + n as i64
                },
            ),
            Row::new(
                "Y2 bytes borrowed once, scanned natively",
                "one lock, then a plain byte loop — what a native parser does",
                move || {
                    slab::sharded::with(h, |e| match e {
                        Some(Entry::String(s)) => {
                            let mut acc = 0i64;
                            for &b in s.iter() {
                                acc += i64::from(
                                    b == b' ' || b == b'\t' || b == b'\n' || b == b'\r',
                                );
                            }
                            acc + s.len() as i64
                        }
                        _ => 0,
                    })
                },
            ),
        ],
    );
}

fn str_len(h: u64) -> usize {
    slab::sharded::with(h, |e| match e {
        Some(Entry::String(s)) => s.len(),
        _ => 0,
    })
}

/// `s[i]` — resolve the unit, then build a one-character string and register it
/// as a NEW handle, which is what `intern_poly` does per `abi_adapter.rs:62-67`.
fn index_as_string(h: u64, i: usize) -> u64 {
    let unit = slab::sharded::with(h, |e| match e {
        Some(Entry::String(s)) => s.get(i).copied().unwrap_or(0),
        _ => 0,
    });
    let s = String::from_utf16_lossy(&[u16::from(unit)]);
    slab::sharded::alloc(Entry::String(s.into_bytes()))
}

/// `s.charCodeAt(i)` — the same lock and trampoline, returning a number.
fn char_code_at(h: u64, i: usize) -> i64 {
    slab::sharded::with(h, |e| match e {
        Some(Entry::String(s)) => i64::from(s.get(i).copied().unwrap_or(0)),
        _ => -1,
    })
}

/// `a === b` for strings: resolve both handles and compare the bytes.
fn str_eq(a: u64, lit: &[u8]) -> bool {
    slab::sharded::with(a, |e| match e {
        Some(Entry::String(s)) => s.as_slice() == lit,
        _ => false,
    })
}

// ---------------------------------------------------------------------------
// The one-entry ASCII cache (`dyndispatch.rs:278-289`). Its comment justifies a
// single entry with "o padrão de uso é varrer uma string do início ao fim". A
// parser does NOT do that — it alternates between the source and the strings it
// builds. Every alternation evicts the entry, and the next source access
// recomputes `bytes.is_ascii()` over the WHOLE document.
//
// This row is the test of whether that turns the scan quadratic. Sizes double;
// if the thrashing row's ns/char doubles with them, it is O(n) per access.
// ---------------------------------------------------------------------------

thread_local! {
    static CACHE: std::cell::Cell<(usize, usize, bool)> =
        const { std::cell::Cell::new((0, 0, false)) };
}

fn is_ascii_cached(bytes: &[u8]) -> bool {
    let key = (bytes.as_ptr() as usize, bytes.len());
    CACHE.with(|c| {
        let (p, l, a) = c.get();
        if p == key.0 && l == key.1 {
            return a;
        }
        let a = bytes.is_ascii();
        c.set((key.0, key.1, a));
        a
    })
}

fn ascii_cache_thrash() {
    for &n in SIZES.iter() {
        let doc: String = "a".repeat(n);
        // A second, short string standing for the one the parser just built.
        let other = String::from("key");
        slab::sharded::reset();
        let h = slab::sharded::alloc(Entry::String(doc.into_bytes()));
        let o = slab::sharded::alloc(Entry::String(other.into_bytes()));

        report(
            &format!("KERNEL COMPLEXITY / ASCII CACHE — {n} chars: does the 1-entry cache hold?"),
            n as i64,
            n as f64,
            Check::Int,
            vec![
                Row::new(
                    "Z0 straight scan — the access pattern the cache assumes",
                    "same string every access, so the entry always hits",
                    move || {
                        let mut acc = 0i64;
                        for i in 0..n {
                            acc += i64::from(cached_unit_at(h, black_box(i)) != 0);
                        }
                        acc
                    },
                ),
                Row::new(
                    "Z1 THRASHED — one other string touched between source reads",
                    "what a parser does; each miss rescans the WHOLE document",
                    move || {
                        let mut acc = 0i64;
                        for i in 0..n {
                            acc += i64::from(cached_unit_at(h, black_box(i)) != 0);
                            // the parser touches the key/value string it built
                            let _ = cached_unit_at(o, 0);
                        }
                        acc
                    },
                ),
            ],
        );
    }
}

/// `utf16_unit_at` in the `dyndispatch.rs:252-267` shape: resolve the handle,
/// consult the one-entry ASCII memo, then index.
fn cached_unit_at(h: u64, i: usize) -> u16 {
    slab::sharded::with(h, |e| match e {
        Some(Entry::String(s)) => {
            if is_ascii_cached(s) {
                return u16::from(s.get(i).copied().unwrap_or(0));
            }
            match std::str::from_utf8(s) {
                Ok(t) => t.encode_utf16().nth(i).unwrap_or(0),
                Err(_) => 0,
            }
        }
        _ => 0,
    })
}
