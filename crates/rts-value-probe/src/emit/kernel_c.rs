//! Kernel C — the ORIGINAL question, isolated: is there a value representation
//! better than the NaN-boxed `PolyValue` word?
//!
//! No heap, no calls. `s = s + x*y` where `x`/`y` are read from a pre-built
//! array — read from memory ON PURPOSE, so the producer is opaque to the egraph
//! and the tag check cannot be folded away. Folding it is exactly what would
//! make a "look how cheap NaN-boxing is" number a lie.
//!
//! - **C0 NaN-box** — one `i64` array of boxed words. Guard is
//!   `(w & BOX_BASE) != BOX_BASE`: one `band` against a 64-bit constant plus an
//!   `icmp`, per operand.
//! - **C1 two-slot `{tag, value}`** — the QuickJS-64 / Porffor-approach-2 shape:
//!   a `f64` value array PLUS a parallel `i64` tag array. Guard is a plain
//!   `icmp` against a small immediate — cheaper per check, but it costs a SECOND
//!   load per operand and a second register per live value.
//! - **C2 native `f64`** — no tag at all. The floor.

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{InstBuilder, MemFlags, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::{Compiled, compile, emit_box_double, emit_is_double, emit_unbox_double};

/// The tag word C1 compares against. Any small immediate would do — the point is
/// that it is NOT a 64-bit constant needing its own materialization.
pub const TAG_F64: i64 = 2;

/// `hdr` layout for kernel C: `[boxed_words_ptr, values_f64_ptr, tags_ptr]`.
fn hdr(fb: &mut FunctionBuilder, p: Value, slot: i32) -> Value {
    fb.ins().load(types::I64, MemFlags::trusted(), p, slot * 8)
}

// ---------------------------------------------------------------------------
// C0 — NaN-boxed word, guarded.
// ---------------------------------------------------------------------------

pub fn c0_nanbox() -> Compiled {
    compile("c0_nanbox", &[], |fb, _im| {
        let (iters, hdr_ptr, mask) = prologue(fb);
        let arr = hdr(fb, hdr_ptr, 0);
        let trusted = MemFlags::trusted();

        let header = fb.create_block();
        fb.append_block_param(header, types::I64); // i
        fb.append_block_param(header, types::F64); // s (native: C0 measures the
        // OPERAND dispatch cost, not accumulator boxing — kernel A already
        // measures a boxed accumulator end to end.)
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
        let idx = fb.ins().band(i, mask);
        let off = fb.ins().imul_imm(idx, 16); // two words per item
        let base = fb.ins().iadd(arr, off);
        let xw = fb.ins().load(types::I64, trusted, base, 0);
        let yw = fb.ins().load(types::I64, trusted, base, 8);

        // The guard the tag scheme forces. Both arms real.
        let dx = emit_is_double(fb, xw);
        let dy = emit_is_double(fb, yw);
        let both = fb.ins().band(dx, dy);
        let fast = fb.create_block();
        let slow = fb.create_block();
        let cont = fb.create_block();
        fb.append_block_param(cont, types::F64);
        fb.ins().brif(both, fast, &[], slow, &[]);

        fb.switch_to_block(fast);
        fb.seal_block(fast);
        let xf = emit_unbox_double(fb, xw);
        let yf = emit_unbox_double(fb, yw);
        let m = fb.ins().fmul(xf, yf);
        let s1 = fb.ins().fadd(s, m);
        fb.ins().jump(cont, &[s1.into()]);

        fb.switch_to_block(slow);
        fb.seal_block(slow);
        let nanv = fb.ins().f64const(f64::NAN);
        let s2 = fb.ins().fadd(s, nanv);
        fb.ins().jump(cont, &[s2.into()]);

        fb.switch_to_block(cont);
        fb.seal_block(cont);
        let s_next = fb.block_params(cont)[0];
        let i_next = fb.ins().iadd_imm(i, 1);
        fb.ins().jump(header, &[i_next.into(), s_next.into()]);
        fb.seal_block(header);

        epilogue(fb, exit);
    })
}

// ---------------------------------------------------------------------------
// C0b — NaN-box with the guard done in the FP DOMAIN.
//
// C0 loads the word into a GPR (the tag check is integer work) and then needs a
// GPR→XMM move to do the arithmetic. That cross-domain move is a real tax and it
// is NOT inherent to NaN-boxing: every boxed value lives in the NaN quadrant, so
// "is this a genuine double" can be answered by an ORDERED self-compare
// (`x == x`, one `ucomisd`) with the value already in an XMM register. A real
// NaN double answers "no" and takes the slow path — a safe false miss, not a
// wrong answer. If C0b matches C1, the two-slot advantage is a lowering artifact
// rather than a property of the representation.
// ---------------------------------------------------------------------------

pub fn c0b_nanbox_fp_guard() -> Compiled {
    compile("c0b_nanbox_fp_guard", &[], |fb, _im| {
        let (iters, hdr_ptr, mask) = prologue(fb);
        let arr = hdr(fb, hdr_ptr, 0);
        let trusted = MemFlags::trusted();

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
        let idx = fb.ins().band(i, mask);
        let off = fb.ins().imul_imm(idx, 16);
        let base = fb.ins().iadd(arr, off);
        // Load the SAME boxed words, straight into the FP domain.
        let xf = fb.ins().load(types::F64, trusted, base, 0);
        let yf = fb.ins().load(types::F64, trusted, base, 8);
        let ox = fb.ins().fcmp(FloatCC::Equal, xf, xf);
        let oy = fb.ins().fcmp(FloatCC::Equal, yf, yf);
        let both = fb.ins().band(ox, oy);

        let fast = fb.create_block();
        let slow = fb.create_block();
        let cont = fb.create_block();
        fb.append_block_param(cont, types::F64);
        fb.ins().brif(both, fast, &[], slow, &[]);

        fb.switch_to_block(fast);
        fb.seal_block(fast);
        let m = fb.ins().fmul(xf, yf);
        let s1 = fb.ins().fadd(s, m);
        fb.ins().jump(cont, &[s1.into()]);

        fb.switch_to_block(slow);
        fb.seal_block(slow);
        let nanv = fb.ins().f64const(f64::NAN);
        let s2 = fb.ins().fadd(s, nanv);
        fb.ins().jump(cont, &[s2.into()]);

        fb.switch_to_block(cont);
        fb.seal_block(cont);
        let s_next = fb.block_params(cont)[0];
        let i_next = fb.ins().iadd_imm(i, 1);
        fb.ins().jump(header, &[i_next.into(), s_next.into()]);
        fb.seal_block(header);

        epilogue(fb, exit);
    })
}

// ---------------------------------------------------------------------------
// C1 — two-slot {tag, value}: cheaper check, double the loads.
// ---------------------------------------------------------------------------

pub fn c1_two_slot() -> Compiled {
    compile("c1_two_slot", &[], |fb, _im| {
        let (iters, hdr_ptr, mask) = prologue(fb);
        let vals = hdr(fb, hdr_ptr, 1);
        let tags = hdr(fb, hdr_ptr, 2);
        let trusted = MemFlags::trusted();

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
        let idx = fb.ins().band(i, mask);
        let off = fb.ins().imul_imm(idx, 16);
        let vbase = fb.ins().iadd(vals, off);
        let tbase = fb.ins().iadd(tags, off);
        let xf = fb.ins().load(types::F64, trusted, vbase, 0);
        let yf = fb.ins().load(types::F64, trusted, vbase, 8);
        let xt = fb.ins().load(types::I64, trusted, tbase, 0);
        let yt = fb.ins().load(types::I64, trusted, tbase, 8);

        let want = fb.ins().iconst(types::I64, TAG_F64);
        let dx = fb.ins().icmp(IntCC::Equal, xt, want);
        let dy = fb.ins().icmp(IntCC::Equal, yt, want);
        let both = fb.ins().band(dx, dy);
        let fast = fb.create_block();
        let slow = fb.create_block();
        let cont = fb.create_block();
        fb.append_block_param(cont, types::F64);
        fb.ins().brif(both, fast, &[], slow, &[]);

        fb.switch_to_block(fast);
        fb.seal_block(fast);
        let m = fb.ins().fmul(xf, yf);
        let s1 = fb.ins().fadd(s, m);
        fb.ins().jump(cont, &[s1.into()]);

        fb.switch_to_block(slow);
        fb.seal_block(slow);
        let nanv = fb.ins().f64const(f64::NAN);
        let s2 = fb.ins().fadd(s, nanv);
        fb.ins().jump(cont, &[s2.into()]);

        fb.switch_to_block(cont);
        fb.seal_block(cont);
        let s_next = fb.block_params(cont)[0];
        let i_next = fb.ins().iadd_imm(i, 1);
        fb.ins().jump(header, &[i_next.into(), s_next.into()]);
        fb.seal_block(header);

        epilogue(fb, exit);
    })
}

// ---------------------------------------------------------------------------
// C2 — untagged f64. The floor.
// ---------------------------------------------------------------------------

pub fn c2_native() -> Compiled {
    compile("c2_native", &[], |fb, _im| {
        let (iters, hdr_ptr, mask) = prologue(fb);
        let vals = hdr(fb, hdr_ptr, 1);
        let trusted = MemFlags::trusted();

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
        let idx = fb.ins().band(i, mask);
        let off = fb.ins().imul_imm(idx, 16);
        let vbase = fb.ins().iadd(vals, off);
        let xf = fb.ins().load(types::F64, trusted, vbase, 0);
        let yf = fb.ins().load(types::F64, trusted, vbase, 8);
        let m = fb.ins().fmul(xf, yf);
        let s1 = fb.ins().fadd(s, m);
        let i_next = fb.ins().iadd_imm(i, 1);
        fb.ins().jump(header, &[i_next.into(), s1.into()]);
        fb.seal_block(header);

        epilogue(fb, exit);
    })
}

// --- shared prologue/epilogue ----------------------------------------------

fn prologue(fb: &mut FunctionBuilder) -> (Value, Value, Value) {
    let entry = fb.create_block();
    fb.append_block_params_for_function_params(entry);
    fb.switch_to_block(entry);
    fb.seal_block(entry);
    (
        fb.block_params(entry)[0],
        fb.block_params(entry)[1],
        fb.block_params(entry)[2],
    )
}

fn epilogue(fb: &mut FunctionBuilder, exit: cranelift_codegen::ir::Block) {
    fb.switch_to_block(exit);
    fb.seal_block(exit);
    let out = fb.block_params(exit)[0];
    let raw = emit_box_double(fb, out);
    fb.ins().return_(&[raw]);
}
