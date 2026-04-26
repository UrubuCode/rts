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

fn run_source(src: &str) -> anyhow::Result<i32> {
    use crate::compile_options::FrontendMode;

    let mut program = crate::parser::parse_source_with_mode(src, FrontendMode::Native)?;
    let (module, _warnings) = crate::codegen::compile_program_to_jit(&mut program)?;

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
        let red = if use_color { "\x1b[1;31m" } else { "" };
        let reset = if use_color { "\x1b[0m" } else { "" };
        let bold = if use_color { "\x1b[1m" } else { "" };
        let mut msg = format!("{red}error{reset}{bold}: {}{reset}\n", report.message);
        if let Some(stack) = &report.stack {
            if !stack.trim().is_empty() {
                msg.push_str(stack.trim_end());
                msg.push('\n');
            }
        }
        eprint!("{msg}");
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
