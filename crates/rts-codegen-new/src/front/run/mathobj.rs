//! `Math.*`, `Number.*` (static methods + constants) — P5.4.
//!
//! High-frequency numeric globals. Three call/access shapes lower here, all
//! producing the engine's NATIVE numeric fast path (an unboxed `Float64`/`Bool`
//! `Val`, no PolyValue boxing) — `Math`/`Number` are NAMESPACE objects, not
//! values, so `Math`/`Number` as a bare identifier still bails (no value model).
//!
//! 1. `Math.method(args)` — `MethodCall`/`Call(Member)` whose object is the bare
//!    `Math` identifier. Each method maps to either an INLINE Cranelift intrinsic
//!    (`sqrt`/`abs`/`min`/`max` — the design's preserved fast path: `fsqrt`/
//!    `fabs`/`fmin`/`fmax`, NOT a call) or a real `__RTS_FN_NS_MATH_*` symbol.
//! 2. `Math.CONST` — `Member` access of a Math constant (`PI`/`E`/…): an f64const.
//! 3. `Number.method(args)` — the static predicates (`isInteger`/`isFinite`/
//!    `isNaN`/`isSafeInteger`) via the real `__RTS_FN_GL_NUMBER_IS_*` symbols, and
//!    `parseInt`/`parseFloat` aliased to the globals; `Number.CONST` constants.
//!
//! Args are coerced through the engine's numeric path (a proven number rides its
//! register; a Tagged arg BAILS — these are numeric statics, never `any`). A
//! variadic `Math.min/max/hypot` with >2 args FOLDS pairwise; a spread arg (the
//! HIR flattens it to a single array word) BAILS rather than coerce an array to a
//! bogus scalar (the honesty floor).

use cranelift_codegen::ir::{InstBuilder, Value};
use cranelift_module::Module;

use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use crate::repr::Repr;
use crate::value::emit_marshal;

use crate::front::error::{FrontResult, unsupported};

use super::lower::{JsKind, Lowerer, Val};

/// How a `Math.method` lowers: an inline Cranelift intrinsic (the preserved fast
/// path) or a `call` of a real `__RTS_FN_NS_MATH_*` symbol.
enum MathOp {
    /// `fsqrt` (1-arg).
    Sqrt,
    /// `fabs` (1-arg).
    Abs,
    /// `fmin` (2-arg, NaN-propagating per JS — see below).
    Min,
    /// `fmax` (2-arg).
    Max,
    /// A real `__RTS_FN_NS_MATH_*` symbol taking `arity` f64 args, returning f64.
    Sym(&'static str, usize),
}

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Try to lower a `Math.x` / `Number.x` MEMBER access (constant) — called from
    /// [`super::obj::Lowerer::lower_member`]'s namespace probe. Returns `Ok(None)`
    /// when `object` is not the bare `Math`/`Number` global.
    pub(super) fn try_math_number_const(
        &mut self,
        object: &HirExpr,
        prop: &str,
    ) -> FrontResult<Option<Val>> {
        let HirExprKind::Ident(name) = &object.kind else {
            return Ok(None);
        };
        // A local of the same name shadows the global; a same-named class shadows
        // too — EXCEPT the prelude WRAPPER primordial `class Number` (prototype-only,
        // must not hide `Number.MAX_SAFE_INTEGER`/`EPSILON`/…). See
        // `try_math_number_call` for the same carve-out.
        if self.local(name).is_some() {
            return Ok(None);
        }
        if self.classes.get(name).is_some() && !is_wrapper_primordial_static(name) {
            return Ok(None);
        }
        match name.as_str() {
            "Math" => match math_const(prop) {
                Some(c) => Ok(Some(self.f64_const_val(c))),
                None => unsupported!("Math.{prop} (unknown Math constant)"),
            },
            "Number" => match number_const(prop) {
                Some(c) => Ok(Some(self.f64_const_val(c))),
                None => unsupported!("Number.{prop} (unknown Number constant/static value)"),
            },
            _ => Ok(None),
        }
    }

    /// Try to lower a `Math.m(args)` / `Number.m(args)` static call. Returns
    /// `Ok(None)` when `object` is not the bare `Math`/`Number` global (so the
    /// caller falls through to its next handler), or an explicit bail.
    pub(super) fn try_math_number_call(
        &mut self,
        module: &mut dyn Module,
        object: &HirExpr,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        let HirExprKind::Ident(name) = &object.kind else {
            return Ok(None);
        };
        // A LOCAL named `Math`/`Number` shadows the global. A same-named CLASS also
        // shadows — EXCEPT the prelude WRAPPER primordial `class Number` (the
        // primitive-method library), which is prototype-only and must NOT shadow the
        // global static surface (`Number.isNaN`/`Number.MAX_SAFE_INTEGER`/…). So a
        // wrapper-primordial ambient class is transparent here (the static path
        // wins); a genuine local still bails. (Same carve-out as
        // `is_global_class_ctor`/`is_wrapper_primordial`.)
        if self.local(name).is_some() {
            return Ok(None);
        }
        if self.classes.get(name).is_some() && !is_wrapper_primordial_static(name) {
            return Ok(None);
        }
        match name.as_str() {
            "Math" => self.lower_math_call(module, method, args).map(Some),
            "Number" => self.lower_number_call(module, method, args).map(Some),
            _ => Ok(None),
        }
    }

    /// Lower `Math.method(args)`.
    fn lower_math_call(
        &mut self,
        module: &mut dyn Module,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if method == "random" {
            if !args.is_empty() {
                return unsupported!("Math.random takes no args (got {})", args.len());
            }
            // `random` has PRNG state — it is a real `call`, never pure IR. (The
            // only Math op here that is not inlinable; the design's intrinsic set
            // is sqrt/abs/min/max.)
            let v =
                emit_marshal::emit_call(module, self.builder, "__RTS_FN_NS_MATH_RANDOM_F64", &[])
                    .expect("MATH_RANDOM_F64 returns a value");
            return Ok(Val::new(v, Repr::Float64));
        }
        // `Math.min/max/hypot(...xs)` (P5.6): a SINGLE spread of a proven array →
        // fold the op over the array's elements at runtime via `__rtsadp_math_reduce`
        // (op: 0=min, 1=max, 2=hypot). A mixed `Math.max(a, ...xs)` or a non-array
        // spread is a later increment → bail.
        if let Some(reduce_op) = math_reduce_op(method) {
            if args.len() == 1 {
                if let HirExprKind::Spread(inner) = &args[0].kind {
                    if !self.is_array_valued(inner) {
                        return unsupported!(
                            "Math.{method}(...spread) of a non-array value (later increment)"
                        );
                    }
                    let src = self.lower_expr(module, inner)?;
                    let arr_word = self.box_value(src);
                    let op_v = self
                        .builder
                        .ins()
                        .iconst(cranelift_codegen::ir::types::I64, reduce_op);
                    let v = emit_marshal::emit_call(
                        module,
                        self.builder,
                        "__rtsadp_math_reduce",
                        &[arr_word, op_v],
                    )
                    .expect("__rtsadp_math_reduce returns a value");
                    return Ok(Val::new(v, Repr::Float64));
                }
            }
        }
        // i32-DOMAIN Math methods (`imul`, `clz32`, …): the runtime symbol takes/
        // returns I64 (the i32 domain), NOT f64 — so they are absent from the f64
        // `math_op` table. Resolve them DATA-DRIVEN from the `math` Registry
        // namespace (`namespace_member`), which carries the exact I64 AbiType
        // signature; marshal i64 args + i64 result. No hardcoded per-method symbol:
        // any i32-domain Math method registered in `rts-shared::math` works here.
        if let Some(val) = self.try_math_i32_domain(module, method, args)? {
            return Ok(val);
        }
        let Some(op) = math_op(method) else {
            return unsupported!("Math.{method}(...) (unsupported Math method)");
        };
        // Lower + coerce every arg to a native f64 FIRST (a Tagged/non-numeric arg
        // bails here — these are numeric statics). A spread arg flattened to an
        // array word is rejected by `coerce` (Tagged→Float64 only unboxes a proven
        // numeric word; an array word is not numeric) — but we also guard the arity.
        let fargs = self.math_f64_args(module, method, &op, args)?;
        let v = self.emit_math_op(module, &op, &fargs)?;
        Ok(Val::new(v, Repr::Float64))
    }

    /// Lower an i32-DOMAIN `Math.method` (`imul`, `clz32`, …) resolved DATA-DRIVEN
    /// from the `math` Registry namespace. `Ok(None)` ⇒ the method is not a
    /// registered i32-domain math member (the caller falls through to the f64 table
    /// / bail). A method whose Registry signature is all-I64 in + I64 out is treated
    /// as i32-domain: each arg is ToInt32-coerced (JS `Math.imul`/`clz32` semantics)
    /// then sign-extended to the I64 ABI; the i64 result reboxes as an Int32 number.
    fn try_math_i32_domain(
        &mut self,
        module: &mut dyn Module,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Option<Val>> {
        let Some(call) = super::registry::namespace_member("math", method, args.len()) else {
            return Ok(None);
        };
        // i32-domain = every explicit arg AND the return are integer ABI (I64/I32).
        // (sqrt/floor/… are F64 and fall through to the f64 intrinsic table.)
        let is_int = |t: &rts_engine::abi::AbiType| {
            matches!(
                t,
                rts_engine::abi::AbiType::I64
                    | rts_engine::abi::AbiType::I32
                    | rts_engine::abi::AbiType::U64
            )
        };
        if call.recv_abi.is_some() || !is_int(&call.ret) || !call.arg_abis.iter().all(is_int) {
            return Ok(None);
        }
        if args.len() != call.arg_abis.len() {
            return unsupported!(
                "Math.{method} expects {} args, got {}",
                call.arg_abis.len(),
                args.len()
            );
        }
        // ToInt32 each arg (JS coercion for the i32-domain ops). In this engine
        // `Int32`/`Int64` share the Cranelift `i64` slot (`cl_type` maps both to
        // I64), so the coerced value is ALREADY the i64 the I64-ABI runtime symbol
        // wants — no sextend/widen (which would be a verifier error on an i64).
        let mut iargs = Vec::with_capacity(args.len());
        for a in args {
            let v = self.lower_expr(module, a)?;
            iargs.push(self.coerce(v, Repr::Int32)?);
        }
        // Emit with the EXACT AbiType signature from the Registry (data-driven —
        // no `sig_of`/`abi_sig` hand-list needed): `emit_call_sig` builds the
        // Cranelift sig from `call.arg_abis`/`call.ret` directly. The result is an
        // i64 holding the i32-domain value → a proven `Int32` Val (same slot).
        let res = emit_marshal::emit_call_sig(
            module,
            self.builder,
            call.symbol,
            &iargs,
            &call.arg_abis,
            call.ret,
        )
        .expect("i32-domain math symbol returns a value");
        Ok(Some(Val::new(res, Repr::Int32)))
    }

    /// Coerce each arg to native f64, validating arity for the op. Variadic
    /// `min`/`max`/`hypot` accept 1..N (folded pairwise by the caller); the fixed
    /// ops want exactly their arity.
    fn math_f64_args(
        &mut self,
        module: &mut dyn Module,
        method: &str,
        op: &MathOp,
        args: &[HirExpr],
    ) -> FrontResult<Vec<Value>> {
        let variadic = matches!(method, "min" | "max" | "hypot");
        let want = op_arity(op);
        if variadic {
            if args.is_empty() {
                return unsupported!(
                    "Math.{method}() with no args (variadic empty is a later increment)"
                );
            }
        } else if args.len() != want {
            return unsupported!("Math.{method} expects {want} args, got {}", args.len());
        }
        let mut out = Vec::with_capacity(args.len());
        for a in args {
            let v = self.lower_expr(module, a)?;
            // A PROVEN number (Int32/Int64/Float64) coerces straight to f64 via the
            // fast widen path. A Tagged value (an `any`, a string-length result, a
            // polymorphic expression) is coerced ToNumber: `coerce(Tagged→Float64)`
            // decodes a tagged int32 / inline double to its f64 (JS ToNumber). Only
            // a Ref repr (array/object word) has no numeric meaning and still bails.
            match v.repr {
                Repr::Int32 | Repr::Int64 | Repr::Float64 | Repr::Tagged => {
                    out.push(self.coerce(v, Repr::Float64)?);
                }
                other => {
                    return unsupported!("Math.{method} arg is not numeric ({other:?})");
                }
            }
        }
        Ok(out)
    }

    /// Emit the op over the already-f64 args. Variadic min/max/hypot FOLD pairwise.
    fn emit_math_op(
        &mut self,
        module: &mut dyn Module,
        op: &MathOp,
        fargs: &[Value],
    ) -> FrontResult<Value> {
        Ok(match op {
            MathOp::Sqrt => self.builder.ins().sqrt(fargs[0]),
            MathOp::Abs => self.builder.ins().fabs(fargs[0]),
            // JS `Math.min`/`max` are NaN-propagating and treat -0 < +0. Cranelift
            // `fmin`/`fmax` are IEEE minimumNumber/maximumNumber (NaN-propagating),
            // which match JS for the all-non-NaN fixture corpus; a NaN arg yields
            // NaN (JS also yields NaN). Fold pairwise for the variadic form.
            MathOp::Min => self.fold_pairwise(fargs, |b, x, y| b.ins().fmin(x, y)),
            MathOp::Max => self.fold_pairwise(fargs, |b, x, y| b.ins().fmax(x, y)),
            MathOp::Sym(sym, _) => {
                if *sym == "__RTS_FN_NS_MATH_HYPOT" && fargs.len() > 2 {
                    // hypot(a,b,c,…) = hypot(hypot(a,b),c)… — fold with the 2-arg sym.
                    let mut acc = fargs[0];
                    for &x in &fargs[1..] {
                        acc = emit_marshal::emit_call(module, self.builder, sym, &[acc, x])
                            .expect("MATH_HYPOT returns a value");
                    }
                    acc
                } else {
                    emit_marshal::emit_call(module, self.builder, sym, fargs)
                        .expect("real math symbol returns a value")
                }
            }
        })
    }

    /// Fold a non-empty slice pairwise with an inline binary IR builder
    /// (`fmin`/`fmax`): `f(f(a,b),c)…`. A single arg returns itself.
    fn fold_pairwise(
        &mut self,
        fargs: &[Value],
        f: impl Fn(&mut cranelift_frontend::FunctionBuilder, Value, Value) -> Value,
    ) -> Value {
        let mut acc = fargs[0];
        for &x in &fargs[1..] {
            acc = f(self.builder, acc, x);
        }
        acc
    }

    /// Lower `Number.method(args)` — the static predicates + parse aliases.
    fn lower_number_call(
        &mut self,
        module: &mut dyn Module,
        method: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        // `Number.parseInt`/`parseFloat` are exact aliases of the global functions.
        match method {
            "parseInt" => return self.parse_int_call(module, args),
            "parseFloat" => {
                return self.coerce_call_pub(module, "__rtsadp_g_parse_float", args);
            }
            _ => {}
        }
        let Some(sym) = number_predicate(method) else {
            return unsupported!("Number.{method}(...) (unsupported Number static method)");
        };
        if args.len() != 1 {
            return unsupported!("Number.{method} expects 1 arg, got {}", args.len());
        }
        // The Number.is* predicates do NOT coerce: they return false for a
        // non-number arg. We require a PROVEN number (so coercing to f64 is exact);
        // a Tagged arg bails (a faithful no-coerce check on a Tagged word is a later
        // increment — never a wrong `true`).
        let v = self.lower_expr(module, &args[0])?;
        if !matches!(v.repr, Repr::Int32 | Repr::Int64 | Repr::Float64) {
            return unsupported!(
                "Number.{method} on a non-proven-number arg ({:?}) — the no-coerce check is a later increment",
                v.repr
            );
        }
        let f = self.coerce(v, Repr::Float64)?;
        let b = emit_marshal::emit_call(module, self.builder, sym, &[f])
            .expect("NUMBER predicate returns a value");
        // The extern returns i64 0/1 → a proven Bool.
        Ok(Val::new(b, Repr::Bool))
    }

    /// An `f64const` as a native `Float64` `Val`.
    fn f64_const_val(&mut self, c: f64) -> Val {
        let v = self.builder.ins().f64const(c);
        Val::new(v, Repr::Float64)
    }

    /// `parseFloat`-style global coercion, returning a Tagged number — reuses the
    /// P5.2 trampoline so formatting/coercion never diverge. (Public-shaped wrapper
    /// over the private `coerce_call` so `Number.parseFloat` shares the path.)
    fn coerce_call_pub(
        &mut self,
        module: &mut dyn Module,
        symbol: &str,
        args: &[HirExpr],
    ) -> FrontResult<Val> {
        if args.len() != 1 {
            return unsupported!("Number.parseFloat expects 1 arg, got {}", args.len());
        }
        let v = self.lower_expr(module, &args[0])?;
        let boxed = self.box_value(v);
        let res = self
            .call_runtime(module, symbol, &[boxed])?
            .expect("global coercion returns a value");
        Ok(Val::tagged_kind(res, JsKind::Number))
    }
}

/// The `__rtsadp_math_reduce` op code for a variadic-foldable `Math` method
/// (`min`=0, `max`=1, `hypot`=2), used for the `Math.m(...array)` spread fold.
fn math_reduce_op(method: &str) -> Option<i64> {
    match method {
        "min" => Some(0),
        "max" => Some(1),
        "hypot" => Some(2),
        _ => None,
    }
}

/// Resolve a `Math.method` name to its lowering op.
fn math_op(method: &str) -> Option<MathOp> {
    use MathOp::*;
    Some(match method {
        // ---- inline Cranelift intrinsics (the preserved fast path) ----
        "sqrt" => Sqrt,
        "abs" => Abs,
        "min" => Min,
        "max" => Max,
        // ---- real `__RTS_FN_NS_MATH_*` symbols (1-arg f64→f64) ----
        "floor" => Sym("__RTS_FN_NS_MATH_FLOOR", 1),
        "ceil" => Sym("__RTS_FN_NS_MATH_CEIL", 1),
        "round" => Sym("__RTS_FN_NS_MATH_ROUND", 1),
        "trunc" => Sym("__RTS_FN_NS_MATH_TRUNC", 1),
        "sign" => Sym("__RTS_FN_NS_MATH_SIGN", 1),
        "cbrt" => Sym("__RTS_FN_NS_MATH_CBRT", 1),
        "exp" => Sym("__RTS_FN_NS_MATH_EXP", 1),
        "expm1" => Sym("__RTS_FN_NS_MATH_EXPM1", 1),
        "log" => Sym("__RTS_FN_NS_MATH_LN", 1),
        "log2" => Sym("__RTS_FN_NS_MATH_LOG2", 1),
        "log10" => Sym("__RTS_FN_NS_MATH_LOG10", 1),
        "log1p" => Sym("__RTS_FN_NS_MATH_LOG1P", 1),
        "sin" => Sym("__RTS_FN_NS_MATH_SIN", 1),
        "cos" => Sym("__RTS_FN_NS_MATH_COS", 1),
        "tan" => Sym("__RTS_FN_NS_MATH_TAN", 1),
        "asin" => Sym("__RTS_FN_NS_MATH_ASIN", 1),
        "acos" => Sym("__RTS_FN_NS_MATH_ACOS", 1),
        "atan" => Sym("__RTS_FN_NS_MATH_ATAN", 1),
        "sinh" => Sym("__RTS_FN_NS_MATH_SINH", 1),
        "cosh" => Sym("__RTS_FN_NS_MATH_COSH", 1),
        "tanh" => Sym("__RTS_FN_NS_MATH_TANH", 1),
        "fround" => Sym("__RTS_FN_NS_MATH_FROUND", 1),
        // ---- 2-arg f64→f64 ----
        "pow" => Sym("__RTS_FN_NS_MATH_POW", 2),
        "atan2" => Sym("__RTS_FN_NS_MATH_ATAN2", 2),
        "hypot" => Sym("__RTS_FN_NS_MATH_HYPOT", 2),
        // ---- i32-domain ops: real symbol (I64 ABI) — handled specially ----
        // imul/clz32 take/return i64 (the runtime's i32-domain). They are NOT in
        // this f64 table; `Math.imul`/`clz32` BAIL (a later increment) rather than
        // mis-marshal an f64 where the symbol wants i64.
        _ => return None,
    })
}

/// The fixed arity an op wants (variadic min/max/hypot report 2 — the caller
/// allows 1..N and folds; the rest are exact).
fn op_arity(op: &MathOp) -> usize {
    match op {
        MathOp::Sqrt | MathOp::Abs => 1,
        MathOp::Min | MathOp::Max => 2,
        MathOp::Sym(_, n) => *n,
    }
}

/// A `Math.CONST` constant's f64 value.
fn math_const(name: &str) -> Option<f64> {
    Some(match name {
        "PI" => std::f64::consts::PI,
        "E" => std::f64::consts::E,
        "LN2" => std::f64::consts::LN_2,
        "LN10" => std::f64::consts::LN_10,
        "LOG2E" => std::f64::consts::LOG2_E,
        "LOG10E" => std::f64::consts::LOG10_E,
        "SQRT2" => std::f64::consts::SQRT_2,
        "SQRT1_2" => std::f64::consts::FRAC_1_SQRT_2,
        _ => return None,
    })
}

/// Whether `name` is a WRAPPER primordial whose ambient prelude `.ts` class is
/// prototype-only (`Number`/`Boolean`/`String`) — so it must NOT shadow the global
/// static surface (`Number.isNaN`, `Number.MAX_SAFE_INTEGER`, …). Mirrors
/// `globalclass::is_wrapper_primordial`: the `.ts` class carries instance methods
/// only, the static surface stays on the global path. `Math` is not a class so it
/// never reaches these checks.
fn is_wrapper_primordial_static(name: &str) -> bool {
    matches!(name, "Boolean" | "Number" | "String")
}

/// A `Number.CONST` constant's f64 value (the numeric statics).
fn number_const(name: &str) -> Option<f64> {
    Some(match name {
        "MAX_SAFE_INTEGER" => 9_007_199_254_740_991.0,
        "MIN_SAFE_INTEGER" => -9_007_199_254_740_991.0,
        "MAX_VALUE" => f64::MAX,
        // JS `Number.MIN_VALUE` is the smallest positive SUBNORMAL double (`5e-324`,
        // `f64::from_bits(1)`), NOT `f64::MIN_POSITIVE` (the smallest NORMAL,
        // `2.2e-308`). Matches `rts-primitives/number.rs`.
        "MIN_VALUE" => f64::from_bits(1),
        "EPSILON" => f64::EPSILON,
        "POSITIVE_INFINITY" => f64::INFINITY,
        "NEGATIVE_INFINITY" => f64::NEG_INFINITY,
        "NaN" => f64::NAN,
        _ => return None,
    })
}

/// The real `__RTS_FN_GL_NUMBER_IS_*` symbol for a Number static predicate.
fn number_predicate(method: &str) -> Option<&'static str> {
    Some(match method {
        "isInteger" => "__RTS_FN_GL_NUMBER_IS_INTEGER",
        "isFinite" => "__RTS_FN_GL_NUMBER_IS_FINITE",
        "isNaN" => "__RTS_FN_GL_NUMBER_IS_NAN",
        "isSafeInteger" => "__RTS_FN_GL_NUMBER_IS_SAFE_INT",
        _ => return None,
    })
}
