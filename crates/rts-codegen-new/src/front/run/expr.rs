//! Expression lowering for the whole-program (Tagged-capable) path.
//!
//! Numeric operands keep the native fast path (`iadd`/`fadd`/`fcmp`/…); the
//! Tagged additions over [`crate::front::hir_lower`] are: string literals,
//! `+`/`===`/`!==`/`typeof` on Tagged/mixed operands (the ONE generic runtime
//! path), `console.log(...)`, and cross-function calls with per-param box/unbox.

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{types, InstBuilder};
use cranelift_module::Module;

use rts_hir::ir::HirExprKind;
use rts_hir::{HirExpr, HirLit, HirUnOp};

use crate::repr::Repr;
use crate::value;
use crate::value::abi_adapter;

use crate::front::error::{unsupported, FrontResult};

use super::lower::{JsKind, Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Lower an expression to its value + repr.
    pub(super) fn lower_expr(
        &mut self,
        module: &mut dyn Module,
        e: &HirExpr,
    ) -> FrontResult<Val> {
        match &e.kind {
            HirExprKind::Lit(lit) => self.lower_lit(lit),
            HirExprKind::Ident(name) => self.lower_ident(module, name),
            HirExprKind::Bin { op, lhs, rhs } => self.lower_bin(module, *op, lhs, rhs),
            HirExprKind::Unary { op, operand } => self.lower_unary(module, *op, operand),
            HirExprKind::Ternary { cond, then, else_ } => {
                self.lower_ternary(module, cond, then, else_)
            }
            HirExprKind::Assign { target, value } => self.lower_assign(module, target, value),
            HirExprKind::AssignOp { op, target, value } => {
                self.lower_assign_op(module, *op, target, value)
            }
            HirExprKind::PreInc(t) => self.lower_incdec(module, t, true, true),
            HirExprKind::PreDec(t) => self.lower_incdec(module, t, false, true),
            HirExprKind::PostInc(t) => self.lower_incdec(module, t, true, false),
            HirExprKind::PostDec(t) => self.lower_incdec(module, t, false, false),
            HirExprKind::Call { callee, args } => self.lower_call(module, callee, args),
            HirExprKind::MethodCall { object, method, args } => {
                self.lower_method_call(module, object, method, args)
            }
            HirExprKind::Member { object, prop } => self.lower_member(module, object, prop),
            HirExprKind::Index { object, index } => self.lower_index(module, object, index),
            HirExprKind::New { class, args } => {
                // `new Array(n)` is the built-in Array constructor (not a user
                // class) → a sized array value (P5.2).
                if self.is_builtin_array_ctor(class) {
                    return self.lower_new_array(module, args);
                }
                // `new Map()` / `new Set()` / `new Error(..)` / wrapper used as a
                // bare expression (P5.3): build the runtime-class instance (a valid
                // TAG_OBJECT word, so `console.log(new Error("x"))` / chaining work).
                if self.is_global_class_ctor(class) {
                    let (val, _class) = self.lower_new_global_class(module, class, args)?;
                    return Ok(val);
                }
                // `new F(args)` where `F` is a FUNCTION (not a class) — a free
                // function used as a constructor (Phase 2/3): run F with a fresh
                // `this`, return F's object-return-or-the-instance.
                if self.is_fn_ctor(class) {
                    return self.lower_new_fn_ctor(module, class, args);
                }
                // A `new C(args)` used as a bare expression (not bound to a local
                // whose class/shape we record) still builds the instance (a valid
                // TAG_OBJECT PolyValue, so `console.log(new C())` / method chaining
                // work); the shape id is discarded.
                let (val, _class, _shape) = self.lower_new(module, class, args)?;
                Ok(val)
            }
            HirExprKind::Array(elems) => self.lower_array_literal(module, elems),
            HirExprKind::Object(fields) => {
                // An object literal used as a bare expression (not bound to a local
                // whose shape we record) has no addressable shape afterward; we
                // still build it (its value is a valid `TAG_OBJECT` PolyValue, so
                // `typeof`/`console.log` work), discarding the shape id.
                let (val, _shape, _lit_class) = self.lower_object_literal(module, fields)?;
                Ok(val)
            }
            // A regex literal `/pat/flags` reaches the HIR as `Raw("regex\0..")`
            // (P5.12): compile it to a RegExp instance word.
            HirExprKind::Raw(_) if super::regex::is_regex_literal(e) => {
                let (pattern, flags) =
                    super::regex::regex_literal_parts(e).expect("is_regex_literal proved parts");
                self.lower_regex_literal(module, &pattern, &flags)
            }
            other => unsupported!("expression {}", super::stmt::expr_variant_name(other)),
        }
    }

    fn lower_lit(&mut self, lit: &HirLit) -> FrontResult<Val> {
        match lit {
            HirLit::Float(f) | HirLit::Number(f) => {
                let v = self.builder.ins().f64const(*f);
                Ok(Val::new(v, Repr::Float64))
            }
            HirLit::Int(n) => {
                let v = self.builder.ins().iconst(types::I64, *n);
                Ok(Val::new(v, Repr::Int64))
            }
            HirLit::Bool(b) => {
                let v = self.builder.ins().iconst(types::I64, *b as i64);
                Ok(Val::new(v, Repr::Bool))
            }
            HirLit::Str(s) => {
                // Intern the literal in the REAL string pool at lowering time and
                // splice the boxed string PolyValue word (whose 48-bit payload is
                // the real handle's slot+shard) in as a constant (Tagged, kind
                // Str). At run time `__RTS_FN_NS_GC_POLY_TO_HANDLE(payload)`
                // reconstructs the full handle (generation read from the live
                // slot); the slot is a normal GC reference, no side table.
                let pv = abi_adapter::intern_poly(s);
                let v = self.builder.ins().iconst(types::I64, pv.raw() as i64);
                Ok(Val::tagged_kind(v, JsKind::Str))
            }
            HirLit::Null => {
                let v = self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::null().raw() as i64);
                Ok(Val::tagged_kind(v, JsKind::Null))
            }
            HirLit::Undefined => {
                let v = self
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
                Ok(Val::tagged_kind(v, JsKind::Undefined))
            }
        }
    }

    fn lower_ident(&mut self, module: &mut dyn Module, name: &str) -> FrontResult<Val> {
        if let Some(local) = self.local(name) {
            let v = self.builder.use_var(local.var);
            // A local's static kind is its repr-implied kind; a Tagged local
            // carries `Unknown` (we do not flow string-ness through vars yet),
            // which makes strict-eq over it conservatively bail.
            return Ok(Val::new(v, local.repr));
        }
        // GLOBAL value constants (P5.2): `NaN`/`Infinity`/`undefined` resolve to
        // their PolyValue when a local of that name does not shadow them. These are
        // the #1 "unbound identifier" bail in the histogram. `NaN`/`Infinity` are
        // genuine doubles (the fast numeric path); `undefined` is the singleton.
        if let Some(g) = global_constant(name) {
            return Ok(g.lower(self));
        }
        // An identifier that is not a local but names a user FUNCTION is a
        // function VALUE reference (typeof f, `f` stored/passed/returned): reify it
        // into a TAG_FUNCTION PolyValue (P4.6). `lower_call` intercepts a direct
        // call `f(x)` before reaching here, so this only fires in value position.
        if self.sigs.contains_key(name) {
            return self.reify_function(module, name);
        }
        unsupported!("unbound identifier `{name}`")
    }

    fn lower_unary(
        &mut self,
        module: &mut dyn Module,
        op: HirUnOp,
        operand: &HirExpr,
    ) -> FrontResult<Val> {
        // `typeof e`: if the operand is a literal we know statically, fold to a
        // constant string; else box + `__rtsadp_typeof`. The result is a string.
        if matches!(op, HirUnOp::TypeOf) {
            if let Some(s) = static_typeof(operand) {
                let pv = abi_adapter::intern_poly(s);
                let v = self.builder.ins().iconst(types::I64, pv.raw() as i64);
                return Ok(Val::tagged_kind(v, JsKind::Str));
            }
            let val = self.lower_expr(module, operand)?;
            let boxed = self.box_value(val);
            let res = self
                .call_runtime(module, "__rtsadp_typeof", &[boxed])?
                .expect("__rtsadp_typeof returns a value");
            return Ok(Val::tagged_kind(res, JsKind::Str));
        }

        let val = self.lower_expr(module, operand)?;
        match op {
            HirUnOp::Neg => match val.repr {
                Repr::Float64 => {
                    let v = self.builder.ins().fneg(val.v);
                    Ok(Val::new(v, Repr::Float64))
                }
                Repr::Int32 | Repr::Int64 => {
                    let v = self.builder.ins().ineg(val.v);
                    Ok(Val::new(v, val.repr))
                }
                // Tagged operand → generic ToNumber-then-negate.
                Repr::Tagged => {
                    let boxed = self.box_value(val);
                    let res = self
                        .call_runtime(module, "__rtsadp_neg", &[boxed])?
                        .expect("__rtsadp_neg returns a value");
                    Ok(Val::new(res, Repr::Tagged))
                }
                other => unsupported!("unary `-` on repr {other:?}"),
            },
            // `~` (ToInt32 then bitwise NOT). Native for proven ints; generic for
            // Tagged. swc maps `~` to `BitNot` UNAMBIGUOUSLY, so this is sound.
            HirUnOp::BitNot => match val.repr {
                Repr::Int32 | Repr::Int64 => {
                    let v = self.builder.ins().bnot(val.v);
                    Ok(Val::new(v, val.repr))
                }
                Repr::Tagged => {
                    let boxed = self.box_value(val);
                    let res = self
                        .call_runtime(module, "__rtsadp_bnot", &[boxed])?
                        .expect("__rtsadp_bnot returns a value");
                    Ok(Val::new(res, Repr::Tagged))
                }
                other => unsupported!("unary `~` on repr {other:?}"),
            },
            // Unary `+` (ToNumber). swc now lowers `+` to a DISTINCT `HirUnOp::Plus`
            // (no longer conflated with `!`), so the engine lowers it soundly. A
            // proven number is the identity (`+5` is `5`); a Tagged operand goes
            // through `__rtsadp_pos` (ToNumber, returns a number PolyValue).
            HirUnOp::Plus => match val.repr {
                Repr::Int32 | Repr::Int64 | Repr::Float64 => Ok(val),
                Repr::Bool => {
                    // `+true` is `1`, `+false` is `0` — widen the i64 0/1 to int.
                    Ok(Val::new(val.v, Repr::Int64))
                }
                Repr::Tagged => {
                    let boxed = self.box_value(val);
                    let res = self
                        .call_runtime(module, "__rtsadp_pos", &[boxed])?
                        .expect("__rtsadp_pos returns a value");
                    Ok(Val::new_with_kind(res, Repr::Tagged, JsKind::Number))
                }
                other => unsupported!("unary `+` on repr {other:?}"),
            },
            // Logical `!` (ToBoolean then invert). swc now lowers `!` to a DISTINCT
            // `HirUnOp::Not`. A proven bool/number folds inline (native ToBoolean +
            // invert); a Tagged operand goes through `__rtsadp_not`.
            HirUnOp::Not => {
                let cond = self.as_bool_value(module, val)?;
                // cond is i64 0/1; invert via `cond == 0`.
                let zero = self.builder.ins().iconst(types::I64, 0);
                let inverted = self.builder.ins().icmp(IntCC::Equal, cond, zero);
                let widened = self.builder.ins().uextend(types::I64, inverted);
                Ok(Val::new(widened, Repr::Bool))
            }
            // `delete obj.key`: a sound minimal behavior. The engine's object model
            // has no slot-removal (the transition tree is a later increment), so a
            // genuine remove BAILS; but `delete` always EVALUATES to `true` in JS
            // for a configurable/missing property, and the common fixture use is
            // `delete o.absent` / discarding the result. We bail on a member/index
            // target (cannot actually remove) and only return `true` for the
            // never-removes cases is unsound — so bail uniformly rather than guess.
            HirUnOp::Delete => unsupported!(
                "`delete` (slot removal needs the transition tree — a later increment)"
            ),
            other => unsupported!("unary operator {other:?}"),
        }
    }

    fn lower_ternary(
        &mut self,
        module: &mut dyn Module,
        cond: &HirExpr,
        then: &HirExpr,
        else_: &HirExpr,
    ) -> FrontResult<Val> {
        let c = self.lower_expr(module, cond)?;
        let cond_v = self.as_bool_value(module, c)?;
        let t = self.lower_expr(module, then)?;
        let e = self.lower_expr(module, else_)?;
        let target = ternary_target(t.repr, e.repr)?;
        let tv = self.coerce(t, target)?;
        let ev = self.coerce(e, target)?;
        let v = self.builder.ins().select(cond_v, tv, ev);
        // Kind is provable only when both arms share it; otherwise fall to the
        // repr-implied kind (Unknown for Tagged).
        let kind = if t.kind == e.kind { t.kind } else { JsKind::Unknown };
        Ok(Val { v, repr: target, kind })
    }
}

// ---------------------------------------------------------------------------
// Free helpers.
// ---------------------------------------------------------------------------

/// The join repr for two ternary arms; widen disagreeing numerics to f64, or
/// fall to `Tagged` when one arm is already Tagged.
fn ternary_target(t: Repr, e: Repr) -> FrontResult<Repr> {
    if t == e {
        return Ok(t);
    }
    if matches!(t, Repr::Tagged) || matches!(e, Repr::Tagged) {
        return Ok(Repr::Tagged);
    }
    if t.is_unboxed() && e.is_unboxed() && !matches!(t, Repr::Bool) && !matches!(e, Repr::Bool) {
        return Ok(Repr::Float64);
    }
    unsupported!("ternary arms have incompatible reprs {t:?} / {e:?}")
}

/// A GLOBAL value constant resolvable as a bare identifier (P5.2). `globalThis`
/// is deliberately NOT here: it has no useful value representation in this engine
/// (no global object), so referencing it stays an unbound-identifier bail.
enum GlobalConst {
    /// `NaN` — a genuine double (the numeric fast path), so `n + NaN` etc. stay
    /// native f64 arithmetic.
    NaN,
    /// `Infinity` — `+∞` as a double.
    Infinity,
    /// `undefined` — the singleton PolyValue (kind Undefined).
    Undefined,
}

impl GlobalConst {
    /// Emit the constant's value into `ctx`.
    fn lower(self, ctx: &mut Lowerer) -> Val {
        match self {
            GlobalConst::NaN => {
                let v = ctx.builder.ins().f64const(f64::NAN);
                Val::new(v, Repr::Float64)
            }
            GlobalConst::Infinity => {
                let v = ctx.builder.ins().f64const(f64::INFINITY);
                Val::new(v, Repr::Float64)
            }
            GlobalConst::Undefined => {
                let v = ctx
                    .builder
                    .ins()
                    .iconst(types::I64, value::PolyValue::undefined().raw() as i64);
                Val::tagged_kind(v, JsKind::Undefined)
            }
        }
    }
}

/// Resolve a bare identifier to a [`GlobalConst`], if it names one.
fn global_constant(name: &str) -> Option<GlobalConst> {
    match name {
        "NaN" => Some(GlobalConst::NaN),
        "Infinity" => Some(GlobalConst::Infinity),
        "undefined" => Some(GlobalConst::Undefined),
        _ => None,
    }
}

/// The compile-time `typeof` string for a literal operand, when statically known.
/// (Only literals fold; an identifier's runtime tag is inspected via the runtime
/// op, which is always correct.)
fn static_typeof(e: &HirExpr) -> Option<&'static str> {
    match &e.kind {
        HirExprKind::Lit(HirLit::Int(_))
        | HirExprKind::Lit(HirLit::Float(_))
        | HirExprKind::Lit(HirLit::Number(_)) => Some("number"),
        HirExprKind::Lit(HirLit::Str(_)) => Some("string"),
        HirExprKind::Lit(HirLit::Bool(_)) => Some("boolean"),
        HirExprKind::Lit(HirLit::Undefined) => Some("undefined"),
        // `typeof null` is the famous `"object"`.
        HirExprKind::Lit(HirLit::Null) => Some("object"),
        _ => None,
    }
}
