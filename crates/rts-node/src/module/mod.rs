//! `node:module` — introspection of the core-module set plus the CJS wrapper.
//! The fully-static, no-dynamic-loader subset (module.md §532 "phase b"):
//! `isBuiltin`/`builtinModules` (against the full canonical Node-25 name list,
//! independent of what RTS has actually implemented), `Module.wrap` (pure string
//! op), `syncBuiltinESMExports` (no-op).
//!
//! Deferred (tightly coupled to RTS's own module loader / a source-map / TS
//! subsystem — module.md §1): the `Module` class instances + `require`/
//! `require.cache`/`require.resolve`, `createRequire`, `registerHooks`/`register`
//! (customization hooks), `SourceMap`/`findSourceMap`, the compile-cache API,
//! `stripTypeScriptTypes`, `findPackageJSON`.
//!
//! Layout: `builtins` (canonical name set + pure ops), `symbols`
//! (`#[rtse::function]` entry points), `mod` (registration).

mod builtins;
mod symbols;

use rts_engine::Engine;

/// Registers the `node:module` surface.
pub fn register(e: &mut Engine) {
    use symbols as s;
    e.ns("node:module")
        .doc("Module system introspection (node:module): isBuiltin, builtinModules, wrap, syncBuiltinESMExports.")
        .member(s::is_builtin_fn_entry())
        .member(s::builtin_modules_entry())
        .member(s::wrap_entry())
        .member(s::sync_builtin_esm_exports_entry())
        .done();
}
