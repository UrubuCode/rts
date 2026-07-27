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
//! Layout: `builtins` (canonical name set + pure ops), `symbols` (extern
//! points), `mod` (registration).

mod builtins;
mod symbols;

use rts_engine::AbiType::{self, Bool, Handle, StrPtr, Void};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

fn f(name: &str, symbol: &str, args: Vec<AbiType>, ret: AbiType, ts: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig: Sig::new(args, ret),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Registers the `node:module` surface.
pub fn register(e: &mut Engine) {
    use symbols as s;
    e.ns("node:module")
        .doc("Module system introspection (node:module): isBuiltin, builtinModules, wrap, syncBuiltinESMExports.")
        .member(f("isBuiltin", "__RTS_FN_NODE_MODULE_IS_BUILTIN", vec![StrPtr], Bool, "isBuiltin(name: string): boolean", s::__RTS_FN_NODE_MODULE_IS_BUILTIN as *const u8))
        .member(f("builtinModules", "__RTS_FN_NODE_MODULE_BUILTIN_MODULES", vec![], Handle, "builtinModules(): string[]", s::__RTS_FN_NODE_MODULE_BUILTIN_MODULES as *const u8))
        .member(f("wrap", "__RTS_FN_NODE_MODULE_WRAP", vec![StrPtr], Handle, "wrap(script: string): string", s::__RTS_FN_NODE_MODULE_WRAP as *const u8))
        .member(f("syncBuiltinESMExports", "__RTS_FN_NODE_MODULE_SYNC_BUILTIN_ESM_EXPORTS", vec![], Void, "syncBuiltinESMExports(): void", s::__RTS_FN_NODE_MODULE_SYNC_BUILTIN_ESM_EXPORTS as *const u8))
        .done();
}
