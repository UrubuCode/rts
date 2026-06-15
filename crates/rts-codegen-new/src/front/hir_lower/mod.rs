//! HIR → Cranelift for the proven-monomorphic numeric subset (the fast path).
//!
//! One typed [`HirFunc`] lowers straight to a Cranelift `Function` over
//! `&mut dyn Module` — the SAME single lowering path the design mandates (no
//! second optimizer tier; Cranelift's egraph is the only optimizer). The
//! front-end's job here is exactly representation choice + native op selection:
//! a `Float64` add is `fadd`, an `Int32` add is `iadd`, a comparison is
//! `fcmp`/`icmp` widened to a `Bool` (i64 0/1). NO value is boxed — the numeric
//! subset stays fully unboxed, which is the whole point of preserving the fast
//! path.
//!
//! Locals (`let`/`const` + assignment) use Cranelift [`Variable`]s, so the
//! `FunctionBuilder` constructs SSA + φ-nodes for us across `if`/`while`. Every
//! unmodeled construct returns an explicit [`Unsupported`] (see [`super::error`])
//! — never a silent miscompile.
//!
//! Submodules: this file holds the driver + function shell + signature + the
//! local/repr scaffolding; [`expr`] lowers expressions; [`stmt`] lowers
//! statements + control flow. Each stays well under 500 lines.

mod expr;
mod stmt;

use std::collections::HashMap;

use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, Value, types};
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::Module;

use rts_hir::{HirFunc, HirType};

use crate::repr::Repr;

use super::error::{FrontResult, Unsupported, unsupported};
use super::repr_map::repr_of;

/// The Cranelift type a value of `repr` is carried in: `Float64` in an `f64`,
/// everything unboxed-integer (`Int32`/`Bool`) sign/zero-extended in an `i64`.
pub fn cl_type(repr: Repr) -> types::Type {
    match repr {
        Repr::Float64 => types::F64,
        _ => types::I64,
    }
}

/// An SSA value tagged with the representation it carries.
#[derive(Clone, Copy)]
pub(crate) struct Val {
    pub v: Value,
    pub repr: Repr,
}

/// A local binding: its Cranelift variable and the repr it holds. The numeric
/// subset requires a local to have a single stable repr across the function
/// (assigning a different repr is an explicit bail — no Tagged join here).
#[derive(Clone, Copy)]
struct Local {
    var: Variable,
    repr: Repr,
}

/// The per-function lowering context.
pub(crate) struct Lowerer<'a, 'b> {
    pub builder: &'a mut FunctionBuilder<'b>,
    /// Name → local binding, innermost scope wins. A flat map is sufficient: the
    /// numeric subset has no shadowing within one function that changes repr, and
    /// re-`let` of the same name in a nested block reuses the variable slot.
    locals: HashMap<String, Local>,
    /// The function's declared return repr (every `return e` coerces to it, and a
    /// disagreeing repr is a bail).
    ret: Repr,
    /// True once a terminator (a `return`) has been emitted in the current block,
    /// so the driver can avoid appending a fallthrough terminator.
    block_terminated: bool,
}

/// Build the Cranelift `Signature` for `func` under the host call convention,
/// failing if any param or the return type is outside the numeric subset.
pub fn signature_for(func: &HirFunc, module: &dyn Module) -> FrontResult<Signature> {
    let mut sig = Signature::new(module.isa().default_call_conv());
    for p in &func.params {
        if p.variadic || p.has_default {
            return unsupported!("parameter `{}` is variadic/defaulted", p.name);
        }
        let r = numeric_repr(&p.ty).ok_or_else(|| {
            Unsupported::new(format!(
                "parameter `{}` has non-numeric type {:?}",
                p.name, p.ty
            ))
        })?;
        sig.params.push(AbiParam::new(cl_type(r)));
    }
    let ret_r = numeric_repr(&func.ret)
        .ok_or_else(|| Unsupported::new(format!("return type {:?} is non-numeric", func.ret)))?;
    sig.returns.push(AbiParam::new(cl_type(ret_r)));
    Ok(sig)
}

/// The unboxed repr of `t`, or `None` if `t` is outside the numeric subset.
fn numeric_repr(t: &HirType) -> Option<Repr> {
    let r = repr_of(t);
    r.is_unboxed().then_some(r)
}

/// Lower `func` into `builder` (whose `Function` already carries the signature
/// from [`signature_for`]). On success the builder holds a complete, finalizable
/// function. `module` is unused today (the numeric subset emits no externs) but
/// kept in the signature so cross-fn calls can declare imports in a follow-up.
pub fn lower_func(
    _module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    func: &HirFunc,
) -> FrontResult<()> {
    let ret = numeric_repr(&func.ret)
        .ok_or_else(|| Unsupported::new(format!("return type {:?} is non-numeric", func.ret)))?;

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);

    let mut ctx = Lowerer {
        builder,
        locals: HashMap::new(),
        ret,
        block_terminated: false,
    };

    // Bind each parameter to a fresh local Variable so it can be reassigned and
    // so control flow sees it as a normal SSA-managed binding.
    for (i, p) in func.params.iter().enumerate() {
        let repr = numeric_repr(&p.ty).ok_or_else(|| {
            Unsupported::new(format!(
                "parameter `{}` has non-numeric type {:?}",
                p.name, p.ty
            ))
        })?;
        let block_val = ctx.builder.block_params(entry)[i];
        let var = ctx.builder.declare_var(cl_type(repr));
        ctx.builder.def_var(var, block_val);
        ctx.locals.insert(p.name.clone(), Local { var, repr });
    }

    ctx.lower_block(&func.body)?;

    // If control can fall off the end without a `return`, the function would be
    // ill-formed (no terminator). The numeric subset requires an explicit return
    // on every path; we surface a missing tail return as a bail rather than
    // emit a bogus default.
    if !ctx.block_terminated {
        return unsupported!(
            "function `{}` may fall through without returning",
            func.name
        );
    }
    Ok(())
}

impl<'a, 'b> Lowerer<'a, 'b> {
    /// Lower a statement block (does not open a Cranelift block — control flow is
    /// statement-driven). Stops emitting once a block is terminated.
    fn lower_block(&mut self, stmts: &[rts_hir::HirStmt]) -> FrontResult<()> {
        for s in stmts {
            if self.block_terminated {
                // Dead code after a return in the same straight-line block: the
                // numeric subset rejects it rather than silently drop it.
                return unsupported!("unreachable statement after return");
            }
            self.lower_stmt(s)?;
        }
        Ok(())
    }

    /// Coerce `val` to `target`. Legal numeric-subset coercions:
    /// - integer (`Int32`/`Int64`) → `Float64`: JS widening to a double
    ///   (`fcvt_from_sint`).
    /// - `Int32` ↔ `Int64`: a relabel only — both share the i64 register, so the
    ///   machine value passes through unchanged.
    ///
    /// A `Bool` and a number never silently interconvert, and `Tagged` never
    /// appears in the numeric subset. Anything else is an explicit bail.
    fn coerce(&mut self, val: Val, target: Repr) -> FrontResult<Value> {
        if val.repr == target {
            return Ok(val.v);
        }
        match (val.repr, target) {
            (Repr::Int32, Repr::Float64) | (Repr::Int64, Repr::Float64) => {
                // sign-extended int lives in i64; convert that to f64.
                Ok(self.builder.ins().fcvt_from_sint(types::F64, val.v))
            }
            // Both integer reprs live in the same i64 register: relabel only.
            (Repr::Int32, Repr::Int64) | (Repr::Int64, Repr::Int32) => Ok(val.v),
            (from, to) => unsupported!("cannot coerce {from:?} to {to:?} in the numeric subset"),
        }
    }
}
