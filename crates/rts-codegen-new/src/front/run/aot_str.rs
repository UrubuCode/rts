//! AOT string-literal support: emit a string literal as a read-only DATA object
//! in the module + a runtime `string_from_static(ptr,len)` call, instead of the
//! JIT path that bakes a compile-time-interned handle as an `iconst`.
//!
//! WHY: the JIT lowering interns each `HirLit::Str` in the COMPILER process's
//! string pool and splices the resulting handle (slot+shard) as a constant. That
//! is valid only in-process (`rts run`). An AOT binary is a DIFFERENT process with
//! a FRESH empty pool, so the baked slot points at nothing → reads back as
//! garbage/`null`. For AOT the bytes must live in the object's data section and be
//! interned at the binary's OWN runtime via `__RTS_FN_NS_GC_STRING_FROM_STATIC`.
//!
//! A thread-local flag gates the two paths: [`compile_program_aot`] turns it on
//! for the duration of an AOT build; the JIT path leaves it off (default) and
//! keeps the faster bake.

use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

use cranelift_codegen::ir::{InstBuilder, Value, types};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{DataDescription, Linkage, Module};

use crate::front::error::{FrontResult, Unsupported};

thread_local! {
    /// True while an AOT object is being built (set by `compile_program_aot`).
    static AOT_MODE: Cell<bool> = const { Cell::new(false) };
}

/// Monotonic counter for unique data-symbol names. Process-wide (not reset): a
/// fresh name per literal occurrence avoids duplicate-symbol errors without a
/// content-dedup map. Duplicate bytes across occurrences are cheap.
static DATA_CTR: AtomicU64 = AtomicU64::new(0);

/// Enter/leave AOT string mode. `compile_program_aot` sets it `true` before
/// lowering and `false` after, so a later JIT build on the same thread is
/// unaffected.
pub(crate) fn set_aot_mode(on: bool) {
    AOT_MODE.with(|c| c.set(on));
}

/// Whether string literals must be lowered as data objects (AOT) vs baked handles
/// (JIT).
pub(crate) fn aot_mode() -> bool {
    AOT_MODE.with(|c| c.get())
}

/// Run `f` with AOT string mode ON, restoring the previous value after
/// (nesting-safe). Used by the prelude baker, which needs data-section strings
/// exactly like an AOT build.
pub(crate) fn with_aot_mode<T>(f: impl FnOnce() -> T) -> T {
    // `Drop` guard so a PANIC in `f` still restores the flag — a leaked
    // `AOT_MODE=true` would make every later JIT string literal in this process
    // lower as a data object (broken bake handle) instead of the fast handle.
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            AOT_MODE.with(|c| c.set(self.0));
        }
    }
    let _g = Restore(AOT_MODE.with(|c| c.replace(true)));
    f()
}

/// Emit a read-only data object holding `s`'s UTF-8 bytes and return its
/// `(ptr, len)` as Cranelift `Value`s (`I64` each). The pointer is a relocatable
/// reference to the data symbol — resolved by the linker at AOT link time.
pub(crate) fn emit_str_data(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    s: &str,
) -> FrontResult<(Value, Value)> {
    let id = DATA_CTR.fetch_add(1, Ordering::Relaxed);
    let name = format!("__rtsstr_{id}");
    let data_id = module
        .declare_data(&name, Linkage::Local, false, false)
        .map_err(|e| Unsupported::new(format!("declare str data `{name}`: {e}")))?;
    let mut desc = DataDescription::new();
    desc.define(s.as_bytes().to_vec().into_boxed_slice());
    module
        .define_data(data_id, &desc)
        .map_err(|e| Unsupported::new(format!("define str data `{name}`: {e}")))?;
    let gv = module.declare_data_in_func(data_id, builder.func);
    let ptr = builder.ins().global_value(types::I64, gv);
    let len = builder.ins().iconst(types::I64, s.len() as i64);
    Ok((ptr, len))
}
