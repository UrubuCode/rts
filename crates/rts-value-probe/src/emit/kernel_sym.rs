//! Kernel SYM — "let Cranelift only ORDER calls to precompiled native symbols".
//!
//! The architecture under test: instead of Cranelift generating the work inline,
//! the work lives in precompiled native symbols (LLVM-compiled Rust) and
//! Cranelift emits nothing but the sequencing. The symbol table already resolves
//! those to absolute addresses at module-build time, so the call is a direct
//! `call` with zero runtime lookup — the architecture gets its best case.
//!
//! The variable is HOW MUCH WORK each call does. `k = 1` is today's engine (a
//! call per element). `k = n` is one call for the whole loop. Somewhere between
//! the two, the call overhead amortizes; this kernel finds where.
//!
//! The honest boundary of the idea, which no measurement can move: a
//! superinstruction can only be precompiled for a shape known in advance
//! (`sum`, `map`, `indexOf`). User code — `s = s + a[i] * 2 - b[i]` — is by
//! definition not known in advance, and only the code generator can emit it.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, MemFlags, types};
use cranelift_frontend::FunctionBuilder;

use super::{Compiled, compile};

const CHUNK_ADD: (&str, usize) = ("probe_chunk_add", 4);
const CHUNK_RAW: (&str, usize) = ("probe_chunk_add_raw", 4);

/// `for i in (0..iters).step_by(k) { acc = call sym(acc, arg, i, k) }`.
///
/// `k` arrives as a runtime parameter so ONE compiled kernel serves every chunk
/// size — the comparison across `k` is then a pure measure of call amortization
/// with the emitted code held fixed.
fn chunked(name: &str, sym: (&'static str, usize), from_handle: bool) -> Compiled {
    compile(name, &[sym], move |fb, im| {
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let iters = fb.block_params(entry)[0];
        let hdr_ptr = fb.block_params(entry)[1];
        let k = fb.block_params(entry)[2];

        // hdr = [payload_handle, data_base]; the locked symbol wants the handle,
        // the raw one wants the base address.
        let off = if from_handle { 0 } else { 8 };
        let arg = fb.ins().load(types::I64, MemFlags::trusted(), hdr_ptr, off);
        let callee = im[sym.0];

        let header = fb.create_block();
        fb.append_block_param(header, types::I64); // i
        fb.append_block_param(header, types::I64); // acc, as f64 bits
        let lbody = fb.create_block();
        let exit = fb.create_block();
        fb.append_block_param(exit, types::I64);

        let zero = fb.ins().iconst(types::I64, 0);
        let acc0 = fb.ins().iconst(types::I64, 0); // +0.0 bits
        fb.ins().jump(header, &[zero.into(), acc0.into()]);

        fb.switch_to_block(header);
        let i = fb.block_params(header)[0];
        let acc = fb.block_params(header)[1];
        let go = fb.ins().icmp(IntCC::SignedLessThan, i, iters);
        fb.ins().brif(go, lbody, &[], exit, &[acc.into()]);

        fb.switch_to_block(lbody);
        fb.seal_block(lbody);
        let inst = fb.ins().call(callee, &[acc, arg, i, k]);
        let acc_next = fb.inst_results(inst)[0];
        let i_next = fb.ins().iadd(i, k);
        fb.ins().jump(header, &[i_next.into(), acc_next.into()]);
        fb.seal_block(header);

        fb.switch_to_block(exit);
        fb.seal_block(exit);
        let out = fb.block_params(exit)[0];
        fb.ins().return_(&[out]);
    })
}

/// The superinstruction reached through the slab handle — one lock per CALL, so
/// a bigger `k` also amortizes the lock.
pub fn s_chunk_locked() -> Compiled {
    chunked("sym_chunk_locked", CHUNK_ADD, true)
}

/// The superinstruction with no container at all: a raw base pointer. This is
/// the ceiling of the architecture.
pub fn s_chunk_raw() -> Compiled {
    chunked("sym_chunk_raw", CHUNK_RAW, false)
}

/// Cranelift emits the loop itself: a `load` and an `fadd` per element, no call
/// anywhere. The thing the architecture proposes to replace.
pub fn s_inline() -> Compiled {
    compile("sym_inline", &[], move |fb: &mut FunctionBuilder, _im| {
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let iters = fb.block_params(entry)[0];
        let hdr_ptr = fb.block_params(entry)[1];
        let base = fb.ins().load(types::I64, MemFlags::trusted(), hdr_ptr, 8);

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
        let off = fb.ins().imul_imm(i, 8);
        let addr = fb.ins().iadd(base, off);
        let flags = MemFlags::trusted().with_readonly();
        let x = fb.ins().load(types::F64, flags, addr, 0);
        let s_next = fb.ins().fadd(s, x);
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
