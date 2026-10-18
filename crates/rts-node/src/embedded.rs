//! `node:` modules AUTHORED IN TYPESCRIPT and compiled with the program.
//!
//! A name on this table is not built by a Rust `namespace` function: its source
//! is embedded here, the loader (`rts-host`'s graph walk) puts it in the graph
//! under the canonical specifier `node:<name>`, and the one `compile_all` that
//! compiles the program compiles it. The native module of the same name, if one
//! is still registered, is a fallback for a program that never names the module
//! statically — the compiled body replaces it the moment it is registered
//! (`rts_core::entry::declare_module_entry`).

/// `(name, source)` — `name` without the `node:` prefix.
const MODULES: &[(&str, &str)] = &[
    ("querystring", include_str!("../std/node/querystring.ts")),
    ("string_decoder", include_str!("../std/node/string_decoder.ts")),
    ("path", include_str!("../std/node/path.ts")),
    ("path/posix", include_str!("../std/node/path_posix.ts")),
    ("path/win32", include_str!("../std/node/path_win32.ts")),
];

/// The canonical specifier and embedded source for a written specifier
/// (`node:querystring` and `querystring` are the same module), if it is one.
pub fn lookup(specifier: &str) -> Option<(String, &'static str)> {
    let bare = specifier.strip_prefix("node:").unwrap_or(specifier);
    MODULES
        .iter()
        .find(|(name, _)| *name == bare)
        .map(|(name, source)| (format!("node:{name}"), *source))
}

/// The embedded source for an already-canonical `node:<name>` specifier.
pub fn source_of(canonical: &str) -> Option<&'static str> {
    let bare = canonical.strip_prefix("node:")?;
    MODULES.iter().find(|(name, _)| *name == bare).map(|(_, source)| *source)
}
