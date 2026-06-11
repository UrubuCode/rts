//! `env` namespace — environment variables, process argv, and cwd.
//!
//! Migrado pro modelo builder do `rts-engine` (Fase 2; ver `namespaces/hint`).
//! Args `Str` chegam como (ptr, len) e são reconstruídos via `from_abi`
//! (retorna o default em null/UTF-8 inválido).

use std::env;
use std::path::Path;

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

use rts_engine::abi::str_abi::from_abi;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Returns a string handle with the environment variable's value, or 0 when absent.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENV_GET_VAR(name_ptr: *const u8, name_len: i64) -> u64 {
    let Some(name) = (unsafe { from_abi(name_ptr, name_len) }) else {
        return 0;
    };
    match env::var(name) {
        Ok(value) => intern(&value),
        Err(_) => 0,
    }
}

/// Sets an environment variable.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENV_SET_VAR(
    name_ptr: *const u8,
    name_len: i64,
    value_ptr: *const u8,
    value_len: i64,
) {
    let Some(name) = (unsafe { from_abi(name_ptr, name_len) }) else {
        return;
    };
    let Some(value) = (unsafe { from_abi(value_ptr, value_len) }) else {
        return;
    };
    // SAFETY: std::env::set_var e unsafe no Rust 2024 (estado global); o RTS run
    // path e single-threaded por construcao.
    unsafe { env::set_var(name, value) };
}

/// Removes an environment variable.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENV_REMOVE_VAR(name_ptr: *const u8, name_len: i64) {
    let Some(name) = (unsafe { from_abi(name_ptr, name_len) }) else {
        return;
    };
    // SAFETY: mesma justificativa de set_var.
    unsafe { env::remove_var(name) };
}

/// Number of command-line arguments (including argv[0]).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENV_ARGS_COUNT() -> i64 {
    env::args().count() as i64
}

/// Returns the argv entry at `index` as a string handle; 0 when out of range.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENV_ARG_AT(index: i64) -> u64 {
    if index < 0 {
        return 0;
    }
    let Some(arg) = env::args().nth(index as usize) else {
        return 0;
    };
    intern(&arg)
}

/// Returns the current working directory as a string handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENV_CWD() -> u64 {
    match env::current_dir() {
        Ok(path) => intern(&path.to_string_lossy()),
        Err(_) => 0,
    }
}

/// Changes the current working directory. Returns 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_ENV_SET_CWD(path_ptr: *const u8, path_len: i64) -> i64 {
    let Some(path) = (unsafe { from_abi(path_ptr, path_len) }) else {
        return -1;
    };
    match env::set_current_dir(Path::new(path)) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

fn func(name: &str, symbol: &str, sig: rts_engine::Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
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
        pure: false,
        intrinsic: None,
    }
}

/// Registra a namespace `env` no motor (Fase 2).
pub fn register(e: &mut Engine) {
    e.ns("env")
        .doc("Environment variables, process argv, and current working directory.")
        .member(func(
            "get_var",
            "__RTS_FN_NS_ENV_GET_VAR",
            sig!(StrPtr => Handle),
            "get_var(name: string): string",
            "Returns a string handle with the environment variable's value, or 0 when absent.",
            __RTS_FN_NS_ENV_GET_VAR as *const u8,
        ))
        .member(func(
            "set_var",
            "__RTS_FN_NS_ENV_SET_VAR",
            sig!(StrPtr, StrPtr => Void),
            "set_var(name: string, value: string): void",
            "Sets an environment variable.",
            __RTS_FN_NS_ENV_SET_VAR as *const u8,
        ))
        .member(func(
            "remove_var",
            "__RTS_FN_NS_ENV_REMOVE_VAR",
            sig!(StrPtr => Void),
            "remove_var(name: string): void",
            "Removes an environment variable.",
            __RTS_FN_NS_ENV_REMOVE_VAR as *const u8,
        ))
        .member(func(
            "args_count",
            "__RTS_FN_NS_ENV_ARGS_COUNT",
            sig!(=> I64),
            "args_count(): number",
            "Number of command-line arguments (including argv[0]).",
            __RTS_FN_NS_ENV_ARGS_COUNT as *const u8,
        ))
        .member(func(
            "arg_at",
            "__RTS_FN_NS_ENV_ARG_AT",
            sig!(I64 => Handle),
            "arg_at(index: number): string",
            "Returns the argv entry at `index` as a string handle; 0 when out of range.",
            __RTS_FN_NS_ENV_ARG_AT as *const u8,
        ))
        .member(func(
            "cwd",
            "__RTS_FN_NS_ENV_CWD",
            sig!(=> Handle),
            "cwd(): string",
            "Returns the current working directory as a string handle.",
            __RTS_FN_NS_ENV_CWD as *const u8,
        ))
        .member(func(
            "set_cwd",
            "__RTS_FN_NS_ENV_SET_CWD",
            sig!(StrPtr => I64),
            "set_cwd(path: string): number",
            "Changes the current working directory. Returns 0 on success, -1 on error.",
            __RTS_FN_NS_ENV_SET_CWD as *const u8,
        ))
        .done();
}
