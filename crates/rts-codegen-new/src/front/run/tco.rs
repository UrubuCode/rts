//! Tail-call optimization (Phase 4): the decision + emission for lowering a
//! `return f(args)` to a Cranelift `return_call`, kept out of [`super::stmt`] (the
//! <500-line module rule). [`super::stmt::Lowerer::lower_return`] calls
//! [`Lowerer::try_tail_return`]; the actual `return_call` is
//! [`Lowerer::emit_user_call_tail`] (in [`super::call_spread`]).
//!
//! TCO is sound here because `return f(args)` already propagates whatever `f`
//! produces — including a thrown value (which sets the thread pending-error slot
//! and returns a sentinel, both flowing to OUR caller) — so handing our frame
//! straight to `f` changes nothing observable but the stack depth.

use cranelift_module::Module;

use rts_hir::HirExpr;
use rts_hir::ir::HirExprKind;

use crate::repr::Repr;

use super::lower::Lowerer;
use crate::front::error::FrontResult;

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// Try to lower `return e` as a tail `return_call`. Returns `Ok(true)` (and emits
    /// the `return_call`) iff `e` is a DIRECT call `f(args)` to a top-level user
    /// function that:
    /// - reaches the native user-call path (callee is a bare ident that is NOT a
    ///   builtin-import or a capturing closure — mirroring `lower_call`'s precedence,
    ///   so we never tail-call a closure body without its env or misroute a builtin);
    /// - is itself `tail_callable` AND so are we (`self_tail_callable`), so both ends
    ///   use the `tail` conv `return_call` requires;
    /// - returns the SAME repr as the current function (`return_call` requires the
    ///   callee's return type equal the caller's);
    /// - takes no `this` receiver, has no rest param, is not async, and the call has
    ///   no spread arg (these reuse the exact `marshal_call_args` the normal path
    ///   uses, but the receiver/rest/spread shapes are out of this increment's scope).
    /// Otherwise returns `Ok(false)` and emits nothing (the caller does the normal
    /// call+return). A marshaling bail propagates as `Err`, exactly as the normal path.
    pub(super) fn try_tail_return(
        &mut self,
        module: &mut dyn Module,
        e: &HirExpr,
        ret: Repr,
    ) -> FrontResult<bool> {
        if !self.self_tail_callable {
            return Ok(false);
        }
        let HirExprKind::Call { callee, args } = &e.kind else {
            return Ok(false);
        };
        let HirExprKind::Ident(name) = &callee.kind else {
            return Ok(false);
        };
        // Mirror lower_call's precedence: a builtin-import or a capturing closure name
        // does NOT reach the native user-call path, so it is not a tail-call target.
        if self.builtins.contains_key(name) || self.captures.contains_key(name) {
            return Ok(false);
        }
        let Some(sig) = self.sigs.get(name).cloned() else {
            return Ok(false);
        };
        let eligible = sig.tail_callable
            && sig.ret == Some(ret)
            && sig.rest_param.is_none()
            && !sig.has_this
            && !sig.is_async
            && !args.iter().any(|a| matches!(a.kind, HirExprKind::Spread(_)));
        if !eligible {
            return Ok(false);
        }
        let lowered = self.marshal_call_args(module, &sig, None, args)?;
        self.emit_user_call_tail(module, &sig, &lowered)?;
        Ok(true)
    }
}
