//! Kernel NUM — what each NUMERIC WIDTH is actually good at.
//!
//! The question this answers: "could RTS use `f64`/`f32` where it uses
//! `i64`/`i32` today?" A JS `number` is an `f64` by spec and a handle is a
//! `u64`, so the value model itself is not up for negotiation — but the engine
//! also moves a lot of numbers that are NOT user values (element buffers,
//! counters, bulk data), and for those the width is a free choice. This kernel
//! prices that choice on four different shapes, because the answer is not the
//! same for all of them:
//!
//! | shape | what decides the winner |
//! |---|---|
//! | `chain` | LATENCY of one op — the accumulator feeds the next iteration |
//! | `indep` | THROUGHPUT — four independent accumulators, ports not latency |
//! | `bulk` | MEMORY BANDWIDTH — bytes per element dominates the arithmetic |
//! | `simd` | LANES PER VECTOR — 4x`f32` against 2x`f64` in the same register |
//! | `conv` | the cost of crossing between the two domains at all |
//!
//! Every variant runs the same iteration count over the same ring of elements,
//! so a row is only comparable to the rows in its own shape — never across
//! shapes (a `bulk` row touches memory, a `chain` row does not).

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, MemFlags, Type, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::{Compiled, compile};

/// Zero of `ty`, for whichever domain it belongs to.
fn zero_of(fb: &mut FunctionBuilder, ty: Type) -> Value {
    match ty {
        types::F32 => fb.ins().f32const(0.0),
        types::F64 => fb.ins().f64const(0.0),
        _ => fb.ins().iconst(ty, 0),
    }
}

/// `one` of `ty`.
fn one_of(fb: &mut FunctionBuilder, ty: Type) -> Value {
    match ty {
        types::F32 => fb.ins().f32const(1.0),
        types::F64 => fb.ins().f64const(1.0),
        _ => fb.ins().iconst(ty, 1),
    }
}

fn add_of(fb: &mut FunctionBuilder, ty: Type, a: Value, b: Value) -> Value {
    if ty.is_float() {
        fb.ins().fadd(a, b)
    } else {
        fb.ins().iadd(a, b)
    }
}

/// Reduce the accumulator to the `i64` the harness checksums. Floats go through
/// `fcvt_to_sint_sat` (never traps), ints through extend/identity.
fn to_i64(fb: &mut FunctionBuilder, ty: Type, v: Value) -> Value {
    match ty {
        types::F32 | types::F64 => fb.ins().fcvt_to_sint_sat(types::I64, v),
        types::I32 => fb.ins().sextend(types::I64, v),
        _ => v,
    }
}

/// SCALAR: `n` independent accumulator chains of `ty`, each doing one add per
/// iteration against a value derived from the loop counter (so nothing is
/// loop-invariant and nothing can be hoisted).
///
/// `n = 1` measures LATENCY (every add waits for the previous one); `n = 4`
/// measures THROUGHPUT (four chains keep the units fed).
pub fn scalar_chain(name: &str, ty: Type, n: usize) -> Compiled {
    compile(name, &[], move |fb, _im| {
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let iters = fb.block_params(entry)[0];

        let header = fb.create_block();
        fb.append_block_param(header, types::I64);
        for _ in 0..n {
            fb.append_block_param(header, ty);
        }
        let body = fb.create_block();
        let exit = fb.create_block();
        for _ in 0..n {
            fb.append_block_param(exit, ty);
        }

        let i0 = fb.ins().iconst(types::I64, 0);
        let z = zero_of(fb, ty);
        let mut args: Vec<cranelift_codegen::ir::BlockArg> = vec![i0.into()];
        for _ in 0..n {
            args.push(z.into());
        }
        fb.ins().jump(header, &args);

        fb.switch_to_block(header);
        let i = fb.block_params(header)[0];
        let accs: Vec<Value> = (0..n).map(|k| fb.block_params(header)[1 + k]).collect();
        let go = fb.ins().icmp(IntCC::SignedLessThan, i, iters);
        let exit_args: Vec<_> = accs.iter().map(|a| (*a).into()).collect();
        fb.ins().brif(go, body, &[], exit, &exit_args);

        fb.switch_to_block(body);
        fb.seal_block(body);
        // The addend comes from the counter, converted into the accumulator's
        // domain ONCE — the conversion is not what this shape is measuring.
        let x = match ty {
            types::F32 => {
                let d = fb.ins().fcvt_from_sint(types::F32, i);
                d
            }
            types::F64 => fb.ins().fcvt_from_sint(types::F64, i),
            types::I32 => fb.ins().ireduce(types::I32, i),
            _ => i,
        };
        let mut next: Vec<_> = Vec::with_capacity(n + 1);
        let one = one_of(fb, types::I64);
        let ni = fb.ins().iadd(i, one);
        next.push(ni.into());
        for a in &accs {
            let v = add_of(fb, ty, *a, x);
            next.push(v.into());
        }
        fb.ins().jump(header, &next);

        fb.switch_to_block(exit);
        fb.seal_block(exit);
        fb.seal_block(header);
        let outs: Vec<Value> = (0..n).map(|k| fb.block_params(exit)[k]).collect();
        let mut sum = outs[0];
        for o in &outs[1..] {
            sum = add_of(fb, ty, sum, *o);
        }
        let r = to_i64(fb, ty, sum);
        fb.ins().return_(&[r]);
    })
}

/// BULK: `s += a[i & mask]` over an array of `elem` — the shape where the
/// element WIDTH decides how many cache lines the loop drags in. The
/// accumulator is always the widest type of the element's own domain, so the
/// only variable is the load.
pub fn bulk_sum(name: &str, elem: Type) -> Compiled {
    let acc_ty = if elem.is_float() {
        types::F64
    } else {
        types::I64
    };
    let elem_size = elem.bytes() as i64;
    compile(name, &[], move |fb, _im| {
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let iters = fb.block_params(entry)[0];
        let base = fb.block_params(entry)[1];
        let mask = fb.block_params(entry)[2];

        let header = fb.create_block();
        fb.append_block_param(header, types::I64);
        fb.append_block_param(header, acc_ty);
        let body = fb.create_block();
        let exit = fb.create_block();
        fb.append_block_param(exit, acc_ty);

        let i0 = fb.ins().iconst(types::I64, 0);
        let z = zero_of(fb, acc_ty);
        fb.ins().jump(header, &[i0.into(), z.into()]);

        fb.switch_to_block(header);
        let i = fb.block_params(header)[0];
        let s = fb.block_params(header)[1];
        let go = fb.ins().icmp(IntCC::SignedLessThan, i, iters);
        fb.ins().brif(go, body, &[], exit, &[s.into()]);

        fb.switch_to_block(body);
        fb.seal_block(body);
        let idx = fb.ins().band(i, mask);
        let off = fb.ins().imul_imm(idx, elem_size);
        let addr = fb.ins().iadd(base, off);
        let raw = fb.ins().load(elem, MemFlags::trusted(), addr, 0);
        // Widen into the accumulator's type: f32 -> f64, i32 -> i64.
        let widened = match elem {
            types::F32 => fb.ins().fpromote(types::F64, raw),
            types::I32 => fb.ins().sextend(types::I64, raw),
            _ => raw,
        };
        let ns = add_of(fb, acc_ty, s, widened);
        let one = fb.ins().iconst(types::I64, 1);
        let ni = fb.ins().iadd(i, one);
        fb.ins().jump(header, &[ni.into(), ns.into()]);

        fb.switch_to_block(exit);
        fb.seal_block(exit);
        fb.seal_block(header);
        let out = fb.block_params(exit)[0];
        let r = to_i64(fb, acc_ty, out);
        fb.ins().return_(&[r]);
    })
}

/// SIMD: the same bulk sum, one VECTOR per iteration. `vec` is `F32X4` /
/// `F64X2` / `I32X4` / `I64X2`, so this is where "half the bytes" turns into
/// "twice the lanes" — the one place a narrower type buys real parallelism
/// rather than just smaller loads.
pub fn simd_sum(name: &str, vec: Type) -> Compiled {
    let lane = vec.lane_type();
    let stride = vec.bytes() as i64;
    compile(name, &[], move |fb, _im| {
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let iters = fb.block_params(entry)[0];
        let base = fb.block_params(entry)[1];
        let mask = fb.block_params(entry)[2];

        let header = fb.create_block();
        fb.append_block_param(header, types::I64);
        fb.append_block_param(header, vec);
        let body = fb.create_block();
        let exit = fb.create_block();
        fb.append_block_param(exit, vec);

        let i0 = fb.ins().iconst(types::I64, 0);
        let zl = zero_of(fb, lane);
        let z = fb.ins().splat(vec, zl);
        fb.ins().jump(header, &[i0.into(), z.into()]);

        fb.switch_to_block(header);
        let i = fb.block_params(header)[0];
        let s = fb.block_params(header)[1];
        let go = fb.ins().icmp(IntCC::SignedLessThan, i, iters);
        fb.ins().brif(go, body, &[], exit, &[s.into()]);

        fb.switch_to_block(body);
        fb.seal_block(body);
        let idx = fb.ins().band(i, mask);
        let off = fb.ins().imul_imm(idx, stride);
        let addr = fb.ins().iadd(base, off);
        let v = fb.ins().load(vec, MemFlags::trusted(), addr, 0);
        let ns = add_of(fb, lane, s, v);
        let one = fb.ins().iconst(types::I64, 1);
        let ni = fb.ins().iadd(i, one);
        fb.ins().jump(header, &[ni.into(), ns.into()]);

        fb.switch_to_block(exit);
        fb.seal_block(exit);
        fb.seal_block(header);
        let out = fb.block_params(exit)[0];
        // Horizontal reduce: extract each lane and add them in the lane domain.
        let mut acc = fb.ins().extractlane(out, 0);
        for l in 1..vec.lane_count() {
            let e = fb.ins().extractlane(out, l as u8);
            acc = add_of(fb, lane, acc, e);
        }
        let r = to_i64(fb, lane, acc);
        fb.ins().return_(&[r]);
    })
}

/// CONVERSION: `s += (i64)((f64)i + 1.0)` — one int→float and one float→int per
/// iteration. This is the tax any design pays for keeping a value in the wrong
/// domain for the operation it is about to do, and it is the number that decides
/// whether a mixed representation is worth it at all.
pub fn conv_roundtrip(name: &str, float: Type) -> Compiled {
    compile(name, &[], move |fb, _im| {
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let iters = fb.block_params(entry)[0];

        let header = fb.create_block();
        fb.append_block_param(header, types::I64);
        fb.append_block_param(header, types::I64);
        let body = fb.create_block();
        let exit = fb.create_block();
        fb.append_block_param(exit, types::I64);

        let i0 = fb.ins().iconst(types::I64, 0);
        let z = fb.ins().iconst(types::I64, 0);
        fb.ins().jump(header, &[i0.into(), z.into()]);

        fb.switch_to_block(header);
        let i = fb.block_params(header)[0];
        let s = fb.block_params(header)[1];
        let go = fb.ins().icmp(IntCC::SignedLessThan, i, iters);
        fb.ins().brif(go, body, &[], exit, &[s.into()]);

        fb.switch_to_block(body);
        fb.seal_block(body);
        let f = fb.ins().fcvt_from_sint(float, i);
        let one = one_of(fb, float);
        let g = fb.ins().fadd(f, one);
        let back = fb.ins().fcvt_to_sint_sat(types::I64, g);
        let ns = fb.ins().iadd(s, back);
        let onei = fb.ins().iconst(types::I64, 1);
        let ni = fb.ins().iadd(i, onei);
        fb.ins().jump(header, &[ni.into(), ns.into()]);

        fb.switch_to_block(exit);
        fb.seal_block(exit);
        fb.seal_block(header);
        let out = fb.block_params(exit)[0];
        fb.ins().return_(&[out]);
    })
}
