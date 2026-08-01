//! Kernel VISIBLE — is the plan ADDITIVE or MULTIPLICATIVE?
//!
//! Every other kernel prices ONE access. This one asks the structural question:
//! when a heap access stops being an opaque extern call and becomes a `load`
//! that Cranelift can see, does the optimizer then remove work that is
//! *impossible* to remove today?
//!
//! Three things an optimizer does to memory access, none of which can fire
//! across an opaque call:
//!
//! - **CSE** — two reads of the same field in one iteration become one load.
//! - **LICM** — a loop-invariant read is hoisted out of the loop entirely.
//! - **store→load forwarding** — a field written then read feeds the value
//!   directly, with no reload.
//!
//! If those fire, the "make the read a load" change is worth far more than its
//! own 14 ns, because every redundant access in the program disappears with it.
//! If they do not fire, the plan is additive and the ~14 ns is all there is.
//!
//! The object is at a FIXED offset here (passed in the `mask` slot), so the
//! address is loop-invariant by construction — which is the precondition LICM
//! needs and which `objs[i & mask]` in the other kernels deliberately denies.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, MemFlags, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::{Compiled, Imports, call1, compile, emit_unbox_double};

const VEC_GET_LOCKED: (&str, usize) = ("probe_vec_get_locked", 2);
const VEC_GET_UNLOCKED: (&str, usize) = ("probe_vec_get_unlocked", 2);

/// `for i in 0..iters { s += body() }` with a FIXED object — `mask` carries the
/// object's payload/offset, not an index mask.
fn fixed_obj_loop<F>(name: &str, needed: &[(&'static str, usize)], body: F) -> Compiled
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
        let obj_payload = fb.block_params(entry)[2];
        let arena_base = fb
            .ins()
            .load(types::I64, MemFlags::trusted(), hdr, 8);

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
        let s_next = body(fb, imports, obj_payload, arena_base, s);
        let i_next = fb.ins().iadd_imm(i, 1);
        fb.ins().jump(header, &[i_next.into(), s_next.into()]);
        fb.seal_block(header);

        fb.switch_to_block(exit);
        fb.seal_block(exit);
        let out = fb.block_params(exit)[0];
        let raw = super::emit_box_double(fb, out);
        fb.ins().return_(&[raw]);
    })
}

fn obj_addr(fb: &mut FunctionBuilder, base: Value, payload: Value) -> Value {
    let off = fb.ins().imul_imm(payload, 8);
    fb.ins().iadd(base, off)
}

// ---------------------------------------------------------------------------
// CSE: read the SAME field twice per iteration.
// ---------------------------------------------------------------------------

/// V0 — today. Two opaque calls; Cranelift cannot know they return the same
/// value, so both survive.
pub fn v0_cse_call() -> Compiled {
    fixed_obj_loop("vis_v0_cse_call", &[VEC_GET_LOCKED], |fb, im, p, _b, s| {
        let idx = fb.ins().iconst(types::I64, 1);
        let a = call1(fb, im["probe_vec_get_locked"], &[p, idx]);
        let idx2 = fb.ins().iconst(types::I64, 1);
        let b = call1(fb, im["probe_vec_get_locked"], &[p, idx2]);
        let af = emit_unbox_double(fb, a);
        let bf = emit_unbox_double(fb, b);
        let sum = fb.ins().fadd(af, bf);
        fb.ins().fadd(s, sum)
    })
}

/// V1 — two `load`s of the same address, `MemFlags::trusted()`. If the egraph
/// CSEs them, this costs ONE load per iteration, not two.
pub fn v1_cse_load_trusted() -> Compiled {
    fixed_obj_loop("vis_v1_cse_trusted", &[], |fb, _im, p, base, s| {
        let obj = obj_addr(fb, base, p);
        let f = MemFlags::trusted();
        let a = fb.ins().load(types::F64, f, obj, 8);
        let b = fb.ins().load(types::F64, f, obj, 8);
        let sum = fb.ins().fadd(a, b);
        fb.ins().fadd(s, sum)
    })
}

/// V2 — same, but the loads are marked `readonly`: a promise that nothing
/// mutates this location for the whole function.
pub fn v2_cse_load_readonly() -> Compiled {
    fixed_obj_loop("vis_v2_cse_readonly", &[], |fb, _im, p, base, s| {
        let obj = obj_addr(fb, base, p);
        let mut f = MemFlags::trusted();
        f.set_readonly();
        let a = fb.ins().load(types::F64, f, obj, 8);
        let b = fb.ins().load(types::F64, f, obj, 8);
        let sum = fb.ins().fadd(a, b);
        fb.ins().fadd(s, sum)
    })
}

// ---------------------------------------------------------------------------
// LICM: a single loop-INVARIANT read.
// ---------------------------------------------------------------------------

/// V3 — today. One opaque call per iteration; hoisting is impossible because
/// Cranelift must assume the call has side effects.
pub fn v3_licm_call() -> Compiled {
    fixed_obj_loop("vis_v3_licm_call", &[VEC_GET_LOCKED], |fb, im, p, _b, s| {
        let idx = fb.ins().iconst(types::I64, 1);
        let a = call1(fb, im["probe_vec_get_locked"], &[p, idx]);
        let af = emit_unbox_double(fb, a);
        fb.ins().fadd(s, af)
    })
}

/// V3u — the same call with the shard `Mutex` removed, to separate "the lock
/// blocks hoisting" from "the CALL blocks hoisting". It is still opaque, so it
/// should still not hoist.
pub fn v3u_licm_call_unlocked() -> Compiled {
    fixed_obj_loop("vis_v3u_licm_unlocked", &[VEC_GET_UNLOCKED], |fb, im, p, _b, s| {
        let idx = fb.ins().iconst(types::I64, 1);
        let a = call1(fb, im["probe_vec_get_unlocked"], &[p, idx]);
        let af = emit_unbox_double(fb, a);
        fb.ins().fadd(s, af)
    })
}

/// V4 — one `load` of a loop-invariant address, `trusted`. Does Cranelift hoist
/// it out of the loop?
pub fn v4_licm_load_trusted() -> Compiled {
    fixed_obj_loop("vis_v4_licm_trusted", &[], |fb, _im, p, base, s| {
        let obj = obj_addr(fb, base, p);
        let a = fb.ins().load(types::F64, MemFlags::trusted(), obj, 8);
        fb.ins().fadd(s, a)
    })
}

/// V5 — same load marked `readonly`, the flag whose documented meaning is "no
/// memory dependencies for the whole function".
pub fn v5_licm_load_readonly() -> Compiled {
    fixed_obj_loop("vis_v5_licm_readonly", &[], |fb, _im, p, base, s| {
        let obj = obj_addr(fb, base, p);
        let mut f = MemFlags::trusted();
        f.set_readonly();
        let a = fb.ins().load(types::F64, f, obj, 8);
        fb.ins().fadd(s, a)
    })
}

/// V6 — the hand-hoisted form: read once before the loop. This is what LICM
/// would produce if it fired, and therefore the target V4/V5 are measured
/// against.
pub fn v6_licm_hand_hoisted() -> Compiled {
    compile("vis_v6_hoisted", &[], |fb, _im| {
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let iters = fb.block_params(entry)[0];
        let hdr = fb.block_params(entry)[1];
        let p = fb.block_params(entry)[2];
        let f = MemFlags::trusted();
        let base = fb.ins().load(types::I64, f, hdr, 8);
        let obj = obj_addr(fb, base, p);
        let a = fb.ins().load(types::F64, f, obj, 8);

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
        let s1 = fb.ins().fadd(s, a);
        let i1 = fb.ins().iadd_imm(i, 1);
        fb.ins().jump(header, &[i1.into(), s1.into()]);
        fb.seal_block(header);

        fb.switch_to_block(exit);
        fb.seal_block(exit);
        let out = fb.block_params(exit)[0];
        let raw = super::emit_box_double(fb, out);
        fb.ins().return_(&[raw]);
    })
}
