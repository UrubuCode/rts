//! `runtime` namespace — AOT/subprocess `eval` / `eval_file` + dynamic-import
//! stubs.
//!
//! These symbols compile into both the main `rts` binary and the
//! `runtime_support.a` staticlib. Under AOT the pipeline is unavailable, so
//! eval spawns the `rts` binary; the JIT path shadows these symbols with
//! in-process versions (`runtime_eval_src_jit` etc. from `eval_jit.rs`).
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem).
//!
//! Além da namespace `runtime` (eval), este módulo hospeda o suporte async
//! compartilhado do RTS: `async_rt` (runtime tokio global) e `tokio_ctx`
//! (ponte sync↔async), movidos do `rts-runtime` na Fase 1b.

pub mod async_rt;
pub mod tokio_ctx;

use std::path::PathBuf;

use rts_engine::abi::ty::{Handle, I64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

fn spawn_rts_run(path: &std::path::Path) -> i64 {
    let rts = find_rts_binary();
    match std::process::Command::new(&rts)
        .arg("run")
        .arg(path)
        .status()
    {
        Ok(s) => s.code().unwrap_or(-1) as i64,
        Err(_) => -1,
    }
}

fn find_rts_binary() -> PathBuf {
    if let Ok(p) = std::env::var("RTS_BINARY") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return p;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            #[cfg(target_os = "windows")]
            let candidate = dir.join("rts.exe");
            #[cfg(not(target_os = "windows"))]
            let candidate = dir.join("rts");
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    #[cfg(target_os = "windows")]
    return PathBuf::from("rts.exe");
    #[cfg(not(target_os = "windows"))]
    return PathBuf::from("rts");
}

/// Evaluates TS `src`. Returns the program exit code, or -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_RUNTIME_EVAL(src_ptr: *const u8, src_len: i64) -> I64 {
    let src = match unsafe { rts_engine::abi::str_abi::from_abi(src_ptr, src_len) } {
        Some(s) => s,
        None => return -1,
    };
    let tmp = std::env::temp_dir().join(format!("rts_eval_{}.ts", std::process::id()));
    if std::fs::write(&tmp, src).is_err() {
        return -1;
    }
    let code = spawn_rts_run(&tmp);
    let _ = std::fs::remove_file(&tmp);
    code
}

/// Evaluates the TS file at `path`. Returns the program exit code, or -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_RUNTIME_EVAL_FILE(path_ptr: *const u8, path_len: i64) -> I64 {
    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(s) => s,
        None => return -1,
    };
    spawn_rts_run(std::path::Path::new(path))
}

/// AOT stub for dynamic `import(path)` — JIT-only; returns a null handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_RUNTIME_IMPORT_MODULE(_path_ptr: *const u8, _path_len: i64) -> Handle {
    0
}

/// AOT stub: no in-process importer to receive the exports handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_RUNTIME_SET_MODULE_EXPORTS(_ns: Handle) {}

/// Função `runtime.f(args)`.
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
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
        emit: None,
    }
}

/// Registra a namespace `runtime` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("runtime")
        .doc("Dynamic evaluation + hot-reload primitives (AOT/subprocess; JIT shadows).")
        .member(func(
            "eval",
            "__RTS_FN_NS_RUNTIME_EVAL",
            Sig::new(vec![AbiType::StrPtr], AbiType::I64),
            "eval(src: string): number",
            "Evaluates TS `src`. Returns the program exit code, or -1 on error.",
            __RTS_FN_NS_RUNTIME_EVAL as *const u8,
        ))
        .member(func(
            "eval_file",
            "__RTS_FN_NS_RUNTIME_EVAL_FILE",
            Sig::new(vec![AbiType::StrPtr], AbiType::I64),
            "eval_file(path: string): number",
            "Evaluates the TS file at `path`. Returns the program exit code, or -1.",
            __RTS_FN_NS_RUNTIME_EVAL_FILE as *const u8,
        ))
        .member(func(
            "import_module",
            "__RTS_FN_NS_RUNTIME_IMPORT_MODULE",
            Sig::new(vec![AbiType::StrPtr], AbiType::Handle),
            "import_module(path: string): number",
            "AOT stub for dynamic `import(path)` — JIT-only; returns a null handle.",
            __RTS_FN_NS_RUNTIME_IMPORT_MODULE as *const u8,
        ))
        .member(func(
            "set_module_exports",
            "__RTS_FN_NS_RUNTIME_SET_MODULE_EXPORTS",
            Sig::new(vec![AbiType::Handle], AbiType::Void),
            "set_module_exports(ns: number): void",
            "AOT stub: no in-process importer to receive the exports handle.",
            __RTS_FN_NS_RUNTIME_SET_MODULE_EXPORTS as *const u8,
        ))
        .done();
}
