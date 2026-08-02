//! Cranelift ISA-FLAG switches shared by both module paths (JIT and AOT), and
//! the env escape hatches that make their cost measurable in one binary.

/// `RTS_CLIF_VERIFIER=1` — force Cranelift's IR verifier ON in a RELEASE binary,
/// on both the JIT ([`super::module_jit::make_module`]) and the AOT
/// ([`super::module_aot::make_object_module`]) path. Release builds disable it
/// (it runs several times per function and taxes every startup); debug builds
/// keep it, where a malformed lowering must be caught loudly.
///
/// This exists so the verifier's cost is a MEASUREMENT rather than an
/// attribution — `CRANELIFT_IMPLEMENTATION.md` §4 read a 2x JIT/AOT
/// machine-compile gap as "consistent with the verifier", with two variables
/// differing. A/B'd with this switch it is 4.8 ms of the AOT phase (36.4 -> 41.2
/// ms) and 5.5 ms of the JIT phase; the remaining AOT/JIT gap is the AOT path's
/// extra IR (string literals as data objects + a `string_from_static` call
/// instead of the JIT's compile-time-baked handle), not the verifier.
/// `RTS_COLD_BLOCKS=0` — stop marking MISS / BAIL / ERROR / THROW blocks as cold
/// (`FunctionBuilder::set_cold_block`, `RTS_OPTIMIZATION.md` §5 Tier 1.5).
/// Layout-only, so this is an A/B switch for what the hint is worth, not a
/// correctness fallback — the same blocks are emitted either way, Cranelift just
/// sinks the marked ones to the end of the function so the hot path's
/// instructions stay contiguous in the I-cache.
///
/// ONE knob covers the whole set, deliberately, so the set stays measurable as a
/// unit. Current members: the post-call error edge ([`super::trycatch`]), the
/// `catch` handler and the finally-restore/unwind blocks (same), the operator and
/// `%` guard misses ([`super::opguard`] / [`super::remguard`]), the
/// `RTS_INT_OVERFLOW` overflow arm ([`super::binop`]), the property inline-cache
/// miss ([`super::ic`]), the typed-array out-of-bounds read arm
/// ([`super::ta_native`]), and the "callee slot is not a function" throw arm
/// ([`super::method_dyn`]).
///
/// TODO(measure): no A/B has been run for this hint. `RTS_OPTIMIZATION.md` §5
/// Tier 1.5's remark that the §3.3 guard numbers are "likely conservative"
/// because the probe's guard variants did not use cold blocks is an EXPECTATION,
/// not a result. Measure with `RTS_COLD_BLOCKS=1` vs `=0` and
/// `RTS_NO_PRELUDE_CACHE=1` on BOTH arms — [`super::prelude_cache`] keys on the
/// prelude text plus its cache version, so otherwise both arms replay one cached
/// lowering and the A/B measures nothing.
pub(super) fn cold_blocks() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("RTS_COLD_BLOCKS")
            .map(|v| v.trim() != "0")
            .unwrap_or(true)
    })
}

/// `RTS_INT_OVERFLOW=0` — stop overflow-checking the proven-int `+`/`-`/`*` path
/// (`sadd_overflow`/`ssub_overflow`/`smul_overflow` + promote to `f64` on
/// overflow, which is what JS-number semantics require). OFF by default — the Tier 2.2
/// gate landed and the gated form was then MEASURED, see below; `=1` turns it on.
///
/// Unlike [`op_guards`] / [`rem_guard`] / [`escape_analysis`] this IS a semantic
/// switch: the two arms print DIFFERENT things for a program that overflows, and
/// the ON arm is the correct one. `4611686018427387904 + 4611686018427387904`
/// prints `9223372036854776000` (Node's answer) instead of the wrapped
/// `-9223372036854776000`, and `* 4` prints `18446744073709552000` instead of `0`.
///
/// Why it was OFF until now, and what changed. The check is not free — a blanket
/// application cost an int-heavy 50M-iteration loop ~34 ms → ~224 ms (6.6x) when
/// the two edges merge as `Float64`, and ~735 ms (21x) when they merge as
/// `Tagged`. That cost was being paid for a rule that does not govern half the
/// values: RTS has BOTH semantics. A JS `number` is a double, so a 64-bit wrap is
/// a wrong answer; a value declared `i64`/`u32` is a native fixed-width integer,
/// where the wrap is exactly the contract the annotation asked for. The two were
/// indistinguishable because `rts-hir` types EVERY integral literal as
/// `HirType::I64` regardless of annotation, so `let a = 5` and `let a: i64 = 5`
/// arrived identical.
///
/// They are distinguishable now: `HirStmt::Let`/`Const` carry a `native_int` bit
/// (set only by an explicit fixed-width integer annotation; a param's `ty` is
/// annotation-derived already), the lowering records those names in
/// `Lowerer::native_int_locals`, and an ident read stamps `Val::native_int`.
/// [`super::binop::Lowerer::lower_int_arith_checked`] fires only when NEITHER
/// operand carries it — so the check now governs only the values whose semantics
/// it is right for, and the native-int loop keeps its unchecked fast path.
///
/// **And it is still OFF, because the gated form was then measured and two things
/// came back.** The gate works — `let a = 4611686018427387904; a * 4` now prints
/// node's `18446744073709552000` while `let b: i64 = …; b * 4` keeps the native
/// `0`, and an `i64`-declared loop keeps its unchecked speed. But:
///
/// 1. **Cost on ORDINARY code.** Unannotated JS integer arithmetic is the common
///    case (`let i = 0`), and it now pays the full tax: a 50M-iteration
///    `acc = acc + i*3 - 1` loop goes 34 ms -> 221 ms (6.5x). The gate moved the
///    cost off native-int code; it did not reduce it for JS numbers, which is most
///    TypeScript.
/// 2. **It changes the RESULT REPRESENTATION even where nothing overflows.** The
///    two edges merge as `Float64`, so a non-overflowing `a + b` on ints yields a
///    float word. That is observable: `tests/claude-pickle-golden.test.ts` — a
///    serialization format freeze — fails two assertions with the check on. The TS
///    suite goes 3281/3282 to 3279/3282, and 42.4 s to 50.2 s.
///
/// So the remaining blocker is no longer "which values does the rule govern" (that
/// is solved) but **a representation-preserving merge**: the overflow edge must
/// re-tighten back to an int word when the result fits, the way
/// [`super::opguard`]'s `emit_number_result` already does for the Tagged path. The
/// raw-`Float64` merge skips exactly that step. Until it does not, this stays off.
pub(super) fn int_overflow_checks() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("RTS_INT_OVERFLOW")
            .map(|v| v.trim() == "1")
            .unwrap_or(false)
    })
}

/// `RTS_OP_GUARD=0` — stop emitting the inline tag guard in front of the generic
/// operator trampolines (`RTS_OPTIMIZATION.md` §5 Tier 2.1, implemented in
/// [`super::opguard`]). ON by default.
///
/// Unlike [`int_overflow_checks`] this is not a semantic switch: with it OFF the
/// operator lowers to the same `__rtsadp_*` call it always did, and with it ON the
/// fast path is only taken when a RUN-TIME tag test proves both operands are
/// numbers, in which case it reproduces the trampoline's result word bit-for-bit.
/// So it is purely an A/B lever for what the guard is worth on one binary — the
/// doc's expectation is 2–4× across the guarded operators, measured on a probe
/// rather than on this emission.
///
/// TODO(measure): record the A/B here once the operator bench has been run with
/// `RTS_OP_GUARD=1` and `RTS_OP_GUARD=0`. Set `RTS_NO_PRELUDE_CACHE=1` for both
/// arms — [`super::prelude_cache`] keys only on the prelude TEXT plus its cache
/// version, so without it both arms would replay one cached lowering and the A/B
/// would measure nothing.
pub(super) fn op_guards() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("RTS_OP_GUARD")
            .map(|v| v.trim() != "0")
            .unwrap_or(true)
    })
}

/// `RTS_POW_FOLD=0` — stop folding `x ** <literal 2>` into a native `fmul`
/// (`RTS_OPTIMIZATION.md` §5 Tier 1.3, implemented in
/// [`super::opguard::Lowerer::try_pow_literal_square`]). ON by default.
///
/// Its own switch, separate from [`op_guards`], precisely so the two Tier items
/// landing together stay independently A/B-measurable on ONE binary: `**` folds
/// at lowering time on a literal exponent, `%` guards at run time, and attributing
/// a delta to the wrong one is the failure mode a shared switch guarantees.
///
/// Documented expectation: **11.5×** on `x ** 2` (§5 Tier 1.3; §3.4's ladder puts
/// the proven-`Repr` `pow` call at 12.72 and the folded multiply at 1.62). That is
/// the doc's probe, not a measurement of this emission.
///
/// TODO(measure): A/B with `RTS_NO_PRELUDE_CACHE=1` on BOTH arms — `prelude_cache`
/// keys on the prelude text plus its cache version, so otherwise both arms replay
/// one cached lowering and the A/B measures nothing.
pub(super) fn pow_fold() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("RTS_POW_FOLD")
            .map(|v| v.trim() != "0")
            .unwrap_or(true)
    })
}

/// `RTS_REM_GUARD=0` — stop emitting the run-time integer guard in front of `%`
/// (`RTS_OPTIMIZATION.md` §5 Tier 2.3, implemented in [`super::remguard`]). ON by
/// default.
///
/// Like [`op_guards`] and unlike [`int_overflow_checks`] this is NOT a semantic
/// switch: with it OFF `%` lowers to the `__rtsadp_mod` / `__rtsadp_fmod_f64` call
/// it always did, and with it ON the native `srem` fires only when a run-time test
/// has proved the three preconditions that make the integer remainder equal the JS
/// one (see the `remguard` module doc).
///
/// Documented expectation: **1.45×** over the compile-time-proven float path (§3.4:
/// `%` at 4.86 proven vs 3.35 runtime-guarded). The doc's probe, not this emission.
///
/// TODO(measure): A/B with `RTS_NO_PRELUDE_CACHE=1` on both arms, as above.
pub(super) fn rem_guard() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("RTS_REM_GUARD")
            .map(|v| v.trim() != "0")
            .unwrap_or(true)
    })
}

/// `RTS_ESCAPE=0` — stop scalar-replacing a provably non-escaping `new C(..)`
/// (`RTS_OPTIMIZATION.md` §5 Tier 4.1, implemented in [`super::escape`]). ON by
/// default.
///
/// Like [`op_guards`] and [`rem_guard`], and unlike [`int_overflow_checks`], this
/// is NOT a semantic switch: with it OFF the construction lowers to the same heap
/// allocation plus constructor call it always did, and with it ON the object is
/// only removed where a whitelist-based use-scan proved every use of the local is
/// a read of a declared field. Both arms must print the same thing for every
/// program; the switch exists so the pass's cost/benefit is a MEASUREMENT on one
/// binary rather than an attribution.
///
/// Documented calibration, none of it a measurement of THIS emission: `new P(x,y)`
/// costs 632 ns/iter in the engine against node's 4.7; the value probe's EA rows
/// (kernels M6 / H6) put a non-escaping field access at 0.69–0.76 ns against 1.50
/// for a heap one; Roslyn's self-build found 16.1% of allocated objects
/// non-escaping at run time and Graal's PEA measured −8.0% to −22.7% allocated
/// bytes. The probe's 138x is the provably-local BEST CASE, not an expectation.
///
/// TODO(measure): A/B with `RTS_NO_PRELUDE_CACHE=1` on BOTH arms — `prelude_cache`
/// keys on the prelude text plus its cache version, so otherwise both arms replay
/// one cached lowering and the A/B measures nothing.
pub(super) fn escape_analysis() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("RTS_ESCAPE")
            .map(|v| v.trim() != "0")
            .unwrap_or(true)
    })
}

pub(super) fn verifier_forced() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("RTS_CLIF_VERIFIER")
            .map(|v| v.trim() == "1")
            .unwrap_or(false)
    })
}
