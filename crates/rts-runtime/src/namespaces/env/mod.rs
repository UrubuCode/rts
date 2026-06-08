//! `env` namespace — environment variables, process argv, and cwd.
//!
//! `get_var`/`arg_at`/`cwd` return GC string handles (`ts = "...: string"`);
//! `set_cwd` returns 0/-1 (`on_null = -1`). String args arrive as `&str` via the
//! macro's `Str` expansion.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use std::env;
use std::path::Path;

use rts_abi::ty::{Handle, I64};
use rts_macro::rts_namespace;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Environment variables, process argv, and current working directory.
#[rts_namespace(env)]
impl EnvNs {
    /// Returns a string handle with the environment variable's value, or 0 when absent.
    #[rts_fn(ts = "get_var(name: string): string")]
    pub fn get_var(name: Str) -> Handle {
        match env::var(name) {
            Ok(value) => intern(&value),
            Err(_) => 0,
        }
    }

    /// Sets an environment variable.
    #[rts_fn]
    pub fn set_var(name: Str, value: Str) {
        // SAFETY: std::env::set_var e unsafe no Rust 2024 (estado global); o RTS
        // run path e single-threaded por construcao.
        unsafe { env::set_var(name, value) };
    }

    /// Removes an environment variable.
    #[rts_fn]
    pub fn remove_var(name: Str) {
        // SAFETY: mesma justificativa de set_var.
        unsafe { env::remove_var(name) };
    }

    /// Number of command-line arguments (including argv[0]).
    #[rts_fn]
    pub fn args_count() -> I64 {
        env::args().count() as i64
    }

    /// Returns the argv entry at `index` as a string handle; 0 when out of range.
    #[rts_fn(ts = "arg_at(index: number): string")]
    pub fn arg_at(index: I64) -> Handle {
        if index < 0 {
            return 0;
        }
        let Some(arg) = env::args().nth(index as usize) else {
            return 0;
        };
        intern(&arg)
    }

    /// Returns the current working directory as a string handle.
    #[rts_fn(ts = "cwd(): string")]
    pub fn cwd() -> Handle {
        match env::current_dir() {
            Ok(path) => intern(&path.to_string_lossy()),
            Err(_) => 0,
        }
    }

    /// Changes the current working directory. Returns 0 on success, -1 on error.
    #[rts_fn(on_null = -1)]
    pub fn set_cwd(path: Str) -> I64 {
        match env::set_current_dir(Path::new(path)) {
            Ok(_) => 0,
            Err(_) => -1,
        }
    }
}
