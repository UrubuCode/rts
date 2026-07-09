//! Reifying a PRIMORDIAL coercion global (`Boolean`/`Number`/`String`) as a
//! first-class callback FUNCTION VALUE — the `arr.filter(Boolean)` /
//! `arr.map(String)` / `arr.map(Number)` idiom.
//!
//! `Boolean`/`Number`/`String` are PRIMORDIAL (the engine MAY name them). Used as
//! an Array callback the name must become a real callable word; this reifies it
//! through the runtime `__rtsadp_coerce_fn_value(op)` reifier (op: 0 = Boolean,
//! 1 = Number, 2 = String), whose thunk applies the SAME ToBoolean/ToNumber/ToString
//! the direct `Boolean(x)`/`Number(x)`/`String(x)` call uses.

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use crate::front::error::FrontResult;

use super::lower::Lowerer;

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// The `__rtsadp_coerce_fn_value` op if `name` is an UNSHADOWED PRIMORDIAL
    /// coercion global (`Boolean`/`Number`/`String`), else `None`. A same-named local
    /// or user function shadows it (both resolve on their own paths).
    pub(super) fn coerce_fn_op_for(&self, name: &str) -> Option<i64> {
        if self.local(name).is_some() || self.sigs.contains_key(name) {
            return None;
        }
        match name {
            "Boolean" => Some(0),
            "Number" => Some(1),
            "String" => Some(2),
            _ => None,
        }
    }

    /// Emit a coercion FUNCTION-value word (`op`: 0 = Boolean, 1 = Number, 2 = String)
    /// via the runtime `__rtsadp_coerce_fn_value` reifier.
    pub(super) fn emit_coerce_fn_value(
        &mut self,
        module: &mut dyn Module,
        op: i64,
    ) -> FrontResult<Value> {
        let op_v = self.builder.ins().iconst(types::I64, op);
        Ok(self
            .call_runtime(module, "__rtsadp_coerce_fn_value", &[op_v])?
            .expect("__rtsadp_coerce_fn_value returns a fn word"))
    }
}
