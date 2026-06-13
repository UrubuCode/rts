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

use rts_hir::HirFunc;

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
        let ret = repr_or_tagged(repr_of(&func.ret));
        FnSig { name: func.name.clone(), params, ret: Some(ret) }
    }

    /// The synthesized top-level `__rtsn_main`: no params, no return.
    pub fn main_sig() -> FnSig {
        FnSig { name: "__rtsn_main".to_string(), params: Vec::new(), ret: None }
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
