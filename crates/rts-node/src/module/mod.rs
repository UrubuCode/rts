//! `node:module` — builtin-module introspection surface.
//!
//! Native rts-node implementation (no rts-std mirror). This slice covers the
//! pure, synchronous, data-only surface:
//!
//! - `isBuiltin(name: string): boolean` — true when `name` (optionally prefixed
//!   with `node:`) names a known Node core module.
//! - `builtinModules(): string[]` — the list of core module names loadable
//!   *without* the mandatory `node:` scheme (mirrors Node's
//!   `module.builtinModules`, which excludes prefix-only ids like `node:test`).
//!
//! **Deferred** (need object/class machinery this pure slice doesn't have):
//! `createRequire` (returns a `require` function bound to a resolver — no CJS
//! resolver here), `SourceMap` (class wrapping a source-map object),
//! `register`/`registerHooks` (loader hook registration — async, callback-
//! based), and the `Module` class itself (`Module._cache`, `Module.wrap`,
//! `new Module(...)` instances). All need object/function values this
//! string/bool/array-only slice does not model.
//!
//! ABI mirrors the pure-namespace shape used across RTS: `Str` args arrive as
//! `(ptr, len)` and are rebuilt via `from_abi` (`None` on null / invalid
//! UTF-8); string[] results are `Entry::Vec` of string WORDs (see
//! `rts-std/src/fs/mod.rs::__RTS_FN_NS_FS_READDIR` for the reference pattern).
//! Symbols follow the rts-node convention `__RTS_FN_NODE_MODULE_*`.

use rts_engine::abi::str_abi::from_abi;
use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

/// Core module ids loadable *without* the mandatory `node:` scheme — this is
/// exactly the set Node's `module.builtinModules` exposes. Kept alphabetical
/// (matches Node's own sorted output) for a stable, diffable list.
const BUILTIN_MODULES: &[&str] = &[
    "_http_agent",
    "_http_client",
    "_http_common",
    "_http_incoming",
    "_http_outgoing",
    "_http_server",
    "_stream_duplex",
    "_stream_passthrough",
    "_stream_readable",
    "_stream_transform",
    "_stream_wrap",
    "_stream_writable",
    "_tls_common",
    "_tls_wrap",
    "assert",
    "assert/strict",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "diagnostics_channel",
    "dns",
    "dns/promises",
    "domain",
    "events",
    "fs",
    "fs/promises",
    "http",
    "http2",
    "https",
    "inspector",
    "inspector/promises",
    "module",
    "net",
    "os",
    "path",
    "path/posix",
    "path/win32",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "readline/promises",
    "repl",
    "stream",
    "stream/consumers",
    "stream/promises",
    "stream/web",
    "string_decoder",
    "sys",
    "timers",
    "timers/promises",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "util/types",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
];

/// Core module ids that Node only accepts with the mandatory `node:` scheme
/// (i.e. `isBuiltin('node:test')` is true but `isBuiltin('test')` is false, and
/// these are excluded from `builtinModules`).
const PREFIX_ONLY_MODULES: &[&str] = &["sea", "sqlite", "test", "test/reporters"];

/// `module.isBuiltin(name)` semantics: strip an optional leading `node:`
/// scheme, then check the appropriate set — the full (prefixable +
/// prefix-only) set when a `node:` scheme was given, or just the prefixable
/// set when it was not (an id like `test` with no scheme is not a valid
/// specifier for a mandatory-prefix module).
fn is_builtin(name: &str) -> bool {
    match name.strip_prefix("node:") {
        Some(rest) => BUILTIN_MODULES.contains(&rest) || PREFIX_ONLY_MODULES.contains(&rest),
        None => BUILTIN_MODULES.contains(&name),
    }
}

/// `module.isBuiltin(name)` — true when `name` (bare or `node:`-prefixed)
/// names a known Node core module.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_MODULE_IS_BUILTIN(ptr: *const u8, len: i64) -> i64 {
    let Some(name) = (unsafe { from_abi(ptr, len) }) else {
        return 0;
    };
    if is_builtin(name) {
        1
    } else {
        0
    }
}

fn pure_func(
    name: &str,
    symbol: &str,
    sig: rts_engine::Sig,
    ts: &str,
    doc: &str,
    fp: *const u8,
) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: true,
        intrinsic: None,
    }
}

/// Registers the `node:module` surface into the engine Registry.
pub fn register(e: &mut Engine) {
    e.module("node", "module")
        .alias("module")
        .doc(
            "Builtin-module introspection (node:module): isBuiltin. \
             builtinModules (a property array), createRequire/SourceMap/register/ \
             registerHooks/the Module class are deferred (need property/object \
             values, not flat functions).",
        )
        .member(pure_func(
            "isBuiltin",
            "__RTS_FN_NODE_MODULE_IS_BUILTIN",
            sig!(StrPtr => Bool),
            "isBuiltin(name: string): boolean",
            "True when `name` (bare or node:-prefixed) names a known Node core module.",
            __RTS_FN_NODE_MODULE_IS_BUILTIN as *const u8,
        ))
        .done();
}
