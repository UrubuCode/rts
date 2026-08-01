//! Kernel ARCH — prices the three architecture items that were still estimates:
//! A3 (alias regions), A5 (the error poll), A6 (native vs uniform-slot ABI).
//!
//! Each group is a before/after on ONE change, with the "after" written the way
//! a compiled language writes it (per the research in §1d).

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{AliasRegion, InstBuilder, MemFlags, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::{Compiled, Imports, call1, compile, emit_box_double};

const ERR_PENDING: (&str, usize) = ("probe_err_pending", 1);
const CALL_UNIFORM: (&str, usize) = ("probe_call_uniform", 5);
const CALL_NATIVE: (&str, usize) = ("probe_call_native_f64", 2);

/// `for i in 0..iters { s = body(s) }`, `f64` accumulator, arena base in hdr[1].
fn arch_loop<F>(name: &str, needed: &[(&'static str, usize)], body: F) -> Compiled
where
    F: FnOnce(&mut FunctionBuilder, &Imports, Value, Value, Value) -> Value + Copy,
{
    compile(name, needed, move |fb, imports| {
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let iters = fb.block_params(entry)[0];
        let hdr = fb.block_params(entry)[1];
        let obj = fb.block_params(entry)[2];
        let base = fb.ins().load(types::I64, MemFlags::trusted(), hdr, 8);

        let header = fb.create_block();
        fb.append_block_param(header, types::I64);
        fb.append_block_param(header, types::F64);
        let lbody = fb.create_block();
        let exit = fb.create_block();
        fb.append_block_param(exit, types::F64);
        let zero = fb.ins().iconst(types::I64, 0);
        let s0 = fb.ins().f64const(0.0);
        fb.ins().jump(header, &[zero.into(), s0.into()]);

        fb.switch_to_block(header);
        let i = fb.block_params(header)[0];
        let s = fb.block_params(header)[1];
        let go = fb.ins().icmp(IntCC::SignedLessThan, i, iters);
        fb.ins().brif(go, lbody, &[], exit, &[s.into()]);

        fb.switch_to_block(lbody);
        fb.seal_block(lbody);
        let s_next = body(fb, imports, obj, base, s);
        let i_next = fb.ins().iadd_imm(i, 1);
        fb.ins().jump(header, &[i_next.into(), s_next.into()]);
        fb.seal_block(header);

        fb.switch_to_block(exit);
        fb.seal_block(exit);
        let out = fb.block_params(exit)[0];
        let raw = emit_box_double(fb, out);
        fb.ins().return_(&[raw]);
    })
}

fn addr(fb: &mut FunctionBuilder, base: Value, obj: Value) -> Value {
    let off = fb.ins().imul_imm(obj, 8);
    fb.ins().iadd(base, off)
}

// ---------------------------------------------------------------------------
// A3 — alias regions. Read field A, WRITE field B, read field A again.
// Without alias info the store must be assumed to clobber A, so the second read
// reloads. With distinct regions it should be CSE'd away.
// ---------------------------------------------------------------------------

/// A3-0: one region for everything (what a naive lowering emits).
pub fn a3_0_same_region() -> Compiled {
    arch_loop("arch_a3_same", &[], |fb, _im, obj, base, s| {
        let a = addr(fb, base, obj);
        let f = MemFlags::trusted();
        let r1 = fb.ins().load(types::F64, f, a, 8);
        fb.ins().store(f, r1, a, 16); // write field B
        let r2 = fb.ins().load(types::F64, f, a, 8); // read A again
        let sum = fb.ins().fadd(r1, r2);
        fb.ins().fadd(s, sum)
    })
}

/// A3-1: field A in `Heap`, field B in `Table` — distinct alias regions, so the
/// store to B cannot alias the read of A.
pub fn a3_1_distinct_regions() -> Compiled {
    arch_loop("arch_a3_distinct", &[], |fb, _im, obj, base, s| {
        let a = addr(fb, base, obj);
        let fa = MemFlags::trusted().with_alias_region(Some(AliasRegion::Heap));
        let fb_ = MemFlags::trusted().with_alias_region(Some(AliasRegion::Table));
        let r1 = fb.ins().load(types::F64, fa, a, 8);
        fb.ins().store(fb_, r1, a, 16);
        let r2 = fb.ins().load(types::F64, fa, a, 8);
        let sum = fb.ins().fadd(r1, r2);
        fb.ins().fadd(s, sum)
    })
}

/// A3-2: the ceiling — read once, no store in between.
pub fn a3_2_no_store() -> Compiled {
    arch_loop("arch_a3_nostore", &[], |fb, _im, obj, base, s| {
        let a = addr(fb, base, obj);
        let f = MemFlags::trusted();
        let r1 = fb.ins().load(types::F64, f, a, 8);
        let sum = fb.ins().fadd(r1, r1);
        fb.ins().fadd(s, sum)
    })
}

// ---------------------------------------------------------------------------
// A5 — the error poll after every call.
// ---------------------------------------------------------------------------

/// A5-0: today — `call __rtsadp_err_pending` + branch after the work.
pub fn a5_0_call_poll() -> Compiled {
    arch_loop("arch_a5_call", &[ERR_PENDING], |fb, im, obj, base, s| {
        let a = addr(fb, base, obj);
        let v = fb.ins().load(types::F64, MemFlags::trusted(), a, 8);
        let z = fb.ins().iconst(types::I64, 0);
        let pending = call1(fb, im["probe_err_pending"], &[z]);
        let zero = fb.ins().iconst(types::I64, 0);
        let ok = fb.ins().icmp(IntCC::Equal, pending, zero);
        let cont = fb.create_block();
        let bail = fb.create_block();
        let join = fb.create_block();
        fb.append_block_param(join, types::F64);
        fb.ins().brif(ok, cont, &[], bail, &[]);
        fb.switch_to_block(cont);
        fb.seal_block(cont);
        let s1 = fb.ins().fadd(s, v);
        fb.ins().jump(join, &[s1.into()]);
        fb.switch_to_block(bail);
        fb.seal_block(bail);
        let nan = fb.ins().f64const(f64::NAN);
        let s2 = fb.ins().fadd(s, nan);
        fb.ins().jump(join, &[s2.into()]);
        fb.switch_to_block(join);
        fb.seal_block(join);
        fb.block_params(join)[0]
    })
}

/// A5-1: the Rust `?` shape — the flag is a WORD IN MEMORY the caller already
/// has; read it inline and branch. No call, no optimization barrier.
pub fn a5_1_inline_poll() -> Compiled {
    arch_loop("arch_a5_inline", &[], |fb, _im, obj, base, s| {
        let a = addr(fb, base, obj);
        let v = fb.ins().load(types::F64, MemFlags::trusted(), a, 8);
        // The error flag lives at arena slot 0 — stands in for the TLS word.
        let flag = fb.ins().load(types::I64, MemFlags::trusted(), base, 0);
        let zero = fb.ins().iconst(types::I64, 0);
        let ok = fb.ins().icmp(IntCC::Equal, flag, zero);
        let cont = fb.create_block();
        let bail = fb.create_block();
        let join = fb.create_block();
        fb.append_block_param(join, types::F64);
        fb.ins().brif(ok, cont, &[], bail, &[]);
        fb.switch_to_block(cont);
        fb.seal_block(cont);
        let s1 = fb.ins().fadd(s, v);
        fb.ins().jump(join, &[s1.into()]);
        fb.switch_to_block(bail);
        fb.seal_block(bail);
        let nan = fb.ins().f64const(f64::NAN);
        let s2 = fb.ins().fadd(s, nan);
        fb.ins().jump(join, &[s2.into()]);
        fb.switch_to_block(join);
        fb.seal_block(join);
        fb.block_params(join)[0]
    })
}

/// A5-2: no poll at all — the ceiling (what a `nothrow`-proven callee allows).
pub fn a5_2_no_poll() -> Compiled {
    arch_loop("arch_a5_none", &[], |fb, _im, obj, base, s| {
        let a = addr(fb, base, obj);
        let v = fb.ins().load(types::F64, MemFlags::trusted(), a, 8);
        fb.ins().fadd(s, v)
    })
}

// ---------------------------------------------------------------------------
// A6 — uniform 5-slot thunk ABI vs a native two-`f64` signature.
// ---------------------------------------------------------------------------

/// A6-0: today's shape — every argument boxed into a uniform i64 slot, 5 slots
/// passed whatever the real arity, result a tagged word.
pub fn a6_0_uniform_thunk() -> Compiled {
    arch_loop("arch_a6_uniform", &[CALL_UNIFORM], |fb, im, obj, base, s| {
        let a = addr(fb, base, obj);
        let x = fb.ins().load(types::I64, MemFlags::trusted(), a, 0);
        let y = fb.ins().load(types::I64, MemFlags::trusted(), a, 8);
        let z = fb.ins().iconst(types::I64, 0);
        let r = call1(fb, im["probe_call_uniform"], &[z, x, y, z, z]);
        let rf = super::emit_unbox_double(fb, r);
        fb.ins().fadd(s, rf)
    })
}

/// A6-1: a native `fn(f64, f64) -> f64` — what a compiled language emits when
/// the types are known.
pub fn a6_1_native_sig() -> Compiled {
    arch_loop("arch_a6_native", &[CALL_NATIVE], |fb, im, obj, base, s| {
        let a = addr(fb, base, obj);
        let x = fb.ins().load(types::F64, MemFlags::trusted(), a, 0);
        let y = fb.ins().load(types::F64, MemFlags::trusted(), a, 8);
        let inst = fb.ins().call(im["probe_call_native_f64"], &[x, y]);
        let r = fb.inst_results(inst)[0];
        fb.ins().fadd(s, r)
    })
}
