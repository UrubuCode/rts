//! node:module — the `extern "C"` entry points for the fully-static subset:
//! `isBuiltin`, `builtinModules`, `wrap`, `syncBuiltinESMExports`.

use rts_engine::heap::handles::{alloc_entry, Entry};
use rts_engine::heap::shapes::string_word;

use super::builtins::{builtin_module_names, is_builtin, wrap};

use rts_engine::gc_surface::__RTS_FN_NS_GC_STRING_NEW;

fn read(ptr: *const u8, len: i64) -> String {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }
        .unwrap_or("")
        .to_string()
}

/// `module.isBuiltin(name)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_MODULE_IS_BUILTIN(p: *const u8, l: i64) -> i64 {
    is_builtin(&read(p, l)) as i64
}

/// `module.builtinModules` — `string[]`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_MODULE_BUILTIN_MODULES() -> u64 {
    let words: Vec<i64> = builtin_module_names().iter().map(|s| string_word(s.as_bytes()) as i64).collect();
    alloc_entry(Entry::Vec(Box::new(words)))
}

/// `Module.wrap(script)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_MODULE_WRAP(p: *const u8, l: i64) -> u64 {
    let wrapped = wrap(&read(p, l));
    unsafe { __RTS_FN_NS_GC_STRING_NEW(wrapped.as_ptr(), wrapped.len() as i64) }
}

/// `module.syncBuiltinESMExports()` — RTS has no live ESM/CJS export bridge to
/// re-sync, so this is a correct no-op (its Node contract is "re-flush the
/// named exports of builtins", a pure internal-cache operation).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_MODULE_SYNC_BUILTIN_ESM_EXPORTS() {}
