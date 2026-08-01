//! Kernel ARR — array element access: `s += a[j]; a[j] = a[j] + 1`.
//!
//! A JS array is `Entry::Vec(Box<Vec<i64>>)` of PolyValue words, reached by
//! handle. `a[j]` is `__RTS_FN_NS_COLLECTIONS_VEC_GET` (`collections/vec.rs:85`)
//! and `a[j] = v` is `VEC_SET` — each a shard-locked extern call, so one
//! read-modify-write element costs TWO locks plus the generic operator's call.
//!
//! The variants ask: how much of that is the lock, how much is the call, and how
//! much is the BOXING of the elements (a `number[]` whose elements are all
//! doubles still stores 8-byte PolyValue words and unboxes each one — V8's
//! PACKED_DOUBLE_ELEMENTS stores raw `f64` and never tags).

use cranelift_codegen::ir::{InstBuilder, MemFlags, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::{Compiled, call1, emit_box_double, emit_unbox_double, loop_kernel};

const VEC_GET_LOCKED: (&str, usize) = ("probe_vec_get_locked", 2);
const VEC_SET_LOCKED: (&str, usize) = ("probe_vec_set_locked", 3);
const ADP_ADD: (&str, usize) = ("probe_adp_add", 2);

/// Element `j` of the array whose handle payload is `arr` — the array handle is
/// loop-invariant here (one array, many elements), which is the realistic shape:
/// `payload` from the shared skeleton IS that handle, repeated.
fn elem_addr(fb: &mut FunctionBuilder, base: Value, idx: Value) -> Value {
    let off = fb.ins().imul_imm(idx, 8);
    fb.ins().iadd(base, off)
}

// --- R0: today -------------------------------------------------------------

pub fn r0_current() -> Compiled {
    loop_kernel(
        "arr_r0_current",
        &[VEC_GET_LOCKED, VEC_SET_LOCKED, ADP_ADD],
        types::I64,
        |fb, im, v| {
            let x = call1(fb, im["probe_vec_get_locked"], &[v.payload, v.idx]);
            let one = fb.ins().f64const(1.0);
            let one_w = emit_box_double(fb, one);
            let inc = call1(fb, im["probe_adp_add"], &[x, one_w]);
            let _ = call1(fb, im["probe_vec_set_locked"], &[v.payload, v.idx, inc]);
            call1(fb, im["probe_adp_add"], &[v.s, x])
        },
    )
}

// --- R1: locked calls, inline arithmetic -----------------------------------

pub fn r1_inline_arith() -> Compiled {
    loop_kernel(
        "arr_r1_inline_arith",
        &[VEC_GET_LOCKED, VEC_SET_LOCKED],
        types::F64,
        |fb, im, v| {
            let x = call1(fb, im["probe_vec_get_locked"], &[v.payload, v.idx]);
            let xf = emit_unbox_double(fb, x);
            let one = fb.ins().f64const(1.0);
            let inc = fb.ins().fadd(xf, one);
            let inc_w = emit_box_double(fb, inc);
            let _ = call1(fb, im["probe_vec_set_locked"], &[v.payload, v.idx, inc_w]);
            fb.ins().fadd(v.s, xf)
        },
    )
}

// --- R2: direct addressing, elements still BOXED PolyValue words -----------

pub fn r2_direct_boxed() -> Compiled {
    loop_kernel("arr_r2_direct_boxed", &[], types::F64, |fb, _im, v| {
        let trusted = MemFlags::trusted();
        let addr = elem_addr(fb, v.arena_base, v.idx);
        let x = fb.ins().load(types::I64, trusted, addr, 0);
        let xf = emit_unbox_double(fb, x);
        let one = fb.ins().f64const(1.0);
        let inc = fb.ins().fadd(xf, one);
        let inc_w = emit_box_double(fb, inc);
        fb.ins().store(trusted, inc_w, addr, 0);
        fb.ins().fadd(v.s, xf)
    })
}

// --- R3: packed f64 elements — no tag at all (V8 PACKED_DOUBLE_ELEMENTS) ----

pub fn r3_packed_f64() -> Compiled {
    loop_kernel("arr_r3_packed_f64", &[], types::F64, |fb, _im, v| {
        let trusted = MemFlags::trusted();
        let addr = elem_addr(fb, v.arena_base, v.idx);
        let xf = fb.ins().load(types::F64, trusted, addr, 0);
        let one = fb.ins().f64const(1.0);
        let inc = fb.ins().fadd(xf, one);
        fb.ins().store(trusted, inc, addr, 0);
        fb.ins().fadd(v.s, xf)
    })
}
