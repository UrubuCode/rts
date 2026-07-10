//! `node:path` — real filesystem-path manipulation with NODE names and NODE
//! semantics (platform separator: `/` on POSIX, `\` on Windows), backed by
//! `std::path`. Registration only; the extern "C" implementations live in
//! `symbols.rs` (`promises.rs` is doc-only — this namespace has no promise
//! sub-API).
//!
//! Native rts-node implementation (no rts-std mirror). `rts:path` already
//! covers an equivalent `std::path` surface under RTS names/semantics
//! (extension without the dot, snake_case names); this module restates the
//! same primitive under Node's `path.*` names/semantics so `node:path`
//! programs see exactly what Node shows (e.g. `extname` INCLUDES the leading
//! dot, unlike `rts:path.ext`).
//!
//! **Deferred** (need property/object/variadic machinery this flat
//! string/bool function slice doesn't have):
//! - `path.sep` / `path.delimiter` — properties, not functions.
//! - `path.parse(path)` / `path.format(pathObject)` — `parse` returns an
//!   object `{root, dir, base, ext, name}`; `format` takes one. No object
//!   marshalling in this slice.
//! - `path.toNamespacedPath(path)` — Windows-only UNC/long-path escaping,
//!   no POSIX equivalent; low value until UNC paths are exercised.
//! - `path.posix` / `path.win32` — explicit-platform sub-namespaces, each
//!   duplicating the whole surface under a fixed separator.
//! - True variadic `join(...paths)` / `resolve(...paths)` — the engine's
//!   flat-function ABI is fixed-arity; this slice covers the overwhelmingly
//!   common 2-argument call shape (chain calls for more segments, e.g.
//!   `join(join(a, b), c)`, which is associative and gives the identical
//!   result). `basenameExt` is likewise the fixed-arity split of Node's
//!   optional 2nd arg to `basename(path, ext)`.

mod promises;
mod symbols;

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

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

/// Registers the `node:path` surface into the engine Registry. Deliberately
/// NO `.alias("path")` — the bare `path` name is already the RTS-native
/// namespace (`rts:path`, RTS names/semantics); `node:path` resolves to this
/// module's canonical `node/path` key directly via scheme-aware routing, so
/// the two coexist without colliding.
pub fn register(e: &mut Engine) {
    e.ns("node:path")
        .doc(
            "Node-accurate path manipulation (node:path), platform separator. \
             sep/delimiter (properties), parse/format (objects), \
             toNamespacedPath, posix/win32 sub-namespaces, and true variadic \
             join/resolve are deferred — see the module doc comment.",
        )
        .member(pure_func(
            "join",
            "__RTS_FN_NODE_PATH_JOIN",
            sig!(StrPtr, StrPtr => Handle),
            "join(a: string, b: string): string",
            "Joins two path segments with the platform separator and normalizes the result.",
            symbols::__RTS_FN_NODE_PATH_JOIN as *const u8,
        ))
        .member(pure_func(
            "resolve",
            "__RTS_FN_NODE_PATH_RESOLVE",
            sig!(StrPtr, StrPtr => Handle),
            "resolve(a: string, b: string): string",
            "Resolves two path segments to an absolute path against process.cwd(), Node-style.",
            symbols::__RTS_FN_NODE_PATH_RESOLVE as *const u8,
        ))
        .member(pure_func(
            "normalize",
            "__RTS_FN_NODE_PATH_NORMALIZE",
            sig!(StrPtr => Handle),
            "normalize(path: string): string",
            "Collapses `.`/`..` segments and repeated separators; \"\" normalizes to \".\".",
            symbols::__RTS_FN_NODE_PATH_NORMALIZE as *const u8,
        ))
        .member(pure_func(
            "dirname",
            "__RTS_FN_NODE_PATH_DIRNAME",
            sig!(StrPtr => Handle),
            "dirname(path: string): string",
            "Directory portion of `path` (everything before the final component).",
            symbols::__RTS_FN_NODE_PATH_DIRNAME as *const u8,
        ))
        .member(pure_func(
            "basename",
            "__RTS_FN_NODE_PATH_BASENAME",
            sig!(StrPtr => Handle),
            "basename(path: string): string",
            "Final path component; \"\" for a bare root.",
            symbols::__RTS_FN_NODE_PATH_BASENAME as *const u8,
        ))
        .member(pure_func(
            "basenameExt",
            "__RTS_FN_NODE_PATH_BASENAME_EXT",
            sig!(StrPtr, StrPtr => Handle),
            "basenameExt(path: string, suffix: string): string",
            "basename(path) with a trailing `suffix` stripped — the fixed-arity form of Node's basename(path, ext).",
            symbols::__RTS_FN_NODE_PATH_BASENAME_EXT as *const u8,
        ))
        .member(pure_func(
            "extname",
            "__RTS_FN_NODE_PATH_EXTNAME",
            sig!(StrPtr => Handle),
            "extname(path: string): string",
            "Extension of `path` INCLUDING the leading dot; \"\" when absent.",
            symbols::__RTS_FN_NODE_PATH_EXTNAME as *const u8,
        ))
        .member(pure_func(
            "isAbsolute",
            "__RTS_FN_NODE_PATH_IS_ABSOLUTE",
            sig!(StrPtr => Bool),
            "isAbsolute(path: string): boolean",
            "True when `path` is absolute for the current target.",
            symbols::__RTS_FN_NODE_PATH_IS_ABSOLUTE as *const u8,
        ))
        .member(pure_func(
            "relative",
            "__RTS_FN_NODE_PATH_RELATIVE",
            sig!(StrPtr, StrPtr => Handle),
            "relative(from: string, to: string): string",
            "Relative path from `from` to `to`, resolved against process.cwd() like Node.",
            symbols::__RTS_FN_NODE_PATH_RELATIVE as *const u8,
        ))
        .done();
}
