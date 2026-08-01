//! Drivers for the remaining primordials: Array, Object-as-dictionary, Boolean,
//! integer Number.

use crate::emit;
use crate::harness::{Check, Row, report};
use crate::poly;
use crate::rt;
use crate::slab::{self, Entry};

use super::{
    ITERS_ARR, ITERS_OBJ, ITERS_PRIM, ITERS_PRIM_INT, MASK, MASK_PRIM, N_OBJS, SHAPE_ID,
};

// ---------------------------------------------------------------------------
// Array — `s += a[j]; a[j] = a[j] + 1`
// ---------------------------------------------------------------------------

/// Restore the slab-backed array to its initial contents, between timed runs.
fn restore_slab_array(payload: i64, init: &[i64]) {
    slab::sharded::with_mut(payload as u64, |e| {
        if let Some(Entry::Vec(dst)) = e {
            dst.clear();
            dst.extend_from_slice(init);
        }
    });
}

pub fn kernel_arr() {
    slab::sharded::reset();
    slab::arena::reset();

    // One array handle, N elements — the realistic shape (`a[i]` in a loop).
    let init: Vec<i64> = (0..N_OBJS as i64)
        .map(|k| poly::from_f64(k as f64) as i64)
        .collect();
    let arr_payload = slab::sharded::alloc(Entry::Vec(Box::new(init.clone()))) as i64;
    // The skeleton loads `payload` per iteration; feed it the same handle every
    // time so every variant pays the identical dependent load.
    let payload_arr: Vec<i64> = vec![arr_payload; N_OBJS];

    // Boxed-word element array and packed-f64 element array, in plain memory.
    let mut boxed_elems: Vec<i64> = init.clone();
    let mut packed_elems: Vec<f64> = (0..N_OBJS).map(|k| k as f64).collect();

    let hdr_slab = [payload_arr.as_ptr() as i64, 0];
    let hdr_boxed = [payload_arr.as_ptr() as i64, boxed_elems.as_ptr() as i64];
    let hdr_packed = [payload_arr.as_ptr() as i64, packed_elems.as_ptr() as i64];
    let (p_slab, p_boxed, p_packed) = (
        hdr_slab.as_ptr() as i64,
        hdr_boxed.as_ptr() as i64,
        hdr_packed.as_ptr() as i64,
    );

    // Elements are incremented as the loop runs, so the expected sum has to be
    // simulated element by element — this also cross-checks every variant.
    let expect: f64 = {
        let mut e: Vec<f64> = (0..N_OBJS).map(|k| k as f64).collect();
        let mut s = 0.0;
        for i in 0..ITERS_ARR {
            let j = (i & MASK) as usize;
            s += e[j];
            e[j] += 1.0;
        }
        s
    };

    // Two identical closures: `with_setup` takes ownership, and R0 and R1 both
    // need to restore the same slab-backed array.
    let init_r0 = init.clone();
    let init_r1 = init.clone();
    let reset_slab_r0 = move || restore_slab_array(arr_payload, &init_r0);
    let reset_slab_r1 = move || restore_slab_array(arr_payload, &init_r1);
    let boxed_ptr = boxed_elems.as_mut_ptr();
    let packed_ptr = packed_elems.as_mut_ptr();
    let reset_boxed = move || {
        for k in 0..N_OBJS {
            // SAFETY: `boxed_elems` outlives the report call below.
            unsafe { *boxed_ptr.add(k) = poly::from_f64(k as f64) as i64 };
        }
    };
    let reset_packed = move || {
        for k in 0..N_OBJS {
            // SAFETY: as above.
            unsafe { *packed_ptr.add(k) = k as f64 };
        }
    };

    let r0 = emit::kernel_arr::r0_current();
    let r1 = emit::kernel_arr::r1_inline_arith();
    let r2 = emit::kernel_arr::r2_direct_boxed();
    let r3 = emit::kernel_arr::r3_packed_f64();

    report(
        "KERNEL ARR — array element: s += a[j]; a[j] = a[j] + 1, 3M iterations",
        ITERS_ARR,
        expect,
        Check::Poly,
        vec![
            Row::new(
                "R0 today",
                "VEC_GET + VEC_SET locked calls + generic add",
                move || (r0.f)(ITERS_ARR, p_slab, MASK),
            )
            .with_setup(reset_slab_r0),
            Row::new(
                "R1 +inline arith",
                "locked get/set, arithmetic inline",
                move || (r1.f)(ITERS_ARR, p_slab, MASK),
            )
            .with_setup(reset_slab_r1),
            Row::new(
                "R2 +direct, boxed elems",
                "raw load/store of PolyValue words",
                move || (r2.f)(ITERS_ARR, p_boxed, MASK),
            )
            .with_setup(reset_boxed),
            Row::new(
                "R3 +packed f64 elems",
                "untagged f64 elements (V8 PACKED_DOUBLE)",
                move || (r3.f)(ITERS_ARR, p_packed, MASK),
            )
            .with_setup(reset_packed),
        ],
    );
    drop((payload_arr, boxed_elems, packed_elems));
}

// ---------------------------------------------------------------------------
// Object as DICTIONARY vs object as SHAPE
// ---------------------------------------------------------------------------

pub fn kernel_obj() {
    slab::sharded::reset();
    slab::arena::reset();

    const VALUE: f64 = 3.5;
    let dict = rt::dict::new_dict(&[
        ("alpha", poly::from_f64(1.0) as i64),
        ("beta", poly::from_f64(2.0) as i64),
        ("gamma", poly::from_f64(VALUE) as i64),
    ]) as i64;
    let key_handle = rt::strings::new_string(b"gamma") as i64;
    let key_str: &'static str = "gamma";
    let key_ref: &'static &'static str = Box::leak(Box::new(key_str));

    // The same property in a shaped object: slot 0 = shape id, slot 1 = value.
    let shaped = slab::arena::alloc_object(&[poly::from_f64(VALUE) as i64], SHAPE_ID) as i64;

    let payload_dict: Vec<i64> = vec![dict; N_OBJS];
    let payload_shape: Vec<i64> = vec![shaped; N_OBJS];
    let hdr_dict_keyed = [payload_dict.as_ptr() as i64, key_handle];
    let hdr_dict_interned = [
        payload_dict.as_ptr() as i64,
        key_ref as *const &str as i64,
    ];
    let hdr_shape = [payload_shape.as_ptr() as i64, slab::arena::base_addr()];
    let (p0, p1, p2) = (
        hdr_dict_keyed.as_ptr() as i64,
        hdr_dict_interned.as_ptr() as i64,
        hdr_shape.as_ptr() as i64,
    );

    let expect = VALUE * ITERS_OBJ as f64;

    let o0 = emit::kernel_obj::o0_dict_keyed();
    let o1 = emit::kernel_obj::o1_dict_interned_key();
    let o2 = emit::kernel_obj::o2_shape_load(SHAPE_ID);

    report(
        "KERNEL OBJ — one property read, dictionary vs shape, 3M iterations",
        ITERS_OBJ,
        expect,
        Check::Poly,
        vec![
            Row::new(
                "O0 dict + key_text",
                "IndexMap under lock, String alloc per read",
                move || (o0.f)(ITERS_OBJ, p0, MASK),
            ),
            Row::new(
                "O1 dict, interned key",
                "IndexMap under lock, no String alloc",
                move || (o1.f)(ITERS_OBJ, p1, MASK),
            ),
            Row::new(
                "O2 shape + slot load",
                "shape compare + fixed-offset load",
                move || (o2.f)(ITERS_OBJ, p2, MASK),
            ),
        ],
    );
    drop((payload_dict, payload_shape));
}

// ---------------------------------------------------------------------------
// Boolean and integer Number
// ---------------------------------------------------------------------------

pub fn kernel_prim() {
    // Boolean inputs: all non-zero, so every variant yields the same count and
    // the checksum cross-check stays valid. The falsy arm is still emitted.
    let bools: Vec<i64> = (0..N_OBJS)
        .map(|k| poly::from_f64((k + 1) as f64) as i64)
        .collect();
    let hdr_b = [bools.as_ptr() as i64, 0];
    let pb = hdr_b.as_ptr() as i64;

    let t0 = emit::kernel_prim::t0_call_to_boolean();
    let t1 = emit::kernel_prim::t1_inline_guard();
    let t2 = emit::kernel_prim::t2_native_bool();

    report(
        "KERNEL BOOL — s += x ? 1 : 0 on a Tagged value, 20M iterations",
        ITERS_PRIM,
        ITERS_PRIM as f64,
        Check::Int,
        vec![
            Row::new(
                "T0 today",
                "unconditional call __rtsadp_to_boolean",
                move || (t0.f)(ITERS_PRIM, pb, MASK),
            ),
            Row::new(
                "T1 +inline guard",
                "inline double test, call only on miss",
                move || (t1.f)(ITERS_PRIM, pb, MASK),
            ),
            Row::new("T2 proven Repr::Bool", "no test at all", move || {
                (t2.f)(ITERS_PRIM, pb, MASK)
            }),
        ],
    );

    // Integer inputs: tagged int32 words. MASK_PRIM keeps the running sum inside
    // i32 range, so N1's inline arm stays on its fast path for the whole loop
    // instead of silently degrading to the generic call halfway through.
    let ints: Vec<i64> = (0..N_OBJS)
        .map(|k| poly::encode(poly::TAG_INT32, (k as i64 & MASK_PRIM) as u32 as u64) as i64)
        .collect();
    let hdr_i = [ints.as_ptr() as i64, 0];
    let pi = hdr_i.as_ptr() as i64;
    let expect_i: f64 = (0..ITERS_PRIM_INT).map(|i| (i & MASK_PRIM) as f64).sum();

    let n0 = emit::kernel_prim::n0_call_generic();
    let n1 = emit::kernel_prim::n1_inline_int32();
    let n2 = emit::kernel_prim::n2_native_num();

    report(
        "KERNEL INT — s = s + a[j] on tagged int32, 2M iterations",
        ITERS_PRIM_INT,
        expect_i,
        Check::Poly,
        vec![
            Row::new("N0 today", "call __rtsadp_add (via f64 + re-tighten)", move || {
                (n0.f)(ITERS_PRIM_INT, pi, MASK_PRIM)
            }),
            Row::new(
                "N1 +inline int32",
                "tag check + native iadd + rebox (no ovf check)",
                move || (n1.f)(ITERS_PRIM_INT, pi, MASK_PRIM),
            ),
            Row::new("N2 proven Repr::Float64", "plain fadd", move || {
                (n2.f)(ITERS_PRIM_INT, pi, MASK_PRIM)
            }),
        ],
    );
    drop((bools, ints));
}
