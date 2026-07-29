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
//!
//! # Authoring: `#[rtse::function]` for the fixed-arity members
//!
//! `basename`/`dirname`/`extname`/`isAbsolute`/`normalize`/`relative`/`parse`/
//! `format`/`toNamespacedPath`/`matchesGlob` are declared in
//! `symbols::posix`/`symbols::win32` via `#[rtse::function]` — symbol/ABI/TS
//! signature derived from the Rust signature, registered here through the
//! generated `..._entry()` fns.
//!
//! `sep`/`delimiter` (a `MemberKind::Constant`, which the macro has no form
//! for) and `join`/`resolve` (Node-variadic — exposed as fixed 0..8-arity
//! overloads sharing one JS name, which the macro's optional-parameter model
//! does not express: it pads a single `Member`'s trailing args with declared
//! defaults, not a family of same-name/different-arity `Member`s) stay
//! hand-built `Member`s in `add_variadic` below. See the module-level
//! conversion report for the exact blocker.

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

use rts_engine::{sig, AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, ModuleScope, Sig};

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
        ret_class: None,
        pure: false,
        emit: None,
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

/// `sep`/`delimiter` (Constant) + the `join`/`resolve` fixed-arity overload
/// families (Node-variadic) — the members `#[rtse::function]` cannot express.
fn add_variadic(m: &mut ModuleScope<'_>, which: Which) {
    let (sep, delimiter, join, resolve) = match which {
        Which::Posix => (
            ("__RTS_FN_NODE_PATH_POSIX_SEP", symbols::__RTS_FN_NODE_PATH_POSIX_SEP as *const u8),
            ("__RTS_FN_NODE_PATH_POSIX_DELIMITER", symbols::__RTS_FN_NODE_PATH_POSIX_DELIMITER as *const u8),
            posix_join_syms(),
            posix_resolve_syms(),
        ),
        Which::Win32 => (
            ("__RTS_FN_NODE_PATH_WIN32_SEP", symbols::__RTS_FN_NODE_PATH_WIN32_SEP as *const u8),
            ("__RTS_FN_NODE_PATH_WIN32_DELIMITER", symbols::__RTS_FN_NODE_PATH_WIN32_DELIMITER as *const u8),
            win32_join_syms(),
            win32_resolve_syms(),
        ),
    };
    m.member(constant("sep", sep.0, "sep: string", sep.1));
    m.member(constant("delimiter", delimiter.0, "delimiter: string", delimiter.1));
    for (n, (symbol, fp)) in join.iter().enumerate() {
        m.member(func("join", symbol, str_ret(n), "join(...paths: string[]): string", *fp));
    }
    for (n, (symbol, fp)) in resolve.iter().enumerate() {
        m.member(func("resolve", symbol, str_ret(n), "resolve(...paths: string[]): string", *fp));
    }
}

/// The `#[rtse::function]`-declared fixed-arity members, one flavor.
fn add_converted(m: &mut ModuleScope<'_>, which: Which) {
    match which {
        Which::Posix => {
            m.registry(symbols::posix::basename_entry());
            m.registry(symbols::posix::dirname_entry());
            m.registry(symbols::posix::extname_entry());
            m.registry(symbols::posix::is_absolute_entry());
            m.registry(symbols::posix::normalize_entry());
            m.registry(symbols::posix::relative_entry());
            m.registry(symbols::posix::parse_entry());
            m.registry(symbols::posix::format_entry());
            m.registry(symbols::posix::to_namespaced_path_entry());
            m.registry(symbols::posix::matches_glob_entry());
        }
        Which::Win32 => {
            m.registry(symbols::win32::basename_entry());
            m.registry(symbols::win32::dirname_entry());
            m.registry(symbols::win32::extname_entry());
            m.registry(symbols::win32::is_absolute_entry());
            m.registry(symbols::win32::normalize_entry());
            m.registry(symbols::win32::relative_entry());
            m.registry(symbols::win32::parse_entry());
            m.registry(symbols::win32::format_entry());
            m.registry(symbols::win32::to_namespaced_path_entry());
            m.registry(symbols::win32::matches_glob_entry());
        }
    }
}

/// Registers `node:path`, `node:path/posix`, and `node:path/win32`.
pub fn register(e: &mut Engine) {
    e.module("node:path/posix", |m| {
        m.doc("POSIX path utilities.");
        add_converted(m, Which::Posix);
        add_variadic(m, Which::Posix);
    });
    e.module("node:path/win32", |m| {
        m.doc("Win32 path utilities.");
        add_converted(m, Which::Win32);
        add_variadic(m, Which::Win32);
    });
    e.module("node:path", |m| {
        m.doc(
            "Path utilities (node:path), OS-flavor default; posix/win32 via \
             node:path/posix, node:path/win32.",
        );
        let default = if cfg!(windows) { Which::Win32 } else { Which::Posix };
        add_converted(m, default);
        add_variadic(m, default);
    });
}
