//! `node:path` — full Node-25 surface, native (no `.ts` shim). A faithful port
//! of Node's own `lib/path.js` lexical algorithm for BOTH flavors (POSIX +
//! Win32), exposed as three namespaces: `node:path` (the default, aliased to
//! the compile-target flavor), `node:path/posix`, and `node:path/win32`.
//!
//! `resolve`'s only I/O is the process CWD (`std::env::current_dir`, plus the
//! Win32 per-drive `=X:` env var). `parse` returns a real object; `format`
//! reads a real object argument — both through the engine value model, no JSON.
//! Module layout: `flavor` (core `normalizeString`), `classify`
//! (basename/dirname/extname/isAbsolute), `posix`/`win32` (root-aware fns),
//! `win32root` (drive/UNC root scan), `parse`/`relative`/`glob`, `words`
//! (value build/read), `symbols` (extern entry points).
//!
//! `path.posix`/`path.win32` accessed as PROPERTIES of the default export are
//! deferred (namespace-object-valued members need a shim layer rts-node does
//! not ship yet); the `node:path/posix` and `node:path/win32` sub-specifiers
//! cover the same access and ARE wired.

mod classify;
mod flavor;
mod glob;
mod parse;
mod posix;
mod relative;
mod symbols;
mod win32;
mod win32root;
pub(crate) mod words;

use rts_engine::{sig, AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

#[derive(Clone, Copy)]
enum Which {
    Posix,
    Win32,
}

fn func(name: &str, symbol: &str, sig: Sig, ts: &str, fp: *const u8) -> Member {
    member(name, symbol, sig, ts, fp, MemberKind::Function)
}

fn constant(name: &str, symbol: &str, ts: &str, fp: *const u8) -> Member {
    member(name, symbol, sig!(=> Handle), ts, fp, MemberKind::Constant)
}

fn member(name: &str, symbol: &str, sig: Sig, ts: &str, fp: *const u8, kind: MemberKind) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        // resolve reads the CWD; nothing is foldable/cacheable → never pure.
        pure: false,
        intrinsic: None,
    }
}

/// Every `(symbol, fn_ptr)` for one flavor, keyed by role.
struct Syms {
    basename: (&'static str, *const u8),
    basename2: (&'static str, *const u8),
    dirname: (&'static str, *const u8),
    extname: (&'static str, *const u8),
    is_absolute: (&'static str, *const u8),
    normalize: (&'static str, *const u8),
    relative: (&'static str, *const u8),
    parse: (&'static str, *const u8),
    format: (&'static str, *const u8),
    to_namespaced: (&'static str, *const u8),
    matches_glob: (&'static str, *const u8),
    sep: (&'static str, *const u8),
    delimiter: (&'static str, *const u8),
    join: [(&'static str, *const u8); 9],
    resolve: [(&'static str, *const u8); 9],
}

fn syms(which: Which) -> Syms {
    use symbols::{posix as ps, win32 as ws};
    match which {
        Which::Posix => Syms {
            basename: ("__RTS_FN_NODE_PATH_POSIX_BASENAME", ps::__RTS_FN_NODE_PATH_POSIX_BASENAME as *const u8),
            basename2: ("__RTS_FN_NODE_PATH_POSIX_BASENAME2", ps::__RTS_FN_NODE_PATH_POSIX_BASENAME2 as *const u8),
            dirname: ("__RTS_FN_NODE_PATH_POSIX_DIRNAME", ps::__RTS_FN_NODE_PATH_POSIX_DIRNAME as *const u8),
            extname: ("__RTS_FN_NODE_PATH_POSIX_EXTNAME", ps::__RTS_FN_NODE_PATH_POSIX_EXTNAME as *const u8),
            is_absolute: ("__RTS_FN_NODE_PATH_POSIX_ISABSOLUTE", ps::__RTS_FN_NODE_PATH_POSIX_ISABSOLUTE as *const u8),
            normalize: ("__RTS_FN_NODE_PATH_POSIX_NORMALIZE", ps::__RTS_FN_NODE_PATH_POSIX_NORMALIZE as *const u8),
            relative: ("__RTS_FN_NODE_PATH_POSIX_RELATIVE", ps::__RTS_FN_NODE_PATH_POSIX_RELATIVE as *const u8),
            parse: ("__RTS_FN_NODE_PATH_POSIX_PARSE", ps::__RTS_FN_NODE_PATH_POSIX_PARSE as *const u8),
            format: ("__RTS_FN_NODE_PATH_POSIX_FORMAT", ps::__RTS_FN_NODE_PATH_POSIX_FORMAT as *const u8),
            to_namespaced: ("__RTS_FN_NODE_PATH_POSIX_TONAMESPACED", ps::__RTS_FN_NODE_PATH_POSIX_TONAMESPACED as *const u8),
            matches_glob: ("__RTS_FN_NODE_PATH_POSIX_MATCHESGLOB", ps::__RTS_FN_NODE_PATH_POSIX_MATCHESGLOB as *const u8),
            sep: ("__RTS_FN_NODE_PATH_POSIX_SEP", symbols::__RTS_FN_NODE_PATH_POSIX_SEP as *const u8),
            delimiter: ("__RTS_FN_NODE_PATH_POSIX_DELIMITER", symbols::__RTS_FN_NODE_PATH_POSIX_DELIMITER as *const u8),
            join: posix_join_syms(),
            resolve: posix_resolve_syms(),
        },
        Which::Win32 => Syms {
            basename: ("__RTS_FN_NODE_PATH_WIN32_BASENAME", ws::__RTS_FN_NODE_PATH_WIN32_BASENAME as *const u8),
            basename2: ("__RTS_FN_NODE_PATH_WIN32_BASENAME2", ws::__RTS_FN_NODE_PATH_WIN32_BASENAME2 as *const u8),
            dirname: ("__RTS_FN_NODE_PATH_WIN32_DIRNAME", ws::__RTS_FN_NODE_PATH_WIN32_DIRNAME as *const u8),
            extname: ("__RTS_FN_NODE_PATH_WIN32_EXTNAME", ws::__RTS_FN_NODE_PATH_WIN32_EXTNAME as *const u8),
            is_absolute: ("__RTS_FN_NODE_PATH_WIN32_ISABSOLUTE", ws::__RTS_FN_NODE_PATH_WIN32_ISABSOLUTE as *const u8),
            normalize: ("__RTS_FN_NODE_PATH_WIN32_NORMALIZE", ws::__RTS_FN_NODE_PATH_WIN32_NORMALIZE as *const u8),
            relative: ("__RTS_FN_NODE_PATH_WIN32_RELATIVE", ws::__RTS_FN_NODE_PATH_WIN32_RELATIVE as *const u8),
            parse: ("__RTS_FN_NODE_PATH_WIN32_PARSE", ws::__RTS_FN_NODE_PATH_WIN32_PARSE as *const u8),
            format: ("__RTS_FN_NODE_PATH_WIN32_FORMAT", ws::__RTS_FN_NODE_PATH_WIN32_FORMAT as *const u8),
            to_namespaced: ("__RTS_FN_NODE_PATH_WIN32_TONAMESPACED", ws::__RTS_FN_NODE_PATH_WIN32_TONAMESPACED as *const u8),
            matches_glob: ("__RTS_FN_NODE_PATH_WIN32_MATCHESGLOB", ws::__RTS_FN_NODE_PATH_WIN32_MATCHESGLOB as *const u8),
            sep: ("__RTS_FN_NODE_PATH_WIN32_SEP", symbols::__RTS_FN_NODE_PATH_WIN32_SEP as *const u8),
            delimiter: ("__RTS_FN_NODE_PATH_WIN32_DELIMITER", symbols::__RTS_FN_NODE_PATH_WIN32_DELIMITER as *const u8),
            join: win32_join_syms(),
            resolve: win32_resolve_syms(),
        },
    }
}

fn str_ret(n: usize) -> Sig {
    Sig::new(vec![AbiType::StrPtr; n], AbiType::Handle)
}

macro_rules! sym_array {
    ($( $sym:ident ),*) => {
        [ $( (stringify!($sym), symbols::$sym as *const u8) ),* ]
    };
}

fn posix_join_syms() -> [(&'static str, *const u8); 9] {
    sym_array!(
        __RTS_FN_NODE_PATH_POSIX_JOIN0, __RTS_FN_NODE_PATH_POSIX_JOIN1, __RTS_FN_NODE_PATH_POSIX_JOIN2,
        __RTS_FN_NODE_PATH_POSIX_JOIN3, __RTS_FN_NODE_PATH_POSIX_JOIN4, __RTS_FN_NODE_PATH_POSIX_JOIN5,
        __RTS_FN_NODE_PATH_POSIX_JOIN6, __RTS_FN_NODE_PATH_POSIX_JOIN7, __RTS_FN_NODE_PATH_POSIX_JOIN8
    )
}
fn posix_resolve_syms() -> [(&'static str, *const u8); 9] {
    sym_array!(
        __RTS_FN_NODE_PATH_POSIX_RESOLVE0, __RTS_FN_NODE_PATH_POSIX_RESOLVE1, __RTS_FN_NODE_PATH_POSIX_RESOLVE2,
        __RTS_FN_NODE_PATH_POSIX_RESOLVE3, __RTS_FN_NODE_PATH_POSIX_RESOLVE4, __RTS_FN_NODE_PATH_POSIX_RESOLVE5,
        __RTS_FN_NODE_PATH_POSIX_RESOLVE6, __RTS_FN_NODE_PATH_POSIX_RESOLVE7, __RTS_FN_NODE_PATH_POSIX_RESOLVE8
    )
}
fn win32_join_syms() -> [(&'static str, *const u8); 9] {
    sym_array!(
        __RTS_FN_NODE_PATH_WIN32_JOIN0, __RTS_FN_NODE_PATH_WIN32_JOIN1, __RTS_FN_NODE_PATH_WIN32_JOIN2,
        __RTS_FN_NODE_PATH_WIN32_JOIN3, __RTS_FN_NODE_PATH_WIN32_JOIN4, __RTS_FN_NODE_PATH_WIN32_JOIN5,
        __RTS_FN_NODE_PATH_WIN32_JOIN6, __RTS_FN_NODE_PATH_WIN32_JOIN7, __RTS_FN_NODE_PATH_WIN32_JOIN8
    )
}
fn win32_resolve_syms() -> [(&'static str, *const u8); 9] {
    sym_array!(
        __RTS_FN_NODE_PATH_WIN32_RESOLVE0, __RTS_FN_NODE_PATH_WIN32_RESOLVE1, __RTS_FN_NODE_PATH_WIN32_RESOLVE2,
        __RTS_FN_NODE_PATH_WIN32_RESOLVE3, __RTS_FN_NODE_PATH_WIN32_RESOLVE4, __RTS_FN_NODE_PATH_WIN32_RESOLVE5,
        __RTS_FN_NODE_PATH_WIN32_RESOLVE6, __RTS_FN_NODE_PATH_WIN32_RESOLVE7, __RTS_FN_NODE_PATH_WIN32_RESOLVE8
    )
}

fn add_members(mut ns: rts_engine::ModuleBuilder<'_>, which: Which) -> rts_engine::ModuleBuilder<'_> {
    let s = syms(which);
    ns = ns
        .member(func("basename", s.basename.0, sig!(StrPtr => Handle), "basename(path: string): string", s.basename.1))
        .member(func("basename", s.basename2.0, sig!(StrPtr, StrPtr => Handle), "basename(path: string, suffix: string): string", s.basename2.1))
        .member(func("dirname", s.dirname.0, sig!(StrPtr => Handle), "dirname(path: string): string", s.dirname.1))
        .member(func("extname", s.extname.0, sig!(StrPtr => Handle), "extname(path: string): string", s.extname.1))
        .member(func("isAbsolute", s.is_absolute.0, sig!(StrPtr => Bool), "isAbsolute(path: string): boolean", s.is_absolute.1))
        .member(func("normalize", s.normalize.0, sig!(StrPtr => Handle), "normalize(path: string): string", s.normalize.1))
        .member(func("relative", s.relative.0, sig!(StrPtr, StrPtr => Handle), "relative(from: string, to: string): string", s.relative.1))
        .member(func("parse", s.parse.0, sig!(StrPtr => Handle), "parse(path: string): object", s.parse.1))
        .member(func("format", s.format.0, sig!(Handle => Handle), "format(pathObject: object): string", s.format.1))
        .member(func("toNamespacedPath", s.to_namespaced.0, sig!(StrPtr => Handle), "toNamespacedPath(path: string): string", s.to_namespaced.1))
        .member(func("matchesGlob", s.matches_glob.0, sig!(StrPtr, StrPtr => Bool), "matchesGlob(path: string, pattern: string): boolean", s.matches_glob.1))
        .member(constant("sep", s.sep.0, "sep: string", s.sep.1))
        .member(constant("delimiter", s.delimiter.0, "delimiter: string", s.delimiter.1));
    for (n, (symbol, fp)) in s.join.iter().enumerate() {
        ns = ns.member(func("join", symbol, str_ret(n), "join(...paths: string[]): string", *fp));
    }
    for (n, (symbol, fp)) in s.resolve.iter().enumerate() {
        ns = ns.member(func("resolve", symbol, str_ret(n), "resolve(...paths: string[]): string", *fp));
    }
    ns
}

/// Registers `node:path`, `node:path/posix`, and `node:path/win32`.
pub fn register(e: &mut Engine) {
    let default = if cfg!(windows) { Which::Win32 } else { Which::Posix };
    add_members(
        e.ns("node:path").doc("Path utilities (node:path), OS-flavor default; posix/win32 via node:path/posix, node:path/win32."),
        default,
    )
    .done();
    add_members(e.ns("node:path/posix").doc("POSIX path utilities."), Which::Posix).done();
    add_members(e.ns("node:path/win32").doc("Win32 path utilities."), Which::Win32).done();
}
