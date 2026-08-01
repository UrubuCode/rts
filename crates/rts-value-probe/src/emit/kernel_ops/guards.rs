//! The two GUARDED-SPECIAL-CASE variants: the ones the engine does not have.
//!
//! Both exist for the same reason — `%` and `**` keep a CALL even on the
//! proven path, because Cranelift has neither `frem` nor `pow`. Proving the
//! Repr therefore buys almost nothing for them, while a cheap runtime test
//! that swaps the OPERATION does.

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{InstBuilder, types};

use super::super::{Compiled, call1, emit_is_double, emit_unbox_double};
use super::{Op, call_f64, fold_word, ops_loop};

// ---------------------------------------------------------------------------
// X3 — `%` only: the variant the engine does NOT have.
//
// `binop.rs:575` takes the native `srem` path only when the divisor is a KNOWN
// non-zero constant; every other `%` calls fmod, and fmod is the slowest thing
// in this whole kernel. Two integral doubles could use `srem`, which needs a
// RUNTIME guard rather than a compile-time constant:
//
//   both round-trip through i64 exactly  (so they really are integers)
//   AND divisor != 0                     (JS gives NaN; `srem` TRAPS)
//   AND dividend != 0                    (`-0 % 3` is `-0` in JS, `0` via srem)
//
// The question this row answers is whether that guard costs more than the fmod
// it avoids.
// ---------------------------------------------------------------------------

pub fn x3_mod_int_srem() -> Compiled {
    let needed: Vec<(&'static str, usize)> = vec![("probe_mod", 2), ("probe_fmod_f64", 2)];
    ops_loop("ops_x3_mod", Op::Mod, &needed, move |fb, im, v| {
        let da = emit_is_double(fb, v.a);
        let db = emit_is_double(fb, v.b);
        let both = fb.ins().band(da, db);

        let fast = fb.create_block();
        let slow = fb.create_block();
        let cont = fb.create_block();
        fb.append_block_param(cont, types::F64);
        fb.ins().brif(both, fast, &[], slow, &[]);

        fb.switch_to_block(fast);
        fb.seal_block(fast);
        let af = emit_unbox_double(fb, v.a);
        let bf = emit_unbox_double(fb, v.b);
        let xi = fb.ins().fcvt_to_sint_sat(types::I64, af);
        let yi = fb.ins().fcvt_to_sint_sat(types::I64, bf);
        let xr = fb.ins().fcvt_from_sint(types::F64, xi);
        let yr = fb.ins().fcvt_from_sint(types::F64, yi);
        let x_int = fb.ins().fcmp(FloatCC::Equal, xr, af);
        let y_int = fb.ins().fcmp(FloatCC::Equal, yr, bf);
        let zero = fb.ins().iconst(types::I64, 0);
        let y_nz = fb.ins().icmp(IntCC::NotEqual, yi, zero);
        let x_nz = fb.ins().icmp(IntCC::NotEqual, xi, zero);
        let ok1 = fb.ins().band(x_int, y_int);
        let ok2 = fb.ins().band(y_nz, x_nz);
        let ok = fb.ins().band(ok1, ok2);

        let irem = fb.create_block();
        let frem = fb.create_block();
        let join = fb.create_block();
        fb.append_block_param(join, types::F64);
        fb.ins().brif(ok, irem, &[], frem, &[]);

        fb.switch_to_block(irem);
        fb.seal_block(irem);
        let r = fb.ins().srem(xi, yi);
        let rf = fb.ins().fcvt_from_sint(types::F64, r);
        fb.ins().jump(join, &[rf.into()]);

        fb.switch_to_block(frem);
        fb.seal_block(frem);
        let rf2 = call_f64(fb, im["probe_fmod_f64"], af, bf);
        fb.ins().jump(join, &[rf2.into()]);

        fb.switch_to_block(join);
        fb.seal_block(join);
        let got = fb.block_params(join)[0];
        let s_fast = fb.ins().fadd(v.s, got);
        fb.ins().jump(cont, &[s_fast.into()]);

        fb.switch_to_block(slow);
        fb.seal_block(slow);
        let w = call1(fb, im["probe_mod"], &[v.a, v.b]);
        let s_slow = fold_word(fb, Op::Mod, v.s, w);
        fb.ins().jump(cont, &[s_slow.into()]);

        fb.switch_to_block(cont);
        fb.seal_block(cont);
        fb.block_params(cont)[0]
    })
}

// ---------------------------------------------------------------------------
// X3 — `**` only: the same shape as the `%` case.
//
// `binop.rs:644` calls `__RTS_FN_NS_MATH_POW` even on the proven path, so the
// proof does not remove the call. `x ** 2` is the overwhelmingly common form in
// real code, and it is one `fmul`. Guard the exponent at runtime.
// ---------------------------------------------------------------------------

pub fn x3_exp_square() -> Compiled {
    let needed: Vec<(&'static str, usize)> = vec![("probe_pow", 2), ("probe_pow_f64", 2)];
    ops_loop("ops_x3_exp", Op::Exp, &needed, move |fb, im, v| {
        let da = emit_is_double(fb, v.a);
        let db = emit_is_double(fb, v.b);
        let both = fb.ins().band(da, db);

        let fast = fb.create_block();
        let slow = fb.create_block();
        let cont = fb.create_block();
        fb.append_block_param(cont, types::F64);
        fb.ins().brif(both, fast, &[], slow, &[]);

        fb.switch_to_block(fast);
        fb.seal_block(fast);
        let af = emit_unbox_double(fb, v.a);
        let bf = emit_unbox_double(fb, v.b);
        let two = fb.ins().f64const(2.0);
        let is_sq = fb.ins().fcmp(FloatCC::Equal, bf, two);

        let sq = fb.create_block();
        let generic = fb.create_block();
        let join = fb.create_block();
        fb.append_block_param(join, types::F64);
        fb.ins().brif(is_sq, sq, &[], generic, &[]);

        fb.switch_to_block(sq);
        fb.seal_block(sq);
        let r = fb.ins().fmul(af, af);
        fb.ins().jump(join, &[r.into()]);

        fb.switch_to_block(generic);
        fb.seal_block(generic);
        let r2 = call_f64(fb, im["probe_pow_f64"], af, bf);
        fb.ins().jump(join, &[r2.into()]);

        fb.switch_to_block(join);
        fb.seal_block(join);
        let got = fb.block_params(join)[0];
        let s_fast = fb.ins().fadd(v.s, got);
        fb.ins().jump(cont, &[s_fast.into()]);

        fb.switch_to_block(slow);
        fb.seal_block(slow);
        let w = call1(fb, im["probe_pow"], &[v.a, v.b]);
        let s_slow = fold_word(fb, Op::Exp, v.s, w);
        fb.ins().jump(cont, &[s_slow.into()]);

        fb.switch_to_block(cont);
        fb.seal_block(cont);
        fb.block_params(cont)[0]
    })
}
