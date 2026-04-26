//! AOT/subprocess implementation of `runtime.eval` and `runtime.eval_file`.
//!
//! These symbols are compiled into both the main `rts` binary and the
//! `runtime_support.a` staticlib. In the staticlib context the pipeline is
//! unavailable, so both functions locate and spawn the `rts` binary.
//!
//! The JIT path bypasses these symbols entirely: `jit.rs` registers
//! `runtime_eval_src_jit` / `runtime_eval_file_jit` (from `eval_jit.rs`)
//! under the same symbol names, shadowing the subprocess versions.

use std::path::PathBuf;

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_RUNTIME_EVAL(ptr: i64, len: i64) -> i64 {
    let src = match bytes_to_str(ptr, len) {
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

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_RUNTIME_EVAL_FILE(ptr: i64, len: i64) -> i64 {
    let path = match bytes_to_str(ptr, len) {
        Some(s) => s,
        None => return -1,
    };
    spawn_rts_run(std::path::Path::new(path))
}

fn spawn_rts_run(path: &std::path::Path) -> i64 {
    let rts = find_rts_binary();
    match std::process::Command::new(&rts).arg("run").arg(path).status() {
        Ok(s) => s.code().unwrap_or(-1) as i64,
        Err(_) => -1,
    }
}

fn find_rts_binary() -> PathBuf {
    // Prefer RTS_BINARY env var.
    if let Ok(p) = std::env::var("RTS_BINARY") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return p;
        }
    }

    // Try alongside the current executable.
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

    // Fall back to PATH lookup.
    #[cfg(target_os = "windows")]
    return PathBuf::from("rts.exe");
    #[cfg(not(target_os = "windows"))]
    return PathBuf::from("rts");
}

fn bytes_to_str<'a>(ptr: i64, len: i64) -> Option<&'a str> {
    if ptr == 0 || len <= 0 {
        return None;
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    std::str::from_utf8(bytes).ok()
}
