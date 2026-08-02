//! node:module — the `#[rtse::function]` entry points for the fully-static
//! subset: `isBuiltin`, `builtinModules`, `wrap`, `syncBuiltinESMExports`.

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::{alloc_entry, Entry};
use rts_engine::heap::shapes::string_word;

use super::builtins::{builtin_module_names, is_builtin, wrap as wrap_script};

/// `module.isBuiltin(name)`.
#[rtse::function(module = "node:module", value = "isBuiltin")]
fn is_builtin_fn(name: &str) -> bool {
    is_builtin(name)
}

/// `module.builtinModules` — `string[]`.
#[rtse::function(module = "node:module", value = "builtinModules")]
fn builtin_modules() -> Handle {
    let words: Vec<i64> = builtin_module_names().iter().map(|s| string_word(s.as_bytes()) as i64).collect();
    alloc_entry(Entry::vec(words))
}

/// `Module.wrap(script)`.
#[rtse::function(module = "node:module", value = "wrap")]
fn wrap(script: &str) -> String {
    wrap_script(script)
}

/// `module.syncBuiltinESMExports()` — RTS has no live ESM/CJS export bridge to
/// re-sync, so this is a correct no-op (its Node contract is "re-flush the
/// named exports of builtins", a pure internal-cache operation).
#[rtse::function(module = "node:module", value = "syncBuiltinESMExports")]
fn sync_builtin_esm_exports() {}
