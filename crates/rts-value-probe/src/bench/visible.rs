//! Driver for kernel VISIBLE — does making the access a `load` unlock CSE/LICM?

use crate::emit::kernel_visible as kv;
use crate::harness::{Check, Row, report};
use crate::poly;
use crate::slab;

use super::{ITERS_A, SHAPE_ID};

pub fn kernel_visible() {
    slab::sharded::reset();
    slab::arena::reset();

    // ONE object, fixed. Field 1 holds 2.0, so every variant sums the same
    // value and the checksums are directly comparable.
    const FIELD: f64 = 2.0;
    let xw = poly::from_f64(FIELD) as i64;
    let yw = poly::from_f64(3.0) as i64;
    // NB: the two slabs keep independent round-robin shard counters that
    // `reset()` does not clear, so their payloads need not match — each variant
    // gets its own. (An earlier draft asserted they were equal and blew up.)
    let slab_payload = slab::sharded::alloc_object(&[xw, yw], SHAPE_ID) as i64;
    let unlocked_payload = slab::unlocked::alloc_object(&[xw, yw], SHAPE_ID) as i64;
    let arena_off = slab::arena::alloc_object(&[xw, yw], SHAPE_ID) as i64;

    let hdr = [0i64, slab::arena::base_addr()];
    let p = hdr.as_ptr() as i64;

    // CSE group reads the field TWICE per iteration.
    let expect_cse = 2.0 * FIELD * ITERS_A as f64;
    let v0 = kv::v0_cse_call();
    let v1 = kv::v1_cse_load_trusted();
    let v2 = kv::v2_cse_load_readonly();

    report(
        "KERNEL VISIBLE / CSE — same field read TWICE per iteration, 3M iterations",
        ITERS_A,
        expect_cse,
        Check::Poly,
        vec![
            Row::new(
                "V0 today (2 opaque calls)",
                "CSE impossible across a call — both survive",
                move || (v0.f)(ITERS_A, p, slab_payload),
            ),
            Row::new(
                "V1 2 loads, trusted",
                "can the egraph fold them into one?",
                move || (v1.f)(ITERS_A, p, arena_off),
            ),
            Row::new(
                "V2 2 loads, readonly",
                "same, with the no-memory-dependency promise",
                move || (v2.f)(ITERS_A, p, arena_off),
            ),
        ],
    );

    // LICM group reads a loop-INVARIANT field once per iteration.
    let expect_licm = FIELD * ITERS_A as f64;
    let v3 = kv::v3_licm_call();
    let v3u = kv::v3u_licm_call_unlocked();
    let v4 = kv::v4_licm_load_trusted();
    let v5 = kv::v5_licm_load_readonly();
    let v6 = kv::v6_licm_hand_hoisted();

    report(
        "KERNEL VISIBLE / LICM — loop-INVARIANT field read, 3M iterations",
        ITERS_A,
        expect_licm,
        Check::Poly,
        vec![
            Row::new("V3 today (opaque call)", "hoisting impossible", move || {
                (v3.f)(ITERS_A, p, slab_payload)
            }),
            Row::new(
                "V3u call, no lock",
                "isolates 'the lock blocks it' from 'the CALL blocks it'",
                move || (v3u.f)(ITERS_A, p, unlocked_payload),
            ),
            Row::new("V4 load, trusted", "does Cranelift hoist it?", move || {
                (v4.f)(ITERS_A, p, arena_off)
            }),
            Row::new("V5 load, readonly", "does the flag change the answer?", move || {
                (v5.f)(ITERS_A, p, arena_off)
            }),
            Row::new(
                "V6 hand-hoisted",
                "read once before the loop — what LICM would produce",
                move || (v6.f)(ITERS_A, p, arena_off),
            ),
        ],
    );
}
