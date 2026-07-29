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

use rts_engine::Engine;

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
#[rtse::function(module = "runtime", value = "eval")]
fn eval(src: &str) -> i64 {
    let tmp = std::env::temp_dir().join(format!("rts_eval_{}.ts", std::process::id()));
    if std::fs::write(&tmp, src).is_err() {
        return -1;
    }
    let code = spawn_rts_run(&tmp);
    let _ = std::fs::remove_file(&tmp);
    code
}

/// Evaluates the TS file at `path`. Returns the program exit code, or -1.
#[rtse::function(module = "runtime", value = "eval_file")]
fn eval_file(path: &str) -> i64 {
    spawn_rts_run(std::path::Path::new(path))
}

/// AOT stub for dynamic `import(path)` — JIT-only; returns a null handle.
#[rtse::function(module = "runtime", value = "import_module")]
fn import_module(_path: &str) -> u64 {
    0
}

/// AOT stub: no in-process importer to receive the exports handle.
#[rtse::function(module = "runtime", value = "set_module_exports")]
fn set_module_exports(_ns: u64) {}

/// Registra a namespace `runtime` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.module("runtime", |m| {
        m.doc("Dynamic evaluation + hot-reload primitives (AOT/subprocess; JIT shadows).");
        m.registry(eval_entry());
        m.registry(eval_file_entry());
        m.registry(import_module_entry());
        m.registry(set_module_exports_entry());
    });
}
