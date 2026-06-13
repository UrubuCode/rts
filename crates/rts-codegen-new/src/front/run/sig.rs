//! Function ABI signatures for the whole-program lowering.
//!
//! Increment 4 compiles a *module* of user functions plus a synthesized
//! `__rtsn_main` for the top-level code, and cross-function calls must agree on
//! the ABI at each boundary. The rule (design pilar 2): a parameter / return is
//! carried UNBOXED in its native register when the front-end proves it
//! monomorphic-numeric (via the repr lattice [`crate::front::repr_map::repr_of`]),
//! otherwise it is a [`crate::value::PolyValue`] (`Tagged`, an `i64` raw word).
//!
//! [`FnSig`] freezes that decision per function so both the callee's prologue
//! and every call site box/unbox each value to match.

use cranelift_codegen::ir::{AbiParam, Signature};
use cranelift_module::Module;

use rts_hir::{HirFunc, HirStmt};

use crate::front::repr_map::repr_of;
use crate::repr::Repr;

use super::lower::cl_type;

/// The chosen ABI representation of every parameter and the return of one user
/// function. A `None` return repr means the function returns no value (`void` —
/// used for `__rtsn_main`).
#[derive(Clone, Debug)]
pub struct FnSig {
    pub name: String,
    pub params: Vec<Repr>,
    pub ret: Option<Repr>,
    /// True for an `async`/generator function. Such a function cannot be a sound
    /// first-class VALUE this increment (its call returns a Promise / it suspends);
    /// reifying it BAILS. Direct calls in the numeric subset are unaffected.
    pub is_async: bool,
}

impl FnSig {
    /// Derive the ABI signature of a user function from its HIR types: each
    /// numeric annotation rides its native register; anything else is `Tagged`.
    /// The return follows the same rule (a non-numeric / `Unknown` return — which
    /// is what an inferred-from-`console.log` body produces — becomes `Tagged`).
    pub fn of_func(func: &HirFunc) -> FnSig {
        let params = func
            .params
            .iter()
            .map(|p| repr_for_param(&p.ty))
            .collect();
        // The declared return repr — trusted in general (an explicit `boolean` /
        // `i64` annotation, or a `function`-decl's body-inferred type, is correct).
        let declared = repr_or_tagged(repr_of(&func.ret));
        // ONE narrow correction: the parser assigns the `i64` DEFAULT to an
        // expression-bodied arrow (incl. the hoisted `const f = (x: number) => …`
        // form) even when the body returns a `number` (Float64). Detect EXACTLY
        // that — declared `Int64` but every body `return e` is provably Float64 —
        // and use Float64. This fixes the arrow-default bug WITHOUT touching
        // functions whose returns are cross-fn calls / unknown (where the body
        // type is unreliable and the declared annotation must win, e.g. a mutually
        // recursive `boolean` predicate returning the peer's call).
        let ret = if declared == Repr::Int64 && all_returns_are_float64(func) {
            Repr::Float64
        } else {
            declared
        };
        FnSig { name: func.name.clone(), params, ret: Some(ret), is_async: func.is_async }
    }

    /// The synthesized top-level `__rtsn_main`: no params, no return.
    pub fn main_sig() -> FnSig {
        FnSig { name: "__rtsn_main".to_string(), params: Vec::new(), ret: None, is_async: false }
    }

    /// Build the Cranelift `Signature` for this function under the host call conv.
    pub fn to_cranelift(&self, module: &dyn Module) -> Signature {
        let mut sig = Signature::new(module.isa().default_call_conv());
        for &p in &self.params {
            sig.params.push(AbiParam::new(cl_type(p)));
        }
        if let Some(r) = self.ret {
            sig.returns.push(AbiParam::new(cl_type(r)));
        }
        sig
    }
}

/// The ABI repr of a parameter: a proven numeric annotation stays unboxed; an
/// unannotated (`Unknown`) or non-numeric parameter is `Tagged`.
fn repr_for_param(ty: &rts_hir::HirType) -> Repr {
    repr_or_tagged(repr_of(ty))
}

/// Whether the function has at least one `return e` and EVERY such `return e` is
/// provably a `Float64` (`number`/`f64`) expression. This is the precise
/// signature of the parser's expression-bodied-arrow `i64`-default bug: the
/// declared ret is `i64` but the body actually returns a `number`. We only
/// override the declared ret in this exact case (see [`FnSig::of_func`]); a
/// `return` whose type is `Unknown` (a cross-fn call) makes this `false`, so the
/// declared annotation is kept (correct for recursive predicates).
fn all_returns_are_float64(func: &HirFunc) -> bool {
    let mut any = false;
    let mut all_float = true;
    walk_returns(&func.body, &mut any, &mut all_float);
    any && all_float
}

/// Walk the lowering-subset statements; set `any` if any `return e` is seen and
/// clear `all_float` unless every `return e` is a provable Float64 expression.
fn walk_returns(stmts: &[HirStmt], any: &mut bool, all_float: &mut bool) {
    for s in stmts {
        match s {
            HirStmt::Return(Some(e)) => {
                *any = true;
                if repr_of(&e.ty) != Repr::Float64 {
                    *all_float = false;
                }
            }
            HirStmt::If { then, else_, .. } => {
                walk_returns(then, any, all_float);
                if let Some(e) = else_ {
                    walk_returns(e, any, all_float);
                }
            }
            HirStmt::While { body, .. } | HirStmt::Block(body) => {
                walk_returns(body, any, all_float);
            }
            _ => {}
        }
    }
}

/// A native repr passes through; everything else collapses to `Tagged` (no
/// `Bool` is exposed across the call boundary as anything other than its i64
/// carrier, which `cl_type` already handles — `Tagged` vs `Bool` only differ in
/// the value layer's interpretation, both are i64 registers).
fn repr_or_tagged(r: Repr) -> Repr {
    if r.is_unboxed() {
        r
    } else {
        Repr::Tagged
    }
}
