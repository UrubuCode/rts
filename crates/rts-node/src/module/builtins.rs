//! The canonical Node-25 builtin-module name set, kept INDEPENDENT of what
//! rts-node actually implements (per module.md §441): `isBuiltin('worker_threads')`
//! returns `true` like real Node even before RTS ships that module — a later
//! `import` then fails with a clear "not implemented yet" diagnostic instead of a
//! generic "unknown module".

/// Legacy names — importable with OR without the `node:` prefix (`fs`, `node:fs`).
pub const UNPREFIXED: &[&str] = &[
    "assert", "assert/strict", "async_hooks", "buffer", "child_process", "cluster",
    "console", "constants", "crypto", "dgram", "diagnostics_channel", "dns",
    "dns/promises", "domain", "events", "fs", "fs/promises", "http", "http2",
    "https", "inspector", "inspector/promises", "module", "net", "os", "path",
    "path/posix", "path/win32", "perf_hooks", "process", "punycode", "querystring",
    "readline", "readline/promises", "repl", "stream", "stream/consumers",
    "stream/promises", "stream/web", "string_decoder", "sys", "timers",
    "timers/promises", "tls", "trace_events", "tty", "url", "util", "util/types",
    "v8", "vm", "wasi", "worker_threads", "zlib",
];

/// Prefix-mandatory names — only importable as `node:<name>` (since v23.5.0 these
/// appear in `builtinModules` too, prefixed).
pub const PREFIX_ONLY: &[&str] = &["sea", "sqlite", "test", "test/reporters"];

/// `module.isBuiltin(name)` — accepts an optional `node:` prefix; prefix-only
/// names only count WITH the prefix.
pub fn is_builtin(name: &str) -> bool {
    match name.strip_prefix("node:") {
        Some(rest) => UNPREFIXED.contains(&rest) || PREFIX_ONLY.contains(&rest),
        None => UNPREFIXED.contains(&name),
    }
}

/// The `module.builtinModules` entries: the legacy names bare + the prefix-only
/// names carrying their mandatory `node:` prefix (matching real Node's array).
pub fn builtin_module_names() -> Vec<String> {
    UNPREFIXED
        .iter()
        .map(|s| s.to_string())
        .chain(PREFIX_ONLY.iter().map(|s| format!("node:{s}")))
        .collect()
}

/// `Module.wrap(script)` — the exact CommonJS module wrapper Node applies before
/// executing a `.js`/`.cjs` file (keeps top-level declarations file-scoped and
/// supplies the five wrapper parameters).
pub fn wrap(script: &str) -> String {
    format!("(function (exports, require, module, __filename, __dirname) {{ {script}\n}});")
}
