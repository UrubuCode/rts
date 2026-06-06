//! JIT fast path for `runtime.eval` and `runtime.eval_file`.
//!
//! These functions are registered by `jit.rs` under the
//! `__RTS_FN_NS_RUNTIME_EVAL` / `__RTS_FN_NS_RUNTIME_EVAL_FILE` symbol
//! names, shadowing the subprocess-based versions from `eval.rs`.
//!
//! They are only compiled as part of the main `rts` crate (not in the
//! `runtime_support.a` staticlib via `rt_all.rs`).

use cranelift_module::Module;

pub extern "C" fn runtime_eval_src_jit(ptr: i64, len: i64) -> i64 {
    let src = match bytes_to_str(ptr, len) {
        Some(s) => s,
        None => return -1,
    };
    match run_source(src) {
        Ok(code) => code as i64,
        Err(_) => -1,
    }
}

pub extern "C" fn runtime_eval_file_jit(ptr: i64, len: i64) -> i64 {
    let path = match bytes_to_str(ptr, len) {
        Some(s) => s,
        None => return -1,
    };
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    match run_source(&src) {
        Ok(code) => code as i64,
        Err(_) => -1,
    }
}

use std::cell::RefCell;
use std::collections::HashMap;

/// Directory of the entry script, used to resolve relative dynamic-import
/// specifiers (`import("./x.mjs")`) the way Node/Bun do — relative to the
/// importing file, not the process CWD. Global (not thread-local) because async
/// `import` runs on a tokio worker thread, not the thread that set it.
static ENTRY_DIR: std::sync::Mutex<Option<std::path::PathBuf>> =
    std::sync::Mutex::new(None);

/// Records the entry script's directory (called once at `rts run` startup).
pub fn set_entry_dir(dir: std::path::PathBuf) {
    if let Ok(mut g) = ENTRY_DIR.lock() {
        *g = Some(dir);
    }
}

fn entry_dir() -> Option<std::path::PathBuf> {
    ENTRY_DIR.lock().ok().and_then(|g| g.clone())
}

thread_local! {
    /// Slot where a dynamically-imported module stashes its exports namespace
    /// handle (via `runtime.set_module_exports`) so the importer can read it
    /// after the module's `__RTS_MAIN` finishes.
    static MODULE_EXPORTS_SLOT: RefCell<i64> = const { RefCell::new(0) };
    /// Per-path cache of the exports handle — gives `import(p) === import(p)`
    /// identity (ESM module-instance caching).
    static MODULE_CACHE: RefCell<HashMap<String, i64>> = RefCell::new(HashMap::new());
}

/// JIT `runtime.set_module_exports(ns)` — the imported module calls this with
/// its exports namespace handle.
pub extern "C" fn runtime_set_module_exports_jit(handle: i64) {
    MODULE_EXPORTS_SLOT.with(|s| *s.borrow_mut() = handle);
}

/// JIT dynamic `import(path)`: compiles + runs the target module in-process,
/// collecting its `export`s into a namespace object whose handle is returned.
/// Cached per resolved path so re-imports return the same handle (`===`).
pub extern "C" fn runtime_import_module_jit(ptr: i64, len: i64) -> u64 {
    let path = match bytes_to_str(ptr, len) {
        Some(s) => s,
        None => return 0,
    };
    // Resolve relative specifiers against the entry script's dir (Node/Bun
    // semantics), falling back to the raw path (CWD-relative / absolute).
    let candidate = std::path::Path::new(path);
    let actual: std::path::PathBuf = if candidate.is_absolute() || candidate.exists() {
        candidate.to_path_buf()
    } else if let Some(base) = entry_dir() {
        let p = base.join(path);
        if p.exists() { p } else { candidate.to_path_buf() }
    } else {
        candidate.to_path_buf()
    };
    let resolved = std::fs::canonicalize(&actual)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| actual.to_string_lossy().to_string());
    if let Some(h) = MODULE_CACHE.with(|c| c.borrow().get(&resolved).copied()) {
        return h as u64;
    }
    let src = match std::fs::read_to_string(&actual) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let to_run = match transform_module_exports(&src) {
        // Has exports: run the transformed source, then read the slot.
        Some(t) => t,
        // No exports: run for side effects only, return null namespace.
        None => {
            let _ = run_source(&src);
            return 0;
        }
    };
    MODULE_EXPORTS_SLOT.with(|s| *s.borrow_mut() = 0);
    if run_source(&to_run).is_err() {
        return 0;
    }
    let h = MODULE_EXPORTS_SLOT.with(|s| *s.borrow());
    if h != 0 {
        MODULE_CACHE.with(|c| {
            c.borrow_mut().insert(resolved, h);
        });
    }
    h as u64
}

/// Rewrites a module's `export`s into plain top-level declarations plus a
/// trailing `runtime.set_module_exports({...})` that packs every export into a
/// namespace object. Returns `None` when the module has no exports.
///
/// Line-oriented (standard ESM style: each `export` starts its own line). Covers
/// `export const/let/var/function/class NAME`, `export default function/class`
/// (named or anonymous), `export default <expr>` and `export { a, b as c }`
/// (without `from`). Re-exports with `from` and `export *` are passed through
/// unchanged (resolved elsewhere / unsupported v1).
fn transform_module_exports(src: &str) -> Option<String> {
    let mut out = String::with_capacity(src.len() + 256);
    // (exported_name, local_binding_ident)
    let mut exports: Vec<(String, String)> = Vec::new();
    let mut anon = 0usize;

    for line in src.lines() {
        let trimmed = line.trim_start();
        let indent = &line[..line.len() - trimmed.len()];
        let Some(rest) = trimmed.strip_prefix("export ") else {
            out.push_str(line);
            out.push('\n');
            continue;
        };
        let rest = rest.trim_start();

        // export default ...
        if let Some(after) = rest.strip_prefix("default ") {
            let after = after.trim_start();
            if let Some(fnrest) = after.strip_prefix("function") {
                let fnrest = fnrest.trim_start();
                let is_anon = fnrest.starts_with('(') || fnrest.starts_with('*');
                let name = if is_anon {
                    let n = format!("__rts_default_{anon}");
                    anon += 1;
                    n
                } else {
                    let end = fnrest
                        .find(|c: char| c == '(' || c == '<' || c.is_whitespace())
                        .unwrap_or(fnrest.len());
                    fnrest[..end].to_string()
                };
                exports.push(("default".into(), name.clone()));
                out.push_str(indent);
                out.push_str("function ");
                if is_anon {
                    out.push_str(&name);
                    out.push(' ');
                }
                out.push_str(fnrest);
                out.push('\n');
                continue;
            }
            if let Some(clsrest) = after.strip_prefix("class") {
                let clsrest = clsrest.trim_start();
                let is_anon = clsrest.starts_with('{') || clsrest.starts_with("extends");
                let name = if is_anon {
                    let n = format!("__rts_default_{anon}");
                    anon += 1;
                    n
                } else {
                    let end = clsrest
                        .find(|c: char| c == '{' || c.is_whitespace())
                        .unwrap_or(clsrest.len());
                    clsrest[..end].to_string()
                };
                exports.push(("default".into(), name.clone()));
                out.push_str(indent);
                out.push_str("class ");
                out.push_str(&name);
                out.push(' ');
                out.push_str(clsrest);
                out.push('\n');
                continue;
            }
            // export default <expr> ;
            let name = format!("__rts_default_{anon}");
            anon += 1;
            exports.push(("default".into(), name.clone()));
            out.push_str(indent);
            out.push_str("const ");
            out.push_str(&name);
            out.push_str(" = ");
            out.push_str(after);
            if !after.trim_end().ends_with(';') {
                out.push(';');
            }
            out.push('\n');
            continue;
        }

        // export const/let/var/function/class NAME
        let mut handled = false;
        for kw in ["const ", "let ", "var ", "function ", "class ", "async function "] {
            if let Some(declrest) = rest.strip_prefix(kw) {
                let dr = declrest.trim_start();
                let end = dr
                    .find(|c: char| {
                        c == '=' || c == ':' || c == '(' || c == '<' || c == '{'
                            || c == ',' || c.is_whitespace()
                    })
                    .unwrap_or(dr.len());
                let name = dr[..end].trim();
                if !name.is_empty() {
                    exports.push((name.to_string(), name.to_string()));
                }
                out.push_str(indent);
                out.push_str(kw);
                out.push_str(declrest);
                out.push('\n');
                handled = true;
                break;
            }
        }
        if handled {
            continue;
        }

        // export { a, b as c };  (no `from`)
        if let Some(brace) = rest.strip_prefix('{') {
            if !rest.contains(" from ") {
                if let Some(inner) = brace.split('}').next() {
                    for part in inner.split(',') {
                        let part = part.trim();
                        if part.is_empty() {
                            continue;
                        }
                        let (local, exported) = match part.split_once(" as ") {
                            Some((l, e)) => (l.trim().to_string(), e.trim().to_string()),
                            None => (part.to_string(), part.to_string()),
                        };
                        exports.push((exported, local));
                    }
                }
                // drop the line (bindings already declared elsewhere)
                continue;
            }
        }

        // Unhandled export form (re-export with `from`, `export *`): pass through.
        out.push_str(line);
        out.push('\n');
    }

    if exports.is_empty() {
        return None;
    }

    out.push_str("\nruntime.set_module_exports({");
    for (i, (name, local)) in exports.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(" \"{name}\": {local}"));
    }
    out.push_str(" });\n");
    Some(out)
}

fn run_source(src: &str) -> anyhow::Result<i32> {
    use crate::compile_options::FrontendMode;

    let mut program = crate::parser::parse_source_with_mode(src, FrontendMode::Native)?;
    let (module, _warnings) = crate::codegen::compile_program_to_jit(&mut program)?;

    // Mantido hardcoded: este arquivo eh compilado tanto como parte do main
    // crate quanto como parte de `runtime_support` (rt_all.rs), e a constante
    // `crate::abi::symbols::ENTRY_POINT` so existe no main crate. Se mudar la,
    // mudar aqui tambem.
    let name = "__RTS_MAIN";
    let main_id = match module.get_name(name) {
        Some(cranelift_module::FuncOrDataId::Func(id)) => id,
        _ => anyhow::bail!("inner JIT: `{name}` not found"),
    };
    let main_ptr = module.get_finalized_function(main_id);
    let main_fn: extern "C" fn() -> i32 = unsafe { std::mem::transmute(main_ptr) };
    let exit_code = main_fn();
    if let Some(report) = crate::namespaces::gc::error::take_runtime_error_report() {
        let use_color = crate::diagnostics::reporter::stderr_supports_color();
        eprint!("{}", crate::pipeline::format_runtime_error_report(&report, use_color));
        return Ok(1);
    }
    std::mem::forget(module);
    Ok(exit_code)
}

fn bytes_to_str<'a>(ptr: i64, len: i64) -> Option<&'a str> {
    if ptr == 0 || len <= 0 {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    std::str::from_utf8(bytes).ok()
}
