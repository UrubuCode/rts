//! Module-level mutable global cells (epic #195) — the read/write helpers.
//!
//! A top-level `let` that is WRITTEN from inside a function is promoted by
//! [`super::funcval::module_globals`] to a runtime CELL with a compile-time id.
//! Every access — at the top level AND inside the writing function — goes through
//! `__RTS_FN_NS_GC_GCELL_GET/SET` by that id, so the value is genuinely shared
//! (no by-value capture snapshot). The cell stores a PolyValue word; the GC root
//! scanner marks live cell contents (`collector::mark_gcell_roots`).
//!
//! A cell name is resolved ONLY when it is not shadowed by a real local/param in
//! the current function (`self.local(name).is_none()`), so a same-spelled local
//! still wins — matching JS lexical scoping.

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use crate::repr::Repr;

use super::lower::{Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// The cell id for `name`, or `None` when `name` is not a module-global cell
    /// (or is shadowed by a real local/param in the current function).
    pub(super) fn gcell_id(&self, name: &str) -> Option<u32> {
        if self.local(name).is_some() {
            return None;
        }
        self.gcells.get(name).copied()
    }

    /// Load cell `id` → a `Tagged` PolyValue word (the stored value, kind unknown).
    pub(super) fn emit_gcell_get(
        &mut self,
        module: &mut dyn Module,
        id: u32,
    ) -> crate::front::error::FrontResult<Val> {
        let id_v = self.builder.ins().iconst(types::I64, id as i64);
        let w = self
            .call_runtime(module, "__RTS_FN_NS_GC_GCELL_GET", &[id_v])?
            .expect("GCELL_GET returns a word");
        Ok(Val::new(w, Repr::Tagged))
    }

    /// Store `word` (an already-boxed PolyValue) into cell `id`.
    pub(super) fn emit_gcell_set(
        &mut self,
        module: &mut dyn Module,
        id: u32,
        word: Value,
    ) -> crate::front::error::FrontResult<()> {
        let id_v = self.builder.ins().iconst(types::I64, id as i64);
        self.call_runtime(module, "__RTS_FN_NS_GC_GCELL_SET", &[id_v, word])?;
        Ok(())
    }
}
