//! Kernel PRIM — `boolean` and integer `number`, the two primordials whose cost
//! is decided by what the front-end EMITS rather than by how the heap is laid
//! out.
//!
//! **Boolean.** `if (x)` on a `Repr::Tagged` value lowers to an unconditional
//! `call __rtsadp_to_boolean` (`call.rs:1807-1812`). No tag test in front of it.
//! - `T0` today: the call.
//! - `T1` inline test for the common case, call only on a miss.
//! - `T2` proven `Repr::Bool`: no test at all.
//!
//! **Integer.** A tagged int32 `+` goes through `__rtsadp_add` →
//! `to_number` (int32 → `f64`) → `f64` add → `number_result` (exactness check →
//! re-narrow to int32) → rebox. The native `iadd` never happens.
//! - `N0` today: the call.
//! - `N1` inline int32 guard → `iadd` → rebox, call on a miss.
//! - `N2` proven `Repr::Int32`: plain `iadd`.

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{InstBuilder, types};

use super::{Compiled, call1, emit_is_double, emit_unbox_double, loop_kernel};
use crate::poly;

const TO_BOOLEAN: (&str, usize) = ("probe_to_boolean", 1);
const ADP_IADD: (&str, usize) = ("probe_adp_iadd", 2);

// --- boolean ---------------------------------------------------------------

/// `s += x ? 1 : 0` where `x` is a Tagged word read from memory.
pub fn t0_call_to_boolean() -> Compiled {
    loop_kernel("prim_t0_call", &[TO_BOOLEAN], types::I64, |fb, im, v| {
        let b = call1(fb, im["probe_to_boolean"], &[v.payload]);
        fb.ins().iadd(v.s, b)
    })
}

/// The inline arm: a genuine double is truthy iff `x != 0 && x == x`, which is
/// two FP compares — no call. Anything else falls to the generic call.
pub fn t1_inline_guard() -> Compiled {
    loop_kernel(
        "prim_t1_inline_guard",
        &[TO_BOOLEAN],
        types::I64,
        |fb, im, v| {
            let is_d = emit_is_double(fb, v.payload);
            let fast = fb.create_block();
            let slow = fb.create_block();
            let cont = fb.create_block();
            fb.append_block_param(cont, types::I64);
            fb.ins().brif(is_d, fast, &[], slow, &[]);

            fb.switch_to_block(fast);
            fb.seal_block(fast);
            let f = emit_unbox_double(fb, v.payload);
            let zero = fb.ins().f64const(0.0);
            let nz = fb.ins().fcmp(FloatCC::NotEqual, f, zero);
            let ord = fb.ins().fcmp(FloatCC::Equal, f, f);
            let both = fb.ins().band(nz, ord);
            let bw = fb.ins().uextend(types::I64, both);
            fb.ins().jump(cont, &[bw.into()]);

            fb.switch_to_block(slow);
            fb.seal_block(slow);
            let bs = call1(fb, im["probe_to_boolean"], &[v.payload]);
            fb.ins().jump(cont, &[bs.into()]);

            fb.switch_to_block(cont);
            fb.seal_block(cont);
            let b = fb.block_params(cont)[0];
            fb.ins().iadd(v.s, b)
        },
    )
}

/// A proven `Repr::Bool` is already 0/1 in a register.
pub fn t2_native_bool() -> Compiled {
    loop_kernel("prim_t2_native", &[], types::I64, |fb, _im, v| {
        let zero = fb.ins().iconst(types::I64, 0);
        let b = fb.ins().icmp(IntCC::NotEqual, v.payload, zero);
        let bw = fb.ins().uextend(types::I64, b);
        fb.ins().iadd(v.s, bw)
    })
}

// --- integer ---------------------------------------------------------------

/// `s = s + a[j]` on TAGGED INT32 words, through the generic operator.
pub fn n0_call_generic() -> Compiled {
    loop_kernel("prim_n0_call", &[ADP_IADD], types::I64, |fb, im, v| {
        call1(fb, im["probe_adp_iadd"], &[v.s, v.payload])
    })
}

/// Inline int32 fast path: check both operands carry `TAG_INT32`, add the low
/// 32 bits, re-encode. NOTE the honest caveat — a real engine also needs an
/// overflow check here (an int32 sum that leaves range must become a double);
/// this variant omits it, so it is a slight LOWER bound, not an exact one.
pub fn n1_inline_int32() -> Compiled {
    loop_kernel(
        "prim_n1_inline_int32",
        &[ADP_IADD],
        types::I64,
        |fb, im, v| {
            let header = poly::encode(poly::TAG_INT32, 0) as i64;
            let hdr_v = fb.ins().iconst(types::I64, header);
            let mask = fb.ins().iconst(types::I64, 0xFFFF_FFFFu32 as i64);

            // is_int32(w) := (w & ~PAYLOAD32) == BOX_BASE|TAG_INT32<<48
            let tagmask = fb
                .ins()
                .iconst(types::I64, !(0xFFFF_FFFFu64 as i64));
            let ta = fb.ins().band(v.s, tagmask);
            let tb = fb.ins().band(v.payload, tagmask);
            let ia = fb.ins().icmp(IntCC::Equal, ta, hdr_v);
            let ib = fb.ins().icmp(IntCC::Equal, tb, hdr_v);
            let both = fb.ins().band(ia, ib);

            let fast = fb.create_block();
            let slow = fb.create_block();
            let cont = fb.create_block();
            fb.append_block_param(cont, types::I64);
            fb.ins().brif(both, fast, &[], slow, &[]);

            fb.switch_to_block(fast);
            fb.seal_block(fast);
            let xa = fb.ins().band(v.s, mask);
            let xb = fb.ins().band(v.payload, mask);
            let sum = fb.ins().iadd(xa, xb);
            let sum32 = fb.ins().band(sum, mask);
            let out = fb.ins().bor(sum32, hdr_v);
            fb.ins().jump(cont, &[out.into()]);

            fb.switch_to_block(slow);
            fb.seal_block(slow);
            let rs = call1(fb, im["probe_adp_iadd"], &[v.s, v.payload]);
            fb.ins().jump(cont, &[rs.into()]);

            fb.switch_to_block(cont);
            fb.seal_block(cont);
            fb.block_params(cont)[0]
        },
    )
}

/// The proven-`Repr` baseline. It is `Float64`, not `Int32`, on purpose: every
/// TS `number` IS an `f64`, and the tagged-int32 form is an optimization layered
/// on top — so the honest "what if the front-end had proved this" floor is a
/// plain `fadd`, and it also returns a word `poly::to_number` can validate.
pub fn n2_native_num() -> Compiled {
    loop_kernel("prim_n2_native", &[], types::F64, |fb, _im, v| {
        let f = fb.ins().fcvt_from_sint(types::F64, v.idx);
        fb.ins().fadd(v.s, f)
    })
}
