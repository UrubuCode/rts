//! Kernel BACKEND — how far behind LLVM is Cranelift, ON THIS MACHINE?
//!
//! The published figure gets quoted a lot and it is someone else's benchmark on
//! someone else's workload. This kernel emits four loops through Cranelift with
//! the engine's own ISA flags, and `bench/backend.rs` writes the SAME four loops
//! in Rust, which this crate compiles at `opt-level = 3` — i.e. through LLVM.
//! Same machine, same data, same checksum.
//!
//! The four shapes are chosen where Cranelift has a REASON to lose:
//!
//! | shape | why it discriminates |
//! |---|---|
//! | `vec_sum` | an integer reduction LLVM autovectorizes; Cranelift has no loop vectorizer at all |
//! | `fp_chain` | a dependent FP chain neither may reorder — latency-bound, should be parity |
//! | `branchy` | branch layout and cmov selection |
//! | `fp_reduce` | an FP reduction LLVM may NOT reorder without fast-math — parity expected |
//!
//! A gap on `vec_sum` with parity elsewhere localizes the deficit to
//! vectorization, which is the one thing `CLAUDE.md` already records as absent
//! (issue #92, "closed as infeasible without our own loop vectorizer").

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{InstBuilder, MemFlags, Type, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::{Compiled, compile, emit_box_double};

/// `for i in 0..iters { s = body(load(base + i*8), s) }` — a straight
/// contiguous walk with NO masking, so a vectorizer has nothing blocking it.
fn walk<F>(name: &str, s_ty: Type, body: F) -> Compiled
where
    F: FnOnce(&mut FunctionBuilder, Value, Value) -> Value + Copy,
{
    compile(name, &[], move |fb, _im| {
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let iters = fb.block_params(entry)[0];
        let hdr_ptr = fb.block_params(entry)[1];

        let base = fb.ins().load(types::I64, MemFlags::trusted(), hdr_ptr, 8);

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
        let off = fb.ins().imul_imm(i, 8);
        let addr = fb.ins().iadd(base, off);
        // `readonly` says the loaded memory is never written by this function, so
        // the load is free to move; `trusted` alone does not license that.
        let flags = MemFlags::trusted().with_readonly();
        let x = if s_ty == types::F64 {
            fb.ins().load(types::F64, flags, addr, 0)
        } else {
            fb.ins().load(types::I64, flags, addr, 0)
        };
        let s_next = body(fb, x, s);
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

/// `s += a[i]` — an integer reduction. LLVM turns this into 4-wide or 8-wide
/// vector adds; Cranelift emits one scalar add per element.
pub fn vec_sum() -> Compiled {
    walk("bk_vec_sum", types::I64, |fb, x, s| fb.ins().iadd(s, x))
}

/// `s += a[i]`, but with the address carried as an INCREMENTING POINTER instead
/// of `base + i*8` recomputed each iteration, and the loop unrolled 4×.
///
/// This exists to falsify the obvious objection to `vec_sum`: that the gap is my
/// naive IR (a multiply per iteration, no unrolling) rather than a missing
/// vectorizer. If hand-strength-reducing and unrolling closes the gap, the fix
/// belongs in RTS's lowering and the backend is innocent. If it does not, the
/// deficit is vectorization and no amount of better IR reaches it.
pub fn vec_sum_tuned() -> Compiled {
    compile("bk_vec_sum_tuned", &[], move |fb, _im| {
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let iters = fb.block_params(entry)[0];
        let hdr_ptr = fb.block_params(entry)[1];
        let base = fb.ins().load(types::I64, MemFlags::trusted(), hdr_ptr, 8);
        let end_off = fb.ins().imul_imm(iters, 8);
        let end = fb.ins().iadd(base, end_off);

        let header = fb.create_block();
        fb.append_block_param(header, types::I64); // running pointer
        fb.append_block_param(header, types::I64); // s
        let lbody = fb.create_block();
        let exit = fb.create_block();
        fb.append_block_param(exit, types::I64);

        let s0 = fb.ins().iconst(types::I64, 0);
        fb.ins().jump(header, &[base.into(), s0.into()]);

        fb.switch_to_block(header);
        let p = fb.block_params(header)[0];
        let s = fb.block_params(header)[1];
        let go = fb.ins().icmp(IntCC::SignedLessThan, p, end);
        fb.ins().brif(go, lbody, &[], exit, &[s.into()]);

        fb.switch_to_block(lbody);
        fb.seal_block(lbody);
        // Unrolled 4×, with four INDEPENDENT partial sums so the adds are not a
        // single dependence chain — the same trick a vectorizer performs, minus
        // the vector registers. `ELEMS` is a multiple of 4, so no tail is needed.
        let flags = MemFlags::trusted().with_readonly();
        let x0 = fb.ins().load(types::I64, flags, p, 0);
        let x1 = fb.ins().load(types::I64, flags, p, 8);
        let x2 = fb.ins().load(types::I64, flags, p, 16);
        let x3 = fb.ins().load(types::I64, flags, p, 24);
        let a = fb.ins().iadd(x0, x1);
        let b = fb.ins().iadd(x2, x3);
        let ab = fb.ins().iadd(a, b);
        let s_next = fb.ins().iadd(s, ab);
        let p_next = fb.ins().iadd_imm(p, 32);
        fb.ins().jump(header, &[p_next.into(), s_next.into()]);
        fb.seal_block(header);

        fb.switch_to_block(exit);
        fb.seal_block(exit);
        let out = fb.block_params(exit)[0];
        fb.ins().return_(&[out]);
    })
}

/// `s = s * 1.0000001 + a[i]` — every iteration depends on the previous, so
/// there is no parallelism for either compiler to find.
pub fn fp_chain() -> Compiled {
    walk("bk_fp_chain", types::F64, |fb, x, s| {
        let k = fb.ins().f64const(1.000_000_1);
        let m = fb.ins().fmul(s, k);
        fb.ins().fadd(m, x)
    })
}

/// `if (a[i] & 1) s += a[i] else s -= a[i]` — branch layout, and whether the
/// compiler flattens it into a conditional move.
pub fn branchy() -> Compiled {
    walk("bk_branchy", types::I64, |fb, x, s| {
        let odd = fb.ins().band_imm(x, 1);
        let is_odd = fb.ins().icmp_imm(IntCC::NotEqual, odd, 0);
        let add = fb.ins().iadd(s, x);
        let sub = fb.ins().isub(s, x);
        fb.ins().select(is_odd, add, sub)
    })
}

/// `s += a[i] * a[i]` — an FP reduction. Reassociating it changes the result,
/// so LLVM will NOT vectorize it without fast-math. Parity is the expectation,
/// and a gap here would mean something other than vectorization is wrong.
pub fn fp_reduce() -> Compiled {
    walk("bk_fp_reduce", types::F64, |fb, x, s| {
        let sq = fb.ins().fmul(x, x);
        fb.ins().fadd(s, sq)
    })
}

/// `s += a[i] > 0 ? a[i] : 0` in f64 — a predicated reduction, the shape a JS
/// `filter`+`sum` degenerates into once types are proven.
pub fn fp_predicated() -> Compiled {
    walk("bk_fp_predicated", types::F64, |fb, x, s| {
        let z = fb.ins().f64const(0.0);
        let pos = fb.ins().fcmp(FloatCC::GreaterThan, x, z);
        let keep = fb.ins().select(pos, x, z);
        fb.ins().fadd(s, keep)
    })
}
