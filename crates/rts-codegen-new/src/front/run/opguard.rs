//! Inline TAG GUARDS in front of the generic operator trampolines
//! (`RTS_OPTIMIZATION.md` §5 Tier 2.1 — "the cheapest real lever").
//!
//! ## The gap this closes
//!
//! When either operand of an arithmetic or relational operator is `Repr::Tagged`,
//! [`super::binop`] boxes both sides and calls `__rtsadp_add`/`_sub`/`_mul`/`_div`
//! (or `_lt`/`_le`/`_gt`/`_ge`) **unconditionally**. Since `Repr::Ref` is dead,
//! "Tagged" is every value that came off the heap — an `any` param, a callback
//! parameter, a field read, a call return — so ordinary monomorphic TypeScript
//! pays a full call plus two `ToNumber`s per operator.
//!
//! `RTS_OPTIMIZATION.md` §3.3 measures the ladder: `+ - * /` cost **5.30–5.94**
//! today, **1.36–1.39** with an inline guard, **0.69–1.13** with a proven `Repr`;
//! `< <= > >=` are **2.97–3.20** → **1.37–1.40** → **0.91**. The guard therefore
//! recovers 60–75% of the gap **with no type analysis at all** — no shapes, no
//! escape analysis, purely local IR at the operator site. (Those are the doc's
//! numbers, not a measurement of this module; see the `TODO(measure)` below.)
//!
//! TODO(measure): re-run the operator microbench with `RTS_OP_GUARD=1` vs `=0` on
//! one release binary and record the delta here.
//!
//! ## The shape of the emission
//!
//! ```text
//!     if is_number(a) && is_number(b) {      // fallthrough — the common case
//!         <native fadd/fsub/fmul/fdiv/fcmp>  // no call, no ToNumber
//!     } else {
//!         <the existing __rtsadp_* call>     // cold block, taken jump
//!     }
//! ```
//!
//! Four design constraints, all from §5 Tier 2.1 and all deliberate:
//!
//! 1. **Branch, not `select`.** The guard is highly predictable on monomorphic
//!    code; a `select`/`cmov` would force a data dependency on BOTH inputs
//!    regardless of which one is used, i.e. it would pay the call's operand cost
//!    anyway.
//! 2. **Fast path is the FALLTHROUGH, the miss is a taken jump to a block marked
//!    [`FunctionBuilder::set_cold_block`]** — the same hint
//!    [`super::trycatch`] puts on its post-call error edge.
//! 3. **The tag test is plain Cranelift IR** (`band`/`icmp`/`bor`), NEVER an
//!    extern call. This is load-bearing: it is what lets the egraph's GVN/CSE
//!    delete a repeated check on the SAME SSA value across chained operators, so
//!    `a*b + c` checks `a`, `b`, `c` once each rather than once per operator.
//! 4. **No "redundant guard elimination" pass.** The checks are emitted
//!    straightforwardly and the elimination is left to the egraph — duplicating
//!    egraph work is precisely the mistake the deleted MIR tier made.
//!
//! ## Bit-for-bit equivalence with the miss path
//!
//! The fast path must be indistinguishable from the call it replaces, so:
//!
//! - The guard admits **exactly** the two number representations
//!   ([`emit_is_number`]): an inline double and a `TAG_INT32` word. Everything
//!   else — string, bool, null, undefined, object, function, and the reserved
//!   symbol/bigint tags — misses and takes the existing trampoline, which owns
//!   `ToPrimitive`/`ToNumber`/string-concat/lexicographic-compare. Nothing is
//!   guessed from AST shape.
//! - A **mixed pair** (one boxed `TAG_INT32`, one inline double) IS handled by the
//!   fast path: `coerce(_, Float64)` is the tag-selecting decode
//!   (`emit_tagged_number_to_f64`), which is the same `to_number` the trampoline
//!   performs for those two tags. Deliberate — refusing the mixed pair would kill
//!   the fast path for the very common `i + 0.5` / accumulator shapes.
//! - The arithmetic result is re-encoded by [`emit_number_result`], an exact IR
//!   transcription of the runtime's `genops::number_result` (int32 re-tightening,
//!   with `-0.0` kept as a double). Without it the fast path would hand back an
//!   inline double where the trampoline hands back a `TAG_INT32` word — the same
//!   *number*, but a different *word*, and words are what raw-compare consumers
//!   (Map/Set keys, `inspect`) see.
//! - NaN: `fcvt_to_sint_sat` + a round-trip `fcmp Equal` never re-tightens a NaN
//!   (an ordered compare against NaN is false), and `emit_box_double`
//!   canonicalizes it exactly as `PolyValue::from_f64` does. Relational NaN is
//!   free: every JS relational operator is `false` on an unordered pair, which is
//!   what an ordered `fcmp` yields.
//!
//! `RTS_OP_GUARD=0` removes the whole emission (see
//! [`super::clifflags::op_guards`]) so the change is A/B-measurable on one binary.

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{Block, InstBuilder, MemFlags, Value, types};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::Module;

use rts_hir::HirBinOp;

use crate::front::error::FrontResult;
use crate::repr::Repr;
use crate::value;

use super::binop::{float_cc, is_tagged};
use super::lower::{JsKind, Lowerer, Val};

/// `is_number(word)` as pure IR — true for an inline double OR a `TAG_INT32`
/// word, false for every other tag.
///
/// Written as `is_double(v) | (tag(v) == TAG_INT32)` rather than the literal
/// `is_double | (is_boxed & is_int_tag)`: when the word is NOT boxed the tag bits
/// are meaningless, but the left disjunct is already true then, so the `band`
/// with `is_boxed` cannot change the answer. Two instructions saved per operand,
/// same truth table.
fn emit_is_number(builder: &mut FunctionBuilder, v: Value) -> Value {
    let is_double = value::emit_is_double(builder, v);
    let shifted = builder.ins().ushr_imm(v, value::TAG_SHIFT as i64);
    let tag = builder.ins().band_imm(shifted, value::TAG_MASK as i64);
    let is_int32 = builder
        .ins()
        .icmp_imm(IntCC::Equal, tag, value::TAG_INT32 as i64);
    builder.ins().bor(is_double, is_int32)
}

/// The IR transcription of `rts_runtime::adapters::value::genops::number_result`:
/// an f64 arithmetic result becomes a `TAG_INT32` word when it is finite, integral
/// and inside the i32 range, and an inline double otherwise.
///
/// The three source conditions collapse into ONE round-trip test:
/// `fcvt_to_sint_sat` truncates (saturating at the i32 bounds, so out-of-range and
/// ±Inf land on a bound), `fcvt_from_sint` converts back, and `fcmp Equal` accepts
/// only the values that survived unchanged — which is exactly "finite, integral,
/// in range". NaN fails it because an ordered compare against NaN is false.
///
/// The ONE case the round trip gets wrong is `-0.0`: it truncates to `0`, converts
/// back to `+0.0`, and IEEE says `+0.0 == -0.0`. The runtime keeps `-0.0` as a
/// double (an int32 `0` would lose the sign for `Object.is(x, -0)`, `-0 % 5`, and
/// the `-0` rendering), so the sign is excluded explicitly by comparing the raw
/// bit pattern against `0x8000_0000_0000_0000`.
///
/// A `select` is correct here (unlike the operand guard): both arms are a handful
/// of pure ALU instructions with no call and no memory, so there is nothing to
/// branch around and the egraph folds the whole thing when it can prove the input.
fn emit_number_result(builder: &mut FunctionBuilder, f: Value) -> Value {
    let as_i32 = builder.ins().fcvt_to_sint_sat(types::I32, f);
    let back = builder.ins().fcvt_from_sint(types::F64, as_i32);
    let round_trips = builder.ins().fcmp(FloatCC::Equal, back, f);

    let bits = builder.ins().bitcast(types::I64, MemFlags::new(), f);
    let not_neg_zero = builder.ins().icmp_imm(IntCC::NotEqual, bits, i64::MIN);

    let use_int = builder.ins().band(round_trips, not_neg_zero);
    let int_word = value::emit_box_int32(builder, as_i32);
    let dbl_word = value::emit_box_double(builder, f);
    builder.ins().select(use_int, int_word, dbl_word)
}

/// Whether a `Tagged` operand with this static kind can POSSIBLY be a number at
/// run time. A kind the front-end already proved to be a string/object/function/
/// bool/null would make the guard a permanently-false test in front of a
/// permanently-taken call, so those skip the whole emission rather than pay a dead
/// compare and a dead fast-path block.
fn kind_may_be_number(k: JsKind) -> bool {
    matches!(k, JsKind::Unknown | JsKind::Number)
}

/// The guard condition for a `(l, r)` pair: the AND of `is_number` over the
/// operands that are actually `Tagged`.
///
/// An operand already proven non-`Tagged` needs no test — its repr IS the proof —
/// so it contributes nothing to the condition. That matters for the mixed
/// `tagged <op> literal` shape, which is the most common one in real code.
///
/// A `Tagged` operand's PolyValue word is `val.v` itself (`box_value` on a Tagged
/// value is the identity), so nothing is boxed here: the miss arm does its own
/// boxing inside the cold block, where it belongs.
///
/// Returns `None` when neither operand is Tagged (the caller never gets there).
fn emit_guard(lo: &mut Lowerer<'_, '_, '_>, l: Val, r: Val) -> Option<Value> {
    let lg = is_tagged(l).then(|| emit_is_number(lo.builder, l.v));
    let rg = is_tagged(r).then(|| emit_is_number(lo.builder, r.v));
    match (lg, rg) {
        (Some(a), Some(b)) => Some(lo.builder.ins().band(a, b)),
        (Some(a), None) | (None, Some(a)) => Some(a),
        (None, None) => None,
    }
}

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Try to emit a guarded native `+ - * /` in front of the generic trampoline.
    ///
    /// `Ok(None)` means "not applicable — use the unguarded generic path", and the
    /// caller falls back verbatim. Applicability is deliberately narrow:
    ///
    /// - **`+ - * /` only.** `%` and `**` have no Cranelift instruction (§3.4:
    ///   even a compile-time proof leaves the call standing), so a guard around
    ///   them buys only the boxing, not the call. Tier 2.3 handles `%` with a
    ///   different guard (round-trip-to-`i64` → `srem`) and is out of scope here.
    /// - **`+` fires only when the TAG TEST proves both operands are numbers at
    ///   run time.** The generic `__rtsadp_add` implements the whole JS `+`
    ///   algorithm (`ToPrimitive`, string concatenation, `[object Object]`
    ///   rendering); the fast path can only stand in for the both-numbers case,
    ///   and which case it is may NEVER be guessed from AST shape.
    /// - A kind already proven non-numeric skips the emission
    ///   ([`kind_may_be_number`]).
    pub(super) fn try_guarded_arith(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        l: Val,
        r: Val,
    ) -> FrontResult<Option<Val>> {
        if !super::clifflags::op_guards() {
            return Ok(None);
        }
        if !matches!(
            op,
            HirBinOp::Add | HirBinOp::Sub | HirBinOp::Mul | HirBinOp::Div
        ) {
            return Ok(None);
        }
        if !kind_may_be_number(l.kind) || !kind_may_be_number(r.kind) {
            return Ok(None);
        }

        let Some(cond) = emit_guard(self, l, r) else {
            return Ok(None);
        };

        let (fast, miss, merge) = self.open_guard(cond);

        // FAST: both operands decode as numbers, so the op is the native f64
        // instruction and the result is re-encoded exactly as the trampoline
        // would. `coerce(_, Float64)` is the tag-selecting decode for a Tagged
        // operand and a no-op/convert for a proven one.
        self.builder.switch_to_block(fast);
        let lf = self.coerce(l, Repr::Float64)?;
        let rf = self.coerce(r, Repr::Float64)?;
        let fv = match op {
            HirBinOp::Add => self.builder.ins().fadd(lf, rf),
            HirBinOp::Sub => self.builder.ins().fsub(lf, rf),
            HirBinOp::Mul => self.builder.ins().fmul(lf, rf),
            HirBinOp::Div => self.builder.ins().fdiv(lf, rf),
            _ => unreachable!("op filtered above"),
        };
        let fast_word = emit_number_result(self.builder, fv);
        self.builder.ins().jump(merge, &[fast_word.into()]);

        // MISS: the untouched generic call — it stays the single authority for
        // every coercion the fast path refused.
        self.builder.switch_to_block(miss);
        let generic = self.lower_generic_arith(module, op, l, r)?;
        self.builder.ins().jump(merge, &[generic.v.into()]);

        Ok(Some(self.close_guard(merge, generic.kind)))
    }

    /// Try to emit a guarded native `< <= > >=` in front of the generic
    /// trampoline. `Ok(None)` = not applicable, caller falls back.
    ///
    /// The miss path keeps every non-both-number case, which is where the
    /// interesting semantics live: two strings compare lexicographically, and a
    /// `Date`/array/object operand runs `ToPrimitive`. The fast path is a single
    /// ORDERED `fcmp`, which already answers `false` for a NaN operand — exactly
    /// the JS rule the trampoline implements with its explicit `unwrap_or(false)`.
    pub(super) fn try_guarded_relational(
        &mut self,
        module: &mut dyn Module,
        op: HirBinOp,
        l: Val,
        r: Val,
    ) -> FrontResult<Option<Val>> {
        if !super::clifflags::op_guards() {
            return Ok(None);
        }
        if !matches!(
            op,
            HirBinOp::Lt | HirBinOp::Le | HirBinOp::Gt | HirBinOp::Ge
        ) {
            return Ok(None);
        }
        if !kind_may_be_number(l.kind) || !kind_may_be_number(r.kind) {
            return Ok(None);
        }

        let Some(cond) = emit_guard(self, l, r) else {
            return Ok(None);
        };

        let (fast, miss, merge) = self.open_guard(cond);

        self.builder.switch_to_block(fast);
        let lf = self.coerce(l, Repr::Float64)?;
        let rf = self.coerce(r, Repr::Float64)?;
        let cc = float_cc(op)?;
        let b = self.builder.ins().fcmp(cc, lf, rf);
        // Both edges carry the `Repr::Bool` carrier (an i64 0/1), which is what
        // `poly_bool_to_bool` produces on the miss side.
        let fast_word = self.builder.ins().uextend(types::I64, b);
        self.builder.ins().jump(merge, &[fast_word.into()]);

        self.builder.switch_to_block(miss);
        let generic = self.lower_generic_relational(module, op, l, r)?;
        self.builder.ins().jump(merge, &[generic.v.into()]);

        let merged = self.close_guard(merge, JsKind::Bool);
        Ok(Some(Val::new(merged.v, Repr::Bool)))
    }

    /// Open a guarded region: create the fast/miss/merge blocks, mark the miss
    /// COLD, and branch on `cond`.
    ///
    /// The fast block is created and filled FIRST so Cranelift lays it out
    /// immediately after the branch — the fallthrough — and the cold hint sinks
    /// the miss block away from it (constraint 2 in the module doc).
    fn open_guard(&mut self, cond: Value) -> (Block, Block, Block) {
        let fast = self.builder.create_block();
        let miss = self.builder.create_block();
        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I64);
        if super::clifflags::cold_blocks() {
            self.builder.set_cold_block(miss);
        }
        self.builder.ins().brif(cond, fast, &[], miss, &[]);
        self.builder.seal_block(fast);
        self.builder.seal_block(miss);
        (fast, miss, merge)
    }

    /// Close a guarded region: switch to the merge block and read its parameter.
    ///
    /// Sealed only here — both predecessors have jumped by now. The merge value is
    /// `Tagged` because the miss edge carries a PolyValue word; a consumer that
    /// immediately unboxes gets the box folded back out by the egraph, which is the
    /// whole reason box/unbox are pure IR rather than calls.
    fn close_guard(&mut self, merge: Block, kind: JsKind) -> Val {
        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        let v = self.builder.block_params(merge)[0];
        Val {
            v,
            repr: Repr::Tagged,
            kind,
        }
    }
}
