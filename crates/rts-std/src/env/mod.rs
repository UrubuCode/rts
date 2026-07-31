//! `env` namespace — environment variables, process argv, and cwd.
//!
//! Migrado pro modelo builder do `rts-engine` (Fase 2; ver `namespaces/hint`).
//! Args `Str` chegam como (ptr, len) e são reconstruídos via `from_abi`
//! (retorna o default em null/UTF-8 inválido).

use std::env;
use std::path::Path;

use rts_engine::abi::ty::Handle;
use rts_engine::heap::string_pool::__RTS_FN_NS_GC_STRING_NEW;
use rts_engine::Engine;

/// Interns `s` into the GC string pool, returning its handle.
fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Returns a string handle with the environment variable's value, or 0 when
/// absent. Kept as an explicit `Handle` return (not the macro's automatic
/// `String -> Handle` path, which always allocates — even for ""): callers
/// distinguish "absent" (handle 0) from "set to the empty string" (a real,
/// interned empty-string handle).
#[rtse::function(module = "env", value = "get_var", ret_ts = "string")]
fn get_var(name: &str) -> Handle {
    match env::var(name) {
        Ok(value) => intern(&value),
        Err(_) => 0,
    }
}

/// Sets an environment variable.
#[rtse::function(module = "env", value = "set_var")]
fn set_var(name: &str, value: &str) {
    // SAFETY: std::env::set_var e unsafe no Rust 2024 (estado global); o RTS run
    // path e single-threaded por construcao.
    unsafe { env::set_var(name, value) };
}

/// Removes an environment variable.
#[rtse::function(module = "env", value = "remove_var")]
fn remove_var(name: &str) {
    // SAFETY: mesma justificativa de set_var.
    unsafe { env::remove_var(name) };
}

/// Number of command-line arguments (including argv[0]).
#[rtse::function(module = "env", value = "args_count")]
fn args_count() -> i64 {
    env::args().count() as i64
}

/// Returns the argv entry at `index` as a string handle; 0 when out of range.
#[rtse::function(module = "env", value = "arg_at", ret_ts = "string")]
fn arg_at(index: i64) -> Handle {
    if index < 0 {
        return 0;
    }
    match env::args().nth(index as usize) {
        Some(arg) => intern(&arg),
        None => 0,
    }
}

/// Returns the current working directory as a string handle; 0 on error.
#[rtse::function(module = "env", value = "cwd", ret_ts = "string")]
pub fn cwd() -> Handle {
    match env::current_dir() {
        Ok(path) => intern(&path.to_string_lossy()),
        Err(_) => 0,
    }
}

/// Changes the current working directory. Returns 0 on success, -1 on error.
#[rtse::function(module = "env", value = "set_cwd")]
fn set_cwd(path: &str) -> i64 {
    match env::set_current_dir(Path::new(path)) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

/// Registra a namespace `env` no motor (Fase 2).
pub fn register(e: &mut Engine) {
    e.module("env", |m| {
        m.doc("Environment variables, process argv, and current working directory.");
        m.registry(get_var_entry());
        m.registry(set_var_entry());
        m.registry(remove_var_entry());
        m.registry(args_count_entry());
        m.registry(arg_at_entry());
        m.registry(cwd_entry());
        m.registry(set_cwd_entry());
    });
}
