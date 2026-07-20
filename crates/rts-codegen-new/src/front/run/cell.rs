//! Function-local mutable CELLS (epic #195) — a function-scope `let` that a
//! closure CAPTURES and MUTATES.
//!
//! Unlike a module-level gcell (a compile-time id in a process-global store), a
//! function-local cell is a PER-INVOCATION runtime box: a 1-slot `Vec` allocated
//! at the local's declaration. The local VAR holds the cell HANDLE (a `TAG_OBJECT`
//! PolyValue word); the closure captures that HANDLE **by value** (the handle is
//! stable) through the ordinary env mechanism ([`super::call::build_closure_env`]
//! reads `use_var` → the raw handle, never the dereferenced value), so the
//! declaring function and the closure share ONE mutable box.
//!
//! Every READ of the name → `VEC_GET(handle, 0)`; every WRITE → `VEC_SET(handle,
//! 0, word)`. The cell (a `Vec` GC entry) stays reachable via the captured handle
//! even after the declaring function returns (a returned counter closure
//! `function mk(){ let c=0; return () => ++c; }`), so the value survives.
//!
//! Only locals PROVEN captured-and-mutated become cells (see
//! [`super::funcval`] `try_extract`): a non-captured mutable local stays a plain
//! Cranelift Variable (the fast path), and a read-only capture stays by-value.

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_module::Module;

use crate::repr::Repr;
use crate::value::emit_marshal;

use super::lower::{Lowerer, Val};

impl<'a, 'b, 'c> Lowerer<'a, 'b, 'c> {
    /// True when `name` is a function-local cell in the CURRENT function: its local
    /// Variable holds the cell HANDLE, not the value directly, so a read/write must
    /// route through [`Self::emit_cell_get`]/[`Self::emit_cell_set`].
    pub(super) fn is_cell_local(&self, name: &str) -> bool {
        self.cell_locals.contains(name)
    }

    /// Allocate a fresh 1-slot cell holding `init_word` and return its boxed HANDLE
    /// word (a `TAG_OBJECT` PolyValue). Used at a cell-local's `let` declaration —
    /// the handle is then bound to the local's Variable (a `Tagged` register).
    pub(super) fn emit_cell_alloc(&mut self, module: &mut dyn Module, init_word: Value) -> Value {
        let handle = emit_marshal::emit_new_vec_object(module, self.builder);
        emit_marshal::emit_vec_push(module, self.builder, handle, init_word);
        handle
    }

    /// Load the cell behind `handle_word` (slot 0) → a `Tagged` PolyValue word (the
    /// stored value, kind unknown — same shape as a gcell read).
    pub(super) fn emit_cell_get(&mut self, module: &mut dyn Module, handle_word: Value) -> Val {
        let idx = self.builder.ins().iconst(types::I64, 0);
        let w = emit_marshal::emit_vec_get(module, self.builder, handle_word, idx);
        Val::new(w, Repr::Tagged)
    }

    /// Store `word` (an already-boxed PolyValue) into the cell behind `handle_word`
    /// (slot 0). The cell was allocated with one slot at declaration, so index 0
    /// always exists.
    pub(super) fn emit_cell_set(
        &mut self,
        module: &mut dyn Module,
        handle_word: Value,
        word: Value,
    ) {
        let idx = self.builder.ins().iconst(types::I64, 0);
        emit_marshal::emit_vec_set(module, self.builder, handle_word, idx, word);
    }
}
