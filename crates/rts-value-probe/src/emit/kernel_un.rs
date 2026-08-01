//! Kernel UN — the unary and short-circuit forms: `typeof`, `!`, unary `-`,
//! `?:`, `??`.
//!
//! `typeof` is const-folded by the engine for every statically-known operand
//! (`expr.rs:301-390` folds literals, `Function`, `Symbol`, global classes,
//! `Math`, `Math.<m>`, and unbound idents), so the `__rtsadp_typeof` call is
//! reached only for a genuinely dynamic operand — which is the case measured
//! here.
//!
//! `??` and `?:` are interesting for the opposite reason: they are already pure
//! control flow, so the question is whether the CONDITION test costs anything.
//! `??` tests for null/undefined, which is two integer compares against
//! singleton words — no call needed even today. `?:` on a Tagged condition goes
//! through `__rtsadp_to_boolean` (the BOOL kernel's T rows).

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{InstBuilder, MemFlags, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::{Compiled, Imports, call1, compile, emit_is_double, emit_unbox_double};
use crate::poly;

#[derive(Clone, Copy, PartialEq)]
pub enum Un {
    TypeOf,
    Not,
    Neg,
    Nullish,
}

pub const ALL_UN: [Un; 4] = [Un::TypeOf, Un::Not, Un::Neg, Un::Nullish];

impl Un {
    pub fn symbol(self) -> &'static str {
        match self {
            Un::TypeOf => "probe_typeof",
            Un::Not => "probe_not",
            Un::Neg => "probe_neg",
            // `??` has no trampoline: it is already pure control flow. The row
            // exists to show that, not to compare against a call.
            Un::Nullish => "probe_not",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Un::TypeOf => "typeof",
            Un::Not => "!",
            Un::Neg => "unary -",
            Un::Nullish => "??",
        }
    }
}

/// `for i { s = f(s, A[i&mask]) }` — one operand, `i64` accumulator throughout
/// (these all fold to a count or a summed number).
fn un_loop<F>(name: &str, needed: &[(&'static str, usize)], body: F) -> Compiled
where
    F: FnOnce(&mut FunctionBuilder, &Imports, Value, Value) -> Value + Copy,
{
    compile(name, needed, move |fb, imports| {
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let iters = fb.block_params(entry)[0];
        let hdr_ptr = fb.block_params(entry)[1];
        let mask = fb.block_params(entry)[2];
        let trusted = MemFlags::trusted();
        let arr = fb.ins().load(types::I64, trusted, hdr_ptr, 0);

        let header = fb.create_block();
        fb.append_block_param(header, types::I64);
        fb.append_block_param(header, types::I64);
        let lbody = fb.create_block();
        let exit = fb.create_block();
        fb.append_block_param(exit, types::I64);

        let zero = fb.ins().iconst(types::I64, 0);
        let s0 = fb.ins().iconst(types::I64, 0);
        fb.ins().jump(header, &[zero.into(), s0.into()]);

        fb.switch_to_block(header);
        let i = fb.block_params(header)[0];
        let s = fb.block_params(header)[1];
        let go = fb.ins().icmp(IntCC::SignedLessThan, i, iters);
        fb.ins().brif(go, lbody, &[], exit, &[s.into()]);

        fb.switch_to_block(lbody);
        fb.seal_block(lbody);
        let idx = fb.ins().band(i, mask);
        let off = fb.ins().imul_imm(idx, 8);
        let addr = fb.ins().iadd(arr, off);
        let a = fb.ins().load(types::I64, trusted, addr, 0);

        let s_next = body(fb, imports, a, s);
        let i_next = fb.ins().iadd_imm(i, 1);
        fb.ins().jump(header, &[i_next.into(), s_next.into()]);
        fb.seal_block(header);

        fb.switch_to_block(exit);
        fb.seal_block(exit);
        let out = fb.block_params(exit)[0];
        fb.ins().return_(&[out]);
    })
}

/// Add 1 when the returned word is the `true` singleton (predicates), or the
/// truncated numeric value (unary `-`).
fn fold(fb: &mut FunctionBuilder, un: Un, s: Value, w: Value) -> Value {
    match un {
        Un::Neg => {
            let f = {
                let d = emit_is_double(fb, w);
                let asf = emit_unbox_double(fb, w);
                let low = fb.ins().ireduce(types::I32, w);
                let sext = fb.ins().sextend(types::I64, low);
                let asi = fb.ins().fcvt_from_sint(types::F64, sext);
                fb.ins().select(d, asf, asi)
            };
            let iv = fb.ins().fcvt_to_sint_sat(types::I64, f);
            fb.ins().iadd(s, iv)
        }
        _ => {
            let t = fb.ins().iconst(types::I64, poly::bool_word(true) as i64);
            let is_true = fb.ins().icmp(IntCC::Equal, w, t);
            let one = fb.ins().uextend(types::I64, is_true);
            fb.ins().iadd(s, one)
        }
    }
}

// --- U0: today -------------------------------------------------------------

pub fn u0_today(un: Un) -> Compiled {
    if un == Un::Nullish {
        return u1_guarded(un); // `??` has no call form to compare against
    }
    let needed: Vec<(&'static str, usize)> = vec![(un.symbol(), 1)];
    un_loop("un_u0", &needed, move |fb, im, a, s| {
        let w = call1(fb, im[un.symbol()], &[a]);
        fold(fb, un, s, w)
    })
}

// --- U1: inline ------------------------------------------------------------

pub fn u1_guarded(un: Un) -> Compiled {
    let needed: Vec<(&'static str, usize)> = if un == Un::Nullish {
        vec![]
    } else {
        vec![(un.symbol(), 1)]
    };
    un_loop("un_u1", &needed, move |fb, im, a, s| {
        if un == Un::Nullish {
            // `a ?? b`: two integer compares against the null/undefined
            // singleton words. No call, today or ever — this row is the baseline
            // that shows the operator is already at its floor.
            let nullw = fb
                .ins()
                .iconst(types::I64, poly::encode(poly::TAG_SINGLETON, 1) as i64);
            let undefw = fb
                .ins()
                .iconst(types::I64, poly::encode(poly::TAG_SINGLETON, 0) as i64);
            let is_null = fb.ins().icmp(IntCC::Equal, a, nullw);
            let is_undef = fb.ins().icmp(IntCC::Equal, a, undefw);
            let nullish = fb.ins().bor(is_null, is_undef);
            let one = fb.ins().uextend(types::I64, nullish);
            return fb.ins().iadd(s, one);
        }

        let is_d = emit_is_double(fb, a);
        let fast = fb.create_block();
        let slow = fb.create_block();
        let cont = fb.create_block();
        fb.append_block_param(cont, types::I64);
        fb.ins().brif(is_d, fast, &[], slow, &[]);

        fb.switch_to_block(fast);
        fb.seal_block(fast);
        let s_fast = match un {
            // A word that IS an inline double is a number, full stop — `typeof`
            // needs no dispatch at all once the tag is known.
            Un::TypeOf => {
                let one = fb.ins().iconst(types::I64, 1);
                fb.ins().iadd(s, one)
            }
            Un::Not => {
                let f = emit_unbox_double(fb, a);
                let z = fb.ins().f64const(0.0);
                let nz = fb.ins().fcmp(FloatCC::NotEqual, f, z);
                let ord = fb.ins().fcmp(FloatCC::Equal, f, f);
                let truthy = fb.ins().band(nz, ord);
                // `!x` counts the FALSY ones.
                let falsy = fb.ins().bxor_imm(truthy, 1);
                let one = fb.ins().uextend(types::I64, falsy);
                fb.ins().iadd(s, one)
            }
            Un::Neg => {
                let f = emit_unbox_double(fb, a);
                let n = fb.ins().fneg(f);
                let iv = fb.ins().fcvt_to_sint_sat(types::I64, n);
                fb.ins().iadd(s, iv)
            }
            Un::Nullish => unreachable!("handled above"),
        };
        fb.ins().jump(cont, &[s_fast.into()]);

        fb.switch_to_block(slow);
        fb.seal_block(slow);
        let w = call1(fb, im[un.symbol()], &[a]);
        let s_slow = fold(fb, un, s, w);
        fb.ins().jump(cont, &[s_slow.into()]);

        fb.switch_to_block(cont);
        fb.seal_block(cont);
        fb.block_params(cont)[0]
    })
}
