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
    // Install the periodic GC hook so alloc_entry can trigger finish_cycle()
    // without a circular dependency between handles and collector.
    crate::namespaces::gc::handles::install_gc_hook(
        crate::namespaces::gc::collector::finish_cycle,
    );

    let mut module = build_jit_module()?;
    let mut extern_cache = HashMap::new();
    let mut data_counter: u32 = 0;

    let warnings = compile_program(program, &mut module, &mut extern_cache, &mut data_counter)?;

    module
        .finalize_definitions()
        .map_err(|e| anyhow!("JIT finalise failed: {e}"))?;

    // Resolve pending GC stack map entries to absolute return PCs.
    // During define_function each function's stack maps were stored as
    // (func_id_raw, [(ret_pc_offset, [sp_offsets])]). Now that
    // finalize_definitions() has fixed up all code addresses, we can
    // translate offsets to absolute pointers.
    {
        use crate::namespaces::gc::stack_map_registry;
        use cranelift_module::FuncId;

        let pending = stack_map_registry::drain_pending();
        let mut total_safepoints = 0usize;
        for entry in pending {
            let func_id = FuncId::from_u32(entry.func_id_raw);
            let base = module.get_finalized_function(func_id) as usize;
            for (ret_offset, sp_offsets) in entry.maps {
                let return_pc = base + ret_offset as usize;
                total_safepoints += 1;
                stack_map_registry::register(return_pc, sp_offsets);
            }
        }
        if std::env::var("RTS_GC_DEBUG").is_ok() {
            eprintln!("[gc] registered {total_safepoints} safepoints in JIT stack map registry");
        }
    }

    // Resolve DataIds das globals com Handle pra enderecos reais
    // (issue #407, epic #419). Sem isso o sweep coleta handles vivos
    // armazenados em globals top-level.
    {
        use crate::namespaces::gc::global_roots;
        use cranelift_module::DataId;
        let pending = global_roots::drain_pending_data_ids();
        let mut total = 0usize;
        for raw_id in pending {
            let data_id = DataId::from_u32(raw_id);
            let (ptr, _size) = module.get_finalized_data(data_id);
            global_roots::add(ptr as usize);
            total += 1;
        }
        if std::env::var("RTS_GC_DEBUG").is_ok() {
            eprintln!("[gc] registered {total} global roots (handle-typed top-level vars)");
        }
    }

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

    rts_runtime::namespaces::globals::function::ops::register_compile_fn(|params, body| {
        use crate::function_eval_compile as eval_compile;
        let compiled = eval_compile::compile_function(params, body)?;
        Ok(rts_runtime::namespaces::globals::function::ops::CompiledFn {
            fn_ptr: compiled.fn_ptr,
            arity: compiled.arity,
            keep_alive: compiled.keep_alive,
        })
    });

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

    // ── this binding slot (used by .call/.apply on plain fns) ─────────
    {
        use crate::namespaces::gc::this_slot::*;
        add_fn!("__RTS_FN_RT_THIS_PUSH", __RTS_FN_RT_THIS_PUSH);
        add_fn!("__RTS_FN_RT_THIS_POP", __RTS_FN_RT_THIS_POP);
        add_fn!("__RTS_FN_RT_THIS_GET", __RTS_FN_RT_THIS_GET);
    }

    // ── namespaces::gc ────────────────────────────────────────────────
    use crate::namespaces::gc::string_pool::*;
    add_fn!("__RTS_FN_NS_GC_GENERATOR_NEXT", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GENERATOR_NEXT);
    add_fn!("__RTS_FN_NS_GC_GENERATOR_SET_RET", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GENERATOR_SET_RET);
    add_fn!("__RTS_FN_NS_GC_GENERATOR_GET_RET", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GENERATOR_GET_RET);
    add_fn!("__RTS_FN_GL_ITERATOR_FROM", crate::namespaces::gc::generator::__RTS_FN_GL_ITERATOR_FROM);
    add_fn!("__RTS_FN_GL_ITERATOR_TO_ARRAY", crate::namespaces::gc::generator::__RTS_FN_GL_ITERATOR_TO_ARRAY);
    add_fn!("__RTS_FN_GL_ARRAY_VALUES_ITER", crate::namespaces::gc::generator::__RTS_FN_GL_ARRAY_VALUES_ITER);
    add_fn!("__RTS_FN_GL_ARRAY_ITERATOR_FN", crate::namespaces::gc::generator::__RTS_FN_GL_ARRAY_ITERATOR_FN);
    add_fn!("__RTS_FN_NS_GC_GENERATOR_RETURN", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GENERATOR_RETURN);
    add_fn!("__RTS_FN_NS_GC_SYMBOL_ITERATOR_OF", crate::namespaces::gc::generator::__RTS_FN_NS_GC_SYMBOL_ITERATOR_OF);
    add_fn!("__RTS_FN_NS_GC_GEN_SM_SENT", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_SENT);
    add_fn!("__RTS_FN_NS_GC_GEN_SM_ENTER_TRY_CATCH", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_ENTER_TRY_CATCH);
    add_fn!("__RTS_FN_NS_GC_GEN_SM_EXIT_TRY_CATCH", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_EXIT_TRY_CATCH);
    add_fn!("__RTS_FN_NS_GC_GEN_SM_CAUGHT", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_CAUGHT);
    add_fn!("__RTS_FN_NS_GC_GENERATOR_NEXT_SENT", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GENERATOR_NEXT_SENT);
    add_fn!("__RTS_FN_NS_GC_GEN_DELEGATE_START", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_DELEGATE_START);
    add_fn!("__RTS_FN_NS_GC_GEN_DELEGATE_NEXT", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_DELEGATE_NEXT);
    add_fn!("__RTS_FN_NS_GC_GEN_DELEGATE_DONE", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_DELEGATE_DONE);
    add_fn!("__RTS_FN_NS_GC_GEN_SM_NEW", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_NEW);
    add_fn!("__RTS_FN_NS_GC_AGEN_NEW", crate::namespaces::gc::generator::__RTS_FN_NS_GC_AGEN_NEW);
    add_fn!("__RTS_FN_NS_GC_AGEN_NEXT", crate::namespaces::gc::generator::__RTS_FN_NS_GC_AGEN_NEXT);
    add_fn!("__RTS_FN_NS_GC_GEN_SM_FGET", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_FGET);
    add_fn!("__RTS_FN_NS_GC_GEN_SM_FSET", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_FSET);
    add_fn!("__RTS_FN_NS_GC_GEN_SM_STATE", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_STATE);
    add_fn!("__RTS_FN_NS_GC_GEN_SM_SETSTATE", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_SETSTATE);
    add_fn!("__RTS_FN_NS_GC_GEN_SM_YIELD", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_YIELD);
    add_fn!("__RTS_FN_NS_GC_GEN_SM_DONE", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_DONE);
    add_fn!("__RTS_FN_NS_GC_GEN_SM_NEXT", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_NEXT);
    add_fn!("__RTS_FN_NS_GC_GEN_SM_RETURN", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_RETURN);
    add_fn!("__RTS_FN_NS_GC_GEN_SM_THROW", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_THROW);
    add_fn!("__RTS_FN_NS_GC_GEN_SM_ENTER_TRY", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_ENTER_TRY);
    add_fn!("__RTS_FN_NS_GC_GEN_SM_END_FINALLY", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_END_FINALLY);
    add_fn!("__RTS_FN_NS_GC_GENERATOR_THROW", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GENERATOR_THROW);
    add_fn!("__RTS_FN_NS_GC_GEN_SM_IS", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_IS);
    add_fn!("__RTS_FN_NS_GC_GEN_SM_DRAIN", crate::namespaces::gc::generator::__RTS_FN_NS_GC_GEN_SM_DRAIN);
    add_fn!("__RTS_FN_NS_GC_ASYNC_SM_NEW", crate::namespaces::gc::generator::__RTS_FN_NS_GC_ASYNC_SM_NEW);
    add_fn!("__RTS_FN_NS_GC_ASYNC_SM_START", crate::namespaces::gc::generator::__RTS_FN_NS_GC_ASYNC_SM_START);
    add_fn!("__RTS_FN_NS_GC_ASYNC_SM_SUSPEND", crate::namespaces::gc::generator::__RTS_FN_NS_GC_ASYNC_SM_SUSPEND);
    add_fn!("__RTS_FN_NS_GC_ASYNC_SM_AWAITED", crate::namespaces::gc::generator::__RTS_FN_NS_GC_ASYNC_SM_AWAITED);
    add_fn!("__RTS_FN_NS_GC_ASYNC_SM_RESOLVE", crate::namespaces::gc::generator::__RTS_FN_NS_GC_ASYNC_SM_RESOLVE);
    add_fn!("__RTS_FN_NS_GC_TAGGED_RAW_GET", crate::namespaces::gc::tagged_raw::__RTS_FN_NS_GC_TAGGED_RAW_GET);
    add_fn!("__RTS_FN_NS_GC_TAGGED_RAW_REGISTER", crate::namespaces::gc::tagged_raw::__RTS_FN_NS_GC_TAGGED_RAW_REGISTER);
    add_fn!("__RTS_FN_NS_GC_STRING_NEW", __RTS_FN_NS_GC_STRING_NEW);
    add_fn!("__RTS_FN_NS_GC_STRING_LEN", __RTS_FN_NS_GC_STRING_LEN);
    add_fn!("__RTS_FN_NS_GC_STRING_PTR", __RTS_FN_NS_GC_STRING_PTR);
    add_fn!("__RTS_FN_NS_GC_STRING_FREE", __RTS_FN_NS_GC_STRING_FREE);
    add_fn!("__RTS_FN_NS_GC_HANDLE_LEN", __RTS_FN_NS_GC_HANDLE_LEN);
    add_fn!("__RTS_FN_NS_GC_IS_VEC", __RTS_FN_NS_GC_IS_VEC);
    add_fn!("__RTS_FN_NS_GC_IS_DATE", __RTS_FN_NS_GC_IS_DATE);
    add_fn!("__RTS_FN_NS_GC_IS_REGEX", __RTS_FN_NS_GC_IS_REGEX);
    add_fn!("__RTS_FN_NS_GC_IS_MAP_LIKE", __RTS_FN_NS_GC_IS_MAP_LIKE);
    add_fn!("__RTS_FN_NS_GC_IS_PROMISE", __RTS_FN_NS_GC_IS_PROMISE);
    add_fn!(
        "__RTS_FN_NS_GC_STRING_FROM_I64",
        __RTS_FN_NS_GC_STRING_FROM_I64
    );
    add_fn!(
        "__RTS_FN_NS_GC_STRING_FROM_I64_TPL",
        __RTS_FN_NS_GC_STRING_FROM_I64_TPL
    );
    add_fn!(
        "__RTS_FN_NS_GC_STRING_FROM_F64",
        __RTS_FN_NS_GC_STRING_FROM_F64
    );
    add_fn!("__RTS_FN_NS_GC_STRING_CONCAT", __RTS_FN_NS_GC_STRING_CONCAT);
    // (#195 mutable closures) heap cells for captured-mutated locals.
    add_fn!("__RTS_FN_RT_CELL_NEW", __RTS_FN_RT_CELL_NEW);
    add_fn!("__RTS_FN_RT_CELL_GET", __RTS_FN_RT_CELL_GET);
    add_fn!("__RTS_FN_RT_CELL_SET", __RTS_FN_RT_CELL_SET);
    add_fn!(
        "__RTS_FN_NS_GC_STRING_FROM_STATIC",
        __RTS_FN_NS_GC_STRING_FROM_STATIC
    );
    add_fn!("__RTS_FN_NS_GC_STRING_EQ", __RTS_FN_NS_GC_STRING_EQ);
    add_fn!("__RTS_FN_NS_GC_STRING_CMP", __RTS_FN_NS_GC_STRING_CMP);
    use crate::namespaces::gc::env::*;
    add_fn!("__RTS_FN_NS_GC_ENV_ALLOC", __RTS_FN_NS_GC_ENV_ALLOC);
    add_fn!("__RTS_FN_NS_GC_ENV_GET", __RTS_FN_NS_GC_ENV_GET);
    add_fn!("__RTS_FN_NS_GC_ENV_SET", __RTS_FN_NS_GC_ENV_SET);
    add_fn!("__RTS_FN_NS_GC_ENV_FREE", __RTS_FN_NS_GC_ENV_FREE);
    use crate::namespaces::gc::closure::*;
    add_fn!("__RTS_FN_NS_GC_CLOSURE_ALLOC", __RTS_FN_NS_GC_CLOSURE_ALLOC);
    add_fn!("__RTS_FN_NS_GC_CLOSURE_FN_PTR", __RTS_FN_NS_GC_CLOSURE_FN_PTR);
    add_fn!("__RTS_FN_NS_GC_CLOSURE_ENV", __RTS_FN_NS_GC_CLOSURE_ENV);
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
    use crate::namespaces::io::*;
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
    use crate::namespaces::json::*;
    add_fn!("__RTS_FN_NS_JSON_PARSE", __RTS_FN_NS_JSON_PARSE);
    add_fn!("__RTS_FN_NS_JSON_PARSE_REVIVER", __RTS_FN_NS_JSON_PARSE_REVIVER);
    add_fn!("__RTS_FN_NS_JSON_PARSE5", __RTS_FN_NS_JSON_PARSE5);
    add_fn!("__RTS_FN_NS_JSON_STRINGIFY", __RTS_FN_NS_JSON_STRINGIFY);
    add_fn!("__RTS_FN_NS_JSON_STRINGIFY_KEYS", __RTS_FN_NS_JSON_STRINGIFY_KEYS);
    add_fn!("__RTS_FN_NS_JSON_STRINGIFY_REPLACER_FN", __RTS_FN_NS_JSON_STRINGIFY_REPLACER_FN);
    add_fn!(
        "__RTS_FN_NS_JSON_STRINGIFY_TYPED",
        __RTS_FN_NS_JSON_STRINGIFY_TYPED
    );
    add_fn!(
        "__RTS_FN_NS_JSON_STRINGIFY_PRETTY",
        __RTS_FN_NS_JSON_STRINGIFY_PRETTY
    );
    add_fn!(
        "__RTS_FN_NS_JSON_STRINGIFY_PRETTY_STR",
        __RTS_FN_NS_JSON_STRINGIFY_PRETTY_STR
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
    use crate::namespaces::globals::events::*;
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
    use crate::namespaces::globals::regexp::*;
    add_fn!("__RTS_FN_GL_REGEXP_NEW", __RTS_FN_GL_REGEXP_NEW);
    add_fn!("__RTS_FN_GL_REGEXP_NEW_WITH_FLAGS", __RTS_FN_GL_REGEXP_NEW_WITH_FLAGS);
    add_fn!("__RTS_FN_GL_REGEXP_TEST", __RTS_FN_GL_REGEXP_TEST);
    add_fn!("__RTS_FN_GL_REGEXP_EXEC", __RTS_FN_GL_REGEXP_EXEC);
    add_fn!("__RTS_FN_GL_REGEXP_SOURCE", __RTS_FN_GL_REGEXP_SOURCE);
    add_fn!("__RTS_FN_GL_REGEXP_FLAGS", __RTS_FN_GL_REGEXP_FLAGS);
    add_fn!("__RTS_FN_GL_REGEXP_GLOBAL", __RTS_FN_GL_REGEXP_GLOBAL);
    add_fn!("__RTS_FN_GL_REGEXP_IGNORE_CASE", __RTS_FN_GL_REGEXP_IGNORE_CASE);
    add_fn!("__RTS_FN_GL_REGEXP_MULTILINE", __RTS_FN_GL_REGEXP_MULTILINE);
    add_fn!("__RTS_FN_GL_REGEXP_LAST_INDEX_GET", __RTS_FN_GL_REGEXP_LAST_INDEX_GET);
    add_fn!("__RTS_FN_GL_REGEXP_LAST_INDEX_SET", __RTS_FN_GL_REGEXP_LAST_INDEX_SET);
    add_fn!("__RTS_FN_GL_REGEXP_INDICES_GROUPS", __RTS_FN_GL_REGEXP_INDICES_GROUPS);

    // ── namespaces::globals::error (Error class family) ───────────────
    use crate::namespaces::globals::error::instance::*;
    add_fn!("__RTS_FN_GL_ERROR_NEW", __RTS_FN_GL_ERROR_NEW);
    add_fn!("__RTS_FN_GL_TYPE_ERROR_NEW", __RTS_FN_GL_TYPE_ERROR_NEW);
    add_fn!("__RTS_FN_GL_RANGE_ERROR_NEW", __RTS_FN_GL_RANGE_ERROR_NEW);
    add_fn!("__RTS_FN_GL_REF_ERROR_NEW", __RTS_FN_GL_REF_ERROR_NEW);
    add_fn!("__RTS_FN_GL_SYNTAX_ERROR_NEW", __RTS_FN_GL_SYNTAX_ERROR_NEW);
    add_fn!("__RTS_FN_GL_URI_ERROR_NEW", __RTS_FN_GL_URI_ERROR_NEW);
    add_fn!("__RTS_FN_GL_EVAL_ERROR_NEW", __RTS_FN_GL_EVAL_ERROR_NEW);
    add_fn!("__RTS_FN_GL_AGGREGATE_ERROR_NEW", __RTS_FN_GL_AGGREGATE_ERROR_NEW);
    add_fn!("__RTS_FN_GL_AGGREGATE_ERROR_ERRORS", __RTS_FN_GL_AGGREGATE_ERROR_ERRORS);
    add_fn!("__RTS_FN_GL_ERROR_MESSAGE", __RTS_FN_GL_ERROR_MESSAGE);
    add_fn!("__RTS_FN_GL_ERROR_NAME", __RTS_FN_GL_ERROR_NAME);
    add_fn!("__RTS_FN_GL_ERROR_TO_STRING", __RTS_FN_GL_ERROR_TO_STRING);
    add_fn!("__RTS_FN_GL_ERROR_STACK", __RTS_FN_GL_ERROR_STACK);
    add_fn!("__RTS_FN_GL_ERROR_CAUSE", __RTS_FN_GL_ERROR_CAUSE);
    add_fn!("__RTS_FN_GL_ERROR_CAPTURE_STACK_TRACE", __RTS_FN_GL_ERROR_CAPTURE_STACK_TRACE);
    add_fn!("__RTS_FN_GL_IS_ERROR", __RTS_FN_GL_IS_ERROR);
    add_fn!("__RTS_FN_GL_IS_ERROR_NAMED", __RTS_FN_GL_IS_ERROR_NAMED);

    // ── namespaces::globals::intl (Intl.* global classes) ─────────────
    {
        use crate::namespaces::globals::intl::instance::*;
        add_fn!("__RTS_FN_GL_INTL_NUMBER_FORMAT_NEW", __RTS_FN_GL_INTL_NUMBER_FORMAT_NEW);
        add_fn!("__RTS_FN_GL_INTL_NUMBER_FORMAT_FORMAT", __RTS_FN_GL_INTL_NUMBER_FORMAT_FORMAT);
        add_fn!("__RTS_FN_GL_INTL_DATE_TIME_FORMAT_NEW", __RTS_FN_GL_INTL_DATE_TIME_FORMAT_NEW);
        add_fn!("__RTS_FN_GL_INTL_DATE_TIME_FORMAT_FORMAT", __RTS_FN_GL_INTL_DATE_TIME_FORMAT_FORMAT);
        add_fn!("__RTS_FN_GL_INTL_COLLATOR_NEW", __RTS_FN_GL_INTL_COLLATOR_NEW);
        add_fn!("__RTS_FN_GL_INTL_COLLATOR_COMPARE", __RTS_FN_GL_INTL_COLLATOR_COMPARE);
        add_fn!("__RTS_FN_GL_INTL_SEGMENTER_NEW", __RTS_FN_GL_INTL_SEGMENTER_NEW);
        add_fn!("__RTS_FN_GL_INTL_SEGMENTER_SEGMENT", __RTS_FN_GL_INTL_SEGMENTER_SEGMENT);
        add_fn!("__RTS_FN_GL_INTL_PLURAL_RULES_NEW", __RTS_FN_GL_INTL_PLURAL_RULES_NEW);
        add_fn!("__RTS_FN_GL_INTL_PLURAL_RULES_SELECT", __RTS_FN_GL_INTL_PLURAL_RULES_SELECT);
        add_fn!("__RTS_FN_GL_INTL_LIST_FORMAT_NEW", __RTS_FN_GL_INTL_LIST_FORMAT_NEW);
        add_fn!("__RTS_FN_GL_INTL_LIST_FORMAT_FORMAT", __RTS_FN_GL_INTL_LIST_FORMAT_FORMAT);
        add_fn!("__RTS_FN_GL_INTL_RELATIVE_TIME_FORMAT_NEW", __RTS_FN_GL_INTL_RELATIVE_TIME_FORMAT_NEW);
        add_fn!("__RTS_FN_GL_INTL_RELATIVE_TIME_FORMAT_FORMAT", __RTS_FN_GL_INTL_RELATIVE_TIME_FORMAT_FORMAT);
    }

    // ── namespaces::globals::readable_stream (Web Streams) ────────────
    {
        use crate::namespaces::globals::readable_stream::instance::*;
        add_fn!("__RTS_FN_GL_READABLE_STREAM_NEW", __RTS_FN_GL_READABLE_STREAM_NEW);
        add_fn!("__RTS_FN_GL_READABLE_STREAM_GET_READER", __RTS_FN_GL_READABLE_STREAM_GET_READER);
        add_fn!("__RTS_FN_GL_READABLE_STREAM_READER_READ", __RTS_FN_GL_READABLE_STREAM_READER_READ);
        add_fn!("__RTS_FN_GL_READABLE_STREAM_CONTROLLER_ENQUEUE", __RTS_FN_GL_READABLE_STREAM_CONTROLLER_ENQUEUE);
        add_fn!("__RTS_FN_GL_READABLE_STREAM_CONTROLLER_CLOSE", __RTS_FN_GL_READABLE_STREAM_CONTROLLER_CLOSE);
        add_fn!("__RTS_FN_GL_TRANSFORM_STREAM_NEW", __RTS_FN_GL_TRANSFORM_STREAM_NEW);
        add_fn!("__RTS_FN_GL_TRANSFORM_STREAM_WRITABLE", __RTS_FN_GL_TRANSFORM_STREAM_WRITABLE);
        add_fn!("__RTS_FN_GL_TRANSFORM_STREAM_READABLE", __RTS_FN_GL_TRANSFORM_STREAM_READABLE);
        add_fn!("__RTS_FN_GL_READABLE_STREAM_PIPE_THROUGH", __RTS_FN_GL_READABLE_STREAM_PIPE_THROUGH);
        add_fn!("__RTS_FN_GL_TEXT_ENCODER_STREAM_NEW", __RTS_FN_GL_TEXT_ENCODER_STREAM_NEW);
        add_fn!("__RTS_FN_GL_TEXT_DECODER_STREAM_NEW", __RTS_FN_GL_TEXT_DECODER_STREAM_NEW);
        add_fn!("__RTS_FN_GL_COMPRESSION_STREAM_NEW", __RTS_FN_GL_COMPRESSION_STREAM_NEW);
        add_fn!("__RTS_FN_GL_WRITABLE_STREAM_GET_WRITER", __RTS_FN_GL_WRITABLE_STREAM_GET_WRITER);
        add_fn!("__RTS_FN_GL_WRITABLE_STREAM_WRITER_WRITE", __RTS_FN_GL_WRITABLE_STREAM_WRITER_WRITE);
        add_fn!("__RTS_FN_GL_WRITABLE_STREAM_WRITER_CLOSE", __RTS_FN_GL_WRITABLE_STREAM_WRITER_CLOSE);
    }

    {
        use crate::namespaces::globals::message_channel::*;
        add_fn!("__RTS_FN_GL_MESSAGE_CHANNEL_NEW", __RTS_FN_GL_MESSAGE_CHANNEL_NEW);
        add_fn!("__RTS_FN_GL_MESSAGE_CHANNEL_PORT1", __RTS_FN_GL_MESSAGE_CHANNEL_PORT1);
        add_fn!("__RTS_FN_GL_MESSAGE_CHANNEL_PORT2", __RTS_FN_GL_MESSAGE_CHANNEL_PORT2);
        add_fn!("__RTS_FN_GL_MESSAGE_PORT_POST_MESSAGE", __RTS_FN_GL_MESSAGE_PORT_POST_MESSAGE);
        add_fn!("__RTS_FN_GL_MESSAGE_PORT_CLOSE", __RTS_FN_GL_MESSAGE_PORT_CLOSE);
    }

    // ── namespaces::globals::date (Date global class) ─────────────────
    use crate::namespaces::globals::date::instance::*;
    add_fn!("__RTS_FN_GL_DATE_NEW_NOW", __RTS_FN_GL_DATE_NEW_NOW);
    add_fn!("__RTS_FN_GL_DATE_NEW_FROM_MS", __RTS_FN_GL_DATE_NEW_FROM_MS);
    add_fn!("__RTS_FN_GL_DATE_NEW_FROM_ISO", __RTS_FN_GL_DATE_NEW_FROM_ISO);
    add_fn!("__RTS_FN_GL_DATE_NEW_FROM_FIELDS", __RTS_FN_GL_DATE_NEW_FROM_FIELDS);
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
    // (#220) UTC getters + extras.
    add_fn!("__RTS_FN_GL_DATE_GET_UTC_FULL_YEAR", __RTS_FN_GL_DATE_GET_UTC_FULL_YEAR);
    add_fn!("__RTS_FN_GL_DATE_GET_UTC_MONTH", __RTS_FN_GL_DATE_GET_UTC_MONTH);
    add_fn!("__RTS_FN_GL_DATE_GET_UTC_DATE", __RTS_FN_GL_DATE_GET_UTC_DATE);
    add_fn!("__RTS_FN_GL_DATE_GET_UTC_DAY", __RTS_FN_GL_DATE_GET_UTC_DAY);
    add_fn!("__RTS_FN_GL_DATE_GET_UTC_HOURS", __RTS_FN_GL_DATE_GET_UTC_HOURS);
    add_fn!("__RTS_FN_GL_DATE_GET_UTC_MINUTES", __RTS_FN_GL_DATE_GET_UTC_MINUTES);
    add_fn!("__RTS_FN_GL_DATE_GET_UTC_SECONDS", __RTS_FN_GL_DATE_GET_UTC_SECONDS);
    add_fn!("__RTS_FN_GL_DATE_GET_UTC_MILLISECONDS", __RTS_FN_GL_DATE_GET_UTC_MILLISECONDS);
    add_fn!("__RTS_FN_GL_DATE_GET_TIMEZONE_OFFSET", __RTS_FN_GL_DATE_GET_TIMEZONE_OFFSET);
    add_fn!("__RTS_FN_GL_DATE_TO_UTC_STRING", __RTS_FN_GL_DATE_TO_UTC_STRING);
    add_fn!("__RTS_FN_GL_DATE_TO_DATE_STRING", __RTS_FN_GL_DATE_TO_DATE_STRING);
    // (#220) Setters.
    add_fn!("__RTS_FN_GL_DATE_SET_FULL_YEAR", __RTS_FN_GL_DATE_SET_FULL_YEAR);
    add_fn!("__RTS_FN_GL_DATE_SET_MONTH", __RTS_FN_GL_DATE_SET_MONTH);
    add_fn!("__RTS_FN_GL_DATE_SET_DATE", __RTS_FN_GL_DATE_SET_DATE);
    add_fn!("__RTS_FN_GL_DATE_SET_HOURS", __RTS_FN_GL_DATE_SET_HOURS);
    add_fn!("__RTS_FN_GL_DATE_SET_MINUTES", __RTS_FN_GL_DATE_SET_MINUTES);
    add_fn!("__RTS_FN_GL_DATE_SET_SECONDS", __RTS_FN_GL_DATE_SET_SECONDS);
    add_fn!("__RTS_FN_GL_DATE_SET_MILLISECONDS", __RTS_FN_GL_DATE_SET_MILLISECONDS);
    add_fn!("__RTS_FN_GL_DATE_SET_TIME", __RTS_FN_GL_DATE_SET_TIME);
    add_fn!("__RTS_FN_GL_DATE_TO_JSON", __RTS_FN_GL_DATE_TO_JSON);
    add_fn!("__RTS_FN_GL_DATE_TO_LOCALE_STRING", __RTS_FN_GL_DATE_TO_LOCALE_STRING);
    add_fn!("__RTS_FN_GL_DATE_TO_LOCALE_TIME_STRING", __RTS_FN_GL_DATE_TO_LOCALE_TIME_STRING);
    add_fn!("__RTS_FN_GL_DATE_TO_TIME_STRING", __RTS_FN_GL_DATE_TO_TIME_STRING);

    // ── namespaces::globals::timers ───────────────────────────────────
    use crate::namespaces::globals::timers::instance::*;
    add_fn!("__RTS_FN_GL_TIMERS_SET_TIMEOUT", __RTS_FN_GL_TIMERS_SET_TIMEOUT);
    add_fn!("__RTS_FN_GL_TIMERS_CLEAR_TIMEOUT", __RTS_FN_GL_TIMERS_CLEAR_TIMEOUT);
    add_fn!("__RTS_FN_GL_TIMERS_SET_INTERVAL", __RTS_FN_GL_TIMERS_SET_INTERVAL);
    add_fn!("__RTS_FN_GL_TIMERS_CLEAR_INTERVAL", __RTS_FN_GL_TIMERS_CLEAR_INTERVAL);
    add_fn!("__RTS_FN_GL_TIMERS_SET_IMMEDIATE", __RTS_FN_GL_TIMERS_SET_IMMEDIATE);
    add_fn!("__RTS_FN_GL_TIMERS_CLEAR_IMMEDIATE", __RTS_FN_GL_TIMERS_CLEAR_IMMEDIATE);

    // ── namespaces::globals::form_data (#72) ──────────────────────────
    {
        use crate::namespaces::globals::form_data::*;
        add_fn!("__RTS_FN_GL_FORM_DATA_NEW", __RTS_FN_GL_FORM_DATA_NEW);
        add_fn!("__RTS_FN_GL_FORM_DATA_APPEND", __RTS_FN_GL_FORM_DATA_APPEND);
        add_fn!("__RTS_FN_GL_FORM_DATA_SET", __RTS_FN_GL_FORM_DATA_SET);
        add_fn!("__RTS_FN_GL_FORM_DATA_DELETE", __RTS_FN_GL_FORM_DATA_DELETE);
        add_fn!("__RTS_FN_GL_FORM_DATA_GET", __RTS_FN_GL_FORM_DATA_GET);
        add_fn!("__RTS_FN_GL_FORM_DATA_GET_ALL", __RTS_FN_GL_FORM_DATA_GET_ALL);
        add_fn!("__RTS_FN_GL_FORM_DATA_HAS", __RTS_FN_GL_FORM_DATA_HAS);
        add_fn!("__RTS_FN_GL_FORM_DATA_ENTRIES", __RTS_FN_GL_FORM_DATA_ENTRIES);
        add_fn!("__RTS_FN_GL_FORM_DATA_KEYS", __RTS_FN_GL_FORM_DATA_KEYS);
        add_fn!("__RTS_FN_GL_FORM_DATA_VALUES", __RTS_FN_GL_FORM_DATA_VALUES);
    }

    // ── namespaces::globals::blob (#74/#75) ───────────────────────────
    {
        use crate::namespaces::globals::blob::*;
        add_fn!("__RTS_FN_GL_BLOB_NEW", __RTS_FN_GL_BLOB_NEW);
        add_fn!("__RTS_FN_GL_BLOB_NEW_EMPTY", __RTS_FN_GL_BLOB_NEW_EMPTY);
        add_fn!("__RTS_FN_GL_BLOB_SIZE", __RTS_FN_GL_BLOB_SIZE);
        add_fn!("__RTS_FN_GL_BLOB_TEXT", __RTS_FN_GL_BLOB_TEXT);
        add_fn!("__RTS_FN_GL_BLOB_STREAM", __RTS_FN_GL_BLOB_STREAM);
        add_fn!("__RTS_FN_GL_FILE_NEW", __RTS_FN_GL_FILE_NEW);
        add_fn!("__RTS_FN_GL_FILE_NAME", __RTS_FN_GL_FILE_NAME);
        add_fn!("__RTS_FN_GL_FILE_LAST_MODIFIED", __RTS_FN_GL_FILE_LAST_MODIFIED);
    }

    // ── namespaces::globals::dom_exception (#77) ──────────────────────
    {
        use crate::namespaces::globals::dom_exception::*;
        add_fn!("__RTS_FN_GL_DOM_EXCEPTION_NEW", __RTS_FN_GL_DOM_EXCEPTION_NEW);
        add_fn!("__RTS_FN_GL_DOM_EXCEPTION_NEW_EMPTY", __RTS_FN_GL_DOM_EXCEPTION_NEW_EMPTY);
        add_fn!("__RTS_FN_GL_DOM_EXCEPTION_NEW_MSG", __RTS_FN_GL_DOM_EXCEPTION_NEW_MSG);
        add_fn!("__RTS_FN_GL_DOM_EXCEPTION_NAME", __RTS_FN_GL_DOM_EXCEPTION_NAME);
        add_fn!("__RTS_FN_GL_DOM_EXCEPTION_MESSAGE", __RTS_FN_GL_DOM_EXCEPTION_MESSAGE);
        add_fn!("__RTS_FN_GL_DOM_EXCEPTION_CODE", __RTS_FN_GL_DOM_EXCEPTION_CODE);
    }

    // ── namespaces::globals::event_target (#63) ───────────────────────
    {
        use crate::namespaces::globals::event_target::*;
        add_fn!("__RTS_FN_GL_EVENT_TARGET_NEW", __RTS_FN_GL_EVENT_TARGET_NEW);
        add_fn!("__RTS_FN_GL_EVENT_TARGET_ADD_LISTENER", __RTS_FN_GL_EVENT_TARGET_ADD_LISTENER);
        add_fn!("__RTS_FN_GL_EVENT_TARGET_REMOVE_LISTENER", __RTS_FN_GL_EVENT_TARGET_REMOVE_LISTENER);
        add_fn!("__RTS_FN_GL_EVENT_TARGET_DISPATCH", __RTS_FN_GL_EVENT_TARGET_DISPATCH);
        add_fn!("__RTS_FN_GL_EVENT_NEW", __RTS_FN_GL_EVENT_NEW);
        add_fn!("__RTS_FN_GL_EVENT_TYPE", __RTS_FN_GL_EVENT_TYPE);
    }

    // ── namespaces::globals::abort (#62) ──────────────────────────────
    {
        use crate::namespaces::globals::abort::*;
        add_fn!("__RTS_FN_GL_ABORT_CONTROLLER_NEW", __RTS_FN_GL_ABORT_CONTROLLER_NEW);
        add_fn!("__RTS_FN_GL_ABORT_CONTROLLER_SIGNAL", __RTS_FN_GL_ABORT_CONTROLLER_SIGNAL);
        add_fn!("__RTS_FN_GL_ABORT_CONTROLLER_ABORT", __RTS_FN_GL_ABORT_CONTROLLER_ABORT);
        add_fn!("__RTS_FN_GL_ABORT_SIGNAL_ABORTED", __RTS_FN_GL_ABORT_SIGNAL_ABORTED);
        add_fn!("__RTS_FN_GL_ABORT_SIGNAL_REASON", __RTS_FN_GL_ABORT_SIGNAL_REASON);
        add_fn!("__RTS_FN_GL_ABORT_SIGNAL_ADD_LISTENER", __RTS_FN_GL_ABORT_SIGNAL_ADD_LISTENER);
        add_fn!("__RTS_FN_GL_ABORT_SIGNAL_REMOVE_LISTENER", __RTS_FN_GL_ABORT_SIGNAL_REMOVE_LISTENER);
        add_fn!("__RTS_FN_GL_ABORT_SIGNAL_THROW_IF_ABORTED", __RTS_FN_GL_ABORT_SIGNAL_THROW_IF_ABORTED);
        add_fn!("__RTS_FN_GL_ABORT_SIGNAL_STATIC_ABORT", __RTS_FN_GL_ABORT_SIGNAL_STATIC_ABORT);
        add_fn!("__RTS_FN_GL_ABORT_SIGNAL_TIMEOUT", __RTS_FN_GL_ABORT_SIGNAL_TIMEOUT);
        add_fn!("__RTS_FN_GL_ABORT_SIGNAL_ANY", __RTS_FN_GL_ABORT_SIGNAL_ANY);
    }

    // ── namespaces::globals::headers (#289) ───────────────────────────
    {
        use crate::namespaces::globals::headers::*;
        add_fn!("__RTS_FN_GL_HEADERS_NEW", __RTS_FN_GL_HEADERS_NEW);
        add_fn!("__RTS_FN_GL_HEADERS_NEW_FROM", __RTS_FN_GL_HEADERS_NEW_FROM);
        add_fn!("__RTS_FN_GL_HEADERS_APPEND", __RTS_FN_GL_HEADERS_APPEND);
        add_fn!("__RTS_FN_GL_HEADERS_SET", __RTS_FN_GL_HEADERS_SET);
        add_fn!("__RTS_FN_GL_HEADERS_GET", __RTS_FN_GL_HEADERS_GET);
        add_fn!("__RTS_FN_GL_HEADERS_HAS", __RTS_FN_GL_HEADERS_HAS);
        add_fn!("__RTS_FN_GL_HEADERS_DELETE", __RTS_FN_GL_HEADERS_DELETE);
        add_fn!("__RTS_FN_GL_HEADERS_GET_SET_COOKIE", __RTS_FN_GL_HEADERS_GET_SET_COOKIE);
        add_fn!("__RTS_FN_GL_HEADERS_ENTRIES", __RTS_FN_GL_HEADERS_ENTRIES);
        add_fn!("__RTS_FN_GL_HEADERS_KEYS", __RTS_FN_GL_HEADERS_KEYS);
        add_fn!("__RTS_FN_GL_HEADERS_VALUES", __RTS_FN_GL_HEADERS_VALUES);
    }

    // ── namespaces::globals::fetch ────────────────────────────────────
    use crate::namespaces::globals::fetch::instance::*;
    add_fn!("__RTS_FN_GL_FETCH", __RTS_FN_GL_FETCH);
    add_fn!("__RTS_FN_GL_PROMISE_THEN", __RTS_FN_GL_PROMISE_THEN);
    add_fn!("__RTS_FN_GL_PROMISE_THEN2", __RTS_FN_GL_PROMISE_THEN2);
    add_fn!("__RTS_FN_GL_PROMISE_CATCH", __RTS_FN_GL_PROMISE_CATCH);
    add_fn!("__RTS_FN_GL_PROMISE_FINALLY", __RTS_FN_GL_PROMISE_FINALLY);
    add_fn!("__RTS_FN_GL_PROMISE_RESOLVE", __RTS_FN_GL_PROMISE_RESOLVE);
    add_fn!("__RTS_FN_GL_PROMISE_RESOLVE_EMPTY", __RTS_FN_GL_PROMISE_RESOLVE_EMPTY);
    add_fn!("__RTS_FN_GL_PROMISE_REJECT", __RTS_FN_GL_PROMISE_REJECT);
    add_fn!("__RTS_FN_GL_PROMISE_TRY", __RTS_FN_GL_PROMISE_TRY);
    add_fn!("__RTS_FN_GL_PROMISE_WITH_RESOLVERS", __RTS_FN_GL_PROMISE_WITH_RESOLVERS);
    add_fn!("__RTS_FN_GL_PROMISE_NEW", __RTS_FN_GL_PROMISE_NEW);
    add_fn!("__RTS_FN_GL_PROMISE_RESOLVER_TRAMP_RESOLVE", __RTS_FN_GL_PROMISE_RESOLVER_TRAMP_RESOLVE);
    add_fn!("__RTS_FN_GL_PROMISE_RESOLVER_TRAMP_REJECT", __RTS_FN_GL_PROMISE_RESOLVER_TRAMP_REJECT);
    add_fn!("__RTS_FN_GL_FETCH_RESPONSE_STATUS", __RTS_FN_GL_FETCH_RESPONSE_STATUS);
    add_fn!("__RTS_FN_GL_FETCH_RESPONSE_OK", __RTS_FN_GL_FETCH_RESPONSE_OK);
    add_fn!("__RTS_FN_GL_FETCH_RESPONSE_STATUS_TEXT", __RTS_FN_GL_FETCH_RESPONSE_STATUS_TEXT);
    add_fn!("__RTS_FN_GL_FETCH_RESPONSE_TEXT", __RTS_FN_GL_FETCH_RESPONSE_TEXT);
    add_fn!("__RTS_FN_GL_FETCH_RESPONSE_JSON", __RTS_FN_GL_FETCH_RESPONSE_JSON);
    add_fn!("__RTS_FN_GL_FETCH_RESPONSE_ARRAY_BUFFER", __RTS_FN_GL_FETCH_RESPONSE_ARRAY_BUFFER);
    add_fn!("__RTS_FN_GL_FETCH_RESPONSE_URL", __RTS_FN_GL_FETCH_RESPONSE_URL);
    add_fn!("__RTS_FN_GL_FETCH_RESPONSE_FREE", __RTS_FN_GL_FETCH_RESPONSE_FREE);
    add_fn!("__RTS_FN_GL_FETCH_RESPONSE_THEN", __RTS_FN_GL_FETCH_RESPONSE_THEN);
    add_fn!("__RTS_FN_GL_FETCH_RESPONSE_NEW", __RTS_FN_GL_FETCH_RESPONSE_NEW);
    add_fn!("__RTS_FN_GL_FETCH_RESPONSE_HEADERS", __RTS_FN_GL_FETCH_RESPONSE_HEADERS);
    add_fn!("__RTS_FN_GL_REQUEST_NEW", __RTS_FN_GL_REQUEST_NEW);
    add_fn!("__RTS_FN_GL_REQUEST_METHOD", __RTS_FN_GL_REQUEST_METHOD);
    add_fn!("__RTS_FN_GL_REQUEST_URL", __RTS_FN_GL_REQUEST_URL);
    add_fn!("__RTS_FN_GL_REQUEST_TEXT", __RTS_FN_GL_REQUEST_TEXT);

    // ── namespaces::globals::function (#359) ──────────────────────────
    use crate::namespaces::globals::function::ops::*;
    add_fn!("__RTS_FN_GL_FUNCTION_REIFY", __RTS_FN_GL_FUNCTION_REIFY);
    add_fn!("__RTS_FN_GL_FUNCTION_REIFY_BOUND", __RTS_FN_GL_FUNCTION_REIFY_BOUND);
    add_fn!("__RTS_FN_GL_FUNCTION_REIFY_BOUND_TYPED", __RTS_FN_GL_FUNCTION_REIFY_BOUND_TYPED);
    add_fn!("__RTS_FN_GL_FUNCTION_REIFY_CAPTURED", __RTS_FN_GL_FUNCTION_REIFY_CAPTURED);
    add_fn!("__RTS_FN_GL_FUNCTION_NEW", __RTS_FN_GL_FUNCTION_NEW);
    add_fn!("__RTS_FN_GL_FUNCTION_CALL", __RTS_FN_GL_FUNCTION_CALL);
    add_fn!("__RTS_FN_GL_FUNCTION_APPLY", __RTS_FN_GL_FUNCTION_APPLY);
    add_fn!("__RTS_FN_GL_FUNCTION_APPLY_TYPED", __RTS_FN_GL_FUNCTION_APPLY_TYPED);
    add_fn!("__RTS_FN_GL_FUNCTION_BIND", __RTS_FN_GL_FUNCTION_BIND);
    add_fn!("__RTS_FN_GL_FUNCTION_NAME", __RTS_FN_GL_FUNCTION_NAME);
    add_fn!("__RTS_FN_GL_FUNCTION_LENGTH", __RTS_FN_GL_FUNCTION_LENGTH);
    add_fn!("__RTS_FN_GL_FUNCTION_TO_STRING", __RTS_FN_GL_FUNCTION_TO_STRING);
    {
        use crate::namespaces::globals::function::props::{
            __RTS_FN_RT_FUNCTION_GET_PROP, __RTS_FN_RT_FUNCTION_SET_PROP,
            __RTS_FN_RT_FUNCTION_TO_STRING_DYN,
        };
        add_fn!("__RTS_FN_RT_FUNCTION_SET_PROP", __RTS_FN_RT_FUNCTION_SET_PROP);
        add_fn!("__RTS_FN_RT_FUNCTION_GET_PROP", __RTS_FN_RT_FUNCTION_GET_PROP);
        add_fn!("__RTS_FN_RT_FUNCTION_TO_STRING_DYN", __RTS_FN_RT_FUNCTION_TO_STRING_DYN);
    }
    add_fn!("__RTS_FN_GL_FUNCTION_PROTOTYPE_GET", __RTS_FN_GL_FUNCTION_PROTOTYPE_GET);
    add_fn!("__RTS_FN_GL_FUNCTION_PROTOTYPE_SET", __RTS_FN_GL_FUNCTION_PROTOTYPE_SET);
    add_fn!("__RTS_FN_RT_OBJECT_PROTOTYPE_HANDLE", __RTS_FN_RT_OBJECT_PROTOTYPE_HANDLE);
    add_fn!("__RTS_FN_RT_INVOKE_AUTO", __RTS_FN_RT_INVOKE_AUTO);
    add_fn!("__RTS_FN_RT_INVOKE_AUTO_TYPED", __RTS_FN_RT_INVOKE_AUTO_TYPED);
    add_fn!("__RTS_FN_RT_INVOKE_AUTO_AS_F64", __RTS_FN_RT_INVOKE_AUTO_AS_F64);
    add_fn!("__RTS_FN_RT_REGISTER_FN_KINDS", __RTS_FN_RT_REGISTER_FN_KINDS);
    add_fn!("__RTS_FN_RT_REGISTER_FN_DEFAULTS", __RTS_FN_RT_REGISTER_FN_DEFAULTS);
    add_fn!("__RTS_FN_RT_INSTANCEOF_PROTO", __RTS_FN_RT_INSTANCEOF_PROTO);
    {
        use crate::namespaces::gc::string_pool::__RTS_FN_RT_TPL_COERCE_AUTO;
        add_fn!("__RTS_FN_RT_TPL_COERCE_AUTO", __RTS_FN_RT_TPL_COERCE_AUTO);
    }
    {
        use crate::namespaces::gc::string_pool::__RTS_FN_RT_TPL_COERCE_NUM_BIAS;
        add_fn!("__RTS_FN_RT_TPL_COERCE_NUM_BIAS", __RTS_FN_RT_TPL_COERCE_NUM_BIAS);
    }
    {
        use crate::namespaces::gc::string_pool::__RTS_FN_RT_ADD_AUTO;
        add_fn!("__RTS_FN_RT_ADD_AUTO", __RTS_FN_RT_ADD_AUTO);
    }
    {
        use crate::namespaces::gc::string_pool::__RTS_FN_RT_TPL_COERCE_VEC_SLOT;
        add_fn!("__RTS_FN_RT_TPL_COERCE_VEC_SLOT", __RTS_FN_RT_TPL_COERCE_VEC_SLOT);
    }
    {
        use crate::namespaces::gc::string_pool::__RTS_FN_RT_TO_NUMBER;
        add_fn!("__RTS_FN_RT_TO_NUMBER", __RTS_FN_RT_TO_NUMBER);
    }
    {
        use crate::namespaces::gc::string_pool::__RTS_FN_RT_INSPECT;
        add_fn!("__RTS_FN_RT_INSPECT", __RTS_FN_RT_INSPECT);
    }
    {
        use crate::namespaces::gc::string_pool::__RTS_FN_RT_STRICT_EQ_AMBIG;
        add_fn!("__RTS_FN_RT_STRICT_EQ_AMBIG", __RTS_FN_RT_STRICT_EQ_AMBIG);
    }
    {
        use crate::namespaces::gc::string_pool::__RTS_FN_RT_OBJECT_TO_STRING;
        add_fn!("__RTS_FN_RT_OBJECT_TO_STRING", __RTS_FN_RT_OBJECT_TO_STRING);
    }
    {
        use crate::namespaces::collections::map::{
            __RTS_FN_NS_COLLECTIONS_MARK_AS_MAP,
            __RTS_FN_NS_COLLECTIONS_MARK_AS_SET,
        };
        add_fn!("__RTS_FN_NS_COLLECTIONS_MARK_AS_MAP", __RTS_FN_NS_COLLECTIONS_MARK_AS_MAP);
        add_fn!("__RTS_FN_NS_COLLECTIONS_MARK_AS_SET", __RTS_FN_NS_COLLECTIONS_MARK_AS_SET);
    }
    {
        use crate::namespaces::gc::string_pool::__RTS_FN_RT_UNIVERSAL_LENGTH;
        add_fn!("__RTS_FN_RT_UNIVERSAL_LENGTH", __RTS_FN_RT_UNIVERSAL_LENGTH);
    }
    {
        use crate::namespaces::gc::string_pool::__RTS_FN_RT_SPREAD_INTO_VEC;
        add_fn!("__RTS_FN_RT_SPREAD_INTO_VEC", __RTS_FN_RT_SPREAD_INTO_VEC);
    }
    {
        use crate::namespaces::gc::string_pool::__RTS_FN_RT_TRUTHY;
        add_fn!("__RTS_FN_RT_TRUTHY", __RTS_FN_RT_TRUTHY);
    }
    {
        use crate::namespaces::globals::console::rt::{
            __RTS_FN_RT_CONSOLE_GET_OVERRIDE, __RTS_FN_RT_CONSOLE_SET_OVERRIDE,
            __RTS_FN_RT_CONSOLE_OVERRIDE_IS_VARIADIC,
        };
        add_fn!("__RTS_FN_RT_CONSOLE_SET_OVERRIDE", __RTS_FN_RT_CONSOLE_SET_OVERRIDE);
        add_fn!("__RTS_FN_RT_CONSOLE_GET_OVERRIDE", __RTS_FN_RT_CONSOLE_GET_OVERRIDE);
        add_fn!("__RTS_FN_RT_CONSOLE_OVERRIDE_IS_VARIADIC", __RTS_FN_RT_CONSOLE_OVERRIDE_IS_VARIADIC);
    }
    {
        use crate::namespaces::gc::string_pool::{
            __RTS_FN_RT_TYPEOF_HANDLE, __RTS_FN_RT_TYPEOF_MEMBER_FALLBACK,
        };
        add_fn!("__RTS_FN_RT_TYPEOF_HANDLE", __RTS_FN_RT_TYPEOF_HANDLE);
        add_fn!("__RTS_FN_RT_TYPEOF_MEMBER_FALLBACK", __RTS_FN_RT_TYPEOF_MEMBER_FALLBACK);
    }
    {
        use crate::namespaces::gc::string_pool::__RTS_FN_RT_TO_STRING_HANDLE;
        add_fn!("__RTS_FN_RT_TO_STRING_HANDLE", __RTS_FN_RT_TO_STRING_HANDLE);
    }

    // ── namespaces::globals::symbol (#216) ───────────────────────────
    {
        use crate::namespaces::globals::symbol::*;
        add_fn!("__RTS_FN_GL_SYMBOL_NEW", __RTS_FN_GL_SYMBOL_NEW);
        add_fn!("__RTS_FN_GL_SYMBOL_FOR", __RTS_FN_GL_SYMBOL_FOR);
        add_fn!("__RTS_FN_GL_SYMBOL_KEY_FOR", __RTS_FN_GL_SYMBOL_KEY_FOR);
        add_fn!("__RTS_FN_GL_SYMBOL_DESCRIPTION", __RTS_FN_GL_SYMBOL_DESCRIPTION);
        add_fn!("__RTS_FN_GL_SYMBOL_TO_STRING", __RTS_FN_GL_SYMBOL_TO_STRING);
        // Well-known symbols.
        add_fn!("__RTS_FN_GL_SYMBOL_ITERATOR", __RTS_FN_GL_SYMBOL_ITERATOR);
        add_fn!("__RTS_FN_GL_SYMBOL_ASYNC_ITERATOR", __RTS_FN_GL_SYMBOL_ASYNC_ITERATOR);
        add_fn!("__RTS_FN_GL_SYMBOL_HAS_INSTANCE", __RTS_FN_GL_SYMBOL_HAS_INSTANCE);
        add_fn!("__RTS_FN_GL_SYMBOL_TO_PRIMITIVE", __RTS_FN_GL_SYMBOL_TO_PRIMITIVE);
        add_fn!("__RTS_FN_GL_SYMBOL_TO_STRING_TAG", __RTS_FN_GL_SYMBOL_TO_STRING_TAG);
        add_fn!("__RTS_FN_RT_TO_PRIMITIVE", __RTS_FN_RT_TO_PRIMITIVE);
    }

    // Boolean class
    {
        use crate::namespaces::globals::boolean::*;
        add_fn!("__RTS_FN_GL_BOOLEAN_COERCE", __RTS_FN_GL_BOOLEAN_COERCE);
        add_fn!("__RTS_FN_GL_BOOLEAN_TO_STRING", __RTS_FN_GL_BOOLEAN_TO_STRING);
        add_fn!("__RTS_FN_GL_BOOLEAN_VALUE_OF", __RTS_FN_GL_BOOLEAN_VALUE_OF);
        add_fn!("__RTS_FN_GL_BOOLEAN_NEW", __RTS_FN_GL_BOOLEAN_NEW);
        add_fn!("__RTS_FN_GL_BOOLEAN_NEW_EMPTY", __RTS_FN_GL_BOOLEAN_NEW_EMPTY);
    }

    // (cross-runtime #742) BigInt.asIntN / asUintN helpers staticos.
    {
        use crate::namespaces::globals::bigint::*;
        add_fn!("__RTS_FN_GL_BIGINT_AS_INT_N", __RTS_FN_GL_BIGINT_AS_INT_N);
        add_fn!("__RTS_FN_GL_BIGINT_AS_UINT_N", __RTS_FN_GL_BIGINT_AS_UINT_N);
    }

    // (#208) encodeURIComponent / decodeURIComponent globais.
    {
        use crate::namespaces::globals::global_this::rt::*;
        add_fn!("__RTS_FN_GL_ENCODE_URI_COMPONENT", __RTS_FN_GL_ENCODE_URI_COMPONENT);
        add_fn!("__RTS_FN_GL_DECODE_URI_COMPONENT", __RTS_FN_GL_DECODE_URI_COMPONENT);
        add_fn!("__RTS_FN_GL_ENCODE_URI", __RTS_FN_GL_ENCODE_URI);
        add_fn!("__RTS_FN_GL_DECODE_URI", __RTS_FN_GL_DECODE_URI);
    }

    // ── namespaces::globals::weakmap (#217 v0) ───────────────────────
    {
        use crate::namespaces::globals::weakmap::*;
        add_fn!("__RTS_FN_GL_WEAKMAP_NEW", __RTS_FN_GL_WEAKMAP_NEW);
        add_fn!("__RTS_FN_GL_WEAKMAP_SET", __RTS_FN_GL_WEAKMAP_SET);
        add_fn!("__RTS_FN_GL_WEAKMAP_GET", __RTS_FN_GL_WEAKMAP_GET);
        add_fn!("__RTS_FN_GL_WEAKMAP_HAS", __RTS_FN_GL_WEAKMAP_HAS);
        add_fn!("__RTS_FN_GL_WEAKMAP_DELETE", __RTS_FN_GL_WEAKMAP_DELETE);
    }

    // ── namespaces::globals::weakset (#217 v0) ───────────────────────
    {
        use crate::namespaces::globals::weakset::*;
        add_fn!("__RTS_FN_GL_WEAKSET_NEW", __RTS_FN_GL_WEAKSET_NEW);
        add_fn!("__RTS_FN_GL_WEAKSET_ADD", __RTS_FN_GL_WEAKSET_ADD);
        add_fn!("__RTS_FN_GL_WEAKSET_HAS", __RTS_FN_GL_WEAKSET_HAS);
        add_fn!("__RTS_FN_GL_WEAKSET_DELETE", __RTS_FN_GL_WEAKSET_DELETE);
    }

    // ── namespaces::globals::weakref (#685 v0) ────────────────────────
    {
        use crate::namespaces::globals::weakref::*;
        add_fn!("__RTS_FN_GL_WEAKREF_NEW", __RTS_FN_GL_WEAKREF_NEW);
        add_fn!("__RTS_FN_GL_WEAKREF_DEREF", __RTS_FN_GL_WEAKREF_DEREF);
    }

    // ── namespaces::globals::finalization_registry (#685 v0) ──────────
    {
        use crate::namespaces::globals::finalization_registry::*;
        add_fn!("__RTS_FN_GL_FINREG_NEW", __RTS_FN_GL_FINREG_NEW);
        add_fn!("__RTS_FN_GL_FINREG_REGISTER", __RTS_FN_GL_FINREG_REGISTER);
        add_fn!("__RTS_FN_GL_FINREG_UNREGISTER", __RTS_FN_GL_FINREG_UNREGISTER);
    }

    // ── namespaces::globals::reflect (#218) ──────────────────────────
    {
        use crate::namespaces::globals::reflect::ops::*;
        add_fn!(
            "__RTS_FN_GL_REFLECT_GET_OWN_PROPERTY_DESCRIPTOR",
            __RTS_FN_GL_REFLECT_GET_OWN_PROPERTY_DESCRIPTOR
        );
        add_fn!(
            "__RTS_FN_GL_OBJECT_GET_OWN_PROPERTY_DESCRIPTORS",
            __RTS_FN_GL_OBJECT_GET_OWN_PROPERTY_DESCRIPTORS
        );
        add_fn!(
            "__RTS_FN_GL_REFLECT_DEFINE_PROPERTY",
            __RTS_FN_GL_REFLECT_DEFINE_PROPERTY
        );
    }

    // ── namespaces::globals::proxy (#218 phase 1+2+3) ────────────────
    {
        use crate::namespaces::globals::proxy::ops::*;
        add_fn!("__RTS_FN_GL_PROXY_NEW", __RTS_FN_GL_PROXY_NEW);
        add_fn!("__RTS_FN_GL_REFLECT_CONSTRUCT", __RTS_FN_GL_REFLECT_CONSTRUCT);
        add_fn!(
            "__RTS_FN_GL_REFLECT_SET_PROTOTYPE_OF",
            __RTS_FN_GL_REFLECT_SET_PROTOTYPE_OF
        );
        add_fn!(
            "__RTS_FN_GL_REFLECT_DEFINE_PROPERTY_PROXY",
            __RTS_FN_GL_REFLECT_DEFINE_PROPERTY_PROXY
        );
        add_fn!(
            "__RTS_FN_GL_REFLECT_GET_OWN_PROPERTY_DESCRIPTOR_PROXY",
            __RTS_FN_GL_REFLECT_GET_OWN_PROPERTY_DESCRIPTOR_PROXY
        );
    }

    // ── namespaces::globals::text_encoding ───────────────────────────
    use crate::namespaces::globals::text_encoding::instance::*;
    add_fn!("__RTS_FN_GL_TEXTENC_ENCODE", __RTS_FN_GL_TEXTENC_ENCODE);
    add_fn!("__RTS_FN_GL_TEXTENC_DECODE", __RTS_FN_GL_TEXTENC_DECODE);
    add_fn!("__RTS_FN_GL_TEXTENC_BTOA", __RTS_FN_GL_TEXTENC_BTOA);
    add_fn!("__RTS_FN_GL_TEXTENC_ATOB", __RTS_FN_GL_TEXTENC_ATOB);
    add_fn!("__RTS_FN_GL_TEXTENC_STRUCTURED_CLONE", __RTS_FN_GL_TEXTENC_STRUCTURED_CLONE);
    add_fn!("__RTS_FN_GL_TEXTENC_QUEUE_MICROTASK", __RTS_FN_GL_TEXTENC_QUEUE_MICROTASK);
    add_fn!("__RTS_FN_GL_TEXTENC_NEW", __RTS_FN_GL_TEXTENC_NEW);
    add_fn!("__RTS_FN_GL_TEXTDEC_NEW", __RTS_FN_GL_TEXTDEC_NEW);
    add_fn!("__RTS_FN_GL_TEXTENC_ENCODE_INSTANCE", __RTS_FN_GL_TEXTENC_ENCODE_INSTANCE);
    add_fn!("__RTS_FN_GL_TEXTDEC_DECODE_INSTANCE", __RTS_FN_GL_TEXTDEC_DECODE_INSTANCE);

    // ── namespaces::globals::performance ─────────────────────────────
    use crate::namespaces::globals::performance::*;
    add_fn!("__RTS_FN_GL_PERF_NOW", __RTS_FN_GL_PERF_NOW);
    add_fn!("__RTS_FN_GL_PERF_TIME_ORIGIN", __RTS_FN_GL_PERF_TIME_ORIGIN);

    // ── namespaces::globals::url ──────────────────────────────────────
    use crate::namespaces::globals::url::instance::*;
    add_fn!("__RTS_FN_GL_URL_NEW", __RTS_FN_GL_URL_NEW);
    add_fn!("__RTS_FN_GL_URL_NEW_WITH_BASE", __RTS_FN_GL_URL_NEW_WITH_BASE);
    add_fn!("__RTS_FN_GL_URL_CAN_PARSE", __RTS_FN_GL_URL_CAN_PARSE);
    add_fn!("__RTS_FN_GL_URL_CAN_PARSE_BASE", __RTS_FN_GL_URL_CAN_PARSE_BASE);
    add_fn!("__RTS_FN_GL_URL_HREF", __RTS_FN_GL_URL_HREF);
    add_fn!("__RTS_FN_GL_URL_PROTOCOL", __RTS_FN_GL_URL_PROTOCOL);
    add_fn!("__RTS_FN_GL_URL_HOST", __RTS_FN_GL_URL_HOST);
    add_fn!("__RTS_FN_GL_URL_HOSTNAME", __RTS_FN_GL_URL_HOSTNAME);
    add_fn!("__RTS_FN_GL_URL_PORT", __RTS_FN_GL_URL_PORT);
    add_fn!("__RTS_FN_GL_URL_PATHNAME", __RTS_FN_GL_URL_PATHNAME);
    add_fn!("__RTS_FN_GL_URL_SEARCH", __RTS_FN_GL_URL_SEARCH);
    add_fn!("__RTS_FN_GL_URL_HASH", __RTS_FN_GL_URL_HASH);
    add_fn!("__RTS_FN_GL_URL_ORIGIN", __RTS_FN_GL_URL_ORIGIN);
    add_fn!("__RTS_FN_GL_URL_USERNAME", __RTS_FN_GL_URL_USERNAME);
    add_fn!("__RTS_FN_GL_URL_PASSWORD", __RTS_FN_GL_URL_PASSWORD);
    add_fn!("__RTS_FN_GL_URL_FREE", __RTS_FN_GL_URL_FREE);
    add_fn!("__RTS_FN_GL_URL_SEARCH_PARAMS", __RTS_FN_GL_URL_SEARCH_PARAMS);
    // (#67) URL setters + dynamic toString.
    add_fn!("__RTS_FN_GL_URL_SET_PATHNAME", __RTS_FN_GL_URL_SET_PATHNAME);
    add_fn!("__RTS_FN_GL_URL_TO_STRING", __RTS_FN_GL_URL_TO_STRING);
    // (#373) URLSearchParams.
    add_fn!("__RTS_FN_GL_USP_NEW", __RTS_FN_GL_USP_NEW);
    add_fn!("__RTS_FN_GL_USP_GET", __RTS_FN_GL_USP_GET);
    add_fn!("__RTS_FN_GL_USP_HAS", __RTS_FN_GL_USP_HAS);
    add_fn!("__RTS_FN_GL_USP_SET", __RTS_FN_GL_USP_SET);
    add_fn!("__RTS_FN_GL_USP_DELETE", __RTS_FN_GL_USP_DELETE);
    add_fn!("__RTS_FN_GL_USP_TO_STRING", __RTS_FN_GL_USP_TO_STRING);
    add_fn!("__RTS_FN_GL_USP_APPEND", __RTS_FN_GL_USP_APPEND);
    add_fn!("__RTS_FN_GL_USP_GET_ALL", __RTS_FN_GL_USP_GET_ALL);
    add_fn!("__RTS_FN_GL_USP_KEYS", __RTS_FN_GL_USP_KEYS);
    add_fn!("__RTS_FN_GL_USP_VALUES", __RTS_FN_GL_USP_VALUES);
    add_fn!("__RTS_FN_GL_USP_SORT", __RTS_FN_GL_USP_SORT);

    // ── namespaces::date ──────────────────────────────────────────────
    use crate::namespaces::date::*;
    add_fn!("__RTS_FN_NS_DATE_NOW_MS", __RTS_FN_NS_DATE_NOW_MS);
    add_fn!("__RTS_FN_NS_DATE_FROM_ISO", __RTS_FN_NS_DATE_FROM_ISO);
    add_fn!("__RTS_FN_NS_DATE_PARSE_F64", __RTS_FN_NS_DATE_PARSE_F64);
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
    add_fn!("__RTS_FN_NS_FS_READ", __RTS_FN_NS_FS_READ);
    add_fn!("__RTS_FN_NS_FS_READ_ALL", __RTS_FN_NS_FS_READ_ALL);
    add_fn!("__RTS_FN_NS_FS_READ_TEXT", __RTS_FN_NS_FS_READ_TEXT);
    add_fn!("__RTS_FN_NS_FS_WRITE", __RTS_FN_NS_FS_WRITE);
    add_fn!(
        "__RTS_FN_NS_FS_WRITE_BYTES",
        __RTS_FN_NS_FS_WRITE_BYTES
    );
    add_fn!("__RTS_FN_NS_FS_APPEND", __RTS_FN_NS_FS_APPEND);
    add_fn!("__RTS_FN_NS_FS_EXISTS", __RTS_FN_NS_FS_EXISTS);
    add_fn!("__RTS_FN_NS_FS_IS_FILE", __RTS_FN_NS_FS_IS_FILE);
    add_fn!("__RTS_FN_NS_FS_IS_DIR", __RTS_FN_NS_FS_IS_DIR);
    add_fn!("__RTS_FN_NS_FS_SIZE", __RTS_FN_NS_FS_SIZE);
    add_fn!(
        "__RTS_FN_NS_FS_MODIFIED_MS",
        __RTS_FN_NS_FS_MODIFIED_MS
    );
    add_fn!("__RTS_FN_NS_FS_CREATE_DIR", __RTS_FN_NS_FS_CREATE_DIR);
    add_fn!(
        "__RTS_FN_NS_FS_CREATE_DIR_ALL",
        __RTS_FN_NS_FS_CREATE_DIR_ALL
    );
    add_fn!("__RTS_FN_NS_FS_REMOVE_DIR", __RTS_FN_NS_FS_REMOVE_DIR);
    add_fn!(
        "__RTS_FN_NS_FS_REMOVE_DIR_ALL",
        __RTS_FN_NS_FS_REMOVE_DIR_ALL
    );
    add_fn!(
        "__RTS_FN_NS_FS_REMOVE_FILE",
        __RTS_FN_NS_FS_REMOVE_FILE
    );
    add_fn!("__RTS_FN_NS_FS_RENAME", __RTS_FN_NS_FS_RENAME);
    add_fn!("__RTS_FN_NS_FS_COPY", __RTS_FN_NS_FS_COPY);
    add_fn!("__RTS_FN_NS_FS_READDIR", __RTS_FN_NS_FS_READDIR);

    // ── namespaces::math ──────────────────────────────────────────────
    use crate::namespaces::math::*;
    add_fn!("__RTS_FN_NS_MATH_FLOOR", __RTS_FN_NS_MATH_FLOOR);
    add_fn!("__RTS_FN_NS_MATH_CEIL", __RTS_FN_NS_MATH_CEIL);
    add_fn!("__RTS_FN_NS_MATH_ROUND", __RTS_FN_NS_MATH_ROUND);
    add_fn!("__RTS_FN_NS_MATH_TRUNC", __RTS_FN_NS_MATH_TRUNC);
    add_fn!("__RTS_FN_NS_MATH_SQRT", __RTS_FN_NS_MATH_SQRT);
    add_fn!("__RTS_FN_NS_MATH_CBRT", __RTS_FN_NS_MATH_CBRT);
    add_fn!("__RTS_FN_NS_MATH_POW", __RTS_FN_NS_MATH_POW);
    add_fn!("__RTS_FN_NS_MATH_EXP", __RTS_FN_NS_MATH_EXP);
    add_fn!("__RTS_FN_NS_MATH_LN", __RTS_FN_NS_MATH_LN);
    add_fn!("__RTS_FN_NS_MATH_LOG2", __RTS_FN_NS_MATH_LOG2);
    add_fn!("__RTS_FN_NS_MATH_LOG10", __RTS_FN_NS_MATH_LOG10);
    add_fn!("__RTS_FN_NS_MATH_ABS_F64", __RTS_FN_NS_MATH_ABS_F64);
    add_fn!("__RTS_FN_NS_MATH_ABS_I64", __RTS_FN_NS_MATH_ABS_I64);
    // (#208) Math extras.
    add_fn!("__RTS_FN_NS_MATH_SIGN", __RTS_FN_NS_MATH_SIGN);
    add_fn!("__RTS_FN_NS_MATH_HYPOT", __RTS_FN_NS_MATH_HYPOT);
    add_fn!("__RTS_FN_NS_MATH_EXPM1", __RTS_FN_NS_MATH_EXPM1);
    add_fn!("__RTS_FN_NS_MATH_LOG1P", __RTS_FN_NS_MATH_LOG1P);
    add_fn!("__RTS_FN_NS_MATH_FROUND", __RTS_FN_NS_MATH_FROUND);
    add_fn!("__RTS_FN_NS_MATH_F16ROUND", __RTS_FN_NS_MATH_F16ROUND);
    add_fn!("__RTS_FN_NS_MATH_SINH", __RTS_FN_NS_MATH_SINH);
    add_fn!("__RTS_FN_NS_MATH_COSH", __RTS_FN_NS_MATH_COSH);
    add_fn!("__RTS_FN_NS_MATH_TANH", __RTS_FN_NS_MATH_TANH);
    add_fn!("__RTS_FN_NS_MATH_ASINH", __RTS_FN_NS_MATH_ASINH);
    add_fn!("__RTS_FN_NS_MATH_ACOSH", __RTS_FN_NS_MATH_ACOSH);
    add_fn!("__RTS_FN_NS_MATH_ATANH", __RTS_FN_NS_MATH_ATANH);
    add_fn!("__RTS_FN_NS_MATH_IMUL", __RTS_FN_NS_MATH_IMUL);
    add_fn!("__RTS_FN_NS_MATH_CLZ32", __RTS_FN_NS_MATH_CLZ32);
    add_fn!("__RTS_FN_NS_MATH_SIN", __RTS_FN_NS_MATH_SIN);
    add_fn!("__RTS_FN_NS_MATH_COS", __RTS_FN_NS_MATH_COS);
    add_fn!("__RTS_FN_NS_MATH_TAN", __RTS_FN_NS_MATH_TAN);
    add_fn!("__RTS_FN_NS_MATH_ASIN", __RTS_FN_NS_MATH_ASIN);
    add_fn!("__RTS_FN_NS_MATH_ACOS", __RTS_FN_NS_MATH_ACOS);
    add_fn!("__RTS_FN_NS_MATH_ATAN", __RTS_FN_NS_MATH_ATAN);
    add_fn!("__RTS_FN_NS_MATH_ATAN2", __RTS_FN_NS_MATH_ATAN2);
    add_fn!("__RTS_FN_NS_MATH_MIN_F64", __RTS_FN_NS_MATH_MIN_F64);
    add_fn!("__RTS_FN_NS_MATH_MAX_F64", __RTS_FN_NS_MATH_MAX_F64);
    add_fn!("__RTS_FN_NS_MATH_MIN_I64", __RTS_FN_NS_MATH_MIN_I64);
    add_fn!("__RTS_FN_NS_MATH_MAX_I64", __RTS_FN_NS_MATH_MAX_I64);
    add_fn!(
        "__RTS_FN_NS_MATH_CLAMP_F64",
        __RTS_FN_NS_MATH_CLAMP_F64
    );
    add_fn!(
        "__RTS_FN_NS_MATH_CLAMP_I64",
        __RTS_FN_NS_MATH_CLAMP_I64
    );
    add_fn!(
        "__RTS_FN_NS_MATH_RANDOM_F64",
        __RTS_FN_NS_MATH_RANDOM_F64
    );
    add_fn!(
        "__RTS_FN_NS_MATH_RANDOM_I64_RANGE",
        __RTS_FN_NS_MATH_RANDOM_I64_RANGE
    );
    add_fn!("__RTS_FN_NS_MATH_SEED", __RTS_FN_NS_MATH_SEED);
    add_fn!("__RTS_FN_NS_MATH_PI", __RTS_FN_NS_MATH_PI);
    add_fn!("__RTS_FN_NS_MATH_E", __RTS_FN_NS_MATH_E);
    add_fn!(
        "__RTS_FN_NS_MATH_INFINITY",
        __RTS_FN_NS_MATH_INFINITY
    );
    add_fn!("__RTS_FN_NS_MATH_NAN", __RTS_FN_NS_MATH_NAN);
    // (#208) Math constants extras.
    add_fn!("__RTS_FN_NS_MATH_SQRT2", __RTS_FN_NS_MATH_SQRT2);
    add_fn!("__RTS_FN_NS_MATH_SQRT1_2", __RTS_FN_NS_MATH_SQRT1_2);
    add_fn!("__RTS_FN_NS_MATH_LN2", __RTS_FN_NS_MATH_LN2);
    add_fn!("__RTS_FN_NS_MATH_LN10", __RTS_FN_NS_MATH_LN10);
    add_fn!("__RTS_FN_NS_MATH_LOG2E", __RTS_FN_NS_MATH_LOG2E);
    add_fn!("__RTS_FN_NS_MATH_LOG10E", __RTS_FN_NS_MATH_LOG10E);

    // ── namespaces::num ───────────────────────────────────────────────
    {
        use crate::namespaces::num as n;
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
        use crate::namespaces::mem as m;
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
        use crate::namespaces::trace as tr;
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
        use crate::namespaces::alloc as a;
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
        use crate::namespaces::ptr as p;
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
            __RTS_FN_NS_CRYPTO_RANDOM_BYTES
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_RANDOM_I64",
            __RTS_FN_NS_CRYPTO_RANDOM_I64
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_RANDOM_BUFFER",
            __RTS_FN_NS_CRYPTO_RANDOM_BUFFER
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_RANDOM_UUID",
            __RTS_FN_NS_CRYPTO_RANDOM_UUID
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_HASH_NEW",
            __RTS_FN_NS_CRYPTO_HASH_NEW
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_HASH_UPDATE_STR",
            __RTS_FN_NS_CRYPTO_HASH_UPDATE_STR
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_HASH_UPDATE_BYTES",
            __RTS_FN_NS_CRYPTO_HASH_UPDATE_BYTES
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_HASH_DIGEST_HEX",
            __RTS_FN_NS_CRYPTO_HASH_DIGEST_HEX
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_HASH_DIGEST_BASE64",
            __RTS_FN_NS_CRYPTO_HASH_DIGEST_BASE64
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_SHA256_STR",
            __RTS_FN_NS_CRYPTO_SHA256_STR
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_SHA256_BYTES",
            __RTS_FN_NS_CRYPTO_SHA256_BYTES
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_SHA256_DIGEST",
            __RTS_FN_NS_CRYPTO_SHA256_DIGEST
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_HEX_ENCODE",
            __RTS_FN_NS_CRYPTO_HEX_ENCODE
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_HEX_DECODE",
            __RTS_FN_NS_CRYPTO_HEX_DECODE
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_BASE64_ENCODE",
            __RTS_FN_NS_CRYPTO_BASE64_ENCODE
        );
        add_fn!(
            "__RTS_FN_NS_CRYPTO_BASE64_DECODE",
            __RTS_FN_NS_CRYPTO_BASE64_DECODE
        );
    }

    // ── namespaces::fmt ───────────────────────────────────────────────
    {
        use crate::namespaces::fmt::*;
        add_fn!("__RTS_FN_NS_FMT_PARSE_I64", __RTS_FN_NS_FMT_PARSE_I64);
        // (#208) parseInt JS-spec com radix.
        add_fn!(
            "__RTS_FN_NS_FMT_PARSE_INT_RADIX",
            __RTS_FN_NS_FMT_PARSE_INT_RADIX
        );
        add_fn!("__RTS_FN_NS_FMT_PARSE_F64", __RTS_FN_NS_FMT_PARSE_F64);
        add_fn!("__RTS_FN_NS_FMT_PARSE_BOOL", __RTS_FN_NS_FMT_PARSE_BOOL);
        add_fn!("__RTS_FN_NS_FMT_FMT_I64", __RTS_FN_NS_FMT_FMT_I64);
        add_fn!("__RTS_FN_NS_FMT_FMT_F64", __RTS_FN_NS_FMT_FMT_F64);
        add_fn!("__RTS_FN_NS_FMT_FMT_BOOL", __RTS_FN_NS_FMT_FMT_BOOL);
        add_fn!("__RTS_FN_NS_FMT_FMT_HEX", __RTS_FN_NS_FMT_FMT_HEX);
        add_fn!("__RTS_FN_NS_FMT_FMT_BIN", __RTS_FN_NS_FMT_FMT_BIN);
        add_fn!("__RTS_FN_NS_FMT_FMT_OCT", __RTS_FN_NS_FMT_FMT_OCT);
        add_fn!("__RTS_FN_NS_FMT_FMT_F64_PREC", __RTS_FN_NS_FMT_FMT_F64_PREC);
    }

    // ── namespaces::hash ──────────────────────────────────────────────
    {
        use crate::namespaces::hash as h;
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
        use crate::namespaces::hint as ht;
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
        use crate::namespaces::regex as rx;
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
        use crate::namespaces::events as ev;
        add_fn!("__RTS_FN_NS_EVENTS_EMITTER_NEW", ev::__RTS_FN_NS_EVENTS_EMITTER_NEW);
        add_fn!("__RTS_FN_NS_EVENTS_EMITTER_FREE", ev::__RTS_FN_NS_EVENTS_EMITTER_FREE);
        add_fn!("__RTS_FN_NS_EVENTS_ON", ev::__RTS_FN_NS_EVENTS_ON);
        add_fn!("__RTS_FN_NS_EVENTS_OFF", ev::__RTS_FN_NS_EVENTS_OFF);
        add_fn!("__RTS_FN_NS_EVENTS_REMOVE_ALL_LISTENERS", ev::__RTS_FN_NS_EVENTS_REMOVE_ALL_LISTENERS);
        add_fn!("__RTS_FN_NS_EVENTS_LISTENER_COUNT", ev::__RTS_FN_NS_EVENTS_LISTENER_COUNT);
        add_fn!("__RTS_FN_NS_EVENTS_EMIT0", ev::__RTS_FN_NS_EVENTS_EMIT0);
        add_fn!("__RTS_FN_NS_EVENTS_EMIT1", ev::__RTS_FN_NS_EVENTS_EMIT1);
        add_fn!(
            "__RTS_FN_NS_EVENTS_EMIT0_ASYNC",
            ev::__RTS_FN_NS_EVENTS_EMIT0_ASYNC
        );
        add_fn!(
            "__RTS_FN_NS_EVENTS_EMIT1_ASYNC",
            ev::__RTS_FN_NS_EVENTS_EMIT1_ASYNC
        );
    }

    // ── namespaces::promise (issue #412) ──────────────────────────────
    {
        use crate::namespaces::promise as pr;
        add_fn!("__RTS_FN_NS_PROMISE_NEW_PENDING", pr::__RTS_FN_NS_PROMISE_NEW_PENDING);
        add_fn!("__RTS_FN_NS_PROMISE_NEW_RESOLVED", pr::__RTS_FN_NS_PROMISE_NEW_RESOLVED);
        add_fn!("__RTS_FN_NS_PROMISE_NEW_REJECTED", pr::__RTS_FN_NS_PROMISE_NEW_REJECTED);
        add_fn!("__RTS_FN_NS_PROMISE_RESOLVE", pr::__RTS_FN_NS_PROMISE_RESOLVE);
        add_fn!("__RTS_FN_NS_PROMISE_REJECT", pr::__RTS_FN_NS_PROMISE_REJECT);
        add_fn!("__RTS_FN_NS_PROMISE_STATE", pr::__RTS_FN_NS_PROMISE_STATE);
        add_fn!("__RTS_FN_NS_PROMISE_WAIT", pr::__RTS_FN_NS_PROMISE_WAIT);
        add_fn!("__RTS_FN_NS_PROMISE_AWAIT_VALUE", pr::__RTS_FN_NS_PROMISE_AWAIT_VALUE);
        add_fn!("__RTS_FN_NS_PROMISE_TRY_VALUE", pr::__RTS_FN_NS_PROMISE_TRY_VALUE);
        add_fn!("__RTS_FN_NS_PROMISE_TAKE_ERROR", pr::__RTS_FN_NS_PROMISE_TAKE_ERROR);
        add_fn!("__RTS_FN_NS_PROMISE_THEN", pr::__RTS_FN_NS_PROMISE_THEN);
        add_fn!("__RTS_FN_NS_PROMISE_CATCH", pr::__RTS_FN_NS_PROMISE_CATCH);
        add_fn!("__RTS_FN_NS_PROMISE_FINALLY", pr::__RTS_FN_NS_PROMISE_FINALLY);
        add_fn!("__RTS_FN_NS_PROMISE_ALL", pr::__RTS_FN_NS_PROMISE_ALL);
        add_fn!("__RTS_FN_NS_PROMISE_RACE", pr::__RTS_FN_NS_PROMISE_RACE);
        add_fn!("__RTS_FN_NS_PROMISE_ANY", pr::__RTS_FN_NS_PROMISE_ANY);
        add_fn!("__RTS_FN_NS_PROMISE_ALL_SETTLED", pr::__RTS_FN_NS_PROMISE_ALL_SETTLED);
        add_fn!("__RTS_FN_NS_PROMISE_CREATE", pr::__RTS_FN_NS_PROMISE_CREATE);
        // (#861) Array.fromAsync — implementacao vive em promise/ops.rs por
        // reutilizar a infra de Promise.all e PromiseSlot.
        add_fn!("__RTS_FN_GL_ARRAY_FROM_ASYNC", pr::__RTS_FN_GL_ARRAY_FROM_ASYNC);
    }

    // ── namespaces::collections ───────────────────────────────────────
    {
        use crate::namespaces::collections::*;
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_NEW",
            map::__RTS_FN_NS_COLLECTIONS_MAP_NEW
        );
        add_fn!(
            "__RTS_FN_RT_GLOBAL_THIS_MAP",
            map::__RTS_FN_RT_GLOBAL_THIS_MAP
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
            "__RTS_FN_NS_COLLECTIONS_OBJ_HAS",
            map::__RTS_FN_NS_COLLECTIONS_OBJ_HAS
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_SET_KH",
            map::__RTS_FN_NS_COLLECTIONS_MAP_SET_KH
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_GET_KH",
            map::__RTS_FN_NS_COLLECTIONS_MAP_GET_KH
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_OBJ_SET",
            map::__RTS_FN_NS_COLLECTIONS_OBJ_SET
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_OBJ_GET",
            map::__RTS_FN_NS_COLLECTIONS_OBJ_GET
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_GET",
            map::__RTS_FN_NS_COLLECTIONS_MAP_GET
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_GET_DIRECT",
            map::__RTS_FN_NS_COLLECTIONS_MAP_GET_DIRECT
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_GET_CHAIN",
            map::__RTS_FN_NS_COLLECTIONS_MAP_GET_CHAIN
        );
        add_fn!(
            "__RTS_FN_GL_OBJECT_CREATE",
            map::__RTS_FN_GL_OBJECT_CREATE
        );
        add_fn!(
            "__RTS_FN_GL_OBJECT_APPLY_DESCRIPTORS",
            map::__RTS_FN_GL_OBJECT_APPLY_DESCRIPTORS
        );
        add_fn!(
            "__RTS_FN_GL_OBJECT_HAS_OWN_PROPERTY",
            map::__RTS_FN_GL_OBJECT_HAS_OWN_PROPERTY
        );
        add_fn!(
            "__RTS_FN_GL_OBJECT_GET_OWN_PROPERTY_SYMBOLS",
            map::__RTS_FN_GL_OBJECT_GET_OWN_PROPERTY_SYMBOLS
        );
        add_fn!(
            "__RTS_FN_GL_OBJECT_PROPERTY_IS_ENUMERABLE",
            map::__RTS_FN_GL_OBJECT_PROPERTY_IS_ENUMERABLE
        );
        add_fn!(
            "__RTS_FN_GL_OBJECT_IS_PROTOTYPE_OF",
            map::__RTS_FN_GL_OBJECT_IS_PROTOTYPE_OF
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
            "__RTS_FN_NS_COLLECTIONS_MAP_DELETE_AUTO",
            map::__RTS_FN_NS_COLLECTIONS_MAP_DELETE_AUTO
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_INDEX_DELETE_AUTO",
            vec::__RTS_FN_NS_COLLECTIONS_INDEX_DELETE_AUTO
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
            "__RTS_FN_NS_COLLECTIONS_OBJECT_KEYS_AUTO",
            map::__RTS_FN_NS_COLLECTIONS_OBJECT_KEYS_AUTO
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_FOR_IN_KEYS",
            map::__RTS_FN_NS_COLLECTIONS_FOR_IN_KEYS
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_OBJECT_OWN_PROPERTY_NAMES",
            map::__RTS_FN_NS_COLLECTIONS_OBJECT_OWN_PROPERTY_NAMES
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_VALUES",
            map::__RTS_FN_NS_COLLECTIONS_MAP_VALUES
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_FOR_EACH",
            map::__RTS_FN_NS_COLLECTIONS_MAP_FOR_EACH
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_SET_FOR_EACH",
            map::__RTS_FN_NS_COLLECTIONS_SET_FOR_EACH
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_SET_UNION",
            map::__RTS_FN_NS_COLLECTIONS_SET_UNION
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_SET_INTERSECTION",
            map::__RTS_FN_NS_COLLECTIONS_SET_INTERSECTION
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_SET_DIFFERENCE",
            map::__RTS_FN_NS_COLLECTIONS_SET_DIFFERENCE
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_SET_SYMMETRIC_DIFFERENCE",
            map::__RTS_FN_NS_COLLECTIONS_SET_SYMMETRIC_DIFFERENCE
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_SET_IS_SUBSET",
            map::__RTS_FN_NS_COLLECTIONS_SET_IS_SUBSET
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_SET_IS_SUPERSET",
            map::__RTS_FN_NS_COLLECTIONS_SET_IS_SUPERSET
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_SET_IS_DISJOINT",
            map::__RTS_FN_NS_COLLECTIONS_SET_IS_DISJOINT
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_OBJECT_GROUP_BY",
            map::__RTS_FN_NS_COLLECTIONS_OBJECT_GROUP_BY
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_GROUP_BY",
            map::__RTS_FN_NS_COLLECTIONS_MAP_GROUP_BY
        );
        // (#208 / #479) Object static methods.
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_ENTRIES",
            map::__RTS_FN_NS_COLLECTIONS_MAP_ENTRIES
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_ENTRIES_INSERTION",
            map::__RTS_FN_NS_COLLECTIONS_MAP_ENTRIES_INSERTION
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_ASSIGN",
            map::__RTS_FN_NS_COLLECTIONS_MAP_ASSIGN
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_PREVENT_EXTENSIONS",
            map::__RTS_FN_NS_COLLECTIONS_PREVENT_EXTENSIONS
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_IS_EXTENSIBLE",
            map::__RTS_FN_NS_COLLECTIONS_IS_EXTENSIBLE
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_FREEZE",
            map::__RTS_FN_NS_COLLECTIONS_MAP_FREEZE
        );
        // (#208) Object.seal/isFrozen/isSealed/getPrototypeOf/defineProperty.
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_SEAL",
            map::__RTS_FN_NS_COLLECTIONS_MAP_SEAL
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_IS_FROZEN",
            map::__RTS_FN_NS_COLLECTIONS_MAP_IS_FROZEN
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_IS_SEALED",
            map::__RTS_FN_NS_COLLECTIONS_MAP_IS_SEALED
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_GET_PROTO",
            map::__RTS_FN_NS_COLLECTIONS_MAP_GET_PROTO
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_DEFINE_PROPERTY",
            map::__RTS_FN_NS_COLLECTIONS_MAP_DEFINE_PROPERTY
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_REGISTER_CLASS_METHOD",
            map::__RTS_FN_NS_COLLECTIONS_REGISTER_CLASS_METHOD
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_MAP_FROM_ENTRIES",
            map::__RTS_FN_NS_COLLECTIONS_MAP_FROM_ENTRIES
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_SET_FROM_VEC",
            map::__RTS_FN_NS_COLLECTIONS_SET_FROM_VEC
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_SET_ADD",
            map::__RTS_FN_NS_COLLECTIONS_SET_ADD
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_SET_OR_MAP_HAS",
            map::__RTS_FN_NS_COLLECTIONS_SET_OR_MAP_HAS
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_SET_OR_MAP_DELETE",
            map::__RTS_FN_NS_COLLECTIONS_SET_OR_MAP_DELETE
        );
        add_fn!(
            "__RTS_FN_RT_FOR_OF_NORMALIZE",
            map::__RTS_FN_RT_FOR_OF_NORMALIZE
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
            "__RTS_FN_NS_COLLECTIONS_VEC_EXTEND_FROM",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_EXTEND_FROM
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_FILL_TA_ARG",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_FILL_TA_ARG
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_EXTEND_FROM_BUFFER",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_EXTEND_FROM_BUFFER
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_SET_FROM",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_SET_FROM
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_POP",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_POP
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_MIN",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_MIN
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_MAX",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_MAX
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_SET_LENGTH",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_SET_LENGTH
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_GET",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_GET
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_INDEX_GET_AUTO",
            vec::__RTS_FN_NS_COLLECTIONS_INDEX_GET_AUTO
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_CONCAT_AUTO",
            vec::__RTS_FN_NS_COLLECTIONS_CONCAT_AUTO
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_SLICE_AUTO",
            vec::__RTS_FN_NS_COLLECTIONS_SLICE_AUTO
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_INCLUDES_AUTO",
            vec::__RTS_FN_NS_COLLECTIONS_INCLUDES_AUTO
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_INDEX_OF_AUTO",
            vec::__RTS_FN_NS_COLLECTIONS_INDEX_OF_AUTO
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_LAST_INDEX_OF_AUTO",
            vec::__RTS_FN_NS_COLLECTIONS_LAST_INDEX_OF_AUTO
        );
        add_fn!(
            "__RTS_FN_NS_GC_CLASS_REGISTER_PARENT",
            crate::namespaces::gc::class_registry::__RTS_FN_NS_GC_CLASS_REGISTER_PARENT
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_HAS_INDEX",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_HAS_INDEX
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
        // (#208 / #476) Array methods sem callback.
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF_FROM",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF_FROM
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_LAST_INDEX_OF_FROM",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_LAST_INDEX_OF_FROM
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_INCLUDES_FROM",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_INCLUDES_FROM
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_LAST_INDEX_OF",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_LAST_INDEX_OF
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_INCLUDES",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_INCLUDES
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_REVERSE",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_REVERSE
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_SHIFT",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_SHIFT
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_UNSHIFT",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_UNSHIFT
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_SLICE",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_SLICE
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_CONCAT",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_CONCAT
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_CONCAT_APPEND",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_CONCAT_APPEND
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_FILL",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_FILL
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_FLAT",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_FLAT
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_FLAT_DEPTH",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_FLAT_DEPTH
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_SPLICE_REMOVE",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_SPLICE_REMOVE
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_SPLICE_INSERT",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_SPLICE_INSERT
        );
        // (#208) Array.from variants.
        add_fn!(
            "__RTS_FN_GL_ARRAY_FROM_LENGTH",
            vec::__RTS_FN_GL_ARRAY_FROM_LENGTH
        );
        add_fn!(
            "__RTS_FN_GL_ARRAY_NEW_WITH_LENGTH",
            vec::__RTS_FN_GL_ARRAY_NEW_WITH_LENGTH
        );
        add_fn!(
            "__RTS_FN_GL_ARRAY_FROM_VEC",
            vec::__RTS_FN_GL_ARRAY_FROM_VEC
        );
        // (#208) Array methods adicionais.
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_FIND_LAST",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_FIND_LAST
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_FIND_LAST_INDEX",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_FIND_LAST_INDEX
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_REDUCE_RIGHT",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_REDUCE_RIGHT
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_REDUCE_RIGHT_NO_INIT",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_REDUCE_RIGHT_NO_INIT
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_FLAT_MAP",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_FLAT_MAP
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_COPY_WITHIN",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_COPY_WITHIN
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_SORT",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_SORT
        );
        // (#208) Iterators eager: arr.values()/keys()/entries().
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_VALUES",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_VALUES
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_KEYS",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_KEYS
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_ENTRIES",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_ENTRIES
        );
        // (#208 ES2023) Immutable variants.
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_TO_SORTED",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_TO_SORTED
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_TO_REVERSED",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_TO_REVERSED
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_TO_SPLICED",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_TO_SPLICED
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_TO_SPLICED_INSERT",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_TO_SPLICED_INSERT
        );
        add_fn!(
            "__RTS_FN_NS_COLLECTIONS_VEC_WITH",
            vec::__RTS_FN_NS_COLLECTIONS_VEC_WITH
        );
    }

    // ── namespaces::os ────────────────────────────────────────────────
    {
        use crate::namespaces::os::*;
        add_fn!("__RTS_FN_NS_OS_PLATFORM", __RTS_FN_NS_OS_PLATFORM);
        add_fn!("__RTS_FN_NS_OS_ARCH", __RTS_FN_NS_OS_ARCH);
        add_fn!("__RTS_FN_NS_OS_FAMILY", __RTS_FN_NS_OS_FAMILY);
        add_fn!("__RTS_FN_NS_OS_EOL", __RTS_FN_NS_OS_EOL);
        add_fn!("__RTS_FN_NS_OS_HOME_DIR", __RTS_FN_NS_OS_HOME_DIR);
        add_fn!("__RTS_FN_NS_OS_TEMP_DIR", __RTS_FN_NS_OS_TEMP_DIR);
        add_fn!("__RTS_FN_NS_OS_CONFIG_DIR", __RTS_FN_NS_OS_CONFIG_DIR);
        add_fn!("__RTS_FN_NS_OS_CACHE_DIR", __RTS_FN_NS_OS_CACHE_DIR);
    }

    // ── namespaces::process ───────────────────────────────────────────
    {
        use crate::namespaces::process::*;
        add_fn!("__RTS_FN_NS_PROCESS_EXIT", __RTS_FN_NS_PROCESS_EXIT);
        add_fn!("__RTS_FN_NS_PROCESS_ABORT", __RTS_FN_NS_PROCESS_ABORT);
        add_fn!("__RTS_FN_NS_PROCESS_PID", __RTS_FN_NS_PROCESS_PID);
        add_fn!(
            "__RTS_FN_NS_PROCESS_ARGS_COUNT",
            __RTS_FN_NS_PROCESS_ARGS_COUNT
        );
        add_fn!(
            "__RTS_FN_NS_PROCESS_ARG_AT",
            __RTS_FN_NS_PROCESS_ARG_AT
        );
        add_fn!(
            "__RTS_FN_NS_PROCESS_SPAWN",
            __RTS_FN_NS_PROCESS_SPAWN
        );
        add_fn!("__RTS_FN_NS_PROCESS_WAIT", __RTS_FN_NS_PROCESS_WAIT);
        add_fn!("__RTS_FN_NS_PROCESS_KILL", __RTS_FN_NS_PROCESS_KILL);
    }

    // ── namespaces::net ───────────────────────────────────────────────
    {
        use crate::namespaces::net::*;
        add_fn!("__RTS_FN_NS_NET_TCP_LISTEN", __RTS_FN_NS_NET_TCP_LISTEN);
        add_fn!("__RTS_FN_NS_NET_TCP_ACCEPT", __RTS_FN_NS_NET_TCP_ACCEPT);
        add_fn!("__RTS_FN_NS_NET_TCP_CONNECT", __RTS_FN_NS_NET_TCP_CONNECT);
        add_fn!("__RTS_FN_NS_NET_TCP_SEND", __RTS_FN_NS_NET_TCP_SEND);
        add_fn!("__RTS_FN_NS_NET_TCP_RECV", __RTS_FN_NS_NET_TCP_RECV);
        add_fn!("__RTS_FN_NS_NET_TCP_LOCAL_ADDR", __RTS_FN_NS_NET_TCP_LOCAL_ADDR);
        add_fn!("__RTS_FN_NS_NET_TCP_CLOSE", __RTS_FN_NS_NET_TCP_CLOSE);
        add_fn!("__RTS_FN_NS_NET_UDP_BIND", __RTS_FN_NS_NET_UDP_BIND);
        add_fn!("__RTS_FN_NS_NET_UDP_SEND_TO", __RTS_FN_NS_NET_UDP_SEND_TO);
        add_fn!("__RTS_FN_NS_NET_UDP_RECV_FROM", __RTS_FN_NS_NET_UDP_RECV_FROM);
        add_fn!("__RTS_FN_NS_NET_UDP_LAST_PEER", __RTS_FN_NS_NET_UDP_LAST_PEER);
        add_fn!("__RTS_FN_NS_NET_UDP_LOCAL_ADDR", __RTS_FN_NS_NET_UDP_LOCAL_ADDR);
        add_fn!("__RTS_FN_NS_NET_UDP_CLOSE", __RTS_FN_NS_NET_UDP_CLOSE);
        add_fn!("__RTS_FN_NS_NET_RESOLVE", __RTS_FN_NS_NET_RESOLVE);
    }

    // ── namespaces::tls ───────────────────────────────────────────────
    {
        use crate::namespaces::tls::*;
        add_fn!("__RTS_FN_NS_TLS_CLIENT", __RTS_FN_NS_TLS_CLIENT);
        add_fn!("__RTS_FN_NS_TLS_CLOSE", __RTS_FN_NS_TLS_CLOSE);
        add_fn!("__RTS_FN_NS_TLS_SEND", __RTS_FN_NS_TLS_SEND);
        add_fn!("__RTS_FN_NS_TLS_RECV", __RTS_FN_NS_TLS_RECV);
    }

    // ── namespaces::globals::string ───────────────────────────────────
    use crate::namespaces::globals::string::*;
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
    // (#208) match/search via regex.
    add_fn!("__RTS_FN_NS_STRING_MATCH", search::__RTS_FN_NS_STRING_MATCH);
    add_fn!("__RTS_FN_NS_STRING_MATCH_REGEX", search::__RTS_FN_NS_STRING_MATCH_REGEX);
    add_fn!("__RTS_FN_NS_STRING_REPLACE_REGEX", search::__RTS_FN_NS_STRING_REPLACE_REGEX);
    add_fn!("__RTS_FN_NS_STRING_SEARCH_REGEX", search::__RTS_FN_NS_STRING_SEARCH_REGEX);
    add_fn!("__RTS_FN_NS_STRING_SEARCH", search::__RTS_FN_NS_STRING_SEARCH);
    add_fn!("__RTS_FN_NS_STRING_MATCH_ALL", search::__RTS_FN_NS_STRING_MATCH_ALL);
    add_fn!("__RTS_FN_NS_STRING_MATCH_ALL_REGEX", search::__RTS_FN_NS_STRING_MATCH_ALL_REGEX);
    add_fn!("__RTS_FN_NS_STRING_REPLACE_REGEX_FN", search::__RTS_FN_NS_STRING_REPLACE_REGEX_FN);
    add_fn!("__RTS_FN_GL_STRING_SPLIT_REGEX",       search::__RTS_FN_GL_STRING_SPLIT_REGEX);
    add_fn!("__RTS_FN_GL_STRING_SPLIT_REGEX_LIMIT", search::__RTS_FN_GL_STRING_SPLIT_REGEX_LIMIT);
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
    // ── globals::string instance + static methods ─────────────────────
    add_fn!("__RTS_FN_GL_STRING_NEW_FROM",         rt::__RTS_FN_GL_STRING_NEW_FROM);
    add_fn!("__RTS_FN_GL_STRING_NEW_BOXED",        rt::__RTS_FN_GL_STRING_NEW_BOXED);
    add_fn!("__RTS_FN_GL_STRING_BOX_VALUE_OF",     rt::__RTS_FN_GL_STRING_BOX_VALUE_OF);
    add_fn!("__RTS_FN_GL_STRING_NEW_EMPTY",        rt::__RTS_FN_GL_STRING_NEW_EMPTY);
    add_fn!("__RTS_FN_GL_STRING_FROM_CHAR_CODE",   rt::__RTS_FN_GL_STRING_FROM_CHAR_CODE);
    add_fn!("__RTS_FN_GL_STRING_FROM_CODE_POINT",  rt::__RTS_FN_GL_STRING_FROM_CODE_POINT);
    add_fn!("__RTS_FN_GL_STRING_INDEX_OF",         rt::__RTS_FN_GL_STRING_INDEX_OF);
    add_fn!("__RTS_FN_GL_STRING_LAST_INDEX_OF",    rt::__RTS_FN_GL_STRING_LAST_INDEX_OF);
    add_fn!("__RTS_FN_GL_STRING_INDEX_OF_FROM",    rt::__RTS_FN_GL_STRING_INDEX_OF_FROM);
    add_fn!("__RTS_FN_GL_STRING_LAST_INDEX_OF_FROM", rt::__RTS_FN_GL_STRING_LAST_INDEX_OF_FROM);
    add_fn!("__RTS_FN_GL_STRING_INCLUDES",         rt::__RTS_FN_GL_STRING_INCLUDES);
    add_fn!("__RTS_FN_GL_STRING_INCLUDES_AT",      rt::__RTS_FN_GL_STRING_INCLUDES_AT);
    add_fn!("__RTS_FN_GL_STRING_STARTS_WITH",      rt::__RTS_FN_GL_STRING_STARTS_WITH);
    add_fn!("__RTS_FN_GL_STRING_ENDS_WITH",        rt::__RTS_FN_GL_STRING_ENDS_WITH);
    add_fn!("__RTS_FN_GL_STRING_CHAR_AT",          rt::__RTS_FN_GL_STRING_CHAR_AT);
    add_fn!("__RTS_FN_GL_STRING_CHAR_CODE_AT",     rt::__RTS_FN_GL_STRING_CHAR_CODE_AT);
    add_fn!("__RTS_FN_GL_STRING_CHAR_CODE_AT_F64", rt::__RTS_FN_GL_STRING_CHAR_CODE_AT_F64);
    add_fn!("__RTS_FN_GL_STRING_CODE_POINT_AT",    rt::__RTS_FN_GL_STRING_CODE_POINT_AT);
    add_fn!("__RTS_FN_GL_STRING_AT",               rt::__RTS_FN_GL_STRING_AT);
    add_fn!("__RTS_FN_GL_STRING_SLICE",            rt::__RTS_FN_GL_STRING_SLICE);
    add_fn!("__RTS_FN_GL_STRING_SUBSTRING",        rt::__RTS_FN_GL_STRING_SUBSTRING);
    add_fn!("__RTS_FN_GL_STRING_SUBSTR",           rt::__RTS_FN_GL_STRING_SUBSTR);
    add_fn!("__RTS_FN_GL_STRING_TO_UPPER_CASE",    rt::__RTS_FN_GL_STRING_TO_UPPER_CASE);
    add_fn!("__RTS_FN_GL_STRING_TO_LOWER_CASE",    rt::__RTS_FN_GL_STRING_TO_LOWER_CASE);
    add_fn!("__RTS_FN_GL_STRING_TRIM",             rt::__RTS_FN_GL_STRING_TRIM);
    add_fn!("__RTS_FN_GL_STRING_TRIM_START",       rt::__RTS_FN_GL_STRING_TRIM_START);
    add_fn!("__RTS_FN_GL_STRING_TRIM_END",         rt::__RTS_FN_GL_STRING_TRIM_END);
    add_fn!("__RTS_FN_GL_STRING_REPEAT",           rt::__RTS_FN_GL_STRING_REPEAT);
    add_fn!("__RTS_FN_GL_STRING_REPLACE",          rt::__RTS_FN_GL_STRING_REPLACE);
    add_fn!("__RTS_FN_GL_STRING_REPLACE_ALL",      rt::__RTS_FN_GL_STRING_REPLACE_ALL);
    add_fn!("__RTS_FN_GL_STRING_CONCAT",           rt::__RTS_FN_GL_STRING_CONCAT);
    add_fn!("__RTS_FN_GL_STRING_PAD_START",        rt::__RTS_FN_GL_STRING_PAD_START);
    add_fn!("__RTS_FN_GL_STRING_PAD_END",          rt::__RTS_FN_GL_STRING_PAD_END);
    add_fn!("__RTS_FN_GL_STRING_SPLIT",            rt::__RTS_FN_GL_STRING_SPLIT);
    add_fn!("__RTS_FN_GL_STRING_SPLIT_LIMIT",      rt::__RTS_FN_GL_STRING_SPLIT_LIMIT);
    add_fn!("__RTS_FN_GL_STRING_STARTS_WITH_AT",   rt::__RTS_FN_GL_STRING_STARTS_WITH_AT);
    add_fn!("__RTS_FN_GL_STRING_ENDS_WITH_AT",     rt::__RTS_FN_GL_STRING_ENDS_WITH_AT);
    add_fn!("__RTS_FN_GL_STRING_LOCALE_COMPARE",   rt::__RTS_FN_GL_STRING_LOCALE_COMPARE);
    add_fn!("__RTS_FN_GL_STRING_TO_STRING",        rt::__RTS_FN_GL_STRING_TO_STRING);
    add_fn!("__RTS_FN_GL_STRING_IS_WELL_FORMED",   rt::__RTS_FN_GL_STRING_IS_WELL_FORMED);
    add_fn!("__RTS_FN_GL_STRING_TO_WELL_FORMED",   rt::__RTS_FN_GL_STRING_TO_WELL_FORMED);
    add_fn!("__RTS_FN_GL_STRING_NORMALIZE",        rt::__RTS_FN_GL_STRING_NORMALIZE);
    add_fn!("__RTS_FN_GL_STRING_LENGTH_UTF16",     rt::__RTS_FN_GL_STRING_LENGTH_UTF16);

    // ── namespaces::globals::number ───────────────────────────────────
    use crate::namespaces::globals::number as num_rt;
    add_fn!("__RTS_FN_GL_NUMBER_NEW_FROM",    num_rt::__RTS_FN_GL_NUMBER_NEW_FROM);
    add_fn!("__RTS_FN_GL_NUMBER_NEW_EMPTY",   num_rt::__RTS_FN_GL_NUMBER_NEW_EMPTY);
    add_fn!("__RTS_FN_GL_NUMBER_NEW_BOXED",   num_rt::__RTS_FN_GL_NUMBER_NEW_BOXED);
    add_fn!("__RTS_FN_GL_NUMBER_NEW_BOXED_EMPTY", num_rt::__RTS_FN_GL_NUMBER_NEW_BOXED_EMPTY);
    add_fn!("__RTS_FN_GL_NUMBER_BOX_VALUE_OF", num_rt::__RTS_FN_GL_NUMBER_BOX_VALUE_OF);
    add_fn!("__RTS_FN_GL_NUMBER_IS_NAN",      num_rt::__RTS_FN_GL_NUMBER_IS_NAN);
    add_fn!("__RTS_FN_GL_NUMBER_IS_FINITE",   num_rt::__RTS_FN_GL_NUMBER_IS_FINITE);
    add_fn!("__RTS_FN_GL_NUMBER_IS_INTEGER",  num_rt::__RTS_FN_GL_NUMBER_IS_INTEGER);
    add_fn!("__RTS_FN_GL_NUMBER_IS_SAFE_INT", num_rt::__RTS_FN_GL_NUMBER_IS_SAFE_INT);
    add_fn!("__RTS_FN_GL_NUMBER_VALUE_OF",    num_rt::__RTS_FN_GL_NUMBER_VALUE_OF);
    add_fn!("__RTS_FN_GL_NUMBER_TO_FIXED",    num_rt::__RTS_FN_GL_NUMBER_TO_FIXED);
    add_fn!("__RTS_FN_GL_NUMBER_TO_PRECISION",    num_rt::__RTS_FN_GL_NUMBER_TO_PRECISION);
    add_fn!("__RTS_FN_GL_NUMBER_TO_EXPONENTIAL",  num_rt::__RTS_FN_GL_NUMBER_TO_EXPONENTIAL);
    add_fn!("__RTS_FN_GL_NUMBER_FROM_STR",         num_rt::__RTS_FN_GL_NUMBER_FROM_STR);
    add_fn!("__RTS_FN_GL_NUMBER_TO_STRING_RADIX",  num_rt::__RTS_FN_GL_NUMBER_TO_STRING_RADIX);

    // ── namespaces::buffer ────────────────────────────────────────────
    use crate::namespaces::buffer as buf;
    add_fn!("__RTS_FN_NS_BUFFER_ALLOC", buf::__RTS_FN_NS_BUFFER_ALLOC);
    // ArrayBuffer / DataView (globals, backing buffer ns)
    add_fn!("__RTS_FN_GL_ARRAY_BUFFER_NEW", buf::__RTS_FN_GL_ARRAY_BUFFER_NEW);
    add_fn!("__RTS_FN_GL_ARRAY_BUFFER_SLICE", buf::__RTS_FN_GL_ARRAY_BUFFER_SLICE);
    add_fn!("__RTS_FN_GL_TA_GET_ELEM", buf::__RTS_FN_GL_TA_GET_ELEM);
    add_fn!("__RTS_FN_GL_TA_SET_ELEM", buf::__RTS_FN_GL_TA_SET_ELEM);
    add_fn!("__RTS_FN_GL_TA_LENGTH", buf::__RTS_FN_GL_TA_LENGTH);
    add_fn!("__RTS_FN_GL_TA_SET_FROM", buf::__RTS_FN_GL_TA_SET_FROM);
    add_fn!("__RTS_FN_GL_BUFFER_DETACH", buf::__RTS_FN_GL_BUFFER_DETACH);
    add_fn!("__RTS_FN_GL_ATOMICS_RMW", buf::__RTS_FN_GL_ATOMICS_RMW);
    add_fn!("__RTS_FN_GL_ATOMICS_CAS", buf::__RTS_FN_GL_ATOMICS_CAS);
    add_fn!("__RTS_FN_GL_ATOMICS_LOAD", buf::__RTS_FN_GL_ATOMICS_LOAD);
    add_fn!("__RTS_FN_GL_ATOMICS_STORE", buf::__RTS_FN_GL_ATOMICS_STORE);
    add_fn!("__RTS_FN_GL_DATAVIEW_NEW", buf::__RTS_FN_GL_DATAVIEW_NEW);
    add_fn!("__RTS_FN_GL_DATAVIEW_BYTE_OFFSET", buf::__RTS_FN_GL_DATAVIEW_BYTE_OFFSET);
    add_fn!("__RTS_FN_GL_DATAVIEW_BYTE_LENGTH", buf::__RTS_FN_GL_DATAVIEW_BYTE_LENGTH);
    add_fn!("__RTS_FN_GL_DATAVIEW_SET_UINT8", buf::__RTS_FN_GL_DATAVIEW_SET_UINT8);
    add_fn!("__RTS_FN_GL_DATAVIEW_GET_UINT8", buf::__RTS_FN_GL_DATAVIEW_GET_UINT8);
    add_fn!("__RTS_FN_GL_DATAVIEW_SET_UINT16", buf::__RTS_FN_GL_DATAVIEW_SET_UINT16);
    add_fn!("__RTS_FN_GL_DATAVIEW_GET_UINT16", buf::__RTS_FN_GL_DATAVIEW_GET_UINT16);
    add_fn!("__RTS_FN_GL_DATAVIEW_SET_INT32", buf::__RTS_FN_GL_DATAVIEW_SET_INT32);
    add_fn!("__RTS_FN_GL_DATAVIEW_GET_INT32", buf::__RTS_FN_GL_DATAVIEW_GET_INT32);
    add_fn!("__RTS_FN_GL_DATAVIEW_SET_UINT16_LE", buf::__RTS_FN_GL_DATAVIEW_SET_UINT16_LE);
    add_fn!("__RTS_FN_GL_DATAVIEW_GET_UINT16_LE", buf::__RTS_FN_GL_DATAVIEW_GET_UINT16_LE);
    add_fn!("__RTS_FN_GL_DATAVIEW_SET_INT32_LE", buf::__RTS_FN_GL_DATAVIEW_SET_INT32_LE);
    add_fn!("__RTS_FN_GL_DATAVIEW_GET_INT32_LE", buf::__RTS_FN_GL_DATAVIEW_GET_INT32_LE);
    add_fn!("__RTS_FN_GL_DATAVIEW_SET_FLOAT64", buf::__RTS_FN_GL_DATAVIEW_SET_FLOAT64);
    add_fn!("__RTS_FN_GL_DATAVIEW_GET_FLOAT64", buf::__RTS_FN_GL_DATAVIEW_GET_FLOAT64);
    add_fn!("__RTS_FN_GL_DATAVIEW_SET_FLOAT32", buf::__RTS_FN_GL_DATAVIEW_SET_FLOAT32);
    add_fn!("__RTS_FN_GL_DATAVIEW_GET_FLOAT32", buf::__RTS_FN_GL_DATAVIEW_GET_FLOAT32);
    add_fn!("__RTS_FN_GL_DATAVIEW_SET_BIGINT64", buf::__RTS_FN_GL_DATAVIEW_SET_BIGINT64);
    add_fn!("__RTS_FN_GL_DATAVIEW_GET_BIGINT64", buf::__RTS_FN_GL_DATAVIEW_GET_BIGINT64);
    add_fn!("__RTS_FN_GL_DATAVIEW_SET_BIGUINT64", buf::__RTS_FN_GL_DATAVIEW_SET_BIGUINT64);
    add_fn!("__RTS_FN_GL_DATAVIEW_GET_BIGUINT64", buf::__RTS_FN_GL_DATAVIEW_GET_BIGUINT64);
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
        "__RTS_FN_NS_BUFFER_READ_F32",
        buf::__RTS_FN_NS_BUFFER_READ_F32
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
    add_fn!(
        "__RTS_FN_NS_BUFFER_WRITE_F32",
        buf::__RTS_FN_NS_BUFFER_WRITE_F32
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
    use crate::namespaces::ffi::*;
    add_fn!(
        "__RTS_FN_NS_FFI_CSTR_FROM_PTR",
        __RTS_FN_NS_FFI_CSTR_FROM_PTR
    );
    add_fn!(
        "__RTS_FN_NS_FFI_CSTR_LEN",
        __RTS_FN_NS_FFI_CSTR_LEN
    );
    add_fn!(
        "__RTS_FN_NS_FFI_CSTR_TO_STR",
        __RTS_FN_NS_FFI_CSTR_TO_STR
    );
    add_fn!(
        "__RTS_FN_NS_FFI_CSTRING_NEW",
        __RTS_FN_NS_FFI_CSTRING_NEW
    );
    add_fn!(
        "__RTS_FN_NS_FFI_CSTRING_PTR",
        __RTS_FN_NS_FFI_CSTRING_PTR
    );
    add_fn!(
        "__RTS_FN_NS_FFI_CSTRING_FREE",
        __RTS_FN_NS_FFI_CSTRING_FREE
    );
    add_fn!(
        "__RTS_FN_NS_FFI_OSSTR_FROM_STR",
        __RTS_FN_NS_FFI_OSSTR_FROM_STR
    );
    add_fn!(
        "__RTS_FN_NS_FFI_OSSTR_TO_STR",
        __RTS_FN_NS_FFI_OSSTR_TO_STR
    );
    add_fn!(
        "__RTS_FN_NS_FFI_OSSTR_FREE",
        __RTS_FN_NS_FFI_OSSTR_FREE
    );

    // ── namespaces::atomic ────────────────────────────────────────────
    use crate::namespaces::atomic::*;
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_NEW",
        __RTS_FN_NS_ATOMIC_I64_NEW
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_LOAD",
        __RTS_FN_NS_ATOMIC_I64_LOAD
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_STORE",
        __RTS_FN_NS_ATOMIC_I64_STORE
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_FETCH_ADD",
        __RTS_FN_NS_ATOMIC_I64_FETCH_ADD
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_FETCH_SUB",
        __RTS_FN_NS_ATOMIC_I64_FETCH_SUB
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_FETCH_AND",
        __RTS_FN_NS_ATOMIC_I64_FETCH_AND
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_FETCH_OR",
        __RTS_FN_NS_ATOMIC_I64_FETCH_OR
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_FETCH_XOR",
        __RTS_FN_NS_ATOMIC_I64_FETCH_XOR
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_SWAP",
        __RTS_FN_NS_ATOMIC_I64_SWAP
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_I64_CAS",
        __RTS_FN_NS_ATOMIC_I64_CAS
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_BOOL_NEW",
        __RTS_FN_NS_ATOMIC_BOOL_NEW
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_BOOL_LOAD",
        __RTS_FN_NS_ATOMIC_BOOL_LOAD
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_BOOL_STORE",
        __RTS_FN_NS_ATOMIC_BOOL_STORE
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_BOOL_SWAP",
        __RTS_FN_NS_ATOMIC_BOOL_SWAP
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_F64_NEW",
        __RTS_FN_NS_ATOMIC_F64_NEW
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_F64_LOAD",
        __RTS_FN_NS_ATOMIC_F64_LOAD
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_F64_STORE",
        __RTS_FN_NS_ATOMIC_F64_STORE
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_F64_FETCH_ADD",
        __RTS_FN_NS_ATOMIC_F64_FETCH_ADD
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_F64_SWAP",
        __RTS_FN_NS_ATOMIC_F64_SWAP
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_FENCE_ACQUIRE",
        __RTS_FN_NS_ATOMIC_FENCE_ACQUIRE
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_FENCE_RELEASE",
        __RTS_FN_NS_ATOMIC_FENCE_RELEASE
    );
    add_fn!(
        "__RTS_FN_NS_ATOMIC_FENCE_SEQ_CST",
        __RTS_FN_NS_ATOMIC_FENCE_SEQ_CST
    );

    // ── namespaces::sync ──────────────────────────────────────────────
    use crate::namespaces::sync::*;
    add_fn!(
        "__RTS_FN_NS_SYNC_MUTEX_NEW",
        __RTS_FN_NS_SYNC_MUTEX_NEW
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_MUTEX_LOCK",
        __RTS_FN_NS_SYNC_MUTEX_LOCK
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_MUTEX_TRY_LOCK",
        __RTS_FN_NS_SYNC_MUTEX_TRY_LOCK
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_MUTEX_SET",
        __RTS_FN_NS_SYNC_MUTEX_SET
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_MUTEX_UNLOCK",
        __RTS_FN_NS_SYNC_MUTEX_UNLOCK
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_MUTEX_FREE",
        __RTS_FN_NS_SYNC_MUTEX_FREE
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_RWLOCK_NEW",
        __RTS_FN_NS_SYNC_RWLOCK_NEW
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_RWLOCK_READ",
        __RTS_FN_NS_SYNC_RWLOCK_READ
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_RWLOCK_WRITE",
        __RTS_FN_NS_SYNC_RWLOCK_WRITE
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_RWLOCK_UNLOCK",
        __RTS_FN_NS_SYNC_RWLOCK_UNLOCK
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_ONCE_NEW",
        __RTS_FN_NS_SYNC_ONCE_NEW
    );
    add_fn!(
        "__RTS_FN_NS_SYNC_ONCE_CALL",
        __RTS_FN_NS_SYNC_ONCE_CALL
    );

    // ── namespaces::thread ────────────────────────────────────────────
    {
        use crate::namespaces::thread::*;
        add_fn!(
            "__RTS_FN_NS_THREAD_SPAWN",
            __RTS_FN_NS_THREAD_SPAWN
        );
        add_fn!(
            "__RTS_FN_NS_THREAD_SPAWN_WITH_UD",
            __RTS_FN_NS_THREAD_SPAWN_WITH_UD
        );
        add_fn!(
            "__RTS_FN_NS_THREAD_SPAWN_DETACHED",
            __RTS_FN_NS_THREAD_SPAWN_DETACHED
        );
        add_fn!(
            "__RTS_FN_NS_THREAD_SPAWN_ASYNC",
            __RTS_FN_NS_THREAD_SPAWN_ASYNC
        );
        add_fn!(
            "__RTS_FN_NS_THREAD_SPAWN_ASYNC_JOIN",
            __RTS_FN_NS_THREAD_SPAWN_ASYNC_JOIN
        );
        add_fn!(
            "__RTS_FN_NS_THREAD_JOIN_ASYNC",
            __RTS_FN_NS_THREAD_JOIN_ASYNC
        );
        add_fn!(
            "__RTS_FN_NS_THREAD_SCOPE",
            __RTS_FN_NS_THREAD_SCOPE
        );
        add_fn!(
            "__RTS_FN_NS_THREAD_SCOPE_WITH_UD",
            __RTS_FN_NS_THREAD_SCOPE_WITH_UD
        );
        add_fn!(
            "__RTS_FN_NS_THREAD_JOIN",
            __RTS_FN_NS_THREAD_JOIN
        );
        add_fn!(
            "__RTS_FN_NS_THREAD_DETACH",
            __RTS_FN_NS_THREAD_DETACH
        );
        add_fn!("__RTS_FN_NS_THREAD_ID", __RTS_FN_NS_THREAD_ID);
        add_fn!(
            "__RTS_FN_NS_THREAD_SLEEP_MS",
            __RTS_FN_NS_THREAD_SLEEP_MS
        );
    }

    // ── namespaces::parallel ──────────────────────────────────────────
    {
        use crate::namespaces::parallel as parallel_ops;
        add_fn!(
            "__RTS_FN_NS_PARALLEL_MAP",
            parallel_ops::__RTS_FN_NS_PARALLEL_MAP
        );
        // (#195) Variantes BOUND: callback eh Function handle com bound_args.
        add_fn!(
            "__RTS_FN_NS_PARALLEL_MAP_BOUND",
            parallel_ops::__RTS_FN_NS_PARALLEL_MAP_BOUND
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_FILTER_BOUND",
            parallel_ops::__RTS_FN_NS_PARALLEL_FILTER_BOUND
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_FOR_EACH_BOUND",
            parallel_ops::__RTS_FN_NS_PARALLEL_FOR_EACH_BOUND
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_REDUCE_BOUND",
            parallel_ops::__RTS_FN_NS_PARALLEL_REDUCE_BOUND
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_REDUCE_NO_INIT_BOUND",
            parallel_ops::__RTS_FN_NS_PARALLEL_REDUCE_NO_INIT_BOUND
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
            "__RTS_FN_NS_PARALLEL_REDUCE_NO_INIT",
            parallel_ops::__RTS_FN_NS_PARALLEL_REDUCE_NO_INIT
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_NUM_THREADS",
            parallel_ops::__RTS_FN_NS_PARALLEL_NUM_THREADS
        );
        // (#208) Predicate methods — backing pra arr.filter/find/findIndex/some/every.
        add_fn!(
            "__RTS_FN_NS_PARALLEL_FILTER",
            parallel_ops::__RTS_FN_NS_PARALLEL_FILTER
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_FIND",
            parallel_ops::__RTS_FN_NS_PARALLEL_FIND
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_FIND_INDEX",
            parallel_ops::__RTS_FN_NS_PARALLEL_FIND_INDEX
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_SOME",
            parallel_ops::__RTS_FN_NS_PARALLEL_SOME
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_EVERY",
            parallel_ops::__RTS_FN_NS_PARALLEL_EVERY
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_FIND_BOUND",
            parallel_ops::__RTS_FN_NS_PARALLEL_FIND_BOUND
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_FIND_INDEX_BOUND",
            parallel_ops::__RTS_FN_NS_PARALLEL_FIND_INDEX_BOUND
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_SOME_BOUND",
            parallel_ops::__RTS_FN_NS_PARALLEL_SOME_BOUND
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_EVERY_BOUND",
            parallel_ops::__RTS_FN_NS_PARALLEL_EVERY_BOUND
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_REDUCE_RIGHT_BOUND",
            parallel_ops::__RTS_FN_NS_PARALLEL_REDUCE_RIGHT_BOUND
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_REDUCE_RIGHT_NO_INIT_BOUND",
            parallel_ops::__RTS_FN_NS_PARALLEL_REDUCE_RIGHT_NO_INIT_BOUND
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_FIND_LAST_BOUND",
            parallel_ops::__RTS_FN_NS_PARALLEL_FIND_LAST_BOUND
        );
        add_fn!(
            "__RTS_FN_NS_PARALLEL_FIND_LAST_INDEX_BOUND",
            parallel_ops::__RTS_FN_NS_PARALLEL_FIND_LAST_INDEX_BOUND
        );
    }

    // ── namespaces::path ──────────────────────────────────────────────
    use crate::namespaces::path::*;
    add_fn!("__RTS_FN_NS_PATH_JOIN", __RTS_FN_NS_PATH_JOIN);
    add_fn!("__RTS_FN_NS_PATH_PARENT", __RTS_FN_NS_PATH_PARENT);
    add_fn!("__RTS_FN_NS_PATH_FILE_NAME", __RTS_FN_NS_PATH_FILE_NAME);
    add_fn!("__RTS_FN_NS_PATH_STEM", __RTS_FN_NS_PATH_STEM);
    add_fn!("__RTS_FN_NS_PATH_EXT", __RTS_FN_NS_PATH_EXT);
    add_fn!("__RTS_FN_NS_PATH_IS_ABSOLUTE", __RTS_FN_NS_PATH_IS_ABSOLUTE);
    add_fn!("__RTS_FN_NS_PATH_NORMALIZE", __RTS_FN_NS_PATH_NORMALIZE);
    add_fn!("__RTS_FN_NS_PATH_WITH_EXT", __RTS_FN_NS_PATH_WITH_EXT);

    // ── namespaces::env ───────────────────────────────────────────────
    use crate::namespaces::env::*;
    add_fn!("__RTS_FN_NS_ENV_GET_VAR", __RTS_FN_NS_ENV_GET_VAR);
    add_fn!("__RTS_FN_NS_ENV_SET_VAR", __RTS_FN_NS_ENV_SET_VAR);
    add_fn!("__RTS_FN_NS_ENV_REMOVE_VAR", __RTS_FN_NS_ENV_REMOVE_VAR);
    add_fn!("__RTS_FN_NS_ENV_ARGS_COUNT", __RTS_FN_NS_ENV_ARGS_COUNT);
    add_fn!("__RTS_FN_NS_ENV_ARG_AT", __RTS_FN_NS_ENV_ARG_AT);
    add_fn!("__RTS_FN_NS_ENV_CWD", __RTS_FN_NS_ENV_CWD);
    add_fn!("__RTS_FN_NS_ENV_SET_CWD", __RTS_FN_NS_ENV_SET_CWD);

    // ── namespaces::time ──────────────────────────────────────────────
    use crate::namespaces::time::*;
    add_fn!("__RTS_FN_NS_TIME_NOW_MS", __RTS_FN_NS_TIME_NOW_MS);
    add_fn!("__RTS_FN_NS_TIME_NOW_NS", __RTS_FN_NS_TIME_NOW_NS);
    add_fn!("__RTS_FN_NS_TIME_UNIX_MS", __RTS_FN_NS_TIME_UNIX_MS);
    add_fn!("__RTS_FN_NS_TIME_UNIX_NS", __RTS_FN_NS_TIME_UNIX_NS);
    add_fn!("__RTS_FN_NS_TIME_SLEEP_MS", __RTS_FN_NS_TIME_SLEEP_MS);
    add_fn!("__RTS_FN_NS_TIME_SLEEP_NS", __RTS_FN_NS_TIME_SLEEP_NS);

    // ── namespaces::bigfloat ──────────────────────────────────────────
    use crate::namespaces::bigfloat::*;
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

    // ── namespaces::audio ─────────────────────────────────────────────
    {
        use crate::namespaces::audio as audio;
        add_fn!(
            "__RTS_FN_NS_AUDIO_DEFAULT_SAMPLE_RATE",
            audio::__RTS_FN_NS_AUDIO_DEFAULT_SAMPLE_RATE
        );
        add_fn!(
            "__RTS_FN_NS_AUDIO_DEFAULT_CHANNELS",
            audio::__RTS_FN_NS_AUDIO_DEFAULT_CHANNELS
        );
        add_fn!(
            "__RTS_FN_NS_AUDIO_OPEN_OUTPUT",
            audio::__RTS_FN_NS_AUDIO_OPEN_OUTPUT
        );
        add_fn!(
            "__RTS_FN_NS_AUDIO_SAMPLE_RATE",
            audio::__RTS_FN_NS_AUDIO_SAMPLE_RATE
        );
        add_fn!("__RTS_FN_NS_AUDIO_CHANNELS", audio::__RTS_FN_NS_AUDIO_CHANNELS);
        add_fn!("__RTS_FN_NS_AUDIO_IS_OPEN", audio::__RTS_FN_NS_AUDIO_IS_OPEN);
        add_fn!(
            "__RTS_FN_NS_AUDIO_AVAILABLE_FRAMES",
            audio::__RTS_FN_NS_AUDIO_AVAILABLE_FRAMES
        );
        add_fn!(
            "__RTS_FN_NS_AUDIO_QUEUED_FRAMES",
            audio::__RTS_FN_NS_AUDIO_QUEUED_FRAMES
        );
        add_fn!("__RTS_FN_NS_AUDIO_WRITE", audio::__RTS_FN_NS_AUDIO_WRITE);
        add_fn!(
            "__RTS_FN_NS_AUDIO_MASTER_VOLUME",
            audio::__RTS_FN_NS_AUDIO_MASTER_VOLUME
        );
        add_fn!("__RTS_FN_NS_AUDIO_UNDERRUNS", audio::__RTS_FN_NS_AUDIO_UNDERRUNS);
        add_fn!("__RTS_FN_NS_AUDIO_CLOSE", audio::__RTS_FN_NS_AUDIO_CLOSE);
    }

    // ── namespaces::asio_audio (feature `asio`) ───────────────────────
    #[cfg(feature = "asio")]
    {
        use crate::namespaces::asio_audio::ops as asio;
        add_fn!(
            "__RTS_FN_NS_ASIO_AUDIO_IS_AVAILABLE",
            asio::__RTS_FN_NS_ASIO_AUDIO_IS_AVAILABLE
        );
        add_fn!(
            "__RTS_FN_NS_ASIO_AUDIO_DEFAULT_SAMPLE_RATE",
            asio::__RTS_FN_NS_ASIO_AUDIO_DEFAULT_SAMPLE_RATE
        );
        add_fn!(
            "__RTS_FN_NS_ASIO_AUDIO_DEFAULT_CHANNELS",
            asio::__RTS_FN_NS_ASIO_AUDIO_DEFAULT_CHANNELS
        );
        add_fn!(
            "__RTS_FN_NS_ASIO_AUDIO_OPEN_OUTPUT",
            asio::__RTS_FN_NS_ASIO_AUDIO_OPEN_OUTPUT
        );
        add_fn!(
            "__RTS_FN_NS_ASIO_AUDIO_SAMPLE_RATE",
            asio::__RTS_FN_NS_ASIO_AUDIO_SAMPLE_RATE
        );
        add_fn!(
            "__RTS_FN_NS_ASIO_AUDIO_CHANNELS",
            asio::__RTS_FN_NS_ASIO_AUDIO_CHANNELS
        );
        add_fn!(
            "__RTS_FN_NS_ASIO_AUDIO_IS_OPEN",
            asio::__RTS_FN_NS_ASIO_AUDIO_IS_OPEN
        );
        add_fn!(
            "__RTS_FN_NS_ASIO_AUDIO_AVAILABLE_FRAMES",
            asio::__RTS_FN_NS_ASIO_AUDIO_AVAILABLE_FRAMES
        );
        add_fn!(
            "__RTS_FN_NS_ASIO_AUDIO_QUEUED_FRAMES",
            asio::__RTS_FN_NS_ASIO_AUDIO_QUEUED_FRAMES
        );
        add_fn!(
            "__RTS_FN_NS_ASIO_AUDIO_WRITE",
            asio::__RTS_FN_NS_ASIO_AUDIO_WRITE
        );
        add_fn!(
            "__RTS_FN_NS_ASIO_AUDIO_MASTER_VOLUME",
            asio::__RTS_FN_NS_ASIO_AUDIO_MASTER_VOLUME
        );
        add_fn!(
            "__RTS_FN_NS_ASIO_AUDIO_UNDERRUNS",
            asio::__RTS_FN_NS_ASIO_AUDIO_UNDERRUNS
        );
        add_fn!(
            "__RTS_FN_NS_ASIO_AUDIO_CLOSE",
            asio::__RTS_FN_NS_ASIO_AUDIO_CLOSE
        );
    }


    // ── namespaces::runtime ───────────────────────────────────────────
    // JIT fast path: inline pipeline instead of subprocess spawn.
    {
        use crate::namespaces::runtime::eval_jit::*;
        add_fn!("__RTS_FN_NS_RUNTIME_EVAL", runtime_eval_src_jit);
        add_fn!("__RTS_FN_NS_RUNTIME_EVAL_FILE", runtime_eval_file_jit);
        add_fn!("__RTS_FN_NS_RUNTIME_IMPORT_MODULE", runtime_import_module_jit);
        add_fn!("__RTS_FN_NS_RUNTIME_SET_MODULE_EXPORTS", runtime_set_module_exports_jit);
    }

    // ── namespaces::test ─────────────────────────────────────────────
    {
        use crate::namespaces::test::*;
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
    // ── namespaces::http_server ─────────────────────────────────────
    {
        use crate::namespaces::http_server::*;
        add_fn!("__RTS_FN_NS_HTTP_SERVER_SERVE", __RTS_FN_NS_HTTP_SERVER_SERVE);
        add_fn!("__RTS_FN_NS_HTTP_SERVER_REQ_METHOD", __RTS_FN_NS_HTTP_SERVER_REQ_METHOD);
        add_fn!("__RTS_FN_NS_HTTP_SERVER_REQ_PATH", __RTS_FN_NS_HTTP_SERVER_REQ_PATH);
        add_fn!("__RTS_FN_NS_HTTP_SERVER_REQ_BODY", __RTS_FN_NS_HTTP_SERVER_REQ_BODY);
        add_fn!("__RTS_FN_NS_HTTP_SERVER_RESPOND", __RTS_FN_NS_HTTP_SERVER_RESPOND);
    }

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
