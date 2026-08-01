//! Kernel IR — the engine's ACTUAL emitted shape, stripped one defect at a time.
//!
//! Every instruction below was read off `rts ir` for
//! `function sumArray(a: number[], n: number) { let s = 0; for (...) s = s + a[i] }`
//! — see `bench/backend.rs` for how the dump was obtained. That dump is
//! PRE-optimization (`parcompile.rs:338` prints right after `fb.finalize()`, before
//! `define_function` runs the egraph), so reading a cost off it directly would be
//! wrong: the egraph still gets a pass at it.
//!
//! This kernel settles that by MEASURING. `e0` emits the engine's shape verbatim
//! and lets the same optimizer at it; each later row removes exactly one defect.
//! The difference between two adjacent rows is what that defect costs after the
//! optimizer has done whatever it can.
//!
//! | row | what it removes |
//! |---|---|
//! | `e0` | nothing — the engine's emitted IR, verbatim |
//! | `e1` | the `fcvt_from_sint` per iteration (integer loop bound) |
//! | `e2` | the array-read CALL becomes a `load` |
//! | `e3` | the generic-add CALL and its box/unbox round trip |

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{InstBuilder, MemFlags, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::{Compiled, Imports, call1, compile};

const VEC_GET: (&str, usize) = ("probe_vec_get_locked", 2);
const ADP_ADD: (&str, usize) = ("probe_adp_add", 2);

/// The engine's hole check: compare against the HOLE sentinel and substitute
/// undefined. Two constants and a `select`, exactly as `rts ir` shows.
fn hole_check(fb: &mut FunctionBuilder, v: Value) -> Value {
    let hole = fb.ins().iconst(types::I64, -1_688_849_860_263_932);
    let undef = fb.ins().iconst(types::I64, -1_688_849_860_263_936);
    let is_hole = fb.ins().icmp(IntCC::Equal, v, hole);
    fb.ins().select(is_hole, undef, v)
}

/// Box an f64 accumulator the way the engine does: bitcast, then canonicalize a
/// NaN to the positive quiet NaN so a real double never lands in boxed space.
fn box_acc(fb: &mut FunctionBuilder, s: Value) -> Value {
    let bits = fb.ins().bitcast(types::I64, MemFlags::new(), s);
    let is_nan = fb.ins().fcmp(FloatCC::Unordered, s, s);
    let qnan = fb.ins().iconst(types::I64, 0x7ff8_0000_0000_0000u64 as i64);
    fb.ins().select(is_nan, qnan, bits)
}

/// Unbox a PolyValue result into an f64 with the FULL tag check the engine
/// emits: sign-extend the low 32 bits as the int32 arm, bitcast as the f64 arm,
/// then test `(v & BOX_BASE) == BOX_BASE && tag == INT32` to pick between them.
fn unbox_acc(fb: &mut FunctionBuilder, v: Value) -> Value {
    let sh = fb.ins().ishl_imm(v, 32);
    let sext = fb.ins().sshr_imm(sh, 32);
    let as_int = fb.ins().fcvt_from_sint(types::F64, sext);
    let as_f64 = fb.ins().bitcast(types::F64, MemFlags::new(), v);
    let base = fb.ins().iconst(types::I64, crate::poly::BOX_BASE as i64);
    let masked = fb.ins().band(v, base);
    let boxed = fb.ins().icmp(IntCC::Equal, masked, base);
    let tag = fb.ins().ushr_imm(v, 48);
    let tag3 = fb.ins().band_imm(tag, 7);
    let is_int = fb.ins().icmp_imm(IntCC::Equal, tag3, 1);
    let both = fb.ins().band(boxed, is_int);
    fb.ins().select(both, as_int, as_f64)
}

/// The shared loop skeleton. `float_bound` reproduces the engine's
/// `fcvt_from_sint` + `fcmp` comparison against an `n: number`; otherwise the
/// bound is an integer and the compare is a plain `icmp`.
fn ladder<F>(name: &str, needed: &[(&'static str, usize)], float_bound: bool, body: F) -> Compiled
where
    F: FnOnce(&mut FunctionBuilder, &Imports, Value, Value, Value, Value) -> Value + Copy,
{
    compile(name, needed, move |fb, im| {
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let iters = fb.block_params(entry)[0];
        let hdr_ptr = fb.block_params(entry)[1];
        let recv = fb.block_params(entry)[2]; // array handle
        let base = fb.ins().load(types::I64, MemFlags::trusted(), hdr_ptr, 8);
        let n_f = fb.ins().fcvt_from_sint(types::F64, iters);

        let header = fb.create_block();
        fb.append_block_param(header, types::I64);
        fb.append_block_param(header, types::F64);
        let lbody = fb.create_block();
        let exit = fb.create_block();
        fb.append_block_param(exit, types::F64);

        let z = fb.ins().iconst(types::I64, 0);
        let s0 = fb.ins().f64const(0.0);
        fb.ins().jump(header, &[z.into(), s0.into()]);

        fb.switch_to_block(header);
        let i = fb.block_params(header)[0];
        let s = fb.block_params(header)[1];
        let go = if float_bound {
            // The engine's shape: `i` is i64, `n` is f64, so `i` is converted
            // EVERY iteration to make the comparison.
            let i_f = fb.ins().fcvt_from_sint(types::F64, i);
            let c = fb.ins().fcmp(FloatCC::LessThan, i_f, n_f);
            fb.ins().uextend(types::I64, c)
        } else {
            let c = fb.ins().icmp(IntCC::SignedLessThan, i, iters);
            fb.ins().uextend(types::I64, c)
        };
        fb.ins().brif(go, lbody, &[], exit, &[s.into()]);

        fb.switch_to_block(lbody);
        fb.seal_block(lbody);
        let s_next = body(fb, im, recv, base, i, s);
        let i_next = fb.ins().iadd_imm(i, 1);
        fb.ins().jump(header, &[i_next.into(), s_next.into()]);
        fb.seal_block(header);

        fb.switch_to_block(exit);
        fb.seal_block(exit);
        let out = fb.block_params(exit)[0];
        let raw = fb.ins().bitcast(types::I64, MemFlags::new(), out);
        fb.ins().return_(&[raw]);
    })
}

/// The array read as the engine emits it: mask the handle to its 48-bit payload,
/// then an opaque call.
fn elem_call(fb: &mut FunctionBuilder, im: &Imports, recv: Value, i: Value) -> Value {
    let mask = fb.ins().iconst(types::I64, 0xffff_ffff_ffff);
    let payload = fb.ins().band(recv, mask);
    let raw = call1(fb, im["probe_vec_get_locked"], &[payload, i]);
    hole_check(fb, raw)
}

// ---------------------------------------------------------------------------

/// E0 — the engine's emitted IR, verbatim.
pub fn e0_engine() -> Compiled {
    ladder("ir_e0_engine", &[VEC_GET, ADP_ADD], true, |fb, im, recv, _b, i, s| {
        let x = elem_call(fb, im, recv, i);
        let boxed = box_acc(fb, s);
        let sum = call1(fb, im["probe_adp_add"], &[boxed, x]);
        unbox_acc(fb, sum)
    })
}

/// E1 — integer loop bound. The engine already produces this when the bound is
/// `a.length`; it does not when the bound is an `n: number` parameter.
pub fn e1_int_bound() -> Compiled {
    ladder("ir_e1_int_bound", &[VEC_GET, ADP_ADD], false, |fb, im, recv, _b, i, s| {
        let x = elem_call(fb, im, recv, i);
        let boxed = box_acc(fb, s);
        let sum = call1(fb, im["probe_adp_add"], &[boxed, x]);
        unbox_acc(fb, sum)
    })
}

/// E1b — the array read is STILL an opaque call, but the generic add grows the
/// inline tag-check fast path the design doc specifies (§Pilar 3: "ONE
/// `ADD_GENERIC` ... with an inline tag-check fast path for the secretly-
/// monomorphic case"). The `rts ir` dump has no such check — it calls
/// unconditionally — so this row prices the gap between the doc and the code.
///
/// Both operands here are genuine inline doubles at run time, so the fast path
/// always hits and the call is never executed; the branch and the two tag tests
/// are still paid every iteration.
///
/// This is independent of E2/E3: it needs no arena and no stable addressing, so
/// it is shippable on its own.
pub fn e1b_inline_fastpath() -> Compiled {
    ladder(
        "ir_e1b_fastpath",
        &[VEC_GET, ADP_ADD],
        false,
        |fb, im, recv, _b, i, s| {
            let x = elem_call(fb, im, recv, i);
            let sb = box_acc(fb, s);
            // `(w & BOX_BASE) != BOX_BASE` on both operands — a genuine inline
            // double is anything outside the boxed quadrant.
            let base = fb.ins().iconst(types::I64, crate::poly::BOX_BASE as i64);
            let ma = fb.ins().band(sb, base);
            let mb = fb.ins().band(x, base);
            let da = fb.ins().icmp(IntCC::NotEqual, ma, base);
            let db = fb.ins().icmp(IntCC::NotEqual, mb, base);
            let both = fb.ins().band(da, db);

            let fast = fb.create_block();
            let slow = fb.create_block();
            let merge = fb.create_block();
            fb.append_block_param(merge, types::F64);
            fb.ins().brif(both, fast, &[], slow, &[]);

            fb.switch_to_block(fast);
            fb.seal_block(fast);
            let fa = fb.ins().bitcast(types::F64, MemFlags::new(), sb);
            let fx = fb.ins().bitcast(types::F64, MemFlags::new(), x);
            let sum = fb.ins().fadd(fa, fx);
            fb.ins().jump(merge, &[sum.into()]);

            fb.switch_to_block(slow);
            fb.seal_block(slow);
            let raw = call1(fb, im["probe_adp_add"], &[sb, x]);
            let un = unbox_acc(fb, raw);
            fb.ins().jump(merge, &[un.into()]);

            fb.switch_to_block(merge);
            fb.seal_block(merge);
            fb.block_params(merge)[0]
        },
    )
}

/// E2 — the array read becomes a `load` from a stable base. The generic add and
/// its box/unbox round trip are still there.
pub fn e2_load() -> Compiled {
    ladder("ir_e2_load", &[ADP_ADD], false, |fb, im, _recv, base, i, s| {
        let off = fb.ins().imul_imm(i, 8);
        let addr = fb.ins().iadd(base, off);
        let flags = MemFlags::trusted().with_readonly();
        let x = fb.ins().load(types::I64, flags, addr, 0);
        let boxed = box_acc(fb, s);
        let sum = call1(fb, im["probe_adp_add"], &[boxed, x]);
        unbox_acc(fb, sum)
    })
}

/// E3 — the add is proven, so it is an `fadd` and nothing is boxed.
pub fn e3_proven() -> Compiled {
    ladder("ir_e3_proven", &[], false, |fb, _im, _recv, base, i, s| {
        let off = fb.ins().imul_imm(i, 8);
        let addr = fb.ins().iadd(base, off);
        let flags = MemFlags::trusted().with_readonly();
        let x = fb.ins().load(types::F64, flags, addr, 0);
        fb.ins().fadd(s, x)
    })
}

/// E2b — the read is a `load` but the add is still generic, AND the same element
/// is read TWICE (`s += a[i] * a[i]`). With a call the two reads cannot be
/// merged; with a load the egraph should CSE them into one.
pub fn e2b_double_read_load() -> Compiled {
    ladder("ir_e2b_double_load", &[ADP_ADD], false, |fb, im, _recv, base, i, s| {
        let off = fb.ins().imul_imm(i, 8);
        let addr = fb.ins().iadd(base, off);
        let flags = MemFlags::trusted().with_readonly();
        let x1 = fb.ins().load(types::F64, flags, addr, 0);
        let x2 = fb.ins().load(types::F64, flags, addr, 0);
        let prod = fb.ins().fmul(x1, x2);
        let boxed = box_acc(fb, s);
        let pb = fb.ins().bitcast(types::I64, MemFlags::new(), prod);
        let sum = call1(fb, im["probe_adp_add"], &[boxed, pb]);
        unbox_acc(fb, sum)
    })
}

/// E0b — the same double read, but through the CALL the engine emits. Two
/// identical opaque calls, which the optimizer may not merge.
pub fn e0b_double_read_call() -> Compiled {
    ladder("ir_e0b_double_call", &[VEC_GET, ADP_ADD], false, |fb, im, recv, _b, i, s| {
        let x1 = elem_call(fb, im, recv, i);
        let x2 = elem_call(fb, im, recv, i);
        let f1 = fb.ins().bitcast(types::F64, MemFlags::new(), x1);
        let f2 = fb.ins().bitcast(types::F64, MemFlags::new(), x2);
        let prod = fb.ins().fmul(f1, f2);
        let boxed = box_acc(fb, s);
        let pb = fb.ins().bitcast(types::I64, MemFlags::new(), prod);
        let sum = call1(fb, im["probe_adp_add"], &[boxed, pb]);
        unbox_acc(fb, sum)
    })
}
