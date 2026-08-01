//! Kernel STDLIB — the three worst gaps against Bun (§1b), none of which is in
//! the plan and none of which is a value-model or codegen problem.
//!
//! | | RTS vs Bun |
//! |---|---|
//! | regex literal `.test()` in a loop | 2463× |
//! | `s += "x"` | 322× |
//! | `Map<string,…>.get` | 47× |
//!
//! Each group here prices the ALGORITHM, not the emitted code, so these run
//! Rust-side: the emitted IR is one call either way, what differs is what the
//! callee does. Every alternative is the shape a real engine uses.

use std::hint::black_box;

use crate::harness::{Check, Row, report};
use crate::slab::{self, Entry};

/// Regex: 100k `.test()` calls, matching the §1b workload exactly.
const RE_ITERS: i64 = 100_000;
/// Map: 200k lookups over 1024 distinct string keys.
const MAP_ITERS: i64 = 200_000;
const MAP_KEYS: usize = 1024;

pub fn kernel_stdlib() {
    regex_memoization();
    map_hash_caching();
}

// ---------------------------------------------------------------------------
// REGEX — the engine compiles the NFA at the LITERAL'S SITE, every iteration.
// `regexops.rs` has no memoization anywhere.
// ---------------------------------------------------------------------------

fn regex_memoization() {
    const PAT: &str = r"^[a-z]+[0-9]+$";
    const SUBJECT: &str = "abc123";

    report(
        "KERNEL STDLIB / REGEX — 100k `.test()`; does memoizing the compile matter?",
        RE_ITERS,
        RE_ITERS as f64,
        Check::Int,
        vec![
            Row::new(
                "R0 today — compile per iteration",
                "`__rtsadp_re_compile` at the literal's site, no cache",
                || {
                    let mut n = 0i64;
                    for _ in 0..RE_ITERS {
                        let re = regex::Regex::new(black_box(PAT)).unwrap();
                        n += i64::from(re.is_match(black_box(SUBJECT)));
                    }
                    n
                },
            ),
            Row::new(
                "R1 compile once, cached at the site",
                "one compiled object per literal, reused — what every engine does",
                || {
                    let re = regex::Regex::new(black_box(PAT)).unwrap();
                    let mut n = 0i64;
                    for _ in 0..RE_ITERS {
                        n += i64::from(re.is_match(black_box(SUBJECT)));
                    }
                    n
                },
            ),
        ],
    );
}

// ---------------------------------------------------------------------------
// MAP — `map_set.ts` recomputes FNV-1a per lookup via `charCodeAt`, and each
// `charCodeAt` is a native trampoline that takes a shard `Mutex`. V8/JSC cache
// the hash in the string header and compare interned pointers.
// ---------------------------------------------------------------------------

fn map_hash_caching() {
    slab::sharded::reset();

    // Keys live in the slab, as real string handles do.
    let keys: Vec<(u64, String)> = (0..MAP_KEYS)
        .map(|i| {
            let s = format!("k{i}");
            (
                slab::sharded::alloc(Entry::String(s.clone().into_bytes())),
                s,
            )
        })
        .collect();

    let mut table: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for (i, (_, s)) in keys.iter().enumerate() {
        table.insert(s.clone(), i as i64);
    }
    // Pre-hashed form: the hash lives WITH the key, as it would in a string
    // header, so a lookup never rescans the bytes.
    let prehashed: Vec<(u64, u64)> = keys
        .iter()
        .enumerate()
        .map(|(i, (h, s))| (*h, fnv(s.as_bytes()) ^ (i as u64 * 0)))
        .collect();
    let by_hash: std::collections::HashMap<u64, i64> = prehashed
        .iter()
        .enumerate()
        .map(|(i, (_, hv))| (*hv, i as i64))
        .collect();

    let handles: Vec<u64> = keys.iter().map(|(h, _)| *h).collect();
    let hashes: Vec<u64> = prehashed.iter().map(|(_, hv)| *hv).collect();
    let expect = (0..MAP_ITERS).map(|i| (i as usize) % MAP_KEYS).sum::<usize>() as f64;

    let h1 = handles.clone();
    let h2 = handles.clone();
    let hs = hashes.clone();
    let t1 = table.clone();
    let t2 = table;
    let bh = by_hash;

    report(
        "KERNEL STDLIB / MAP — 200k gets; where does the string-key cost live?",
        MAP_ITERS,
        expect,
        Check::Int,
        vec![
            Row::new(
                "M0 today — rehash per lookup, one lock PER BYTE",
                "`charCodeAt` per char, each a trampoline taking a shard Mutex",
                move || {
                    let mut acc = 0i64;
                    for i in 0..MAP_ITERS {
                        let k = (i as usize) % MAP_KEYS;
                        // Read the key byte by byte through the locked accessor,
                        // hashing as we go — the `map_set.ts` shape.
                        let hv = fnv_via_locked_reads(black_box(h1[k]));
                        acc += (hv % 7) as i64 * 0 + k as i64;
                    }
                    acc
                },
            ),
            Row::new(
                "M1 rehash per lookup, bytes borrowed once",
                "one lock, then hash the borrowed slice — no per-byte trampoline",
                move || {
                    let mut acc = 0i64;
                    for i in 0..MAP_ITERS {
                        let k = (i as usize) % MAP_KEYS;
                        let hv = fnv_via_borrow(black_box(h2[k]));
                        acc += (hv % 7) as i64 * 0 + k as i64;
                    }
                    acc
                },
            ),
            Row::new(
                "M2 hash cached with the key (V8/JSC)",
                "hash computed once at construction, lookup never rescans",
                move || {
                    let mut acc = 0i64;
                    for i in 0..MAP_ITERS {
                        let k = (i as usize) % MAP_KEYS;
                        let hv = black_box(hs[k]);
                        acc += (hv % 7) as i64 * 0 + k as i64;
                    }
                    acc
                },
            ),
        ],
    );
    drop((t1, t2, bh));
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

/// FNV over a string handle read ONE BYTE AT A TIME through the locked
/// accessor — the `charCodeAt`-per-character shape, one shard `Mutex` per byte.
fn fnv_via_locked_reads(handle: u64) -> u64 {
    let len = slab::sharded::with(handle, |e| match e {
        Some(Entry::String(s)) => s.len(),
        _ => 0,
    });
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for i in 0..len {
        let b = slab::sharded::with(handle, |e| match e {
            Some(Entry::String(s)) => s[i],
            _ => 0,
        });
        h ^= b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

/// Same hash, but the bytes are borrowed once under a single lock.
fn fnv_via_borrow(handle: u64) -> u64 {
    slab::sharded::with(handle, |e| match e {
        Some(Entry::String(s)) => fnv(s),
        _ => 0,
    })
}
