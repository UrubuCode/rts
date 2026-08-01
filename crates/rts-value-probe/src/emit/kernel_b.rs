//! Kernel B — the FULL objbench loop, allocation included:
//! `for i in 0..n { const p = new P(i, i+1); s = s + p.x * p.y }`.
//!
//! B0 is today's profile end to end. B1 is "nursery bump + direct addressing +
//! proven Repr" — every lever except escape analysis. B2 is A5, i.e. what escape
//! analysis would leave behind (`FUTURE_OPTIMIZATION.md:132` claims 13 of the 17
//! in-loop calls vanish; B0→B2 is the size of that claim).

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, MemFlags, Type, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::{Compiled, Imports, call1, compile, emit_box_double, emit_unbox_double};

const ALLOC_LOCKED: (&str, usize) = ("probe_alloc_locked", 3);
const ALLOC_BUMP: (&str, usize) = ("probe_alloc_bump", 3);
const VEC_GET_LOCKED: (&str, usize) = ("probe_vec_get_locked", 2);
const ADP_MUL: (&str, usize) = ("probe_adp_mul", 2);
const ADP_ADD: (&str, usize) = ("probe_adp_add", 2);

const SLOT_X: i64 = 1;
const SLOT_Y: i64 = 2;

struct AllocVals {
    arena_base: Value,
    i: Value,
    s: Value,
    shape: Value,
}

/// Same skeleton as kernel A, minus the object array: the object is created
/// inside the loop, so there is nothing to index.
fn alloc_kernel<F>(name: &str, needed: &[(&'static str, usize)], s_ty: Type, body: F) -> Compiled
where
    F: FnOnce(&mut FunctionBuilder, &Imports, AllocVals) -> Value + Copy,
{
    compile(name, needed, move |fb, imports| {
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let iters = fb.block_params(entry)[0];
        let hdr_ptr = fb.block_params(entry)[1];
        let shape = fb.block_params(entry)[2]; // kernel B passes the shape id here

        let arena_base = fb
            .ins()
            .load(types::I64, MemFlags::trusted(), hdr_ptr, 8);

        let header = fb.create_block();
        fb.append_block_param(header, types::I64);
        fb.append_block_param(header, s_ty);
        let lbody = fb.create_block();
        let exit = fb.create_block();
        fb.append_block_param(exit, s_ty);

        let zero = fb.ins().iconst(types::I64, 0);
        let s0 = if s_ty == types::F64 {
            fb.ins().f64const(0.0)
        } else {
            fb.ins().iconst(types::I64, 0)
        };
        fb.ins().jump(header, &[zero.into(), s0.into()]);

        fb.switch_to_block(header);
        let i = fb.block_params(header)[0];
        let s = fb.block_params(header)[1];
        let go = fb.ins().icmp(IntCC::SignedLessThan, i, iters);
        fb.ins().brif(go, lbody, &[], exit, &[s.into()]);

        fb.switch_to_block(lbody);
        fb.seal_block(lbody);
        let s_next = body(
            fb,
            imports,
            AllocVals {
                arena_base,
                i,
                s,
                shape,
            },
        );
        let i_next = fb.ins().iadd_imm(i, 1);
        fb.ins().jump(header, &[i_next.into(), s_next.into()]);
        fb.seal_block(header);

        fb.switch_to_block(exit);
        fb.seal_block(exit);
        let out = fb.block_params(exit)[0];
        let raw = if s_ty == types::F64 {
            emit_box_double(fb, out)
        } else {
            out
        };
        fb.ins().return_(&[raw]);
    })
}

/// `x = i`, `y = i + 1`, both boxed as inline doubles — the field words the
/// constructor stores.
fn ctor_args(fb: &mut FunctionBuilder, i: Value) -> (Value, Value) {
    let xf = fb.ins().fcvt_from_sint(types::F64, i);
    let one = fb.ins().f64const(1.0);
    let yf = fb.ins().fadd(xf, one);
    (emit_box_double(fb, xf), emit_box_double(fb, yf))
}

// ---------------------------------------------------------------------------
// B0 — TODAY: locked slab alloc, locked field reads, generic operators.
// ---------------------------------------------------------------------------

pub fn b0_current() -> Compiled {
    alloc_kernel(
        "b0_current",
        &[ALLOC_LOCKED, VEC_GET_LOCKED, ADP_MUL, ADP_ADD],
        types::I64,
        |fb, im, v| {
            let (x, y) = ctor_args(fb, v.i);
            let p = call1(fb, im["probe_alloc_locked"], &[x, y, v.shape]);
            let sx = fb.ins().iconst(types::I64, SLOT_X);
            let sy = fb.ins().iconst(types::I64, SLOT_Y);
            let rx = call1(fb, im["probe_vec_get_locked"], &[p, sx]);
            let ry = call1(fb, im["probe_vec_get_locked"], &[p, sy]);
            let m = call1(fb, im["probe_adp_mul"], &[rx, ry]);
            call1(fb, im["probe_adp_add"], &[v.s, m])
        },
    )
}

// ---------------------------------------------------------------------------
// B1 — bump alloc into the arena + direct-addressed field reads + inline arith.
// Every lever EXCEPT escape analysis.
// ---------------------------------------------------------------------------

pub fn b1_bump_direct() -> Compiled {
    alloc_kernel("b1_bump_direct", &[ALLOC_BUMP], types::F64, |fb, im, v| {
        let (x, y) = ctor_args(fb, v.i);
        let off = call1(fb, im["probe_alloc_bump"], &[x, y, v.shape]);
        let trusted = MemFlags::trusted();
        let byte_off = fb.ins().imul_imm(off, 8);
        let obj = fb.ins().iadd(v.arena_base, byte_off);
        let rx = fb.ins().load(types::I64, trusted, obj, (8 * SLOT_X) as i32);
        let ry = fb.ins().load(types::I64, trusted, obj, (8 * SLOT_Y) as i32);
        let xf = emit_unbox_double(fb, rx);
        let yf = emit_unbox_double(fb, ry);
        let m = fb.ins().fmul(xf, yf);
        fb.ins().fadd(v.s, m)
    })
}

// ---------------------------------------------------------------------------
// B0f — THE CALIBRATION ROW. B0 emits 5 calls per iteration; the real engine
// emits ELEVEN, named by `RTS_REPR_STATS=1` on `bench/objbench.ts`:
//
//   3x __rtsn_vec_push_by_payload   2x __rtsn_vec_get_by_payload
//   1x __rtsadp_mul                 1x __rtsadp_add
//   1x __rtsn_vec_new_object        1x __rtsadp_obj_set_proto
//   1x __rtsadp_err_pending         (+ the colocated ctor call)
//
// If this row reproduces the engine's measured 577 ns/iter, the probe is
// CALIBRATED and every "after" number in this document can be trusted. If it
// does not, the probe is missing something structural and the gap must be
// found before any "after" number is quoted.
// ---------------------------------------------------------------------------

pub fn b0f_full_call_profile() -> Compiled {
    alloc_kernel(
        "b0f_full_profile",
        &[
            ALLOC_LOCKED,
            VEC_GET_LOCKED,
            ADP_MUL,
            ADP_ADD,
            ("probe_vec_push_locked", 2),
            ("probe_obj_set_proto", 2),
            ("probe_err_pending", 1),

            ("probe_gc_tick", 1),
        ],
        types::I64,
        |fb, im, v| {
            let (x, y) = ctor_args(fb, v.i);
            // vec_new_object + 3 pushes: the shape word and the two fields are
            // pushed one at a time, which is what `obj.rs:97-116` emits.
            let p = call1(fb, im["probe_alloc_locked"], &[x, y, v.shape]);
            let _ = call1(fb, im["probe_vec_push_locked"], &[p, v.shape]);
            let _ = call1(fb, im["probe_vec_push_locked"], &[p, x]);
            let _ = call1(fb, im["probe_vec_push_locked"], &[p, y]);
            // prototype wiring per construction (pre-hoist shape)
            let _ = call1(fb, im["probe_obj_set_proto"], &[p, v.shape]);
            // the two field reads
            let sx = fb.ins().iconst(types::I64, SLOT_X);
            let sy = fb.ins().iconst(types::I64, SLOT_Y);
            let rx = call1(fb, im["probe_vec_get_locked"], &[p, sx]);
            let ry = call1(fb, im["probe_vec_get_locked"], &[p, sy]);
            // generic arithmetic
            let m = call1(fb, im["probe_adp_mul"], &[rx, ry]);
            let acc = call1(fb, im["probe_adp_add"], &[v.s, m]);
            // the post-call error poll
            let z = fb.ins().iconst(types::I64, 0);
            let _ = call1(fb, im["probe_err_pending"], &[z]);

            // the GC tick that fires every 256 allocations

            let _ = call1(fb, im["probe_gc_tick"], &[z]);
            acc
        },
    )
}

// ---------------------------------------------------------------------------
// B1b — B1 plus a real CARD-MARK WRITE BARRIER on each field store, which is
// what a non-moving generational collector must emit once the shard `Mutex` is
// gone (HotSpot measures card marking at ~12 instructions / ~22 cycles). B1
// without it is not an honest end state; this row is.
// ---------------------------------------------------------------------------

pub fn b1b_bump_direct_barrier() -> Compiled {
    alloc_kernel("b1b_bump_barrier", &[ALLOC_BUMP], types::F64, |fb, im, v| {
        let (x, y) = ctor_args(fb, v.i);
        let off = call1(fb, im["probe_alloc_bump"], &[x, y, v.shape]);
        let trusted = MemFlags::trusted();
        let byte_off = fb.ins().imul_imm(off, 8);
        let obj = fb.ins().iadd(v.arena_base, byte_off);
        // Card mark: card index = addr >> 9, then a byte store into the card
        // table. The table lives at arena slot 0 here; in the engine it is a
        // side allocation with a base in a global.
        let card = fb.ins().ushr_imm(obj, 9);
        let card_lo = fb.ins().band_imm(card, 0xFF);
        let card_addr = fb.ins().iadd(v.arena_base, card_lo);
        let one = fb.ins().iconst(types::I8, 1);
        fb.ins().store(trusted, one, card_addr, 0);

        let rx = fb.ins().load(types::I64, trusted, obj, (8 * SLOT_X) as i32);
        let ry = fb.ins().load(types::I64, trusted, obj, (8 * SLOT_Y) as i32);
        let xf = emit_unbox_double(fb, rx);
        let yf = emit_unbox_double(fb, ry);
        let m = fb.ins().fmul(xf, yf);
        fb.ins().fadd(v.s, m)
    })
}

// ---------------------------------------------------------------------------
// B2 — escape analysis removed the object. Identical to A5.
// ---------------------------------------------------------------------------

pub fn b2_scalarized() -> Compiled {
    alloc_kernel("b2_scalarized", &[], types::F64, |fb, _im, v| {
        let xf = fb.ins().fcvt_from_sint(types::F64, v.i);
        let one = fb.ins().f64const(1.0);
        let yf = fb.ins().fadd(xf, one);
        let m = fb.ins().fmul(xf, yf);
        fb.ins().fadd(v.s, m)
    })
}
