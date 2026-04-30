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

use crate::codegen::lower::compile_program;
use crate::parser::ast::Program;

/// Compiles a program into a JIT module and returns an owned `JITModule`
/// plus the FuncId for `__RTS_MAIN`. Caller invokes
/// `module.get_finalized_function(id)` to obtain the pointer to execute.
pub fn compile_program_to_jit(program: &mut Program) -> Result<(JITModule, Vec<String>)> {
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
        .set("opt_level", crate::compile_options::opt_level())
        .map_err(|e| anyhow!("cranelift flag error: {e}"))?;
    let _ = flag_builder.set("use_egraphs", "true");
    let _ = flag_builder.set("enable_alias_analysis", "true");
    let _ = flag_builder.set("enable_jump_tables", "true");
    flag_builder
        .set("preserve_frame_pointers", "true")
        .map_err(|e| anyhow!("cranelift flag error: {e}"))?;
    let flags = settings::Flags::new(flag_builder);

    let isa_builder =
        cranelift_native::builder().map_err(|e| anyhow!("failed to detect native target: {e}"))?;
    let isa = isa_builder
        .finish(flags)
        .map_err(|e| anyhow!("failed to finalise ISA: {e}"))?;

    let mut jit_builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());

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

    // ── runtime error slot (used by try/catch/throw in codegen) ──────
    {
        use crate::namespaces::gc::error::*;
        add_fn!("__RTS_FN_RT_ERROR_SET", __RTS_FN_RT_ERROR_SET);
        add_fn!("__RTS_FN_RT_ERROR_GET", __RTS_FN_RT_ERROR_GET);
        add_fn!("__RTS_FN_RT_ERROR_GET_STACK", __RTS_FN_RT_ERROR_GET_STACK);
        add_fn!("__RTS_FN_RT_ERROR_CLEAR", __RTS_FN_RT_ERROR_CLEAR);
    }

    // ── runtime stack depth limit ─────────────────────────────────────
    {
        use crate::namespaces::gc::stack::*;
        add_fn!("__RTS_FN_RT_STACK_PUSH", __RTS_FN_RT_STACK_PUSH);
        add_fn!("__RTS_FN_RT_STACK_POP", __RTS_FN_RT_STACK_POP);
        add_fn!("__RTS_FN_RT_STACK_DEPTH", __RTS_FN_RT_STACK_DEPTH);
    }

    // ── namespaces::gc ────────────────────────────────────────────────
    use crate::namespaces::gc::string_pool::*;
    add_fn!("__RTS_FN_NS_GC_STRING_NEW", __RTS_FN_NS_GC_STRING_NEW);
    add_fn!("__RTS_FN_NS_GC_STRING_LEN", __RTS_FN_NS_GC_STRING_LEN);
    add_fn!("__RTS_FN_NS_GC_STRING_PTR", __RTS_FN_NS_GC_STRING_PTR);
    add_fn!("__RTS_FN_NS_GC_STRING_FREE", __RTS_FN_NS_GC_STRING_FREE);
    add_fn!("__RTS_FN_NS_GC_HANDLE_LEN", __RTS_FN_NS_GC_HANDLE_LEN);
    add_fn!(
        "__RTS_FN_NS_GC_STRING_FROM_I64",
        __RTS_FN_NS_GC_STRING_FROM_I64
    );
    add_fn!(
        "__RTS_FN_NS_GC_STRING_FROM_F64",
        __RTS_FN_NS_GC_STRING_FROM_F64
    );
    add_fn!("__RTS_FN_NS_GC_STRING_CONCAT", __RTS_FN_NS_GC_STRING_CONCAT);
    add_fn!(
        "__RTS_FN_NS_GC_STRING_FROM_STATIC",
        __RTS_FN_NS_GC_STRING_FROM_STATIC
    );
    add_fn!("__RTS_FN_NS_GC_STRING_EQ", __RTS_FN_NS_GC_STRING_EQ);
    use crate::namespaces::gc::env::*;
    add_fn!("__RTS_FN_NS_GC_ENV_ALLOC", __RTS_FN_NS_GC_ENV_ALLOC);
    add_fn!("__RTS_FN_NS_GC_ENV_GET", __RTS_FN_NS_GC_ENV_GET);
    add_fn!("__RTS_FN_NS_GC_ENV_SET", __RTS_FN_NS_GC_ENV_SET);
    add_fn!("__RTS_FN_NS_GC_ENV_FREE", __RTS_FN_NS_GC_ENV_FREE);
    use crate::namespaces::gc::instance::*;
    add_fn!("__RTS_FN_NS_GC_INSTANCE_NEW", __RTS_FN_NS_GC_INSTANCE_NEW);
    add_fn!(
        "__RTS_FN_NS_GC_INSTANCE_CLASS",
        __RTS_FN_NS_GC_INSTANCE_CLASS
    );
    add_fn!("__RTS_FN_NS_GC_INSTANCE_FREE", __RTS_FN_NS_GC_INSTANCE_FREE);
    add_fn!(
        "__RTS_FN_NS_GC_INSTANCE_LOAD_I64",
        __RTS_FN_NS_GC_INSTANCE_LOAD_I64
    );
    add_fn!(
        "__RTS_FN_NS_GC_INSTANCE_STORE_I64",
        __RTS_FN_NS_GC_INSTANCE_STORE_I64
    );
    add_fn!(
        "__RTS_FN_NS_GC_INSTANCE_LOAD_I32",
        __RTS_FN_NS_GC_INSTANCE_LOAD_I32
    );
    add_fn!(
        "__RTS_FN_NS_GC_INSTANCE_STORE_I32",
        __RTS_FN_NS_GC_INSTANCE_STORE_I32
    );
    add_fn!(
        "__RTS_FN_NS_GC_INSTANCE_LOAD_F64",
        __RTS_FN_NS_GC_INSTANCE_LOAD_F64
    );
    add_fn!(
        "__RTS_FN_NS_GC_INSTANCE_STORE_F64",
        __RTS_FN_NS_GC_INSTANCE_STORE_F64
    );

    // ── gc collector (mark+sweep manual) ──────────────────────────────
    use crate::namespaces::gc::collector::*;
    add_fn!("__RTS_FN_NS_GC_COLLECT", __RTS_FN_NS_GC_COLLECT);
    add_fn!("__RTS_FN_NS_GC_COLLECT_VEC", __RTS_FN_NS_GC_COLLECT_VEC);
    add_fn!("__RTS_FN_NS_GC_LIVE_COUNT", __RTS_FN_NS_GC_LIVE_COUNT);

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
    add_fn!(
        "__RTS_FN_NS_IO_STDIN_READ_LINE",
        __RTS_FN_NS_IO_STDIN_READ_LINE
    );

    // ── namespaces::json ──────────────────────────────────────────────
    use crate::namespaces::json::ops::*;
    add_fn!("__RTS_FN_NS_JSON_PARSE", __RTS_FN_NS_JSON_PARSE);
    add_fn!("__RTS_FN_NS_JSON_STRINGIFY", __RTS_FN_NS_JSON_STRINGIFY);
    add_fn!(
        "__RTS_FN_NS_JSON_STRINGIFY_PRETTY",
        __RTS_FN_NS_JSON_STRINGIFY_PRETTY
    );
    add_fn!("__RTS_FN_NS_JSON_FREE", __RTS_FN_NS_JSON_FREE);
    add_fn!("__RTS_FN_NS_JSON_TYPE_OF", __RTS_FN_NS_JSON_TYPE_OF);
    add_fn!("__RTS_FN_NS_JSON_AS_BOOL", __RTS_FN_NS_JSON_AS_BOOL);
    add_fn!("__RTS_FN_NS_JSON_AS_I64", __RTS_FN_NS_JSON_AS_I64);
    add_fn!("__RTS_FN_NS_JSON_AS_F64", __RTS_FN_NS_JSON_AS_F64);
    add_fn!("__RTS_FN_NS_JSON_AS_STRING", __RTS_FN_NS_JSON_AS_STRING);
    add_fn!("__RTS_FN_NS_JSON_ARRAY_LEN", __RTS_FN_NS_JSON_ARRAY_LEN);
    add_fn!("__RTS_FN_NS_JSON_ARRAY_GET", __RTS_FN_NS_JSON_ARRAY_GET);
    add_fn!("__RTS_FN_NS_JSON_OBJECT_GET", __RTS_FN_NS_JSON_OBJECT_GET);
    add_fn!("__RTS_FN_NS_JSON_OBJECT_HAS", __RTS_FN_NS_JSON_OBJECT_HAS);

    // ── namespaces::globals::events (EventEmitter global class) ──────
    use crate::namespaces::globals::events::instance::*;
    add_fn!("__RTS_FN_GL_EE_NEW", __RTS_FN_GL_EE_NEW);
    add_fn!("__RTS_FN_GL_EE_NEW_ASYNC", __RTS_FN_GL_EE_NEW_ASYNC);
    add_fn!("__RTS_FN_GL_EE_ON", __RTS_FN_GL_EE_ON);
    add_fn!("__RTS_FN_GL_EE_ONCE", __RTS_FN_GL_EE_ONCE);
    add_fn!("__RTS_FN_GL_EE_OFF", __RTS_FN_GL_EE_OFF);
    add_fn!("__RTS_FN_GL_EE_EMIT", __RTS_FN_GL_EE_EMIT);
    add_fn!("__RTS_FN_GL_EE_EMIT_HANDLE", __RTS_FN_GL_EE_EMIT_HANDLE);
    add_fn!("__RTS_FN_GL_EE_REMOVE_ALL", __RTS_FN_GL_EE_REMOVE_ALL);
    add_fn!("__RTS_FN_GL_EE_LISTENER_COUNT", __RTS_FN_GL_EE_LISTENER_COUNT);
    add_fn!("__RTS_FN_GL_EE_EVENT_NAMES", __RTS_FN_GL_EE_EVENT_NAMES);

    // ── namespaces::globals::regexp (RegExp global class) ────────────
    use crate::namespaces::globals::regexp::instance::*;
    add_fn!("__RTS_FN_GL_REGEXP_NEW", __RTS_FN_GL_REGEXP_NEW);
    add_fn!("__RTS_FN_GL_REGEXP_NEW_WITH_FLAGS", __RTS_FN_GL_REGEXP_NEW_WITH_FLAGS);
    add_fn!("__RTS_FN_GL_REGEXP_TEST", __RTS_FN_GL_REGEXP_TEST);
    add_fn!("__RTS_FN_GL_REGEXP_EXEC", __RTS_FN_GL_REGEXP_EXEC);
    add_fn!("__RTS_FN_GL_REGEXP_SOURCE", __RTS_FN_GL_REGEXP_SOURCE);

    // ── namespaces::globals::error (Error class family) ───────────────
    use crate::namespaces::globals::error::instance::*;
    add_fn!("__RTS_FN_GL_ERROR_NEW", __RTS_FN_GL_ERROR_NEW);
    add_fn!("__RTS_FN_GL_TYPE_ERROR_NEW", __RTS_FN_GL_TYPE_ERROR_NEW);
    add_fn!("__RTS_FN_GL_RANGE_ERROR_NEW", __RTS_FN_GL_RANGE_ERROR_NEW);
    add_fn!("__RTS_FN_GL_REF_ERROR_NEW", __RTS_FN_GL_REF_ERROR_NEW);
    add_fn!("__RTS_FN_GL_SYNTAX_ERROR_NEW", __RTS_FN_GL_SYNTAX_ERROR_NEW);
    add_fn!("__RTS_FN_GL_ERROR_MESSAGE", __RTS_FN_GL_ERROR_MESSAGE);
    add_fn!("__RTS_FN_GL_ERROR_NAME", __RTS_FN_GL_ERROR_NAME);
    add_fn!("__RTS_FN_GL_ERROR_TO_STRING", __RTS_FN_GL_ERROR_TO_STRING);

    // ── namespaces::globals::date (Date global class) ─────────────────
    use crate::namespaces::globals::date::instance::*;
    add_fn!("__RTS_FN_GL_DATE_NEW_NOW", __RTS_FN_GL_DATE_NEW_NOW);
    add_fn!("__RTS_FN_GL_DATE_NEW_FROM_MS", __RTS_FN_GL_DATE_NEW_FROM_MS);
    add_fn!("__RTS_FN_GL_DATE_NEW_FROM_ISO", __RTS_FN_GL_DATE_NEW_FROM_ISO);
    add_fn!("__RTS_FN_GL_DATE_GET_TIME", __RTS_FN_GL_DATE_GET_TIME);
    add_fn!("__RTS_FN_GL_DATE_VALUE_OF", __RTS_FN_GL_DATE_VALUE_OF);
    add_fn!("__RTS_FN_GL_DATE_GET_FULL_YEAR", __RTS_FN_GL_DATE_GET_FULL_YEAR);
    add_fn!("__RTS_FN_GL_DATE_GET_MONTH", __RTS_FN_GL_DATE_GET_MONTH);
    add_fn!("__RTS_FN_GL_DATE_GET_DATE", __RTS_FN_GL_DATE_GET_DATE);
    add_fn!("__RTS_FN_GL_DATE_GET_DAY", __RTS_FN_GL_DATE_GET_DAY);
    add_fn!("__RTS_FN_GL_DATE_GET_HOURS", __RTS_FN_GL_DATE_GET_HOURS);
    add_fn!("__RTS_FN_GL_DATE_GET_MINUTES", __RTS_FN_GL_DATE_GET_MINUTES);
    add_fn!("__RTS_FN_GL_DATE_GET_SECONDS", __RTS_FN_GL_DATE_GET_SECONDS);
    add_fn!("__RTS_FN_GL_DATE_GET_MILLISECONDS", __RTS_FN_GL_DATE_GET_MILLISECONDS);
    add_fn!("__RTS_FN_GL_DATE_TO_ISO_STRING", __RTS_FN_GL_DATE_TO_ISO_STRING);
    add_fn!("__RTS_FN_GL_DATE_TO_STRING", __RTS_FN_GL_DATE_TO_STRING);
    add_fn!("__RTS_FN_GL_DATE_TO_LOCALE_DATE_STRING", __RTS_FN_GL_DATE_TO_LOCALE_DATE_STRING);

    // ── namespaces::date ──────────────────────────────────────────────
    use crate::namespaces::date::ops::*;
    add_fn!("__RTS_FN_NS_DATE_NOW_MS", __RTS_FN_NS_DATE_NOW_MS);
    add_fn!("__RTS_FN_NS_DATE_FROM_ISO", __RTS_FN_NS_DATE_FROM_ISO);
    add_fn!("__RTS_FN_NS_DATE_FROM_PARTS", __RTS_FN_NS_DATE_FROM_PARTS);
    add_fn!("__RTS_FN_NS_DATE_YEAR", __RTS_FN_NS_DATE_YEAR);
    add_fn!("__RTS_FN_NS_DATE_MONTH", __RTS_FN_NS_DATE_MONTH);
    add_fn!("__RTS_FN_NS_DATE_DAY", __RTS_FN_NS_DATE_DAY);
    add_fn!("__RTS_FN_NS_DATE_WEEKDAY", __RTS_FN_NS_DATE_WEEKDAY);
    add_fn!("__RTS_FN_NS_DATE_HOUR", __RTS_FN_NS_DATE_HOUR);
    add_fn!("__RTS_FN_NS_DATE_MINUTE", __RTS_FN_NS_DATE_MINUTE);
    add_fn!("__RTS_FN_NS_DATE_SECOND", __RTS_FN_NS_DATE_SECOND);
    add_fn!("__RTS_FN_NS_DATE_MILLISECOND", __RTS_FN_NS_DATE_MILLISECOND);
    add_fn!("__RTS_FN_NS_DATE_TO_ISO", __RTS_FN_NS_DATE_TO_ISO);

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
    add_fn!(
        "__RTS_FN_NS_FS_MODIFIED_MS",
        metadata::__RTS_FN_NS_FS_MODIFIED_MS
    );
    add_fn!("__RTS_FN_NS_FS_CREATE_DIR", dir::__RTS_FN_NS_FS_CREATE_DIR);
    add_fn!(
        "__RTS_FN_NS_FS_CREATE_DIR_ALL",
        dir::__RTS_FN_NS_FS_CREATE_DIR_ALL
    );
    add_fn!("__RTS_FN_NS_FS_REMOVE_DIR", dir::__RTS_FN_NS_FS_REMOVE_DIR);
    add_fn!(
        "__RTS_FN_NS_FS_REMOVE_DIR_ALL",
        dir::__RTS_FN_NS_FS_REMOVE_DIR_ALL
    );
    add_fn!(
        "__RTS_FN_NS_FS_REMOVE_FILE",
        ops::__RTS_FN_NS_FS_REMOVE_FILE
    );
    add_fn!("__RTS_FN_NS_FS_RENAME", ops::__RTS_FN_NS_FS_RENAME);
    add_fn!("__RTS_FN_NS_FS_COPY", ops::__RTS_FN_NS_FS_COPY);
    add_fn!("__RTS_FN_NS_FS_READDIR", dir::__RTS_FN_NS_FS_READDIR);

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
    add_fn!(
        "__RTS_FN_NS_MATH_CLAMP_F64",
        minmax::__RTS_FN_NS_MATH_CLAMP_F64
    );
    add_fn!(
        "__RTS_FN_NS_MATH_CLAMP_I64",
        minmax::__RTS_FN_NS_MATH_CLAMP_I64
    );
    add_fn!(
        "__RTS_FN_NS_MATH_RANDOM_F64",
        random::__RTS_FN_NS_MATH_RANDOM_F64
    );
    add_fn!(
        "__RTS_FN_NS_MATH_RANDOM_I64_RANGE",
        random::__RTS_FN_NS_MATH_RANDOM_I64_RANGE
    );
    add_fn!("__RTS_FN_NS_MATH_SEED", random::__RTS_FN_NS_MATH_SEED);
    add_fn!("__RTS_FN_NS_MATH_PI", consts::__RTS_FN_NS_MATH_PI);
    add_fn!("__RTS_FN_NS_MATH_E", consts::__RTS_FN_NS_MATH_E);
    add_fn!(
        "__RTS_FN_NS_MATH_INFINITY",
        consts::__RTS_FN_NS_MATH_INFINITY
    );
    add_fn!("__RTS_FN_NS_MATH_NAN", consts::__RTS_FN_NS_MATH_NAN);

    // ── namespaces::num ───────────────────────────────────────────────
    {
        use crate::namespaces::num::ops as n;
        add_fn!("__RTS_FN_NS_NUM_CHECKED_ADD", n::__RTS_FN_NS_NUM_CHECKED_ADD);
        add_fn!("__RTS_FN_NS_NUM_CHECKED_SUB", n::__RTS_FN_NS_NUM_CHECKED_SUB);
        add_fn!("__RTS_FN_NS_NUM_CHECKED_MUL", n::__RTS_FN_NS_NUM_CHECKED_MUL);
        add_fn!("__RTS_FN_NS_NUM_CHECKED_DIV", n::__RTS_FN_NS_NUM_CHECKED_DIV);
        add_fn!(
            "__RTS_FN_NS_NUM_SATURATING_ADD",
            n::__RTS_FN_NS_NUM_SATURATING_ADD
        );
        add_fn!(
            "__RTS_FN_NS_NUM_SATURATING_SUB",
            n::__RTS_FN_NS_NUM_SATURATING_SUB
        );
        add_fn!(
            "__RTS_FN_NS_NUM_SATURATING_MUL",
            n::__RTS_FN_NS_NUM_SATURATING_MUL
        );
        add_fn!("__RTS_FN_NS_NUM_WRAPPING_ADD", n::__RTS_FN_NS_NUM_WRAPPING_ADD);
        add_fn!("__RTS_FN_NS_NUM_WRAPPING_SUB", n::__RTS_FN_NS_NUM_WRAPPING_SUB);
        add_fn!("__RTS_FN_NS_NUM_WRAPPING_MUL", n::__RTS_FN_NS_NUM_WRAPPING_MUL);
        add_fn!("__RTS_FN_NS_NUM_WRAPPING_NEG", n::__RTS_FN_NS_NUM_WRAPPING_NEG);
        add_fn!("__RTS_FN_NS_NUM_WRAPPING_SHL", n::__RTS_FN_NS_NUM_WRAPPING_SHL);
        add_fn!("__RTS_FN_NS_NUM_WRAPPING_SHR", n::__RTS_FN_NS_NUM_WRAPPING_SHR);
        add_fn!("__RTS_FN_NS_NUM_COUNT_ONES", n::__RTS_FN_NS_NUM_COUNT_ONES);
        add_fn!("__RTS_FN_NS_NUM_COUNT_ZEROS", n::__RTS_FN_NS_NUM_COUNT_ZEROS);
        add_fn!(
            "__RTS_FN_NS_NUM_LEADING_ZEROS",
            n::__RTS_FN_NS_NUM_LEADING_ZEROS
        );
        add_fn!(
            "__RTS_FN_NS_NUM_TRAILING_ZEROS",
            n::__RTS_FN_NS_NUM_TRAILING_ZEROS
        );
        add_fn!("__RTS_FN_NS_NUM_ROTATE_LEFT", n::__RTS_FN_NS_NUM_ROTATE_LEFT);
        add_fn!(
            "__RTS_FN_NS_NUM_ROTATE_RIGHT",
            n::__RTS_FN_NS_NUM_ROTATE_RIGHT
        );
        add_fn!("__RTS_FN_NS_NUM_REVERSE_BITS", n::__RTS_FN_NS_NUM_REVERSE_BITS);
        add_fn!("__RTS_FN_NS_NUM_SWAP_BYTES", n::__RTS_FN_NS_NUM_SWAP_BYTES);
        add_fn!("__RTS_FN_NS_NUM_F64_FROM_BITS", n::__RTS_FN_NS_NUM_F64_FROM_BITS);
        add_fn!("__RTS_FN_NS_NUM_F64_TO_BITS", n::__RTS_FN_NS_NUM_F64_TO_BITS);
    }

    // ── namespaces::mem ───────────────────────────────────────────────
    {
        use crate::namespaces::mem::ops as m;
        add_fn!("__RTS_FN_NS_MEM_SIZE_OF_I64", m::__RTS_FN_NS_MEM_SIZE_OF_I64);
        add_fn!("__RTS_FN_NS_MEM_SIZE_OF_F64", m::__RTS_FN_NS_MEM_SIZE_OF_F64);
        add_fn!("__RTS_FN_NS_MEM_SIZE_OF_I32", m::__RTS_FN_NS_MEM_SIZE_OF_I32);
        add_fn!(
            "__RTS_FN_NS_MEM_SIZE_OF_BOOL",
            m::__RTS_FN_NS_MEM_SIZE_OF_BOOL
        );
        add_fn!(
            "__RTS_FN_NS_MEM_ALIGN_OF_I64",
            m::__RTS_FN_NS_MEM_ALIGN_OF_I64
        );
        add_fn!(
            "__RTS_FN_NS_MEM_ALIGN_OF_F64",
            m::__RTS_FN_NS_MEM_ALIGN_OF_F64
        );
        add_fn!("__RTS_FN_NS_MEM_SWAP_I64", m::__RTS_FN_NS_MEM_SWAP_I64);
        add_fn!("__RTS_FN_NS_MEM_DROP_HANDLE", m::__RTS_FN_NS_MEM_DROP_HANDLE);
        add_fn!(
            "__RTS_FN_NS_MEM_FORGET_HANDLE",
            m::__RTS_FN_NS_MEM_FORGET_HANDLE
        );
        add_fn!("__RTS_FN_NS_MEM_REPLACE_I64", m::__RTS_FN_NS_MEM_REPLACE_I64);
    }

    // ── namespaces::trace ─────────────────────────────────────────────
    {
        use crate::namespaces::trace::ops as tr;
        add_fn!(
            "__RTS_FN_NS_TRACE_PUSH_FRAME",
            tr::__RTS_FN_NS_TRACE_PUSH_FRAME
        );
        add_fn!("__RTS_FN_NS_TRACE_POP_FRAME", tr::__RTS_FN_NS_TRACE_POP_FRAME);
        add_fn!("__RTS_FN_NS_TRACE_CAPTURE", tr::__RTS_FN_NS_TRACE_CAPTURE);
        add_fn!("__RTS_FN_NS_TRACE_PRINT", tr::__RTS_FN_NS_TRACE_PRINT);
        add_fn!("__RTS_FN_NS_TRACE_DEPTH", tr::__RTS_FN_NS_TRACE_DEPTH);
        add_fn!("__RTS_FN_NS_TRACE_FREE", tr::__RTS_FN_NS_TRACE_FREE);
    }

    // ── namespaces::alloc ─────────────────────────────────────────────
    {
        use crate::namespaces::alloc::ops as a;
        add_fn!("__RTS_FN_NS_ALLOC_ALLOC", a::__RTS_FN_NS_ALLOC_ALLOC);
        add_fn!(
            "__RTS_FN_NS_ALLOC_ALLOC_ZEROED",
            a::__RTS_FN_NS_ALLOC_ALLOC_ZEROED
        );
        add_fn!("__RTS_FN_NS_ALLOC_DEALLOC", a::__RTS_FN_NS_ALLOC_DEALLOC);
        add_fn!("__RTS_FN_NS_ALLOC_REALLOC", a::__RTS_FN_NS_ALLOC_REALLOC);
    }

    // ── namespaces::ptr ───────────────────────────────────────────────
    {
        use crate::namespaces::ptr::ops as p;
        add_fn!("__RTS_FN_NS_PTR_NULL", p::__RTS_FN_NS_PTR_NULL);
        add_fn!("__RTS_FN_NS_PTR_IS_NULL", p::__RTS_FN_NS_PTR_IS_NULL);
        add_fn!("__RTS_FN_NS_PTR_READ_I64", p::__RTS_FN_NS_PTR_READ_I64);
        add_fn!("__RTS_FN_NS_PTR_READ_I32", p::__RTS_FN_NS_PTR_READ_I32);
        add_fn!("__RTS_FN_NS_PTR_READ_U8", p::__RTS_FN_NS_PTR_READ_U8);
        add_fn!("__RTS_FN_NS_PTR_READ_F64", p::__RTS_FN_NS_PTR_READ_F64);
        add_fn!("__RTS_FN_NS_PTR_WRITE_I64", p::__RTS_FN_NS_PTR_WRITE_I64);
        add_fn!("__RTS_FN_NS_PTR_WRITE_I32", p::__RTS_FN_NS_PTR_WRITE_I32);
        add_fn!("__RTS_FN_NS_PTR_WRITE_U8", p::__RTS_FN_NS_PTR_WRITE_U8);
        add_fn!("__RTS_FN_NS_PTR_WRITE_F64", p::__RTS_FN_NS_PTR_WRITE_F64);
        add_fn!("__RTS_FN_NS_PTR_COPY", p::__RTS_FN_NS_PTR_COPY);
        add_fn!(
            "__RTS_FN_NS_PTR_COPY_NONOVERLAPPING",
            p::__RTS_FN_NS_PTR_COPY_NONOVERLAPPING
        );
        add_fn!("__RTS_FN_NS_PTR_WRITE_BYTES", p::__RTS_FN_NS_PTR_WRITE_BYTES);
        add_fn!("__RTS_FN_NS_PTR_OFFSET", p::__RTS_FN_NS_PTR_OFFSET);
    }

    // ── namespaces::crypto ────────────────────────────────────────────
    {
        use crate::namespaces::crypto::*;
        add_fn!(
            "__RTS_FN_NS_CRYPTO_RANDOM_BYTES",
            random::__RTS_FN_NS_CRYPTO_RANDOM_BYTES
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_RANDOM_I64",
            random::__RTS_FN_NS_CRYPTO_RANDOM_I64
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_RANDOM_BUFFER",
            random::__RTS_FN_NS_CRYPTO_RANDOM_BUFFER
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_SHA256_STR",
            hash::__RTS_FN_NS_CRYPTO_SHA256_STR
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_SHA256_BYTES",
            hash::__RTS_FN_NS_CRYPTO_SHA256_BYTES
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_HEX_ENCODE",
            encode::__RTS_FN_NS_CRYPTO_HEX_ENCODE
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_HEX_DECODE",
            encode::__RTS_FN_NS_CRYPTO_HEX_DECODE
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_BASE64_ENCODE",
            encode::__RTS_FN_NS_CRYPTO_BASE64_ENCODE
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_BASE64_DECODE",
            encode::__RTS_FN_NS_CRYPTO_BASE64_DECODE
        );
    }

    // ── namespaces::fmt ───────────────────────────────────────────────
    {
        use crate::namespaces::fmt::*;
        add_fn!(
            "__RTS_FN_NS_FMT_PARSE_I64",
            parse::__RTS_FN_NS_FMT_PARSE_I64
        );
        add_fn!(
            "__RTS_FN_NS_FMT_PARSE_F64",
            parse::__RTS_FN_NS_FMT_PARSE_F64
        );
        add_fn!(
            "__RTS_FN_NS_FMT_PARSE_BOOL",
            parse::__RTS_FN_NS_FMT_PARSE_BOOL
        );
        add_fn!("__RTS_FN_NS_FMT_FMT_I64", format::__RTS_FN_NS_FMT_FMT_I64);
        add_fn!("__RTS_FN_NS_FMT_FMT_F64", format::__RTS_FN_NS_FMT_FMT_F64);
        add_fn!("__RTS_FN_NS_FMT_FMT_BOOL", format::__RTS_FN_NS_FMT_FMT_BOOL);
        add_fn!("__RTS_FN_NS_FMT_FMT_HEX", format::__RTS_FN_NS_FMT_FMT_HEX);
        add_fn!("__RTS_FN_NS_FMT_FMT_BIN", format::__RTS_FN_NS_FMT_FMT_BIN);
        add_fn!("__RTS_FN_NS_FMT_FMT_OCT", format::__RTS_FN_NS_FMT_FMT_OCT);
        add_fn!(
            "__RTS_FN_NS_FMT_FMT_F64_PREC",
            format::__RTS_FN_NS_FMT_FMT_F64_PREC
        );
    }

    // ── namespaces::hash ──────────────────────────────────────────────
    {
        use crate::namespaces::hash::ops as h;
        add_fn!("__RTS_FN_NS_HASH_HASH_STR", h::__RTS_FN_NS_HASH_HASH_STR);
        add_fn!(
            "__RTS_FN_NS_HASH_HASH_BYTES",
            h::__RTS_FN_NS_HASH_HASH_BYTES
        );
        add_fn!("__RTS_FN_NS_HASH_HASH_I64", h::__RTS_FN_NS_HASH_HASH_I64);
        add_fn!(
            "__RTS_FN_NS_HASH_HASH_COMBINE",
            h::__RTS_FN_NS_HASH_HASH_COMBINE
        );
    }

    // ── namespaces::hint ──────────────────────────────────────────────
    {
        use crate::namespaces::hint::ops as ht;
        add_fn!("__RTS_FN_NS_HINT_SPIN_LOOP", ht::__RTS_FN_NS_HINT_SPIN_LOOP);
        add_fn!(
            "__RTS_FN_NS_HINT_BLACK_BOX_I64",
            ht::__RTS_FN_NS_HINT_BLACK_BOX_I64
        );
        add_fn!(
            "__RTS_FN_NS_HINT_BLACK_BOX_F64",
            ht::__RTS_FN_NS_HINT_BLACK_BOX_F64
        );
        add_fn!(
            "__RTS_FN_NS_HINT_UNREACHABLE",
            ht::__RTS_FN_NS_HINT_UNREACHABLE
        );
        add_fn!(
            "__RTS_FN_NS_HINT_ASSERT_UNCHECKED",
            ht::__RTS_FN_NS_HINT_ASSERT_UNCHECKED
        );
    }

    // ── namespaces::regex ─────────────────────────────────────────────
    {
        use crate::namespaces::regex::ops as rx;
        add_fn!("__RTS_FN_NS_REGEX_COMPILE", rx::__RTS_FN_NS_REGEX_COMPILE);
        add_fn!("__RTS_FN_NS_REGEX_FREE", rx::__RTS_FN_NS_REGEX_FREE);
        add_fn!("__RTS_FN_NS_REGEX_TEST", rx::__RTS_FN_NS_REGEX_TEST);
        add_fn!("__RTS_FN_NS_REGEX_FIND", rx::__RTS_FN_NS_REGEX_FIND);
        add_fn!("__RTS_FN_NS_REGEX_FIND_AT", rx::__RTS_FN_NS_REGEX_FIND_AT);
        add_fn!("__RTS_FN_NS_REGEX_REPLACE", rx::__RTS_FN_NS_REGEX_REPLACE);
        add_fn!(
            "__RTS_FN_NS_REGEX_REPLACE_ALL",
            rx::__RTS_FN_NS_REGEX_REPLACE_ALL
        );
        add_fn!(
            "__RTS_FN_NS_REGEX_MATCH_COUNT",
            rx::__RTS_FN_NS_REGEX_MATCH_COUNT
        );
    }

    // ── namespaces::events ────────────────────────────────────────────
    {
        use crate::namespaces::events::ops as ev;
        add_fn!("__RTS_FN_NS_EVENTS_EMITTER_NEW", ev::__RTS_FN_NS_EVENTS_EMITTER_NEW);
        add_fn!("__RTS_FN_NS_EVENTS_EMITTER_FREE", ev::__RTS_FN_NS_EVENTS_EMITTER_FREE);
        add_fn!("__RTS_FN_NS_EVENTS_ON", ev::__RTS_FN_NS_EVENTS_ON);
        add_fn!("__RTS_FN_NS_EVENTS_OFF", ev::__RTS_FN_NS_EVENTS_OFF);
        add_fn!("__RTS_FN_NS_EVENTS_REMOVE_ALL", ev::__RTS_FN_NS_EVENTS_REMOVE_ALL);
        add_fn!("__RTS_FN_NS_EVENTS_LISTENER_COUNT", ev::__RTS_FN_NS_EVENTS_LISTENER_COUNT);
        add_fn!("__RTS_FN_NS_EVENTS_EMIT0", ev::__RTS_FN_NS_EVENTS_EMIT0);
        add_fn!("__RTS_FN_NS_EVENTS_EMIT1", ev::__RTS_FN_NS_EVENTS_EMIT1);
    }

    // ── namespaces::collections ───────────────────────────────────────
    {
        use crate::namespaces::collections::*;
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_NEW",
            map::__RTS_FN_NS_COLLECTIONS_MAP_NEW
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_FREE",
            map::__RTS_FN_NS_COLLECTIONS_MAP_FREE
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_LEN",
            map::__RTS_FN_NS_COLLECTIONS_MAP_LEN
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_HAS",
            map::__RTS_FN_NS_COLLECTIONS_MAP_HAS
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_GET",
            map::__RTS_FN_NS_COLLECTIONS_MAP_GET
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_SET",
            map::__RTS_FN_NS_COLLECTIONS_MAP_SET
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_DELETE",
            map::__RTS_FN_NS_COLLECTIONS_MAP_DELETE
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_CLEAR",
            map::__RTS_FN_NS_COLLECTIONS_MAP_CLEAR
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_CLONE",
            map::__RTS_FN_NS_COLLECTIONS_MAP_CLONE
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_KEY_AT",
            map::__RTS_FN_NS_COLLECTIONS_MAP_KEY_AT
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_KEYS",
            map::__RTS_FN_NS_COLLECTIONS_MAP_KEYS
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_VALUES",
            map::__RTS_FN_NS_COLLECTIONS_MAP_VALUES
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_NEW",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_FREE",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_FREE
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_LEN",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_PUSH",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_POP",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_POP
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_GET",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_GET
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_SET",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_SET
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_CLEAR",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_CLEAR
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_JOIN",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_JOIN
        );
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
        add_fn!(
            "__RTS_FN_NS_PROCESS_ARGS_COUNT",
            info::__RTS_FN_NS_PROCESS_ARGS_COUNT
        );
        add_fn!(
            "__RTS_FN_NS_PROCESS_ARG_AT",
            info::__RTS_FN_NS_PROCESS_ARG_AT
        );
        add_fn!(
            "__RTS_FN_NS_PROCESS_SPAWN",
            spawn::__RTS_FN_NS_PROCESS_SPAWN
        );
        add_fn!("__RTS_FN_NS_PROCESS_WAIT", spawn::__RTS_FN_NS_PROCESS_WAIT);
        add_fn!("__RTS_FN_NS_PROCESS_KILL", spawn::__RTS_FN_NS_PROCESS_KILL);
    }

    // ── namespaces::net ───────────────────────────────────────────────
    {
        use crate::namespaces::net::{addr, tcp, udp};
        add_fn!("__RTS_FN_NS_NET_TCP_LISTEN", tcp::__RTS_FN_NS_NET_TCP_LISTEN);
        add_fn!("__RTS_FN_NS_NET_TCP_ACCEPT", tcp::__RTS_FN_NS_NET_TCP_ACCEPT);
        add_fn!("__RTS_FN_NS_NET_TCP_CONNECT", tcp::__RTS_FN_NS_NET_TCP_CONNECT);
        add_fn!("__RTS_FN_NS_NET_TCP_SEND", tcp::__RTS_FN_NS_NET_TCP_SEND);
        add_fn!("__RTS_FN_NS_NET_TCP_RECV", tcp::__RTS_FN_NS_NET_TCP_RECV);
        add_fn!("__RTS_FN_NS_NET_TCP_LOCAL_ADDR", tcp::__RTS_FN_NS_NET_TCP_LOCAL_ADDR);
        add_fn!("__RTS_FN_NS_NET_TCP_CLOSE", tcp::__RTS_FN_NS_NET_TCP_CLOSE);
        add_fn!("__RTS_FN_NS_NET_UDP_BIND", udp::__RTS_FN_NS_NET_UDP_BIND);
        add_fn!("__RTS_FN_NS_NET_UDP_SEND_TO", udp::__RTS_FN_NS_NET_UDP_SEND_TO);
        add_fn!("__RTS_FN_NS_NET_UDP_RECV_FROM", udp::__RTS_FN_NS_NET_UDP_RECV_FROM);
        add_fn!("__RTS_FN_NS_NET_UDP_LAST_PEER", udp::__RTS_FN_NS_NET_UDP_LAST_PEER);
        add_fn!("__RTS_FN_NS_NET_UDP_LOCAL_ADDR", udp::__RTS_FN_NS_NET_UDP_LOCAL_ADDR);
        add_fn!("__RTS_FN_NS_NET_UDP_CLOSE", udp::__RTS_FN_NS_NET_UDP_CLOSE);
        add_fn!("__RTS_FN_NS_NET_RESOLVE", addr::__RTS_FN_NS_NET_RESOLVE);
    }

    // ── namespaces::tls ───────────────────────────────────────────────
    {
        use crate::namespaces::tls::{client, io};
        add_fn!("__RTS_FN_NS_TLS_CLIENT", client::__RTS_FN_NS_TLS_CLIENT);
        add_fn!("__RTS_FN_NS_TLS_CLOSE", client::__RTS_FN_NS_TLS_CLOSE);
        add_fn!("__RTS_FN_NS_TLS_SEND", io::__RTS_FN_NS_TLS_SEND);
        add_fn!("__RTS_FN_NS_TLS_RECV", io::__RTS_FN_NS_TLS_RECV);
    }

    // ── namespaces::string ────────────────────────────────────────────
    use crate::namespaces::string::*;
    add_fn!(
        "__RTS_FN_NS_STRING_CONTAINS",
        search::__RTS_FN_NS_STRING_CONTAINS
    );
    add_fn!(
        "__RTS_FN_NS_STRING_STARTS_WITH",
        search::__RTS_FN_NS_STRING_STARTS_WITH
    );
    add_fn!(
        "__RTS_FN_NS_STRING_ENDS_WITH",
        search::__RTS_FN_NS_STRING_ENDS_WITH
    );
    add_fn!("__RTS_FN_NS_STRING_FIND", search::__RTS_FN_NS_STRING_FIND);
    add_fn!(
        "__RTS_FN_NS_STRING_TO_UPPER",
        transform::__RTS_FN_NS_STRING_TO_UPPER
    );
    add_fn!(
        "__RTS_FN_NS_STRING_TO_LOWER",
        transform::__RTS_FN_NS_STRING_TO_LOWER
    );
    add_fn!(
        "__RTS_FN_NS_STRING_TRIM",
        transform::__RTS_FN_NS_STRING_TRIM
    );
    add_fn!(
        "__RTS_FN_NS_STRING_TRIM_START",
        transform::__RTS_FN_NS_STRING_TRIM_START
    );
    add_fn!(
        "__RTS_FN_NS_STRING_TRIM_END",
        transform::__RTS_FN_NS_STRING_TRIM_END
    );
    add_fn!(
        "__RTS_FN_NS_STRING_REPEAT",
        transform::__RTS_FN_NS_STRING_REPEAT
    );
    add_fn!(
        "__RTS_FN_NS_STRING_REPLACE",
        replace::__RTS_FN_NS_STRING_REPLACE
    );
    add_fn!(
        "__RTS_FN_NS_STRING_REPLACEN",
        replace::__RTS_FN_NS_STRING_REPLACEN
    );
    add_fn!(
        "__RTS_FN_NS_STRING_CHAR_COUNT",
        split::__RTS_FN_NS_STRING_CHAR_COUNT
    );
    add_fn!(
        "__RTS_FN_NS_STRING_BYTE_LEN",
        split::__RTS_FN_NS_STRING_BYTE_LEN
    );
    add_fn!(
        "__RTS_FN_NS_STRING_CHAR_AT",
        split::__RTS_FN_NS_STRING_CHAR_AT
    );
    add_fn!(
        "__RTS_FN_NS_STRING_CHAR_CODE_AT",
        split::__RTS_FN_NS_STRING_CHAR_CODE_AT
    );

    // ── namespaces::buffer ────────────────────────────────────────────
    use crate::namespaces::buffer::ops as buf;
    add_fn!("__RTS_FN_NS_BUFFER_ALLOC", buf::__RTS_FN_NS_BUFFER_ALLOC);
    add_fn!(
        "__RTS_FN_NS_BUFFER_ALLOC_ZEROED",
        buf::__RTS_FN_NS_BUFFER_ALLOC_ZEROED
    );
    add_fn!("__RTS_FN_NS_BUFFER_FREE", buf::__RTS_FN_NS_BUFFER_FREE);
    add_fn!("__RTS_FN_NS_BUFFER_LEN", buf::__RTS_FN_NS_BUFFER_LEN);
    add_fn!("__RTS_FN_NS_BUFFER_PTR", buf::__RTS_FN_NS_BUFFER_PTR);
    add_fn!(
        "__RTS_FN_NS_BUFFER_READ_U8",
        buf::__RTS_FN_NS_BUFFER_READ_U8
    );
    add_fn!(
        "__RTS_FN_NS_BUFFER_READ_I32",
        buf::__RTS_FN_NS_BUFFER_READ_I32
    );
    add_fn!(
        "__RTS_FN_NS_BUFFER_READ_I64",
        buf::__RTS_FN_NS_BUFFER_READ_I64
    );
    add_fn!(
        "__RTS_FN_NS_BUFFER_READ_F64",
        buf::__RTS_FN_NS_BUFFER_READ_F64
    );
    add_fn!(
        "__RTS_FN_NS_BUFFER_WRITE_U8",
        buf::__RTS_FN_NS_BUFFER_WRITE_U8
    );
    add_fn!(
        "__RTS_FN_NS_BUFFER_WRITE_I32",
        buf::__RTS_FN_NS_BUFFER_WRITE_I32
    );
    add_fn!(
        "__RTS_FN_NS_BUFFER_WRITE_I64",
        buf::__RTS_FN_NS_BUFFER_WRITE_I64
    );
    add_fn!(
        "__RTS_FN_NS_BUFFER_WRITE_F64",
        buf::__RTS_FN_NS_BUFFER_WRITE_F64
    );
    add_fn!("__RTS_FN_NS_BUFFER_COPY", buf::__RTS_FN_NS_BUFFER_COPY);
    add_fn!("__RTS_FN_NS_BUFFER_FILL", buf::__RTS_FN_NS_BUFFER_FILL);
    add_fn!(
        "__RTS_FN_NS_BUFFER_TO_STRING",
        buf::__RTS_FN_NS_BUFFER_TO_STRING
    );
    add_fn!("__RTS_FN_NS_BUFFER_EQUALS", buf::__RTS_FN_NS_BUFFER_EQUALS);
    add_fn!(
        "__RTS_FN_NS_BUFFER_INDEX_OF",
        buf::__RTS_FN_NS_BUFFER_INDEX_OF
    );

    // ── namespaces::ffi ───────────────────────────────────────────────
    use crate::namespaces::ffi::{cstr as ffi_cstr, cstring as ffi_cstring, osstr as ffi_osstr};
    add_fn!(
        "__RTS_FN_NS_FFI_CSTR_FROM_PTR",
        ffi_cstr::__RTS_FN_NS_FFI_CSTR_FROM_PTR
    );
    add_fn!(
        "__RTS_FN_NS_FFI_CSTR_LEN",
        ffi_cstr::__RTS_FN_NS_FFI_CSTR_LEN
    );
    add_fn!(
        "__RTS_FN_NS_FFI_CSTR_TO_STR",
        ffi_cstr::__RTS_FN_NS_FFI_CSTR_TO_STR
    );
    add_fn!(
        "__RTS_FN_NS_FFI_CSTRING_NEW",
        ffi_cstring::__RTS_FN_NS_FFI_CSTRING_NEW
    );
    add_fn!(
        "__RTS_FN_NS_FFI_CSTRING_PTR",
        ffi_cstring::__RTS_FN_NS_FFI_CSTRING_PTR
    );
    add_fn!(
        "__RTS_FN_NS_FFI_CSTRING_FREE",
        ffi_cstring::__RTS_FN_NS_FFI_CSTRING_FREE
    );
    add_fn!(
        "__RTS_FN_NS_FFI_OSSTR_FROM_STR",
        ffi_osstr::__RTS_FN_NS_FFI_OSSTR_FROM_STR
    );
    add_fn!(
        "__RTS_FN_NS_FFI_OSSTR_TO_STR",
        ffi_osstr::__RTS_FN_NS_FFI_OSSTR_TO_STR
    );
    add_fn!(
        "__RTS_FN_NS_FFI_OSSTR_FREE",
        ffi_osstr::__RTS_FN_NS_FFI_OSSTR_FREE
    );

    // ── namespaces::atomic ────────────────────────────────────────────
    use crate::namespaces::atomic::{
        bool as atomic_bool, fence as atomic_fence, float as atomic_float, int as atomic_int,
    };
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_NEW",
        atomic_int::__RTS_FN_NS_ATOMIC_I64_NEW
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_LOAD",
        atomic_int::__RTS_FN_NS_ATOMIC_I64_LOAD
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_STORE",
        atomic_int::__RTS_FN_NS_ATOMIC_I64_STORE
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_FETCH_ADD",
        atomic_int::__RTS_FN_NS_ATOMIC_I64_FETCH_ADD
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_FETCH_SUB",
        atomic_int::__RTS_FN_NS_ATOMIC_I64_FETCH_SUB
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_FETCH_AND",
        atomic_int::__RTS_FN_NS_ATOMIC_I64_FETCH_AND
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_FETCH_OR",
        atomic_int::__RTS_FN_NS_ATOMIC_I64_FETCH_OR
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_FETCH_XOR",
        atomic_int::__RTS_FN_NS_ATOMIC_I64_FETCH_XOR
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_SWAP",
        atomic_int::__RTS_FN_NS_ATOMIC_I64_SWAP
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_CAS",
        atomic_int::__RTS_FN_NS_ATOMIC_I64_CAS
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_BOOL_NEW",
        atomic_bool::__RTS_FN_NS_ATOMIC_BOOL_NEW
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_BOOL_LOAD",
        atomic_bool::__RTS_FN_NS_ATOMIC_BOOL_LOAD
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_BOOL_STORE",
        atomic_bool::__RTS_FN_NS_ATOMIC_BOOL_STORE
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_BOOL_SWAP",
        atomic_bool::__RTS_FN_NS_ATOMIC_BOOL_SWAP
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_F64_NEW",
        atomic_float::__RTS_FN_NS_ATOMIC_F64_NEW
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_F64_LOAD",
        atomic_float::__RTS_FN_NS_ATOMIC_F64_LOAD
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_F64_STORE",
        atomic_float::__RTS_FN_NS_ATOMIC_F64_STORE
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_F64_FETCH_ADD",
        atomic_float::__RTS_FN_NS_ATOMIC_F64_FETCH_ADD
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_F64_SWAP",
        atomic_float::__RTS_FN_NS_ATOMIC_F64_SWAP
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_FENCE_ACQUIRE",
        atomic_fence::__RTS_FN_NS_ATOMIC_FENCE_ACQUIRE
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_FENCE_RELEASE",
        atomic_fence::__RTS_FN_NS_ATOMIC_FENCE_RELEASE
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_FENCE_SEQ_CST",
        atomic_fence::__RTS_FN_NS_ATOMIC_FENCE_SEQ_CST
    );

    // ── namespaces::sync ──────────────────────────────────────────────
    use crate::namespaces::sync::{mutex as sync_mutex, once as sync_once, rwlock as sync_rwlock};
    add_fn!(
        "__RTS_FN_NS_SYNC_MUTEX_NEW",
        sync_mutex::__RTS_FN_NS_SYNC_MUTEX_NEW
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_MUTEX_LOCK",
        sync_mutex::__RTS_FN_NS_SYNC_MUTEX_LOCK
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_MUTEX_TRY_LOCK",
        sync_mutex::__RTS_FN_NS_SYNC_MUTEX_TRY_LOCK
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_MUTEX_SET",
        sync_mutex::__RTS_FN_NS_SYNC_MUTEX_SET
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_MUTEX_UNLOCK",
        sync_mutex::__RTS_FN_NS_SYNC_MUTEX_UNLOCK
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_MUTEX_FREE",
        sync_mutex::__RTS_FN_NS_SYNC_MUTEX_FREE
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_RWLOCK_NEW",
        sync_rwlock::__RTS_FN_NS_SYNC_RWLOCK_NEW
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_RWLOCK_READ",
        sync_rwlock::__RTS_FN_NS_SYNC_RWLOCK_READ
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_RWLOCK_WRITE",
        sync_rwlock::__RTS_FN_NS_SYNC_RWLOCK_WRITE
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_RWLOCK_UNLOCK",
        sync_rwlock::__RTS_FN_NS_SYNC_RWLOCK_UNLOCK
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_ONCE_NEW",
        sync_once::__RTS_FN_NS_SYNC_ONCE_NEW
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_ONCE_CALL",
        sync_once::__RTS_FN_NS_SYNC_ONCE_CALL
    );

    // ── namespaces::thread ────────────────────────────────────────────
    {
        use crate::namespaces::thread::{info as thread_info, join as thread_join, spawn as thread_spawn};
        add_fn!(
            "__RTS_FN_NS_THREAD_SPAWN",
            thread_spawn::__RTS_FN_NS_THREAD_SPAWN
        );
        add_fn!(
            "__RTS_FN_NS_THREAD_SPAWN_WITH_UD",
            thread_spawn::__RTS_FN_NS_THREAD_SPAWN_WITH_UD
        );
        add_fn!(
            "__RTS_FN_NS_THREAD_SCOPE",
            thread_spawn::__RTS_FN_NS_THREAD_SCOPE
        );
        add_fn!(
            "__RTS_FN_NS_THREAD_SCOPE_WITH_UD",
            thread_spawn::__RTS_FN_NS_THREAD_SCOPE_WITH_UD
        );
        add_fn!(
            "__RTS_FN_NS_THREAD_JOIN",
            thread_join::__RTS_FN_NS_THREAD_JOIN
        );
        add_fn!(
            "__RTS_FN_NS_THREAD_DETACH",
            thread_join::__RTS_FN_NS_THREAD_DETACH
        );
        add_fn!("__RTS_FN_NS_THREAD_ID", thread_info::__RTS_FN_NS_THREAD_ID);
        add_fn!(
            "__RTS_FN_NS_THREAD_SLEEP_MS",
            thread_info::__RTS_FN_NS_THREAD_SLEEP_MS
        );
    }

    // ── namespaces::parallel ──────────────────────────────────────────
    {
        use crate::namespaces::parallel::ops as parallel_ops;
        add_fn!(
            "__RTS_FN_NS_PARALLEL_MAP",
            parallel_ops::__RTS_FN_NS_PARALLEL_MAP
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_FOR_EACH",
            parallel_ops::__RTS_FN_NS_PARALLEL_FOR_EACH
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_REDUCE",
            parallel_ops::__RTS_FN_NS_PARALLEL_REDUCE
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_NUM_THREADS",
            parallel_ops::__RTS_FN_NS_PARALLEL_NUM_THREADS
        );
    }

    // ── namespaces::path ──────────────────────────────────────────────
    use crate::namespaces::path::*;
    add_fn!("__RTS_FN_NS_PATH_JOIN", join::__RTS_FN_NS_PATH_JOIN);
    add_fn!(
        "__RTS_FN_NS_PATH_PARENT",
        components::__RTS_FN_NS_PATH_PARENT
    );
    add_fn!(
        "__RTS_FN_NS_PATH_FILE_NAME",
        components::__RTS_FN_NS_PATH_FILE_NAME
    );
    add_fn!("__RTS_FN_NS_PATH_STEM", components::__RTS_FN_NS_PATH_STEM);
    add_fn!("__RTS_FN_NS_PATH_EXT", components::__RTS_FN_NS_PATH_EXT);
    add_fn!(
        "__RTS_FN_NS_PATH_IS_ABSOLUTE",
        join::__RTS_FN_NS_PATH_IS_ABSOLUTE
    );
    add_fn!(
        "__RTS_FN_NS_PATH_NORMALIZE",
        join::__RTS_FN_NS_PATH_NORMALIZE
    );
    add_fn!("__RTS_FN_NS_PATH_WITH_EXT", join::__RTS_FN_NS_PATH_WITH_EXT);

    // ── namespaces::env ───────────────────────────────────────────────
    use crate::namespaces::env::*;
    add_fn!("__RTS_FN_NS_ENV_GET_VAR", vars::__RTS_FN_NS_ENV_GET_VAR);
    add_fn!("__RTS_FN_NS_ENV_SET_VAR", vars::__RTS_FN_NS_ENV_SET_VAR);
    add_fn!(
        "__RTS_FN_NS_ENV_REMOVE_VAR",
        vars::__RTS_FN_NS_ENV_REMOVE_VAR
    );
    add_fn!(
        "__RTS_FN_NS_ENV_ARGS_COUNT",
        args::__RTS_FN_NS_ENV_ARGS_COUNT
    );
    add_fn!("__RTS_FN_NS_ENV_ARG_AT", args::__RTS_FN_NS_ENV_ARG_AT);
    add_fn!("__RTS_FN_NS_ENV_CWD", cwd::__RTS_FN_NS_ENV_CWD);
    add_fn!("__RTS_FN_NS_ENV_SET_CWD", cwd::__RTS_FN_NS_ENV_SET_CWD);

    // ── namespaces::time ──────────────────────────────────────────────
    use crate::namespaces::time::*;
    add_fn!("__RTS_FN_NS_TIME_NOW_MS", instant::__RTS_FN_NS_TIME_NOW_MS);
    add_fn!("__RTS_FN_NS_TIME_NOW_NS", instant::__RTS_FN_NS_TIME_NOW_NS);
    add_fn!("__RTS_FN_NS_TIME_UNIX_MS", system::__RTS_FN_NS_TIME_UNIX_MS);
    add_fn!("__RTS_FN_NS_TIME_UNIX_NS", system::__RTS_FN_NS_TIME_UNIX_NS);
    add_fn!(
        "__RTS_FN_NS_TIME_SLEEP_MS",
        sleep::__RTS_FN_NS_TIME_SLEEP_MS
    );
    add_fn!(
        "__RTS_FN_NS_TIME_SLEEP_NS",
        sleep::__RTS_FN_NS_TIME_SLEEP_NS
    );

    // ── namespaces::bigfloat ──────────────────────────────────────────
    use crate::namespaces::bigfloat::ops::*;
    add_fn!("__RTS_FN_NS_BIGFLOAT_ZERO", __RTS_FN_NS_BIGFLOAT_ZERO);
    add_fn!(
        "__RTS_FN_NS_BIGFLOAT_FROM_F64",
        __RTS_FN_NS_BIGFLOAT_FROM_F64
    );
    add_fn!(
        "__RTS_FN_NS_BIGFLOAT_FROM_I64",
        __RTS_FN_NS_BIGFLOAT_FROM_I64
    );
    add_fn!(
        "__RTS_FN_NS_BIGFLOAT_FROM_STR",
        __RTS_FN_NS_BIGFLOAT_FROM_STR
    );
    add_fn!("__RTS_FN_NS_BIGFLOAT_TO_F64", __RTS_FN_NS_BIGFLOAT_TO_F64);
    add_fn!(
        "__RTS_FN_NS_BIGFLOAT_TO_STRING",
        __RTS_FN_NS_BIGFLOAT_TO_STRING
    );
    add_fn!("__RTS_FN_NS_BIGFLOAT_ADD", __RTS_FN_NS_BIGFLOAT_ADD);
    add_fn!("__RTS_FN_NS_BIGFLOAT_SUB", __RTS_FN_NS_BIGFLOAT_SUB);
    add_fn!("__RTS_FN_NS_BIGFLOAT_MUL", __RTS_FN_NS_BIGFLOAT_MUL);
    add_fn!("__RTS_FN_NS_BIGFLOAT_DIV", __RTS_FN_NS_BIGFLOAT_DIV);
    add_fn!("__RTS_FN_NS_BIGFLOAT_NEG", __RTS_FN_NS_BIGFLOAT_NEG);
    add_fn!("__RTS_FN_NS_BIGFLOAT_SQRT", __RTS_FN_NS_BIGFLOAT_SQRT);
    add_fn!("__RTS_FN_NS_BIGFLOAT_FREE", __RTS_FN_NS_BIGFLOAT_FREE);

    // ── namespaces::ui ────────────────────────────────────────────────
    {
        use crate::namespaces::ui::*;
        // app
        add_fn!("__RTS_FN_NS_UI_APP_NEW", app::__RTS_FN_NS_UI_APP_NEW);
        add_fn!("__RTS_FN_NS_UI_APP_RUN", app::__RTS_FN_NS_UI_APP_RUN);
        add_fn!("__RTS_FN_NS_UI_APP_FREE", app::__RTS_FN_NS_UI_APP_FREE);
        // window
        add_fn!(
            "__RTS_FN_NS_UI_WINDOW_NEW",
            window::__RTS_FN_NS_UI_WINDOW_NEW
        );
        add_fn!(
            "__RTS_FN_NS_UI_WINDOW_SHOW",
            window::__RTS_FN_NS_UI_WINDOW_SHOW
        );
        add_fn!(
            "__RTS_FN_NS_UI_WINDOW_END",
            window::__RTS_FN_NS_UI_WINDOW_END
        );
        add_fn!(
            "__RTS_FN_NS_UI_WINDOW_FREE",
            window::__RTS_FN_NS_UI_WINDOW_FREE
        );
        add_fn!(
            "__RTS_FN_NS_UI_WINDOW_SET_CALLBACK",
            window::__RTS_FN_NS_UI_WINDOW_SET_CALLBACK
        );
        add_fn!(
            "__RTS_FN_NS_UI_WINDOW_SET_COLOR",
            window::__RTS_FN_NS_UI_WINDOW_SET_COLOR
        );
        add_fn!(
            "__RTS_FN_NS_UI_WINDOW_RESIZE",
            window::__RTS_FN_NS_UI_WINDOW_RESIZE
        );
        // generic widget ops
        add_fn!(
            "__RTS_FN_NS_UI_WIDGET_SET_LABEL",
            widgets::__RTS_FN_NS_UI_WIDGET_SET_LABEL
        );
        add_fn!(
            "__RTS_FN_NS_UI_WIDGET_LABEL",
            widgets::__RTS_FN_NS_UI_WIDGET_LABEL
        );
        add_fn!(
            "__RTS_FN_NS_UI_WIDGET_SET_CALLBACK",
            widgets::__RTS_FN_NS_UI_WIDGET_SET_CALLBACK
        );
        add_fn!(
            "__RTS_FN_NS_UI_WIDGET_SET_CALLBACK_WITH_UD",
            widgets::__RTS_FN_NS_UI_WIDGET_SET_CALLBACK_WITH_UD
        );
        add_fn!(
            "__RTS_FN_NS_UI_WIDGET_SET_COLOR",
            widgets::__RTS_FN_NS_UI_WIDGET_SET_COLOR
        );
        add_fn!(
            "__RTS_FN_NS_UI_WIDGET_SET_LABEL_COLOR",
            widgets::__RTS_FN_NS_UI_WIDGET_SET_LABEL_COLOR
        );
        add_fn!(
            "__RTS_FN_NS_UI_WIDGET_RESIZE",
            widgets::__RTS_FN_NS_UI_WIDGET_RESIZE
        );
        add_fn!(
            "__RTS_FN_NS_UI_WIDGET_REDRAW",
            widgets::__RTS_FN_NS_UI_WIDGET_REDRAW
        );
        add_fn!(
            "__RTS_FN_NS_UI_WIDGET_HIDE",
            widgets::__RTS_FN_NS_UI_WIDGET_HIDE
        );
        add_fn!(
            "__RTS_FN_NS_UI_WIDGET_SHOW",
            widgets::__RTS_FN_NS_UI_WIDGET_SHOW
        );
        add_fn!(
            "__RTS_FN_NS_UI_WIDGET_SET_DRAW",
            widgets::__RTS_FN_NS_UI_WIDGET_SET_DRAW
        );
        // button / frame / check / radio
        add_fn!(
            "__RTS_FN_NS_UI_BUTTON_NEW",
            widgets::__RTS_FN_NS_UI_BUTTON_NEW
        );
        add_fn!(
            "__RTS_FN_NS_UI_FRAME_NEW",
            widgets::__RTS_FN_NS_UI_FRAME_NEW
        );
        add_fn!(
            "__RTS_FN_NS_UI_CHECK_NEW",
            widgets::__RTS_FN_NS_UI_CHECK_NEW
        );
        add_fn!(
            "__RTS_FN_NS_UI_CHECK_VALUE",
            widgets::__RTS_FN_NS_UI_CHECK_VALUE
        );
        add_fn!(
            "__RTS_FN_NS_UI_CHECK_SET_VALUE",
            widgets::__RTS_FN_NS_UI_CHECK_SET_VALUE
        );
        add_fn!(
            "__RTS_FN_NS_UI_RADIO_NEW",
            widgets::__RTS_FN_NS_UI_RADIO_NEW
        );
        add_fn!(
            "__RTS_FN_NS_UI_RADIO_VALUE",
            widgets::__RTS_FN_NS_UI_RADIO_VALUE
        );
        add_fn!(
            "__RTS_FN_NS_UI_RADIO_SET_VALUE",
            widgets::__RTS_FN_NS_UI_RADIO_SET_VALUE
        );
        // input / output
        add_fn!(
            "__RTS_FN_NS_UI_INPUT_NEW",
            widgets::__RTS_FN_NS_UI_INPUT_NEW
        );
        add_fn!(
            "__RTS_FN_NS_UI_INPUT_VALUE",
            widgets::__RTS_FN_NS_UI_INPUT_VALUE
        );
        add_fn!(
            "__RTS_FN_NS_UI_INPUT_SET_VALUE",
            widgets::__RTS_FN_NS_UI_INPUT_SET_VALUE
        );
        add_fn!(
            "__RTS_FN_NS_UI_OUTPUT_NEW",
            widgets::__RTS_FN_NS_UI_OUTPUT_NEW
        );
        add_fn!(
            "__RTS_FN_NS_UI_OUTPUT_SET_VALUE",
            widgets::__RTS_FN_NS_UI_OUTPUT_SET_VALUE
        );
        // slider / progress / spinner
        add_fn!(
            "__RTS_FN_NS_UI_SLIDER_NEW",
            widgets::__RTS_FN_NS_UI_SLIDER_NEW
        );
        add_fn!(
            "__RTS_FN_NS_UI_SLIDER_VALUE",
            widgets::__RTS_FN_NS_UI_SLIDER_VALUE
        );
        add_fn!(
            "__RTS_FN_NS_UI_SLIDER_SET_VALUE",
            widgets::__RTS_FN_NS_UI_SLIDER_SET_VALUE
        );
        add_fn!(
            "__RTS_FN_NS_UI_SLIDER_SET_BOUNDS",
            widgets::__RTS_FN_NS_UI_SLIDER_SET_BOUNDS
        );
        add_fn!(
            "__RTS_FN_NS_UI_PROGRESS_NEW",
            widgets::__RTS_FN_NS_UI_PROGRESS_NEW
        );
        add_fn!(
            "__RTS_FN_NS_UI_PROGRESS_VALUE",
            widgets::__RTS_FN_NS_UI_PROGRESS_VALUE
        );
        add_fn!(
            "__RTS_FN_NS_UI_PROGRESS_SET_VALUE",
            widgets::__RTS_FN_NS_UI_PROGRESS_SET_VALUE
        );
        add_fn!(
            "__RTS_FN_NS_UI_SPINNER_NEW",
            widgets::__RTS_FN_NS_UI_SPINNER_NEW
        );
        add_fn!(
            "__RTS_FN_NS_UI_SPINNER_VALUE",
            widgets::__RTS_FN_NS_UI_SPINNER_VALUE
        );
        add_fn!(
            "__RTS_FN_NS_UI_SPINNER_SET_VALUE",
            widgets::__RTS_FN_NS_UI_SPINNER_SET_VALUE
        );
        add_fn!(
            "__RTS_FN_NS_UI_SPINNER_SET_BOUNDS",
            widgets::__RTS_FN_NS_UI_SPINNER_SET_BOUNDS
        );
        // menu
        add_fn!(
            "__RTS_FN_NS_UI_MENUBAR_NEW",
            menu::__RTS_FN_NS_UI_MENUBAR_NEW
        );
        add_fn!(
            "__RTS_FN_NS_UI_MENUBAR_ADD",
            menu::__RTS_FN_NS_UI_MENUBAR_ADD
        );
        add_fn!(
            "__RTS_FN_NS_UI_MENUBAR_FREE",
            widgets::__RTS_FN_NS_UI_MENUBAR_FREE
        );
        // text
        add_fn!(
            "__RTS_FN_NS_UI_TEXTBUF_NEW",
            text::__RTS_FN_NS_UI_TEXTBUF_NEW
        );
        add_fn!(
            "__RTS_FN_NS_UI_TEXTBUF_SET_TEXT",
            text::__RTS_FN_NS_UI_TEXTBUF_SET_TEXT
        );
        add_fn!(
            "__RTS_FN_NS_UI_TEXTBUF_TEXT",
            text::__RTS_FN_NS_UI_TEXTBUF_TEXT
        );
        add_fn!(
            "__RTS_FN_NS_UI_TEXTBUF_APPEND",
            text::__RTS_FN_NS_UI_TEXTBUF_APPEND
        );
        add_fn!(
            "__RTS_FN_NS_UI_TEXTBUF_FREE",
            widgets::__RTS_FN_NS_UI_TEXTBUF_FREE
        );
        add_fn!(
            "__RTS_FN_NS_UI_TEXTDISPLAY_NEW",
            text::__RTS_FN_NS_UI_TEXTDISPLAY_NEW
        );
        add_fn!(
            "__RTS_FN_NS_UI_TEXTDISPLAY_SET_BUFFER",
            text::__RTS_FN_NS_UI_TEXTDISPLAY_SET_BUFFER
        );
        add_fn!(
            "__RTS_FN_NS_UI_TEXTEDITOR_NEW",
            text::__RTS_FN_NS_UI_TEXTEDITOR_NEW
        );
        add_fn!(
            "__RTS_FN_NS_UI_TEXTEDITOR_SET_BUFFER",
            text::__RTS_FN_NS_UI_TEXTEDITOR_SET_BUFFER
        );
        // draw
        add_fn!("__RTS_FN_NS_UI_DRAW_RECT", draw::__RTS_FN_NS_UI_DRAW_RECT);
        add_fn!(
            "__RTS_FN_NS_UI_DRAW_RECT_FILL",
            draw::__RTS_FN_NS_UI_DRAW_RECT_FILL
        );
        add_fn!("__RTS_FN_NS_UI_DRAW_LINE", draw::__RTS_FN_NS_UI_DRAW_LINE);
        add_fn!(
            "__RTS_FN_NS_UI_DRAW_CIRCLE",
            draw::__RTS_FN_NS_UI_DRAW_CIRCLE
        );
        add_fn!("__RTS_FN_NS_UI_DRAW_ARC", draw::__RTS_FN_NS_UI_DRAW_ARC);
        add_fn!("__RTS_FN_NS_UI_DRAW_TEXT", draw::__RTS_FN_NS_UI_DRAW_TEXT);
        add_fn!(
            "__RTS_FN_NS_UI_SET_DRAW_COLOR",
            draw::__RTS_FN_NS_UI_SET_DRAW_COLOR
        );
        add_fn!("__RTS_FN_NS_UI_SET_FONT", draw::__RTS_FN_NS_UI_SET_FONT);
        add_fn!(
            "__RTS_FN_NS_UI_SET_LINE_STYLE",
            draw::__RTS_FN_NS_UI_SET_LINE_STYLE
        );
        add_fn!(
            "__RTS_FN_NS_UI_MEASURE_WIDTH",
            draw::__RTS_FN_NS_UI_MEASURE_WIDTH
        );
        // dialog
        add_fn!("__RTS_FN_NS_UI_ALERT", dialog::__RTS_FN_NS_UI_ALERT);
        add_fn!(
            "__RTS_FN_NS_UI_DIALOG_ASK",
            dialog::__RTS_FN_NS_UI_DIALOG_ASK
        );
        add_fn!(
            "__RTS_FN_NS_UI_DIALOG_INPUT",
            dialog::__RTS_FN_NS_UI_DIALOG_INPUT
        );
    }

    // ── namespaces::runtime ───────────────────────────────────────────
    // JIT fast path: inline pipeline instead of subprocess spawn.
    {
        use crate::namespaces::runtime::eval_jit::*;
        add_fn!("__RTS_FN_NS_RUNTIME_EVAL", runtime_eval_src_jit);
        add_fn!("__RTS_FN_NS_RUNTIME_EVAL_FILE", runtime_eval_file_jit);
    }

    // ── namespaces::test ─────────────────────────────────────────────
    {
        use crate::namespaces::test::runner::*;
        add_fn!(
            "__RTS_FN_NS_TEST_CORE_SUITE_BEGIN",
            __RTS_FN_NS_TEST_CORE_SUITE_BEGIN
        );
        add_fn!(
            "__RTS_FN_NS_TEST_CORE_SUITE_END",
            __RTS_FN_NS_TEST_CORE_SUITE_END
        );
        add_fn!(
            "__RTS_FN_NS_TEST_CORE_CASE_BEGIN",
            __RTS_FN_NS_TEST_CORE_CASE_BEGIN
        );
        add_fn!(
            "__RTS_FN_NS_TEST_CORE_CASE_END",
            __RTS_FN_NS_TEST_CORE_CASE_END
        );
        add_fn!(
            "__RTS_FN_NS_TEST_CORE_CASE_FAIL",
            __RTS_FN_NS_TEST_CORE_CASE_FAIL
        );
        add_fn!(
            "__RTS_FN_NS_TEST_CORE_CASE_FAIL_DIFF",
            __RTS_FN_NS_TEST_CORE_CASE_FAIL_DIFF
        );
        add_fn!(
            "__RTS_FN_NS_TEST_CORE_PRINT_SUMMARY",
            __RTS_FN_NS_TEST_CORE_PRINT_SUMMARY
        );
    }

    // ── Libc ──────────────────────────────────────────────────────────
    // `fmod` is declared as an extern import for `BinaryOp::Mod` on f64.
    unsafe extern "C" {
        fn fmod(a: f64, b: f64) -> f64;
    }
    add_fn!("fmod", fmod);

    // Sanity: compara o conjunto de fns registradas no JIT com o
    // conjunto declarado em `abi::SPECS`. Em debug, alerta sobre
    // descompassos:
    //
    //   - missing: fn esta em SPECS mas nao no JIT — chamada via
    //     `rts run` vai falhar com symbol unresolved. Erro real,
    //     embora alguns SPECS sejam intencionalmente AOT-only.
    //
    //   - extra: fn registrada no JIT alem do contrato ABI publico.
    //     Esperado para helpers internos chamados direto pelo
    //     codegen (ex: __RTS_FN_RT_ERROR_* do try/catch). Nao e
    //     erro, so informativo.
    #[cfg(debug_assertions)]
    {
        use std::collections::HashSet;
        use crate::abi::SPECS;
        let spec_syms: HashSet<&str> = SPECS
            .iter()
            .flat_map(|s| s.members.iter().map(|m| m.symbol))
            .collect();
        let jit_syms: HashSet<&str> = out
            .iter()
            .filter(|(name, _)| name.starts_with("__RTS_FN_"))
            .map(|(name, _)| *name)
            .collect();
        let missing: Vec<&str> = spec_syms
            .iter()
            .copied()
            .filter(|s| !jit_syms.contains(s))
            .collect();
        let extras: Vec<&str> = jit_syms
            .iter()
            .copied()
            .filter(|s| !spec_syms.contains(s))
            .collect();
        if !missing.is_empty() {
            eprintln!(
                "[warn] {} fns declaradas em abi::SPECS sem entrada no JIT \
                 (chamadas via `rts run` vao falhar com symbol unresolved). \
                 Primeiras: {:?}",
                missing.len(),
                &missing.iter().take(3).collect::<Vec<_>>()
            );
        }
        if !extras.is_empty() {
            eprintln!(
                "[info] {} fns registradas no JIT alem do contrato ABI \
                 (helpers internos do codegen, ex: try/catch slots). \
                 Primeiras: {:?}",
                extras.len(),
                &extras.iter().take(3).collect::<Vec<_>>()
            );
        }
    }

    out
}
