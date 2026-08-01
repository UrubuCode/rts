//! Kernel OPS — one row per JS operator, three lowerings each.
//!
//! The engine ALREADY lowers every one of these natively when both operands are
//! proven non-Tagged (`binop.rs:492` for `& | ^ << >> >>>`, `lower_compare` for
//! the relationals, `lower_arith` for `+ - * / %`). What it does NOT have is a
//! middle rung: with a Tagged operand it goes straight to `box, box, call`, with
//! no inline test for the secretly-monomorphic case (`binop.rs:596`,
//! `binop_eq.rs:52`). Since `Repr::Ref` is dead, "Tagged" is every value that
//! came off the heap — which is most of them.
//!
//! - **X0 today** — `box`, `box`, `call __rtsadp_*`.
//! - **X1 inline guard** — test both words for the inline-double form, do the op
//!   in IR, call only on a miss. Both arms emitted and reachable.
//! - **X2 proven Repr** — operands already `f64`/`i32` in registers.
//!
//! Operand B comes from a second array addressed through the header's second
//! slot, so both operands are opaque loads and no guard can be folded away.

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{FuncRef, InstBuilder, MemFlags, Type, Value, types};
use cranelift_frontend::FunctionBuilder;

use super::{Compiled, Imports, call1, compile, emit_box_double, emit_is_double, emit_unbox_double};
use crate::poly;

pub mod guards;
pub use guards::{x3_exp_square, x3_mod_int_srem};

/// Which operator a row is measuring.
#[derive(Clone, Copy, PartialEq)]
pub enum Op {
    // equality / relational
    StrictEq,
    StrictNe,
    LooseEq,
    LooseNe,
    Lt,
    Le,
    Gt,
    Ge,
    // arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Exp,
    // bitwise / shift
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    UShr,
}

/// Every binary operator under test, in the order the report prints them.
pub const ALL_OPS: [Op; 20] = [
    Op::StrictEq,
    Op::StrictNe,
    Op::LooseEq,
    Op::LooseNe,
    Op::Lt,
    Op::Le,
    Op::Gt,
    Op::Ge,
    Op::Add,
    Op::Sub,
    Op::Mul,
    Op::Div,
    Op::Mod,
    Op::Exp,
    Op::BitAnd,
    Op::BitOr,
    Op::BitXor,
    Op::Shl,
    Op::Shr,
    Op::UShr,
];

impl Op {
    pub fn symbol(self) -> &'static str {
        match self {
            Op::StrictEq => "probe_strict_eq",
            Op::StrictNe => "probe_strict_neq",
            Op::LooseEq => "probe_loose_eq",
            Op::LooseNe => "probe_loose_neq",
            Op::Lt => "probe_lt",
            Op::Le => "probe_le",
            Op::Gt => "probe_gt",
            Op::Ge => "probe_ge",
            Op::Add => "probe_add",
            Op::Sub => "probe_sub",
            Op::Mul => "probe_mul",
            Op::Div => "probe_div",
            Op::Mod => "probe_mod",
            Op::Exp => "probe_pow",
            Op::BitAnd => "probe_band",
            Op::BitOr => "probe_bor",
            Op::BitXor => "probe_bxor",
            Op::Shl => "probe_shl",
            Op::Shr => "probe_shr",
            Op::UShr => "probe_ushr",
        }
    }

    /// Comparisons return a singleton bool word; the rest return a number word.
    pub fn is_predicate(self) -> bool {
        matches!(
            self,
            Op::StrictEq | Op::StrictNe | Op::LooseEq | Op::LooseNe | Op::Lt | Op::Le | Op::Gt | Op::Ge
        )
    }

    /// Ops whose "native" form is still a CALL, because Cranelift has no
    /// instruction for them. Proving the Repr does not remove the call.
    #[allow(dead_code)] // documents WHICH ops keep a call even when proven
    pub fn native_is_call(self) -> bool {
        matches!(self, Op::Mod | Op::Exp)
    }

    pub fn label(self) -> &'static str {
        match self {
            Op::StrictEq => "===",
            Op::StrictNe => "!==",
            Op::LooseEq => "==",
            Op::LooseNe => "!=",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
            Op::Div => "/",
            Op::Mod => "%",
            Op::Exp => "**",
            Op::BitAnd => "&",
            Op::BitOr => "|",
            Op::BitXor => "^",
            Op::Shl => "<<",
            Op::Shr => ">>",
            Op::UShr => ">>>",
        }
    }
}

/// The accumulator type a row uses: a predicate counts hits in an `i64`, an
/// arithmetic op sums the results as `f64`.
fn acc_ty(op: Op) -> Type {
    if op.is_predicate() {
        types::I64
    } else {
        types::F64
    }
}

/// Shared skeleton: `for i { let a = A[i&mask]; let b = B[i&mask]; s = f(s,a,b) }`.
pub(super) fn ops_loop<F>(name: &str, op: Op, needed: &[(&'static str, usize)], body: F) -> Compiled
where
    F: FnOnce(&mut FunctionBuilder, &Imports, OpVals) -> Value + Copy,
{
    let s_ty = acc_ty(op);
    compile(name, needed, move |fb, imports| {
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let iters = fb.block_params(entry)[0];
        let hdr_ptr = fb.block_params(entry)[1];
        let mask = fb.block_params(entry)[2];
        let trusted = MemFlags::trusted();
        let arr_a = fb.ins().load(types::I64, trusted, hdr_ptr, 0);
        let arr_b = fb.ins().load(types::I64, trusted, hdr_ptr, 8);

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
        let idx = fb.ins().band(i, mask);
        let off = fb.ins().imul_imm(idx, 8);
        let aa = fb.ins().iadd(arr_a, off);
        let ab = fb.ins().iadd(arr_b, off);
        let a = fb.ins().load(types::I64, trusted, aa, 0);
        let b = fb.ins().load(types::I64, trusted, ab, 0);

        let s_next = body(fb, imports, OpVals { a, b, s });
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

pub struct OpVals {
    pub a: Value,
    pub b: Value,
    pub s: Value,
}

/// Fold a result word into the accumulator: a predicate adds `1` when the
/// singleton word is `true`, an arithmetic op unboxes and adds.
pub(super) fn fold_word(fb: &mut FunctionBuilder, op: Op, s: Value, w: Value) -> Value {
    if op.is_predicate() {
        let t = fb.ins().iconst(types::I64, poly::bool_word(true) as i64);
        let is_true = fb.ins().icmp(IntCC::Equal, w, t);
        let one = fb.ins().uextend(types::I64, is_true);
        fb.ins().iadd(s, one)
    } else {
        // A number word is either an inline double or a tagged int32; the probe's
        // guarded/native rows produce doubles, and the generic trampoline may
        // re-tighten to int32, so normalise through the same test.
        let f = emit_word_to_f64(fb, w);
        fb.ins().fadd(s, f)
    }
}

/// `is_double(w) ? bitcast(w) : (f64)(i32)low32(w)` — the inline counterpart of
/// `to_number` for the two number forms.
fn emit_word_to_f64(fb: &mut FunctionBuilder, w: Value) -> Value {
    let d = emit_is_double(fb, w);
    let asf = emit_unbox_double(fb, w);
    let low = fb.ins().ireduce(types::I32, w);
    let sext = fb.ins().sextend(types::I64, low);
    let asi = fb.ins().fcvt_from_sint(types::F64, sext);
    fb.ins().select(d, asf, asi)
}

/// Fold a native `f64`/bool result (the X2 rows) into the accumulator.
fn fold_native(fb: &mut FunctionBuilder, op: Op, s: Value, v: Value) -> Value {
    if op.is_predicate() {
        let one = fb.ins().uextend(types::I64, v);
        fb.ins().iadd(s, one)
    } else {
        fb.ins().fadd(s, v)
    }
}

// ---------------------------------------------------------------------------
// X0 — today: box, box, call.
// ---------------------------------------------------------------------------

pub fn x0_today(op: Op) -> Compiled {
    let needed: Vec<(&'static str, usize)> = vec![(op.symbol(), 2)];
    ops_loop("ops_x0", op, &needed, move |fb, im, v| {
        let w = call1(fb, im[op.symbol()], &[v.a, v.b]);
        fold_word(fb, op, v.s, w)
    })
}

// ---------------------------------------------------------------------------
// X1 — inline guard on the inline-double form, generic call on a miss.
// ---------------------------------------------------------------------------

pub fn x1_guarded(op: Op) -> Compiled {
    let mut needed: Vec<(&'static str, usize)> = vec![(op.symbol(), 2)];
    // `%` and `**` still need a call even on the fast path — Cranelift has
    // neither `frem` nor `pow`. What the guard removes there is the boxing and
    // the ToNumber dispatch, not the call: both raw forms take `f64` in and out.
    if op == Op::Mod {
        needed.push(("probe_fmod_f64", 2));
    }
    if op == Op::Exp {
        needed.push(("probe_pow_f64", 2));
    }
    ops_loop("ops_x1", op, &needed, move |fb, im, v| {
        let da = emit_is_double(fb, v.a);
        let db = emit_is_double(fb, v.b);
        let both = fb.ins().band(da, db);

        let fast = fb.create_block();
        let slow = fb.create_block();
        let cont = fb.create_block();
        fb.append_block_param(cont, acc_ty(op));
        fb.ins().brif(both, fast, &[], slow, &[]);

        fb.switch_to_block(fast);
        fb.seal_block(fast);
        let af = emit_unbox_double(fb, v.a);
        let bf = emit_unbox_double(fb, v.b);
        let s_fast = emit_native_op(fb, im, op, af, bf, v.s);
        fb.ins().jump(cont, &[s_fast.into()]);

        fb.switch_to_block(slow);
        fb.seal_block(slow);
        let w = call1(fb, im[op.symbol()], &[v.a, v.b]);
        let s_slow = fold_word(fb, op, v.s, w);
        fb.ins().jump(cont, &[s_slow.into()]);

        fb.switch_to_block(cont);
        fb.seal_block(cont);
        fb.block_params(cont)[0]
    })
}

// ---------------------------------------------------------------------------
// X2 — proven Repr: the operands arrive as f64 already (no tag, no guard).
// ---------------------------------------------------------------------------

pub fn x2_proven(op: Op) -> Compiled {
    let needed: Vec<(&'static str, usize)> = match op {
        Op::Mod => vec![("probe_fmod_f64", 2)],
        Op::Exp => vec![("probe_pow_f64", 2)],
        _ => vec![],
    };
    ops_loop("ops_x2", op, &needed, move |fb, im, v| {
        // The arrays hold boxed doubles; a proven-Repr front-end would have kept
        // them unboxed, so the bitcast here is the free half of that (the egraph
        // folds it against the store side in real code).
        let af = emit_unbox_double(fb, v.a);
        let bf = emit_unbox_double(fb, v.b);
        emit_native_op(fb, im, op, af, bf, v.s)
    })
}

/// The op itself, on two native `f64`s, folded into the accumulator.
fn emit_native_op(
    fb: &mut FunctionBuilder,
    im: &Imports,
    op: Op,
    af: Value,
    bf: Value,
    s: Value,
) -> Value {
    match op {
        Op::StrictEq | Op::LooseEq => {
            let c = fb.ins().fcmp(FloatCC::Equal, af, bf);
            fold_native(fb, op, s, c)
        }
        Op::StrictNe | Op::LooseNe => {
            let c = fb.ins().fcmp(FloatCC::NotEqual, af, bf);
            fold_native(fb, op, s, c)
        }
        Op::Lt => {
            let c = fb.ins().fcmp(FloatCC::LessThan, af, bf);
            fold_native(fb, op, s, c)
        }
        Op::Le => {
            let c = fb.ins().fcmp(FloatCC::LessThanOrEqual, af, bf);
            fold_native(fb, op, s, c)
        }
        Op::Gt => {
            let c = fb.ins().fcmp(FloatCC::GreaterThan, af, bf);
            fold_native(fb, op, s, c)
        }
        Op::Ge => {
            let c = fb.ins().fcmp(FloatCC::GreaterThanOrEqual, af, bf);
            fold_native(fb, op, s, c)
        }
        Op::Add => {
            let r = fb.ins().fadd(af, bf);
            fold_native(fb, op, s, r)
        }
        Op::Sub => {
            let r = fb.ins().fsub(af, bf);
            fold_native(fb, op, s, r)
        }
        Op::Mul => {
            let r = fb.ins().fmul(af, bf);
            fold_native(fb, op, s, r)
        }
        Op::Div => {
            let r = fb.ins().fdiv(af, bf);
            fold_native(fb, op, s, r)
        }
        Op::Mod => {
            let r = call_f64(fb, im["probe_fmod_f64"], af, bf);
            fold_native(fb, op, s, r)
        }
        Op::Exp => {
            let r = call_f64(fb, im["probe_pow_f64"], af, bf);
            fold_native(fb, op, s, r)
        }
        Op::BitAnd | Op::BitOr | Op::BitXor | Op::Shl | Op::Shr | Op::UShr => {
            let x = emit_to_int32(fb, af);
            let y = emit_to_int32(fb, bf);
            let r32 = match op {
                Op::BitAnd => fb.ins().band(x, y),
                Op::BitOr => fb.ins().bor(x, y),
                Op::BitXor => fb.ins().bxor(x, y),
                Op::Shl => {
                    let sh = fb.ins().band_imm(y, 31);
                    fb.ins().ishl(x, sh)
                }
                Op::Shr => {
                    let sh = fb.ins().band_imm(y, 31);
                    fb.ins().sshr(x, sh)
                }
                _ => {
                    let sh = fb.ins().band_imm(y, 31);
                    fb.ins().ushr(x, sh)
                }
            };
            let wide = if op == Op::UShr {
                fb.ins().uextend(types::I64, r32)
            } else {
                fb.ins().sextend(types::I64, r32)
            };
            let f = fb.ins().fcvt_from_sint(types::F64, wide);
            fold_native(fb, op, s, f)
        }
    }
}


/// `emit_to_int32` verbatim from `binop.rs:472-490` — the Float64 arm, including
/// the `!is_finite → 0` select that `fcvt_to_sint_sat` alone gets wrong.
pub(super) fn emit_to_int32(fb: &mut FunctionBuilder, f: Value) -> Value {
    let conv = fb.ins().fcvt_to_sint_sat(types::I64, f);
    let low = fb.ins().ireduce(types::I32, conv);
    let diff = fb.ins().fsub(f, f);
    let zero_f = fb.ins().f64const(0.0);
    let finite = fb.ins().fcmp(FloatCC::Equal, diff, zero_f);
    let zero_i = fb.ins().iconst(types::I32, 0);
    fb.ins().select(finite, low, zero_i)
}

pub(super) fn call_f64(fb: &mut FunctionBuilder, f: FuncRef, a: Value, b: Value) -> Value {
    let inst = fb.ins().call(f, &[a, b]);
    fb.inst_results(inst)[0]
}
