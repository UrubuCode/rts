//! `runtime` namespace — AOT/subprocess `eval` / `eval_file` + dynamic-import
//! stubs.
//!
//! These symbols compile into both the main `rts` binary and the
//! `runtime_support.a` staticlib. Under AOT the pipeline is unavailable, so
//! eval spawns the `rts` binary; the JIT path shadows these symbols with
//! in-process versions (`runtime_eval_src_jit` etc. from `eval_jit.rs`).
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

use std::path::PathBuf;

use rts_abi::ty::{Handle, I64};
use rts_macro::rts_namespace;

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

/// Dynamic evaluation + hot-reload primitives (AOT/subprocess; JIT shadows).
#[rts_namespace(runtime)]
impl RuntimeNs {
    /// Evaluates TS `src`. Returns the program exit code, or -1 on error.
    #[rts_fn(on_null = -1)]
    pub fn eval(src: Str) -> I64 {
        let tmp = std::env::temp_dir().join(format!("rts_eval_{}.ts", std::process::id()));
        if std::fs::write(&tmp, src).is_err() {
            return -1;
        }
        let code = spawn_rts_run(&tmp);
        let _ = std::fs::remove_file(&tmp);
        code
    }

    /// Evaluates the TS file at `path`. Returns the program exit code, or -1.
    #[rts_fn(on_null = -1)]
    pub fn eval_file(path: Str) -> I64 {
        spawn_rts_run(std::path::Path::new(path))
    }

    /// AOT stub for dynamic `import(path)` — JIT-only; returns a null handle.
    #[rts_fn]
    pub fn import_module(_path: Str) -> Handle {
        0
    }

    /// AOT stub: no in-process importer to receive the exports handle.
    #[rts_fn]
    pub fn set_module_exports(_ns: Handle) {}
}
