//! Kernel FINAL — the two workloads that were still losing.
//!
//! Both get the treatment kernel B got: a CALIBRATION row that emits the
//! engine's full per-operation work, so the "after" number is derived from a
//! replica that reproduces the engine's own measurement rather than from a
//! multiplication of factors taken from different kernels.

use std::hint::black_box;

use crate::harness::{Check, Row, report};
use crate::slab::{self, Entry};

const CALL_ITERS: i64 = 2_000_000;
const JSON_ROWS: usize = 2_000;

pub fn kernel_final() {
    method_calibrated();
    json_full_pipeline();
}

// ---------------------------------------------------------------------------
// METHOD — engine measures 31 ms / 2M = 15.5 ns/iter for `c.get()` returning
// `this.v`. The earlier H0 replica read 4.61 ns because it only did the locked
// field read. The engine also walks an O(N) `icmp` dispatch chain
// (`vdispatch.rs:8-10`), goes through the uniform thunk, and polls the error
// slot afterwards. This row emits all of it.
// ---------------------------------------------------------------------------

fn method_calibrated() {
    slab::sharded::reset();
    let recv = slab::sharded::alloc_object(&[crate::poly::from_f64(2.0) as i64], 7);
    // Four candidate classes, as a small hierarchy produces; the chain compares
    // the receiver's shape word against each in turn.
    let candidates: [i64; 4] = [3, 5, 7, 11];

    report(
        "KERNEL FINAL / METHOD — calibrated: does the full engine profile reach 15.5 ns/iter?",
        CALL_ITERS,
        2.0 * CALL_ITERS as f64,
        Check::Int,
        vec![
            Row::new(
                "P0f FULL engine profile (dispatch chain + thunk + field + poll)",
                "the calibration row",
                move || {
                    let mut acc = 0i64;
                    for _ in 0..CALL_ITERS {
                        acc += method_full_profile(black_box(recv), black_box(&candidates)) as i64;
                    }
                    acc
                },
            ),
            Row::new(
                "P1 static dispatch, field as a load, no poll",
                "what a compiled language emits for a receiver of known class",
                || {
                    let obj = [2.0f64];
                    let mut acc = 0i64;
                    for _ in 0..CALL_ITERS {
                        acc += method_static(black_box(&obj)) as i64;
                    }
                    acc
                },
            ),
        ],
    );
}

/// The engine's actual per-call work: read the shape word (locked), walk the
/// candidate chain, go through the uniform thunk, read the field (locked), poll
/// the error slot.
#[inline(never)]
fn method_full_profile(recv: u64, candidates: &[i64; 4]) -> f64 {
    // 1. shape word, through the locked accessor
    let shape = slab::sharded::vec_get(recv, 0);
    // 2. O(N) icmp chain over candidate classes
    let mut hit = usize::MAX;
    for (i, c) in candidates.iter().enumerate() {
        if shape == *c {
            hit = i;
            break;
        }
    }
    if hit == usize::MAX {
        return 0.0;
    }
    // 3. the uniform thunk: 5 slots, tagged in and out
    let r = thunk_invoke(0, recv as i64, 0, 0, 0);
    // 4. the post-call error poll
    let _ = crate::rt::values::probe_err_pending(0);
    crate::poly::to_number(r as u64)
}

/// The uniform 5-slot thunk the engine routes an instance method through; the
/// body reads `this.v` via the locked accessor.
#[inline(never)]
fn thunk_invoke(_env: i64, this: i64, _a: i64, _b: i64, _c: i64) -> i64 {
    slab::sharded::vec_get(this as u64, 1)
}

#[inline(never)]
fn method_static(obj: &[f64; 1]) -> f64 {
    obj[0]
}

// ---------------------------------------------------------------------------
// JSON — engine measures 760 ms for parse+stringify of a 131 KB doc ×5. Only
// the SCANNER was measured before (16×). This row runs the whole pipeline:
// scan, build one object per row, and serialise back — first in the engine's
// shape, then in the shape a compiled language uses.
// ---------------------------------------------------------------------------

fn json_full_pipeline() {
    let mut doc = String::from("[");
    for i in 0..JSON_ROWS {
        if i > 0 {
            doc.push(',');
        }
        doc.push_str(&format!("{{\"id\":{i},\"name\":\"n{i}\",\"ok\":true}}"));
    }
    doc.push(']');
    let doc_len = doc.len();
    let d1 = doc.clone();
    let d2 = doc;

    report(
        "KERNEL FINAL / JSON — the WHOLE pipeline: scan + build objects + serialise",
        JSON_ROWS as i64,
        JSON_ROWS as f64,
        Check::Int,
        vec![
            Row::new(
                "Q0 engine shape — locked byte scan, slab objects, snapshot concat",
                "charCodeAt per byte, one slab object per row, concat via clone",
                move || {
                    slab::sharded::reset();
                    let h = slab::sharded::alloc(Entry::String(d1.clone().into_bytes()));
                    json_pipeline_engine_shape(h)
                },
            ),
            Row::new(
                "Q1 native shape — borrowed scan, bump objects, in-place build",
                "one lock for the source, objects in an arena, one output buffer",
                move || {
                    slab::arena::reset();
                    json_pipeline_native_shape(black_box(&d2))
                },
            ),
        ],
    );
    let _ = doc_len;
}

/// A process-global `Mutex` standing in for `heap::shapes`'s registry, which is
/// taken on every dynamic property resolution (`shapes/mod.rs:76-79`).
fn shape_registry() -> &'static std::sync::Mutex<Vec<Vec<String>>> {
    static R: std::sync::OnceLock<std::sync::Mutex<Vec<Vec<String>>>> =
        std::sync::OnceLock::new();
    R.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

/// `intern_poly` — named "intern" but `abi_adapter.rs:62-67` records that it is
/// NOT one: it allocates a fresh string and a fresh handle on every call.
fn intern_poly_not_really(key: &str) -> u64 {
    slab::sharded::alloc(Entry::String(key.as_bytes().to_vec()))
}

/// `key_text` — resolves a key handle into an OWNED `String`, a malloc per
/// property touch (`objops.rs:221`).
fn key_text(handle: u64) -> String {
    slab::sharded::with(handle, |e| match e {
        Some(Entry::String(s)) => String::from_utf8_lossy(s).into_owned(),
        _ => String::new(),
    })
}

/// A property set the way the dynamic path does it: resolve the key to an owned
/// `String`, take the GLOBAL shape mutex to find/extend the shape, then write
/// the slot under the object's shard lock.
fn obj_set_dynamic(obj: u64, key_handle: u64, slot: usize, value: i64) {
    let k = key_text(key_handle);
    {
        let mut reg = shape_registry().lock().unwrap();
        if reg.is_empty() {
            reg.push(Vec::new());
        }
        let keys = &mut reg[0];
        if !keys.iter().any(|e| e == &k) {
            keys.push(k);
        }
    }
    slab::sharded::vec_set(obj, slot as i64, value);
}

/// The engine's shape, FAITHFUL this time. Per row the engine does: a locked
/// read per source byte, one slab object, and for each of the three keys an
/// `intern_poly` (fresh string + handle), a `key_text` (owned `String`), and a
/// global-shape-registry lock — then `stringify` reads every key back and
/// concatenates through the snapshot path that copies the accumulator.
fn json_pipeline_engine_shape(src: u64) -> i64 {
    let n = slab::sharded::with(src, |e| match e {
        Some(Entry::String(s)) => s.len(),
        _ => 0,
    });
    const KEYS: [&str; 3] = ["id", "name", "ok"];
    let mut rows = 0i64;
    let mut field = 0i64;
    let mut out = crate::rt::strings::new_string(b"");
    for i in 0..n {
        let b = slab::sharded::with(src, |e| match e {
            Some(Entry::String(s)) => s[i],
            _ => 0,
        });
        if b == b'{' {
            let o = slab::sharded::alloc_object(&[0, 0, 0], 7);
            // three properties, each paying the full dynamic set
            for (slot, k) in KEYS.iter().enumerate() {
                let kh = intern_poly_not_really(k);
                obj_set_dynamic(o, kh, slot + 1, field);
            }
            // stringify: read each key back and append through the concat path
            for k in KEYS.iter() {
                let kh = intern_poly_not_really(k);
                let _ = key_text(kh);
                out = crate::rt::strings::concat_today(out, kh);
            }
            rows += 1;
        }
        if b == b':' {
            field += 1;
        }
    }
    black_box(crate::rt::strings::len_of(out));
    rows
}

/// The native shape: borrow the source once, bump-allocate each row's fields
/// into the arena, and build the output into a single growing buffer.
fn json_pipeline_native_shape(src: &str) -> i64 {
    // Keys are compile-time constants, so they are interned ONCE — not per
    // property, per row. This is the only structural difference that matters.
    const KEYS: [&str; 3] = ["id", "name", "ok"];
    let bytes = src.as_bytes();
    let mut rows = 0i64;
    let mut field = 0i64;
    let mut out = String::with_capacity(src.len());
    for &b in bytes {
        if b == b'{' {
            // one bump allocation carrying all three fields
            slab::arena::alloc_raw(&[field, field, field]);
            // stringify: append each key into the single output buffer
            for k in KEYS.iter() {
                out.push_str(k);
            }
            rows += 1;
        }
        if b == b':' {
            field += 1;
        }
    }
    black_box(out.len());
    rows
}
