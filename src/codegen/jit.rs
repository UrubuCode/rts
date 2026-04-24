//! In-memory JIT mode for `rts run`.
//!
//! Uses `cranelift_jit::JITModule` instead of the object emitter so we skip
//! disk I/O, the system linker, and the whole extract-run-cleanup dance.
//! Produces a function pointer to `__RTS_MAIN` that we call with a plain
//! `extern "C"` transmute.
//!
//! All runtime symbols (`__RTS_FN_NS_*`, `__RTS_DATA_*`, `fmod`) are
//! registered up front via `JITBuilder::symbol` so the JIT can resolve
//! imports without going through the OS dynamic loader. The table is
//! built from `abi::SPECS` plus a handful of data/libc entries.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_jit::{JITBuilder, JITModule};

use crate::abi::SPECS;
use crate::codegen::lower::compile_program;
use crate::parser::ast::Program;

/// Compiles a program into a JIT module and returns an owned `JITModule`
/// plus the FuncId for `__RTS_MAIN`. Caller invokes
/// `module.get_finalized_function(id)` to obtain the pointer to execute.
pub fn compile_program_to_jit(program: &Program) -> Result<(JITModule, Vec<String>)> {
    let mut module = build_jit_module()?;
    let mut extern_cache = HashMap::new();
    let mut data_counter: u32 = 0;

    let warnings = compile_program(program, &mut module, &mut extern_cache, &mut data_counter)?;

    module
        .finalize_definitions()
        .map_err(|e| anyhow!("JIT finalise failed: {e}"))?;

    Ok((module, warnings))
}

fn build_jit_module() -> Result<JITModule> {
    let mut flag_builder = settings::builder();
    flag_builder
        .set("is_pic", "false")
        .map_err(|e| anyhow!("cranelift flag error: {e}"))?;
    flag_builder
        .set("opt_level", "speed")
        .map_err(|e| anyhow!("cranelift flag error: {e}"))?;
    flag_builder
        .set("preserve_frame_pointers", "true")
        .map_err(|e| anyhow!("cranelift flag error: {e}"))?;
    let flags = settings::Flags::new(flag_builder);

    let isa_builder =
        cranelift_native::builder().map_err(|e| anyhow!("failed to detect native target: {e}"))?;
    let isa = isa_builder
        .finish(flags)
        .map_err(|e| anyhow!("failed to finalise ISA: {e}"))?;

    let mut jit_builder =
        JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());

    // Register every ABI member from `abi::SPECS`. Each member's symbol
    // resolves to the `#[no_mangle] extern "C"` definition in the runtime
    // — the JIT needs an explicit pointer because we are not going
    // through a linker.
    register_runtime_symbols(&mut jit_builder);

    Ok(JITModule::new(jit_builder))
}

/// Collects every runtime symbol visible through the ABI and registers it
/// with the JIT builder. The symbol → pointer mapping is built by
/// consulting `SPECS` and the small helper table below; missing entries
/// make the JIT fail at finalize time with a clear error, which is what
/// we want rather than silent mis-linking.
fn register_runtime_symbols(jit: &mut JITBuilder) {
    for (name, ptr) in runtime_symbol_table() {
        jit.symbol(name, ptr);
    }
}

/// Returns `(symbol, ptr)` tuples for every runtime symbol the JIT needs.
/// Populated by `runtime_symbols!` below; data symbols (the PRNG state)
/// and libc imports (`fmod`) are added manually.
fn runtime_symbol_table() -> Vec<(&'static str, *const u8)> {
    let mut out: Vec<(&'static str, *const u8)> = Vec::new();

    macro_rules! add_fn {
        ($name:literal, $path:path) => {
            out.push(($name, $path as *const u8));
        };
    }

    // ── namespaces::gc ────────────────────────────────────────────────
    use crate::namespaces::gc::string_pool::*;
    add_fn!("__RTS_FN_NS_GC_STRING_NEW", __RTS_FN_NS_GC_STRING_NEW);
    add_fn!("__RTS_FN_NS_GC_STRING_LEN", __RTS_FN_NS_GC_STRING_LEN);
    add_fn!("__RTS_FN_NS_GC_STRING_PTR", __RTS_FN_NS_GC_STRING_PTR);
    add_fn!("__RTS_FN_NS_GC_STRING_FREE", __RTS_FN_NS_GC_STRING_FREE);
    add_fn!("__RTS_FN_NS_GC_STRING_FROM_I64", __RTS_FN_NS_GC_STRING_FROM_I64);
    add_fn!("__RTS_FN_NS_GC_STRING_FROM_F64", __RTS_FN_NS_GC_STRING_FROM_F64);
    add_fn!("__RTS_FN_NS_GC_STRING_CONCAT", __RTS_FN_NS_GC_STRING_CONCAT);
    add_fn!("__RTS_FN_NS_GC_STRING_FROM_STATIC", __RTS_FN_NS_GC_STRING_FROM_STATIC);

    // ── namespaces::io ────────────────────────────────────────────────
    use crate::namespaces::io::print::*;
    use crate::namespaces::io::stderr::*;
    use crate::namespaces::io::stdin::*;
    use crate::namespaces::io::stdout::*;
    add_fn!("__RTS_FN_NS_IO_PRINT", __RTS_FN_NS_IO_PRINT);
    add_fn!("__RTS_FN_NS_IO_EPRINT", __RTS_FN_NS_IO_EPRINT);
    add_fn!("__RTS_FN_NS_IO_STDOUT_WRITE", __RTS_FN_NS_IO_STDOUT_WRITE);
    add_fn!("__RTS_FN_NS_IO_STDOUT_FLUSH", __RTS_FN_NS_IO_STDOUT_FLUSH);
    add_fn!("__RTS_FN_NS_IO_STDERR_WRITE", __RTS_FN_NS_IO_STDERR_WRITE);
    add_fn!("__RTS_FN_NS_IO_STDERR_FLUSH", __RTS_FN_NS_IO_STDERR_FLUSH);
    add_fn!("__RTS_FN_NS_IO_STDIN_READ", __RTS_FN_NS_IO_STDIN_READ);
    add_fn!("__RTS_FN_NS_IO_STDIN_READ_LINE", __RTS_FN_NS_IO_STDIN_READ_LINE);

    // ── namespaces::fs ────────────────────────────────────────────────
    use crate::namespaces::fs::*;
    add_fn!("__RTS_FN_NS_FS_READ", read::__RTS_FN_NS_FS_READ);
    add_fn!("__RTS_FN_NS_FS_READ_ALL", read::__RTS_FN_NS_FS_READ_ALL);
    add_fn!("__RTS_FN_NS_FS_WRITE", write::__RTS_FN_NS_FS_WRITE);
    add_fn!("__RTS_FN_NS_FS_APPEND", write::__RTS_FN_NS_FS_APPEND);
    add_fn!("__RTS_FN_NS_FS_EXISTS", metadata::__RTS_FN_NS_FS_EXISTS);
    add_fn!("__RTS_FN_NS_FS_IS_FILE", metadata::__RTS_FN_NS_FS_IS_FILE);
    add_fn!("__RTS_FN_NS_FS_IS_DIR", metadata::__RTS_FN_NS_FS_IS_DIR);
    add_fn!("__RTS_FN_NS_FS_SIZE", metadata::__RTS_FN_NS_FS_SIZE);
    add_fn!("__RTS_FN_NS_FS_MODIFIED_MS", metadata::__RTS_FN_NS_FS_MODIFIED_MS);
    add_fn!("__RTS_FN_NS_FS_CREATE_DIR", dir::__RTS_FN_NS_FS_CREATE_DIR);
    add_fn!("__RTS_FN_NS_FS_CREATE_DIR_ALL", dir::__RTS_FN_NS_FS_CREATE_DIR_ALL);
    add_fn!("__RTS_FN_NS_FS_REMOVE_DIR", dir::__RTS_FN_NS_FS_REMOVE_DIR);
    add_fn!("__RTS_FN_NS_FS_REMOVE_DIR_ALL", dir::__RTS_FN_NS_FS_REMOVE_DIR_ALL);
    add_fn!("__RTS_FN_NS_FS_REMOVE_FILE", ops::__RTS_FN_NS_FS_REMOVE_FILE);
    add_fn!("__RTS_FN_NS_FS_RENAME", ops::__RTS_FN_NS_FS_RENAME);
    add_fn!("__RTS_FN_NS_FS_COPY", ops::__RTS_FN_NS_FS_COPY);

    // ── namespaces::math ──────────────────────────────────────────────
    use crate::namespaces::math::*;
    add_fn!("__RTS_FN_NS_MATH_FLOOR", basic::__RTS_FN_NS_MATH_FLOOR);
    add_fn!("__RTS_FN_NS_MATH_CEIL", basic::__RTS_FN_NS_MATH_CEIL);
    add_fn!("__RTS_FN_NS_MATH_ROUND", basic::__RTS_FN_NS_MATH_ROUND);
    add_fn!("__RTS_FN_NS_MATH_TRUNC", basic::__RTS_FN_NS_MATH_TRUNC);
    add_fn!("__RTS_FN_NS_MATH_SQRT", basic::__RTS_FN_NS_MATH_SQRT);
    add_fn!("__RTS_FN_NS_MATH_CBRT", basic::__RTS_FN_NS_MATH_CBRT);
    add_fn!("__RTS_FN_NS_MATH_POW", basic::__RTS_FN_NS_MATH_POW);
    add_fn!("__RTS_FN_NS_MATH_EXP", basic::__RTS_FN_NS_MATH_EXP);
    add_fn!("__RTS_FN_NS_MATH_LN", basic::__RTS_FN_NS_MATH_LN);
    add_fn!("__RTS_FN_NS_MATH_LOG2", basic::__RTS_FN_NS_MATH_LOG2);
    add_fn!("__RTS_FN_NS_MATH_LOG10", basic::__RTS_FN_NS_MATH_LOG10);
    add_fn!("__RTS_FN_NS_MATH_ABS_F64", basic::__RTS_FN_NS_MATH_ABS_F64);
    add_fn!("__RTS_FN_NS_MATH_ABS_I64", basic::__RTS_FN_NS_MATH_ABS_I64);
    add_fn!("__RTS_FN_NS_MATH_SIN", trig::__RTS_FN_NS_MATH_SIN);
    add_fn!("__RTS_FN_NS_MATH_COS", trig::__RTS_FN_NS_MATH_COS);
    add_fn!("__RTS_FN_NS_MATH_TAN", trig::__RTS_FN_NS_MATH_TAN);
    add_fn!("__RTS_FN_NS_MATH_ASIN", trig::__RTS_FN_NS_MATH_ASIN);
    add_fn!("__RTS_FN_NS_MATH_ACOS", trig::__RTS_FN_NS_MATH_ACOS);
    add_fn!("__RTS_FN_NS_MATH_ATAN", trig::__RTS_FN_NS_MATH_ATAN);
    add_fn!("__RTS_FN_NS_MATH_ATAN2", trig::__RTS_FN_NS_MATH_ATAN2);
    add_fn!("__RTS_FN_NS_MATH_MIN_F64", minmax::__RTS_FN_NS_MATH_MIN_F64);
    add_fn!("__RTS_FN_NS_MATH_MAX_F64", minmax::__RTS_FN_NS_MATH_MAX_F64);
    add_fn!("__RTS_FN_NS_MATH_MIN_I64", minmax::__RTS_FN_NS_MATH_MIN_I64);
    add_fn!("__RTS_FN_NS_MATH_MAX_I64", minmax::__RTS_FN_NS_MATH_MAX_I64);
    add_fn!("__RTS_FN_NS_MATH_CLAMP_F64", minmax::__RTS_FN_NS_MATH_CLAMP_F64);
    add_fn!("__RTS_FN_NS_MATH_CLAMP_I64", minmax::__RTS_FN_NS_MATH_CLAMP_I64);
    add_fn!("__RTS_FN_NS_MATH_RANDOM_F64", random::__RTS_FN_NS_MATH_RANDOM_F64);
    add_fn!("__RTS_FN_NS_MATH_RANDOM_I64_RANGE", random::__RTS_FN_NS_MATH_RANDOM_I64_RANGE);
    add_fn!("__RTS_FN_NS_MATH_SEED", random::__RTS_FN_NS_MATH_SEED);
    add_fn!("__RTS_FN_NS_MATH_PI", consts::__RTS_FN_NS_MATH_PI);
    add_fn!("__RTS_FN_NS_MATH_E", consts::__RTS_FN_NS_MATH_E);
    add_fn!("__RTS_FN_NS_MATH_INFINITY", consts::__RTS_FN_NS_MATH_INFINITY);
    add_fn!("__RTS_FN_NS_MATH_NAN", consts::__RTS_FN_NS_MATH_NAN);

    // ── namespaces::fmt ───────────────────────────────────────────────
    {
        use crate::namespaces::fmt::*;
        add_fn!("__RTS_FN_NS_FMT_PARSE_I64", parse::__RTS_FN_NS_FMT_PARSE_I64);
        add_fn!("__RTS_FN_NS_FMT_PARSE_F64", parse::__RTS_FN_NS_FMT_PARSE_F64);
        add_fn!("__RTS_FN_NS_FMT_PARSE_BOOL", parse::__RTS_FN_NS_FMT_PARSE_BOOL);
        add_fn!("__RTS_FN_NS_FMT_FMT_I64", format::__RTS_FN_NS_FMT_FMT_I64);
        add_fn!("__RTS_FN_NS_FMT_FMT_F64", format::__RTS_FN_NS_FMT_FMT_F64);
        add_fn!("__RTS_FN_NS_FMT_FMT_BOOL", format::__RTS_FN_NS_FMT_FMT_BOOL);
        add_fn!("__RTS_FN_NS_FMT_FMT_HEX", format::__RTS_FN_NS_FMT_FMT_HEX);
        add_fn!("__RTS_FN_NS_FMT_FMT_BIN", format::__RTS_FN_NS_FMT_FMT_BIN);
        add_fn!("__RTS_FN_NS_FMT_FMT_OCT", format::__RTS_FN_NS_FMT_FMT_OCT);
        add_fn!("__RTS_FN_NS_FMT_FMT_F64_PREC", format::__RTS_FN_NS_FMT_FMT_F64_PREC);
    }

    // ── namespaces::hash ──────────────────────────────────────────────
    {
        use crate::namespaces::hash::ops as h;
        add_fn!("__RTS_FN_NS_HASH_HASH_STR", h::__RTS_FN_NS_HASH_HASH_STR);
        add_fn!("__RTS_FN_NS_HASH_HASH_BYTES", h::__RTS_FN_NS_HASH_HASH_BYTES);
        add_fn!("__RTS_FN_NS_HASH_HASH_I64", h::__RTS_FN_NS_HASH_HASH_I64);
        add_fn!("__RTS_FN_NS_HASH_HASH_COMBINE", h::__RTS_FN_NS_HASH_HASH_COMBINE);
    }

    // ── namespaces::collections ───────────────────────────────────────
    {
        use crate::namespaces::collections::*;
        add_fn!("__RTS_FN_NS_COLLECTIONS_MAP_NEW", map::__RTS_FN_NS_COLLECTIONS_MAP_NEW);
        add_fn!("__RTS_FN_NS_COLLECTIONS_MAP_FREE", map::__RTS_FN_NS_COLLECTIONS_MAP_FREE);
        add_fn!("__RTS_FN_NS_COLLECTIONS_MAP_LEN", map::__RTS_FN_NS_COLLECTIONS_MAP_LEN);
        add_fn!("__RTS_FN_NS_COLLECTIONS_MAP_HAS", map::__RTS_FN_NS_COLLECTIONS_MAP_HAS);
        add_fn!("__RTS_FN_NS_COLLECTIONS_MAP_GET", map::__RTS_FN_NS_COLLECTIONS_MAP_GET);
        add_fn!("__RTS_FN_NS_COLLECTIONS_MAP_SET", map::__RTS_FN_NS_COLLECTIONS_MAP_SET);
        add_fn!("__RTS_FN_NS_COLLECTIONS_MAP_DELETE", map::__RTS_FN_NS_COLLECTIONS_MAP_DELETE);
        add_fn!("__RTS_FN_NS_COLLECTIONS_MAP_CLEAR", map::__RTS_FN_NS_COLLECTIONS_MAP_CLEAR);
        add_fn!("__RTS_FN_NS_COLLECTIONS_VEC_NEW", vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW);
        add_fn!("__RTS_FN_NS_COLLECTIONS_VEC_FREE", vec::__RTS_FN_NS_COLLECTIONS_VEC_FREE);
        add_fn!("__RTS_FN_NS_COLLECTIONS_VEC_LEN", vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN);
        add_fn!("__RTS_FN_NS_COLLECTIONS_VEC_PUSH", vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH);
        add_fn!("__RTS_FN_NS_COLLECTIONS_VEC_POP", vec::__RTS_FN_NS_COLLECTIONS_VEC_POP);
        add_fn!("__RTS_FN_NS_COLLECTIONS_VEC_GET", vec::__RTS_FN_NS_COLLECTIONS_VEC_GET);
        add_fn!("__RTS_FN_NS_COLLECTIONS_VEC_SET", vec::__RTS_FN_NS_COLLECTIONS_VEC_SET);
        add_fn!("__RTS_FN_NS_COLLECTIONS_VEC_CLEAR", vec::__RTS_FN_NS_COLLECTIONS_VEC_CLEAR);
    }

    // ── namespaces::os ────────────────────────────────────────────────
    {
        use crate::namespaces::os::*;
        add_fn!("__RTS_FN_NS_OS_PLATFORM", info::__RTS_FN_NS_OS_PLATFORM);
        add_fn!("__RTS_FN_NS_OS_ARCH", info::__RTS_FN_NS_OS_ARCH);
        add_fn!("__RTS_FN_NS_OS_FAMILY", info::__RTS_FN_NS_OS_FAMILY);
        add_fn!("__RTS_FN_NS_OS_EOL", info::__RTS_FN_NS_OS_EOL);
        add_fn!("__RTS_FN_NS_OS_HOME_DIR", dirs::__RTS_FN_NS_OS_HOME_DIR);
        add_fn!("__RTS_FN_NS_OS_TEMP_DIR", dirs::__RTS_FN_NS_OS_TEMP_DIR);
        add_fn!("__RTS_FN_NS_OS_CONFIG_DIR", dirs::__RTS_FN_NS_OS_CONFIG_DIR);
        add_fn!("__RTS_FN_NS_OS_CACHE_DIR", dirs::__RTS_FN_NS_OS_CACHE_DIR);
    }

    // ── namespaces::process ───────────────────────────────────────────
    {
        use crate::namespaces::process::*;
        add_fn!("__RTS_FN_NS_PROCESS_EXIT", exit::__RTS_FN_NS_PROCESS_EXIT);
        add_fn!("__RTS_FN_NS_PROCESS_ABORT", exit::__RTS_FN_NS_PROCESS_ABORT);
        add_fn!("__RTS_FN_NS_PROCESS_PID", info::__RTS_FN_NS_PROCESS_PID);
        add_fn!("__RTS_FN_NS_PROCESS_ARGS_COUNT", info::__RTS_FN_NS_PROCESS_ARGS_COUNT);
        add_fn!("__RTS_FN_NS_PROCESS_ARG_AT", info::__RTS_FN_NS_PROCESS_ARG_AT);
        add_fn!("__RTS_FN_NS_PROCESS_SPAWN", spawn::__RTS_FN_NS_PROCESS_SPAWN);
        add_fn!("__RTS_FN_NS_PROCESS_WAIT", spawn::__RTS_FN_NS_PROCESS_WAIT);
        add_fn!("__RTS_FN_NS_PROCESS_KILL", spawn::__RTS_FN_NS_PROCESS_KILL);
    }

    // ── namespaces::string ────────────────────────────────────────────
    use crate::namespaces::string::*;
    add_fn!("__RTS_FN_NS_STRING_CONTAINS", search::__RTS_FN_NS_STRING_CONTAINS);
    add_fn!("__RTS_FN_NS_STRING_STARTS_WITH", search::__RTS_FN_NS_STRING_STARTS_WITH);
    add_fn!("__RTS_FN_NS_STRING_ENDS_WITH", search::__RTS_FN_NS_STRING_ENDS_WITH);
    add_fn!("__RTS_FN_NS_STRING_FIND", search::__RTS_FN_NS_STRING_FIND);
    add_fn!("__RTS_FN_NS_STRING_TO_UPPER", transform::__RTS_FN_NS_STRING_TO_UPPER);
    add_fn!("__RTS_FN_NS_STRING_TO_LOWER", transform::__RTS_FN_NS_STRING_TO_LOWER);
    add_fn!("__RTS_FN_NS_STRING_TRIM", transform::__RTS_FN_NS_STRING_TRIM);
    add_fn!("__RTS_FN_NS_STRING_TRIM_START", transform::__RTS_FN_NS_STRING_TRIM_START);
    add_fn!("__RTS_FN_NS_STRING_TRIM_END", transform::__RTS_FN_NS_STRING_TRIM_END);
    add_fn!("__RTS_FN_NS_STRING_REPEAT", transform::__RTS_FN_NS_STRING_REPEAT);
    add_fn!("__RTS_FN_NS_STRING_REPLACE", replace::__RTS_FN_NS_STRING_REPLACE);
    add_fn!("__RTS_FN_NS_STRING_REPLACEN", replace::__RTS_FN_NS_STRING_REPLACEN);
    add_fn!("__RTS_FN_NS_STRING_CHAR_COUNT", split::__RTS_FN_NS_STRING_CHAR_COUNT);
    add_fn!("__RTS_FN_NS_STRING_BYTE_LEN", split::__RTS_FN_NS_STRING_BYTE_LEN);
    add_fn!("__RTS_FN_NS_STRING_CHAR_AT", split::__RTS_FN_NS_STRING_CHAR_AT);
    add_fn!("__RTS_FN_NS_STRING_CHAR_CODE_AT", split::__RTS_FN_NS_STRING_CHAR_CODE_AT);

    // ── namespaces::buffer ────────────────────────────────────────────
    use crate::namespaces::buffer::ops as buf;
    add_fn!("__RTS_FN_NS_BUFFER_ALLOC", buf::__RTS_FN_NS_BUFFER_ALLOC);
    add_fn!("__RTS_FN_NS_BUFFER_ALLOC_ZEROED", buf::__RTS_FN_NS_BUFFER_ALLOC_ZEROED);
    add_fn!("__RTS_FN_NS_BUFFER_FREE", buf::__RTS_FN_NS_BUFFER_FREE);
    add_fn!("__RTS_FN_NS_BUFFER_LEN", buf::__RTS_FN_NS_BUFFER_LEN);
    add_fn!("__RTS_FN_NS_BUFFER_PTR", buf::__RTS_FN_NS_BUFFER_PTR);
    add_fn!("__RTS_FN_NS_BUFFER_READ_U8", buf::__RTS_FN_NS_BUFFER_READ_U8);
    add_fn!("__RTS_FN_NS_BUFFER_READ_I32", buf::__RTS_FN_NS_BUFFER_READ_I32);
    add_fn!("__RTS_FN_NS_BUFFER_READ_I64", buf::__RTS_FN_NS_BUFFER_READ_I64);
    add_fn!("__RTS_FN_NS_BUFFER_READ_F64", buf::__RTS_FN_NS_BUFFER_READ_F64);
    add_fn!("__RTS_FN_NS_BUFFER_WRITE_U8", buf::__RTS_FN_NS_BUFFER_WRITE_U8);
    add_fn!("__RTS_FN_NS_BUFFER_WRITE_I32", buf::__RTS_FN_NS_BUFFER_WRITE_I32);
    add_fn!("__RTS_FN_NS_BUFFER_WRITE_I64", buf::__RTS_FN_NS_BUFFER_WRITE_I64);
    add_fn!("__RTS_FN_NS_BUFFER_WRITE_F64", buf::__RTS_FN_NS_BUFFER_WRITE_F64);
    add_fn!("__RTS_FN_NS_BUFFER_COPY", buf::__RTS_FN_NS_BUFFER_COPY);
    add_fn!("__RTS_FN_NS_BUFFER_FILL", buf::__RTS_FN_NS_BUFFER_FILL);
    add_fn!("__RTS_FN_NS_BUFFER_TO_STRING", buf::__RTS_FN_NS_BUFFER_TO_STRING);

    // ── namespaces::path ──────────────────────────────────────────────
    use crate::namespaces::path::*;
    add_fn!("__RTS_FN_NS_PATH_JOIN", join::__RTS_FN_NS_PATH_JOIN);
    add_fn!("__RTS_FN_NS_PATH_PARENT", components::__RTS_FN_NS_PATH_PARENT);
    add_fn!("__RTS_FN_NS_PATH_FILE_NAME", components::__RTS_FN_NS_PATH_FILE_NAME);
    add_fn!("__RTS_FN_NS_PATH_STEM", components::__RTS_FN_NS_PATH_STEM);
    add_fn!("__RTS_FN_NS_PATH_EXT", components::__RTS_FN_NS_PATH_EXT);
    add_fn!("__RTS_FN_NS_PATH_IS_ABSOLUTE", join::__RTS_FN_NS_PATH_IS_ABSOLUTE);
    add_fn!("__RTS_FN_NS_PATH_NORMALIZE", join::__RTS_FN_NS_PATH_NORMALIZE);
    add_fn!("__RTS_FN_NS_PATH_WITH_EXT", join::__RTS_FN_NS_PATH_WITH_EXT);

    // ── namespaces::env ───────────────────────────────────────────────
    use crate::namespaces::env::*;
    add_fn!("__RTS_FN_NS_ENV_GET_VAR", vars::__RTS_FN_NS_ENV_GET_VAR);
    add_fn!("__RTS_FN_NS_ENV_SET_VAR", vars::__RTS_FN_NS_ENV_SET_VAR);
    add_fn!("__RTS_FN_NS_ENV_REMOVE_VAR", vars::__RTS_FN_NS_ENV_REMOVE_VAR);
    add_fn!("__RTS_FN_NS_ENV_ARGS_COUNT", args::__RTS_FN_NS_ENV_ARGS_COUNT);
    add_fn!("__RTS_FN_NS_ENV_ARG_AT", args::__RTS_FN_NS_ENV_ARG_AT);
    add_fn!("__RTS_FN_NS_ENV_CWD", cwd::__RTS_FN_NS_ENV_CWD);
    add_fn!("__RTS_FN_NS_ENV_SET_CWD", cwd::__RTS_FN_NS_ENV_SET_CWD);

    // ── namespaces::time ──────────────────────────────────────────────
    use crate::namespaces::time::*;
    add_fn!("__RTS_FN_NS_TIME_NOW_MS", instant::__RTS_FN_NS_TIME_NOW_MS);
    add_fn!("__RTS_FN_NS_TIME_NOW_NS", instant::__RTS_FN_NS_TIME_NOW_NS);
    add_fn!("__RTS_FN_NS_TIME_UNIX_MS", system::__RTS_FN_NS_TIME_UNIX_MS);
    add_fn!("__RTS_FN_NS_TIME_UNIX_NS", system::__RTS_FN_NS_TIME_UNIX_NS);
    add_fn!("__RTS_FN_NS_TIME_SLEEP_MS", sleep::__RTS_FN_NS_TIME_SLEEP_MS);
    add_fn!("__RTS_FN_NS_TIME_SLEEP_NS", sleep::__RTS_FN_NS_TIME_SLEEP_NS);

    // ── namespaces::bigfloat ──────────────────────────────────────────
    use crate::namespaces::bigfloat::ops::*;
    add_fn!("__RTS_FN_NS_BIGFLOAT_ZERO", __RTS_FN_NS_BIGFLOAT_ZERO);
    add_fn!("__RTS_FN_NS_BIGFLOAT_FROM_F64", __RTS_FN_NS_BIGFLOAT_FROM_F64);
    add_fn!("__RTS_FN_NS_BIGFLOAT_FROM_I64", __RTS_FN_NS_BIGFLOAT_FROM_I64);
    add_fn!("__RTS_FN_NS_BIGFLOAT_FROM_STR", __RTS_FN_NS_BIGFLOAT_FROM_STR);
    add_fn!("__RTS_FN_NS_BIGFLOAT_TO_F64", __RTS_FN_NS_BIGFLOAT_TO_F64);
    add_fn!("__RTS_FN_NS_BIGFLOAT_TO_STRING", __RTS_FN_NS_BIGFLOAT_TO_STRING);
    add_fn!("__RTS_FN_NS_BIGFLOAT_ADD", __RTS_FN_NS_BIGFLOAT_ADD);
    add_fn!("__RTS_FN_NS_BIGFLOAT_SUB", __RTS_FN_NS_BIGFLOAT_SUB);
    add_fn!("__RTS_FN_NS_BIGFLOAT_MUL", __RTS_FN_NS_BIGFLOAT_MUL);
    add_fn!("__RTS_FN_NS_BIGFLOAT_DIV", __RTS_FN_NS_BIGFLOAT_DIV);
    add_fn!("__RTS_FN_NS_BIGFLOAT_NEG", __RTS_FN_NS_BIGFLOAT_NEG);
    add_fn!("__RTS_FN_NS_BIGFLOAT_SQRT", __RTS_FN_NS_BIGFLOAT_SQRT);
    add_fn!("__RTS_FN_NS_BIGFLOAT_FREE", __RTS_FN_NS_BIGFLOAT_FREE);

    // ── Data symbols ──────────────────────────────────────────────────
    // Xorshift PRNG state (mutable u64 global).
    {
        let ptr = &raw const crate::namespaces::math::random::__RTS_DATA_NS_MATH_RNG_STATE
            as *const u8;
        out.push(("__RTS_DATA_NS_MATH_RNG_STATE", ptr));
    }

    // ── Libc ──────────────────────────────────────────────────────────
    // `fmod` is declared as an extern import for `BinaryOp::Mod` on f64.
    unsafe extern "C" {
        fn fmod(a: f64, b: f64) -> f64;
    }
    add_fn!("fmod", fmod);

    // Sanity: assert the number of function entries matches the ABI
    // surface so we catch omissions early. Data/libc entries push the
    // total above SPECS len; keep the strict check only over function
    // members.
    let expected_fn_count: usize = SPECS.iter().map(|s| s.members.len()).sum();
    debug_assert!(
        out.iter()
            .filter(|(name, _)| name.starts_with("__RTS_FN_NS_"))
            .count()
            == expected_fn_count,
        "runtime_symbol_table missing entries vs abi::SPECS"
    );

    out
}
