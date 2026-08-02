//! The RUN-TIME integer guard in front of `%` (`RTS_OPTIMIZATION.md` §5 Tier 2.3).
//!
//! ## Why `%` needs its own guard rather than [`super::opguard`]'s
//!
//! §3.4 is the reason: `%` and `**` are the two operators for which Cranelift has
//! no instruction (there is no `frem`, and no fmod libcall). Proving the operand
//! `Repr` removes the BOXING but the CALL survives — `%` measures **8.23** today,
//! **5.30** with a plain tag guard, **4.86** with a full compile-time proof, and
//! still **3.35** only when the guard establishes something stronger and reaches
//! `srem`. So the doc's finding is that the RUN-TIME guard beats the compile-time
//! proof by **1.45×**, which is the opposite of the usual ordering and is why this
//! module exists next to `opguard` instead of inside it.
//!
//! ## The three preconditions, and why each one is load-bearing
//!
//! `srem` may only stand in for JS `%` when ALL THREE hold. None is optional and
//! none is a conservative nicety — each has a concrete input that breaks without it:
//!
//! 1. **Both operands round-trip through `i64` exactly.** Otherwise the integer
//!    remainder is simply not the JS remainder: `5.5 % 2` is `1.5`, and truncating
//!    to `5 % 2 = 1` is a wrong answer, not a rounding difference. The test also
//!    disposes of `NaN` and `±Infinity` for free (an ordered compare against `NaN`
//!    is false, and a saturated infinity does not convert back).
//! 2. **The divisor is not `0`.** This one is not a value bug but a CRASH: `srem`
//!    **traps** on a zero divisor, while JS `x % 0` is `NaN`. The existing
//!    compile-time path in [`super::binop`] already refuses a possibly-zero divisor
//!    for exactly this reason — this guard is what lets the same `srem` be reached
//!    when the divisor is a variable.
//! 3. **The dividend is not `0`.** `-0 % 3` is `-0` in JS (the remainder takes the
//!    sign of the dividend, and `-0` is a distinct double), whereas the integer
//!    path yields `+0`. `-0` and `+0` compare equal, so the round-trip test in (1)
//!    cannot catch it — the check has to be explicit. Rejecting `+0` as well costs
//!    one instruction less than distinguishing the two and is harmless: `0 % y`
//!    takes the miss arm and gets the right answer from the call.
//!
//! There is a FOURTH precondition the doc does not name and this module adds,
//! because it is a real hole in (1) as literally stated: **`|x| < 2^53`.**
//! `fcvt_to_sint_sat` saturates `2^63` (a representable double) to `i64::MAX`, and
//! `i64::MAX` converted back to `f64` ROUNDS UP to `2^63` — so the round trip
//! reports success while the integer the guard would then feed to `srem` is off by
//! one. Bounding both operands by `2^53`, the largest magnitude at which every
//! integer is a distinct double, closes it with one `fabs` + one `fcmp` per operand
//! and keeps the "round-trips exactly" claim literally true.
//!
//! ## Why the fast result is exact
//!
//! With (1)+(4) holding, both operands are exact integer doubles, and IEEE-754
//! `fmod` is computed EXACTLY (it never rounds — the true remainder of two
//! representable values is always itself representable). `srem` is truncated-
//! division remainder with the sign of the dividend, which is the same function.
//! So `fcvt_from_sint(srem(a, b))` is bit-identical to `a % b`, and feeding it
//! through [`super::opguard::emit_number_result`] on the Tagged path reproduces
//! `__rtsadp_mod`'s output WORD, not merely its value.
//!
//! ## Design constraints — the same four as `opguard`
//!
//! Branch rather than `select`; the fast path is the fallthrough and the miss is a
//! taken jump to a `set_cold_block`; the test is plain `band`/`icmp`/`fcmp` IR and
//! never an extern call, so the egraph's GVN/CSE can share a repeated check on the
//! same SSA value with `opguard`'s tag test (this module deliberately CALLS
//! `opguard::emit_is_number` rather than writing its own, so the instructions are
//! literally the same ones); and there is NO "redundant guard elimination" pass —
//! the elimination is the egraph's job.
//!
//! `RTS_REM_GUARD=0` removes the whole emission (see
//! [`super::clifflags::rem_guard`]) so this item is A/B-measurable independently of
//! Tier 1.3 and Tier 2.1 on one binary.
//!
//! TODO(measure): record the `RTS_REM_GUARD=1` vs `=0` delta here. The 1.45× above
//! is §3.4's probe number, not a measurement of this emission.

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use rts_hir::HirBinOp;

use crate::front::error::FrontResult;
use crate::repr::Repr;

use super::lower::{JsKind, Lowerer, Val};
use super::opguard::{emit_is_number, emit_number_result};

/// `2^53` — the largest magnitude below which every integer is a distinct double.
/// See precondition (4) in the module doc: this is what makes the `i64` round-trip
/// test honest rather than merely plausible.
const EXACT_INT_LIMIT: f64 = 9_007_199_254_740_992.0;

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// The `i64` truncation of `f` plus the predicate "`f` is an integer this
    /// truncation represents exactly" — preconditions (1) and (4) for one operand.
    ///
    /// `fcvt_to_sint_sat` truncates toward zero and saturates instead of trapping,
    /// `fcvt_from_sint` converts back, and an ORDERED `fcmp Equal` accepts only the
    /// values that survived unchanged — non-integral, `NaN` and `±Infinity` all
    /// fail it. The `|f| < 2^53` bound is ANDed in because the round trip alone
    /// mis-accepts `2^63` (module doc, precondition 4).
    fn emit_exact_i64(&mut self, f: Value) -> (Value, Value) {
        let i = self.builder.ins().fcvt_to_sint_sat(types::I64, f);
        let back = self.builder.ins().fcvt_from_sint(types::F64, i);
        let round_trips = self.builder.ins().fcmp(FloatCC::Equal, back, f);

        let mag = self.builder.ins().fabs(f);
        let limit = self.builder.ins().f64const(EXACT_INT_LIMIT);
        let in_range = self.builder.ins().fcmp(FloatCC::LessThan, mag, limit);

        let ok = self.builder.ins().band(round_trips, in_range);
        (i, ok)
    }

    /// The full `%` guard condition over two ALREADY-DECODED `f64` operands, plus
    /// the two `i64` truncations the fast path will divide.
    ///
    /// Conjunction order mirrors the module doc: exact-`i64` for both operands,
    /// then divisor `!= 0` (else `srem` TRAPS), then dividend `!= 0` (else `-0 % y`
    /// loses its sign). The two `!= 0` tests are on the TRUNCATIONS, which is sound
    /// only because the exactness tests are ANDed in front of them — `0.5`
    /// truncates to `0` but is rejected by its own conjunct, not by these.
    fn emit_rem_guard(&mut self, lf: Value, rf: Value) -> (Value, Value, Value) {
        let (li, l_ok) = self.emit_exact_i64(lf);
        let (ri, r_ok) = self.emit_exact_i64(rf);
        let exact = self.builder.ins().band(l_ok, r_ok);

        // (2) divisor != 0 — `srem` traps where JS yields NaN.
        let r_nz = self.builder.ins().icmp_imm(IntCC::NotEqual, ri, 0);
        // (3) dividend != 0 — `-0 % 3` is `-0` in JS, `0` through the int path.
        // This rejects `+0` too; see the module doc for why that is deliberate.
        let l_nz = self.builder.ins().icmp_imm(IntCC::NotEqual, li, 0);
        let nz = self.builder.ins().band(l_nz, r_nz);

        let cond = self.builder.ins().band(exact, nz);
        (cond, li, ri)
    }

    /// `%` on operands the front-end already PROVED non-`Tagged` numeric.
    ///
    /// Today this lands on `__rtsadp_fmod_f64` — no boxing, but still a call, which
    /// is §3.4's 4.86 row. The guard branches around the call for the integer case
    /// (3.35). Both edges carry a RAW `f64`, so the Val the caller gets back keeps
    /// its native `Float64` repr and nothing downstream has to unbox.
    ///
    /// No tag test here: the `Repr` IS the proof that both words are numbers. Only
    /// the three integer preconditions are tested.
    ///
    /// `Ok(None)` = not applicable, and the caller emits the unchanged fmod call.
    pub(super) fn try_guarded_rem_native(
        &mut self,
        module: &mut dyn Module,
        l: Val,
        r: Val,
    ) -> FrontResult<Option<Val>> {
        if !super::clifflags::rem_guard() {
            return Ok(None);
        }
        let lf = self.coerce(l, Repr::Float64)?;
        let rf = self.coerce(r, Repr::Float64)?;
        let (cond, li, ri) = self.emit_rem_guard(lf, rf);

        let (fast, miss, merge) = self.open_guard_typed(cond, types::F64);

        // FAST: the native remainder, converted back to the f64 both edges carry.
        // Exact — see "Why the fast result is exact" in the module doc.
        self.builder.switch_to_block(fast);
        let rem = self.builder.ins().srem(li, ri);
        let fast_f = self.builder.ins().fcvt_from_sint(types::F64, rem);
        self.builder.ins().jump(merge, &[fast_f.into()]);

        // MISS: the untouched scalar fmod call — still the one authority for
        // fractional operands, a zero divisor (`NaN`), `-0` dividends, and `±Inf`.
        self.builder.switch_to_block(miss);
        let miss_f = self
            .call_runtime(module, "__rtsadp_fmod_f64", &[lf, rf])?
            .expect("fmod returns a value");
        self.builder.ins().jump(merge, &[miss_f.into()]);

        let out = self.close_guard_raw(merge);
        Ok(Some(Val::new(out, Repr::Float64)))
    }

    /// `%` where at least one operand is `Repr::Tagged` — today an unconditional
    /// box + `__rtsadp_mod` (§3.3's 8.23 row, the worst of the arithmetic
    /// operators).
    ///
    /// The condition is [`emit_is_number`] over the Tagged operands (so the decode
    /// below is the same `to_number` the trampoline would run) ANDed with the three
    /// integer preconditions. Both are needed and in that order: the tag test is
    /// what makes `coerce(_, Float64)` a legal decode in the first place.
    ///
    /// Note the fast path is evaluated on the DECODED operands regardless of which
    /// side was Tagged — a proven `Repr` contributes no tag conjunct, exactly as in
    /// [`super::opguard::emit_guard`].
    ///
    /// `Ok(None)` = not applicable, and the caller emits the unchanged generic call.
    pub(super) fn try_guarded_rem_tagged(
        &mut self,
        module: &mut dyn Module,
        l: Val,
        r: Val,
    ) -> FrontResult<Option<Val>> {
        if !super::clifflags::rem_guard() {
            return Ok(None);
        }
        // A kind already proven non-numeric would make the tag test a
        // permanently-false compare in front of a permanently-taken call.
        if !super::opguard::kind_may_be_number(l.kind) || !super::opguard::kind_may_be_number(r.kind)
        {
            return Ok(None);
        }

        // The tag conjunct — only for the operands that are actually Tagged. Same
        // instructions `opguard` emits, so the egraph can share them across a
        // `(a % b) + c` chain.
        let lt = super::binop::is_tagged(l).then(|| emit_is_number(self.builder, l.v));
        let rt = super::binop::is_tagged(r).then(|| emit_is_number(self.builder, r.v));
        let tag_cond = match (lt, rt) {
            (Some(a), Some(b)) => Some(self.builder.ins().band(a, b)),
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => None,
        };
        let Some(tag_cond) = tag_cond else {
            // Neither operand Tagged — the caller routes to the native path above.
            return Ok(None);
        };

        // The DECODE is emitted before the branch so the integer preconditions can
        // be tested on it. That is sound for every word, number or not: for a
        // non-number `coerce(_, Float64)` yields a garbage double which then fails
        // the `tag_cond` conjunct anyway. It is PURE IR — no load, no call, no
        // trap — so evaluating it on the miss-bound path costs a few ALU ops and
        // cannot observe anything.
        let lf = self.coerce(l, Repr::Float64)?;
        let rf = self.coerce(r, Repr::Float64)?;
        let (int_cond, li, ri) = self.emit_rem_guard(lf, rf);
        let cond = self.builder.ins().band(tag_cond, int_cond);

        let (fast, miss, merge) = self.open_guard(cond);

        // FAST: native remainder, re-encoded exactly as `__rtsadp_mod`'s
        // `number_result` does — `7 % 3` must come back as a TAG_INT32 word, not an
        // inline double, because raw-word consumers (Map/Set keys, `inspect`) can
        // tell the difference. `-0` never reaches here (precondition 3), so
        // `emit_number_result`'s `-0` arm is unreachable on this path rather than
        // relied upon.
        self.builder.switch_to_block(fast);
        let rem = self.builder.ins().srem(li, ri);
        let rem_f = self.builder.ins().fcvt_from_sint(types::F64, rem);
        let fast_word = emit_number_result(self.builder, rem_f);
        self.builder.ins().jump(merge, &[fast_word.into()]);

        // MISS: the untouched generic `__rtsadp_mod`.
        self.builder.switch_to_block(miss);
        let generic = self.lower_generic_arith(module, HirBinOp::Rem, l, r)?;
        self.builder.ins().jump(merge, &[generic.v.into()]);

        Ok(Some(self.close_guard(merge, JsKind::Number)))
    }

    /// `%` on two operands with a PROVEN INT repr but a divisor that is NOT a
    /// compile-time constant.
    ///
    /// [`super::binop`]'s existing `HirBinOp::Rem if both_int` arm takes `srem`
    /// only when `const_int_value` can see a non-zero divisor; every
    /// other divisor — a loop variable, a parameter, a field read — falls to the
    /// generic trampoline, which boxes two values it already knows are integers.
    /// That compile-time path is UNCHANGED and still gets first refusal; this is
    /// only the case it cannot reach.
    ///
    /// Preconditions (1) and (4) are free here — the operands are native integers
    /// in registers, so there is nothing to round-trip — and (3) is free too,
    /// because an integer register cannot hold `-0`. Only (2) is emitted: one
    /// `icmp` against `0`, guarding against the `srem` TRAP that JS answers `NaN`.
    ///
    /// The merge carries an `f64` rather than the int repr because the miss value
    /// IS `NaN`. That is not a loss: `box_value` on an int repr goes through `f64`
    /// as well, so the generic path this replaces was already f64-bound.
    ///
    /// `Ok(None)` = not applicable, and the caller emits the unchanged generic call.
    pub(super) fn try_guarded_rem_int(&mut self, l: Val, r: Val) -> FrontResult<Option<Val>> {
        if !super::clifflags::rem_guard() {
            return Ok(None);
        }
        let cond = self.builder.ins().icmp_imm(IntCC::NotEqual, r.v, 0);
        let (fast, miss, merge) = self.open_guard_typed(cond, types::F64);

        self.builder.switch_to_block(fast);
        let rem = self.builder.ins().srem(l.v, r.v);
        let fast_f = self.builder.ins().fcvt_from_sint(types::F64, rem);
        self.builder.ins().jump(merge, &[fast_f.into()]);

        // MISS: `x % 0` is `NaN` for every dividend, so the cold arm is a constant
        // — no call at all. `f64::NAN` is the same quiet NaN `__rtsadp_mod`'s
        // `f64::%` produces, and `emit_box_double` canonicalizes either identically.
        self.builder.switch_to_block(miss);
        let nan = self.builder.ins().f64const(f64::NAN);
        self.builder.ins().jump(merge, &[nan.into()]);

        let out = self.close_guard_raw(merge);
        Ok(Some(Val::new(out, Repr::Float64)))
    }
}
