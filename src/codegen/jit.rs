use std::collections::BTreeMap;
use std::time::Instant;

use anyhow::{Context, Result, anyhow, bail};
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataId, FuncId, Linkage, Module, default_libcall_names};

use crate::mir::{MirFunction, MirModule};

use crate::namespaces::abi::{FN_BIND_IDENTIFIER, FN_EVAL_EXPR, FN_EVAL_STMT};

use super::mir_parse::{
    ABI_ARG_SLOTS, ABI_PARAM_COUNT, ABI_UNDEFINED_HANDLE, RTS_CALL_DISPATCH_SYMBOL,
    RTS_DISPATCH_SYMBOL, is_valid_binding_name, parse_call_statement, parse_declaration_statement,
    parse_enter_parameters, parse_return_expression, parse_return_literal,
};

#[derive(Debug, Clone)]
pub struct JitReport {
    pub entry_function: String,
    pub compiled_functions: usize,
    pub entry_return_value: i64,
    pub executed: bool,
    pub timings: JitTimings,
}

#[derive(Debug, Clone, Default)]
pub struct JitTimings {
    pub initialize_jit_ms: f64,
    pub declare_functions_ms: f64,
    pub scan_synthetic_calls_ms: f64,
    pub declare_helpers_ms: f64,
    pub define_functions_ms: f64,
    pub define_stubs_ms: f64,
    pub finalize_ms: f64,
    pub resolve_entry_ms: f64,
    pub execute_entry_ms: f64,
    pub total_ms: f64,
}

pub fn execute(module: &MirModule, entry_function: &str) -> Result<JitReport> {
    let total_started = Instant::now();
    crate::namespaces::abi::reset_thread_state();
    let mut timings = JitTimings::default();

    let started = Instant::now();
    let mut jit = initialize_jit_module()?;
    timings.initialize_jit_ms = started.elapsed().as_secs_f64() * 1000.0;

    let mut declarations = BTreeMap::<String, FuncId>::new();
    let mut synthetic_stubs = Vec::<String>::new();

    let started = Instant::now();
    let signature = function_signature(&mut jit);
    for function in &module.functions {
        let id = jit
            .declare_function(&function.name, Linkage::Export, &signature)
            .with_context(|| format!("failed to declare function '{}'", function.name))?;
        declarations.insert(function.name.clone(), id);
    }
    timings.declare_functions_ms = started.elapsed().as_secs_f64() * 1000.0;

    let started = Instant::now();
    for function in &module.functions {
        for block in &function.blocks {
            for statement in &block.statements {
                let text = statement.text.trim();
                let Some(call) = parse_call_statement(text) else {
                    continue;
                };
                let callee = call.callee;

                if declarations.contains_key(callee.as_str()) {
                    continue;
                }

                let id = jit
                    .declare_function(callee.as_str(), Linkage::Export, &signature)
                    .with_context(|| {
                        format!("failed to declare synthetic JIT stub function '{}'", callee)
                    })?;
                declarations.insert(callee.clone(), id);
                synthetic_stubs.push(callee);
            }
        }
    }
    timings.scan_synthetic_calls_ms = started.elapsed().as_secs_f64() * 1000.0;

    let started = Instant::now();
    let rts_dispatch_id = declare_rts_dispatch_import(&mut jit, &mut declarations)?;
    let call_dispatch_id = declare_call_dispatch_import(&mut jit, &mut declarations)?;
    timings.declare_helpers_ms = started.elapsed().as_secs_f64() * 1000.0;

    let started = Instant::now();
    for function in &module.functions {
        let id = declarations
            .get(&function.name)
            .copied()
            .ok_or_else(|| anyhow!("missing declaration for function '{}'", function.name))?;
        define_function(&mut jit, &declarations, rts_dispatch_id, id, function)?;
    }
    timings.define_functions_ms = started.elapsed().as_secs_f64() * 1000.0;

    let started = Instant::now();
    for name in &synthetic_stubs {
        let id = declarations
            .get(name)
            .copied()
            .ok_or_else(|| anyhow!("missing declaration for synthetic JIT stub '{}'", name))?;

        let leaked_symbol = Box::leak(name.as_bytes().to_vec().into_boxed_slice());
        define_stub_function(
            &mut jit,
            id,
            name,
            call_dispatch_id,
            leaked_symbol.as_ptr() as i64,
            leaked_symbol.len() as i64,
        )?;
    }
    timings.define_stubs_ms = started.elapsed().as_secs_f64() * 1000.0;

    let started = Instant::now();
    jit.finalize_definitions()
        .context("failed to finalize JIT definitions")?;
    timings.finalize_ms = started.elapsed().as_secs_f64() * 1000.0;

    let started = Instant::now();
    let selected_entry = if declarations.contains_key(entry_function) {
        Some(entry_function.to_string())
    } else if entry_function != "main" && declarations.contains_key("main") {
        Some("main".to_string())
    } else {
        None
    };
    timings.resolve_entry_ms = started.elapsed().as_secs_f64() * 1000.0;

    let started = Instant::now();
    let (entry_name, entry_return_value, executed) = if let Some(entry_name) = selected_entry {
        let entry_id = declarations
            .get(&entry_name)
            .copied()
            .ok_or_else(|| anyhow!("failed to resolve JIT entry '{}'", entry_name))?;

        let address = jit.get_finalized_function(entry_id);
        let entry = unsafe {
            std::mem::transmute::<*const u8, extern "C" fn(i64, i64, i64, i64, i64, i64, i64) -> i64>(
                address,
            )
        };
        (entry_name, entry(0, 0, 0, 0, 0, 0, 0), true)
    } else {
        (entry_function.to_string(), ABI_UNDEFINED_HANDLE, false)
    };
    timings.execute_entry_ms = started.elapsed().as_secs_f64() * 1000.0;
    timings.total_ms = total_started.elapsed().as_secs_f64() * 1000.0;

    Ok(JitReport {
        entry_function: entry_name,
        compiled_functions: declarations.len(),
        entry_return_value,
        executed,
        timings,
    })
}

fn initialize_jit_module() -> Result<JITModule> {
    let mut settings_builder = settings::builder();
    settings_builder
        .set("is_pic", "false")
        .context("failed to configure Cranelift setting 'is_pic'")?;
    let flags = settings::Flags::new(settings_builder);

    let isa_builder = cranelift_native::builder()
        .map_err(|error| anyhow!("failed to build host ISA: {error}"))?;
    let isa = isa_builder
        .finish(flags)
        .context("failed to finalize host ISA")?;

    let mut builder = JITBuilder::with_isa(isa, default_libcall_names());
    builder.symbol(
        RTS_DISPATCH_SYMBOL,
        crate::namespaces::abi::__rts_dispatch as *const u8,
    );
    builder.symbol(
        RTS_CALL_DISPATCH_SYMBOL,
        crate::namespaces::abi::__rts_call_dispatch as *const u8,
    );

    Ok(JITModule::new(builder))
}

fn function_signature(module: &mut JITModule) -> Signature {
    let mut signature = module.make_signature();
    for _ in 0..ABI_PARAM_COUNT {
        signature.params.push(AbiParam::new(types::I64));
    }
    signature.returns.push(AbiParam::new(types::I64));
    signature
}

fn rts_dispatch_signature(module: &mut JITModule) -> Signature {
    let mut sig = module.make_signature();
    for _ in 0..7 {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

fn call_dispatch_signature(module: &mut JITModule) -> Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64)); // callee ptr
    sig.params.push(AbiParam::new(types::I64)); // callee len
    sig.params.push(AbiParam::new(types::I64)); // argc
    for _ in 0..ABI_ARG_SLOTS {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

fn declare_rts_dispatch_import(
    module: &mut JITModule,
    declarations: &mut BTreeMap<String, FuncId>,
) -> Result<FuncId> {
    if let Some(existing) = declarations.get(RTS_DISPATCH_SYMBOL).copied() {
        return Ok(existing);
    }
    let sig = rts_dispatch_signature(module);
    let id = module
        .declare_function(RTS_DISPATCH_SYMBOL, Linkage::Import, &sig)
        .context("failed to declare JIT __rts_dispatch")?;
    declarations.insert(RTS_DISPATCH_SYMBOL.to_string(), id);
    Ok(id)
}

fn declare_call_dispatch_import(
    module: &mut JITModule,
    declarations: &mut BTreeMap<String, FuncId>,
) -> Result<FuncId> {
    if let Some(existing) = declarations.get(RTS_CALL_DISPATCH_SYMBOL).copied() {
        return Ok(existing);
    }
    let sig = call_dispatch_signature(module);
    let id = module
        .declare_function(RTS_CALL_DISPATCH_SYMBOL, Linkage::Import, &sig)
        .context("failed to declare JIT __rts_call_dispatch")?;
    declarations.insert(RTS_CALL_DISPATCH_SYMBOL.to_string(), id);
    Ok(id)
}

fn define_function(
    module: &mut JITModule,
    declarations: &BTreeMap<String, FuncId>,
    rts_dispatch_id: FuncId,
    function_id: FuncId,
    function: &MirFunction,
) -> Result<()> {
    let mut context = module.make_context();
    context.func.signature = function_signature(module);
    let mut builder_context = FunctionBuilderContext::new();

    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let entry_block = builder.create_block();
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        let mut default_return = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
        let entry_params = builder.block_params(entry_block).to_vec();

        let mut terminated = false;

        'emit: for block in &function.blocks {
            for statement in &block.statements {
                let text = statement.text.trim();
                if text.is_empty() || text == "ret" || text == "{" || text == "}" {
                    continue;
                }

                if text.starts_with("enter ") {
                    if let Some(parameter_names) = parse_enter_parameters(text) {
                        for (index, parameter_name) in
                            parameter_names.into_iter().take(ABI_ARG_SLOTS).enumerate()
                        {
                            if !is_valid_binding_name(parameter_name.as_str()) {
                                continue;
                            }

                            let value_handle =
                                entry_params.get(index + 1).copied().unwrap_or_else(|| {
                                    builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)
                                });
                            let _ = lower_runtime_binding(
                                module,
                                &mut builder,
                                rts_dispatch_id,
                                parameter_name.as_str(),
                                value_handle,
                                true,
                            )?;
                        }
                    }
                    continue;
                }

                if text.starts_with("import ") {
                    continue;
                }

                if let Some(value) = parse_return_literal(text) {
                    default_return = builder.ins().iconst(types::I64, value);
                    continue;
                }

                if let Some(return_expr) = parse_return_expression(text) {
                    let return_value = if let Some(expression) = return_expr {
                        lower_runtime_expression(
                            module,
                            &mut builder,
                            rts_dispatch_id,
                            &expression,
                        )?
                    } else {
                        builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)
                    };
                    builder.ins().return_(&[return_value]);
                    terminated = true;
                    break 'emit;
                }

                if let Some(declaration) = parse_declaration_statement(text) {
                    let initializer_handle =
                        if let Some(initializer) = declaration.initializer.as_deref() {
                            lower_runtime_expression(
                                module,
                                &mut builder,
                                rts_dispatch_id,
                                initializer,
                            )?
                        } else {
                            builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)
                        };
                    let _ = lower_runtime_binding(
                        module,
                        &mut builder,
                        rts_dispatch_id,
                        declaration.name.as_str(),
                        initializer_handle,
                        declaration.mutable,
                    )?;
                    continue;
                }

                if let Some(call) = parse_call_statement(text) {
                    let Some(callee_id) = declarations.get(call.callee.as_str()) else {
                        bail!(
                            "unsupported MIR call target '{}' in function '{}'",
                            call.callee,
                            function.name
                        );
                    };

                    if call.args.len() > ABI_ARG_SLOTS {
                        bail!(
                            "function '{}' called '{}' with {} arguments, but RTS ABI supports up to {} arguments per call",
                            function.name,
                            call.callee,
                            call.args.len(),
                            ABI_ARG_SLOTS
                        );
                    }

                    let mut lowered_args = Vec::with_capacity(ABI_PARAM_COUNT);
                    lowered_args.push(builder.ins().iconst(types::I64, call.args.len() as i64));

                    for expression in call.args.iter().take(ABI_ARG_SLOTS) {
                        let value = lower_call_argument(
                            module,
                            declarations,
                            &mut builder,
                            rts_dispatch_id,
                            expression,
                        )?;
                        lowered_args.push(value);
                    }

                    while lowered_args.len() < ABI_PARAM_COUNT {
                        lowered_args.push(builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE));
                    }

                    let local = module.declare_func_in_func(*callee_id, builder.func);
                    let call_inst = builder.ins().call(local, &lowered_args);
                    if let Some(value) = builder.inst_results(call_inst).first().copied() {
                        default_return = value;
                    }
                    continue;
                }

                let value = lower_runtime_statement(module, &mut builder, rts_dispatch_id, text)?;
                default_return = value;
            }
        }

        if !terminated {
            builder.ins().return_(&[default_return]);
        }
        builder.finalize();
    }

    module
        .define_function(function_id, &mut context)
        .with_context(|| format!("failed to define JIT function '{}'", function.name))?;
    module.clear_context(&mut context);
    Ok(())
}

fn emit_rts_dispatch(
    module: &mut JITModule,
    builder: &mut FunctionBuilder,
    dispatch_id: FuncId,
    fn_id: i64,
    args: &[cranelift_codegen::ir::Value],
) -> Result<cranelift_codegen::ir::Value> {
    let mut call_args: Vec<cranelift_codegen::ir::Value> = Vec::with_capacity(7);
    call_args.push(builder.ins().iconst(types::I64, fn_id));
    for &arg in args.iter().take(6) {
        call_args.push(arg);
    }
    while call_args.len() < 7 {
        call_args.push(builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE));
    }
    let local = module.declare_func_in_func(dispatch_id, builder.func);
    let call = builder.ins().call(local, &call_args);
    Ok(builder
        .inst_results(call)
        .first()
        .copied()
        .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)))
}

fn lower_runtime_expression(
    module: &mut JITModule,
    builder: &mut FunctionBuilder,
    dispatch_id: FuncId,
    expression: &str,
) -> Result<cranelift_codegen::ir::Value> {
    let bytes = Box::leak(expression.as_bytes().to_vec().into_boxed_slice());
    let ptr = builder.ins().iconst(types::I64, bytes.as_ptr() as i64);
    let len = builder.ins().iconst(types::I64, bytes.len() as i64);
    emit_rts_dispatch(module, builder, dispatch_id, FN_EVAL_EXPR, &[ptr, len])
}

fn lower_call_argument(
    module: &mut JITModule,
    declarations: &BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
    dispatch_id: FuncId,
    expression: &str,
) -> Result<cranelift_codegen::ir::Value> {
    let text = expression.trim();
    if let Some(nested_call) = parse_call_statement(text) {
        if let Some(callee_id) = declarations.get(nested_call.callee.as_str()) {
            if nested_call.args.len() > ABI_ARG_SLOTS {
                bail!(
                    "function argument call '{}' has {} arguments, but RTS ABI supports up to {} arguments per call",
                    nested_call.callee,
                    nested_call.args.len(),
                    ABI_ARG_SLOTS
                );
            }

            let mut lowered_args = Vec::with_capacity(ABI_PARAM_COUNT);
            lowered_args.push(
                builder
                    .ins()
                    .iconst(types::I64, nested_call.args.len() as i64),
            );

            for nested_arg in nested_call.args.iter().take(ABI_ARG_SLOTS) {
                let lowered_nested_arg =
                    lower_call_argument(module, declarations, builder, dispatch_id, nested_arg)?;
                lowered_args.push(lowered_nested_arg);
            }

            while lowered_args.len() < ABI_PARAM_COUNT {
                lowered_args.push(builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE));
            }

            let local = module.declare_func_in_func(*callee_id, builder.func);
            let call_inst = builder.ins().call(local, &lowered_args);
            return Ok(builder
                .inst_results(call_inst)
                .first()
                .copied()
                .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)));
        }
    }

    lower_runtime_expression(module, builder, dispatch_id, text)
}

fn lower_runtime_binding(
    module: &mut JITModule,
    builder: &mut FunctionBuilder,
    dispatch_id: FuncId,
    name: &str,
    value_handle: cranelift_codegen::ir::Value,
    mutable: bool,
) -> Result<cranelift_codegen::ir::Value> {
    let bytes = Box::leak(name.as_bytes().to_vec().into_boxed_slice());
    let name_ptr = builder.ins().iconst(types::I64, bytes.as_ptr() as i64);
    let name_len = builder.ins().iconst(types::I64, bytes.len() as i64);
    let mutable_flag = builder
        .ins()
        .iconst(types::I64, if mutable { 1 } else { 0 });
    emit_rts_dispatch(
        module,
        builder,
        dispatch_id,
        FN_BIND_IDENTIFIER,
        &[name_ptr, name_len, value_handle, mutable_flag],
    )
}

fn lower_runtime_statement(
    module: &mut JITModule,
    builder: &mut FunctionBuilder,
    dispatch_id: FuncId,
    statement: &str,
) -> Result<cranelift_codegen::ir::Value> {
    let bytes = Box::leak(statement.as_bytes().to_vec().into_boxed_slice());
    let ptr = builder.ins().iconst(types::I64, bytes.as_ptr() as i64);
    let len = builder.ins().iconst(types::I64, bytes.len() as i64);
    emit_rts_dispatch(module, builder, dispatch_id, FN_EVAL_STMT, &[ptr, len])
}

fn define_stub_function(
    module: &mut JITModule,
    function_id: FuncId,
    function_name: &str,
    dispatch_id: FuncId,
    callee_ptr: i64,
    callee_len: i64,
) -> Result<()> {
    let mut context = module.make_context();
    context.func.signature = function_signature(module);
    let mut builder_context = FunctionBuilderContext::new();

    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let entry_block = builder.create_block();
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        let params = builder.block_params(entry_block).to_vec();
        let argc = params
            .first()
            .copied()
            .unwrap_or_else(|| builder.ins().iconst(types::I64, 0));

        let local_dispatch = module.declare_func_in_func(dispatch_id, builder.func);
        let mut args = Vec::with_capacity(3 + ABI_ARG_SLOTS);
        args.push(builder.ins().iconst(types::I64, callee_ptr));
        args.push(builder.ins().iconst(types::I64, callee_len));
        args.push(argc);
        for index in 0..ABI_ARG_SLOTS {
            args.push(
                params
                    .get(index + 1)
                    .copied()
                    .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)),
            );
        }

        let call = builder.ins().call(local_dispatch, &args);
        let returned = builder
            .inst_results(call)
            .first()
            .copied()
            .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE));
        builder.ins().return_(&[returned]);
        builder.finalize();
    }

    module
        .define_function(function_id, &mut context)
        .with_context(|| format!("failed to define synthetic JIT stub '{}'", function_name))?;
    module.clear_context(&mut context);
    Ok(())
}

pub fn execute_typed(
    module: &crate::mir::TypedMirModule,
    entry_function: &str,
) -> Result<JitReport> {
    let total_started = Instant::now();
    crate::namespaces::abi::reset_thread_state();
    let mut timings = JitTimings::default();

    let started = Instant::now();
    let mut jit = initialize_jit_module()?;
    timings.initialize_jit_ms = started.elapsed().as_secs_f64() * 1000.0;

    let mut declarations = BTreeMap::<String, FuncId>::new();
    let mut data_cache = BTreeMap::<String, DataId>::new();

    let started = Instant::now();
    let signature = super::typed::function_signature(&mut jit);
    for function in &module.functions {
        let id = jit
            .declare_function(&function.name, Linkage::Export, &signature)
            .with_context(|| format!("failed to declare typed JIT function '{}'", function.name))?;
        declarations.insert(function.name.clone(), id);
    }
    timings.declare_functions_ms = started.elapsed().as_secs_f64() * 1000.0;
    timings.scan_synthetic_calls_ms = 0.0;

    let started = Instant::now();

    for function in &module.functions {
        let id = declarations.get(&function.name).copied().ok_or_else(|| {
            anyhow!(
                "missing declaration for typed JIT function '{}'",
                function.name
            )
        })?;
        super::typed::define_typed_function(
            &mut jit,
            &mut declarations,
            &mut data_cache,
            id,
            function,
        )?;
    }
    timings.define_functions_ms = started.elapsed().as_secs_f64() * 1000.0;
    timings.declare_helpers_ms = 0.0;
    timings.define_stubs_ms = 0.0;

    let started = Instant::now();
    jit.finalize_definitions()
        .context("failed to finalize typed JIT definitions")?;
    timings.finalize_ms = started.elapsed().as_secs_f64() * 1000.0;

    {
        let mut fn_table = rustc_hash::FxHashMap::default();
        for function in &module.functions {
            if let Some(&func_id) = declarations.get(&function.name) {
                let ptr = jit.get_finalized_function(func_id);
                fn_table.insert(function.name.clone(), ptr as usize);
            }
        }
        crate::namespaces::abi::register_jit_fn_table(fn_table);
    }

    let started = Instant::now();
    let selected_entry = if declarations.contains_key(entry_function) {
        Some(entry_function.to_string())
    } else if entry_function != "main" && declarations.contains_key("main") {
        Some("main".to_string())
    } else {
        None
    };
    timings.resolve_entry_ms = started.elapsed().as_secs_f64() * 1000.0;

    let started = Instant::now();
    let (entry_name, entry_return_value, executed) = if let Some(entry_name) = selected_entry {
        let entry_id = declarations
            .get(&entry_name)
            .copied()
            .ok_or_else(|| anyhow!("failed to resolve typed JIT entry '{}'", entry_name))?;
        let address = jit.get_finalized_function(entry_id);
        let entry = unsafe {
            std::mem::transmute::<*const u8, extern "C" fn(i64, i64, i64, i64, i64, i64, i64) -> i64>(
                address,
            )
        };
        (entry_name, entry(0, 0, 0, 0, 0, 0, 0), true)
    } else {
        (
            entry_function.to_string(),
            super::mir_parse::ABI_UNDEFINED_HANDLE,
            false,
        )
    };
    timings.execute_entry_ms = started.elapsed().as_secs_f64() * 1000.0;
    timings.total_ms = total_started.elapsed().as_secs_f64() * 1000.0;

    Ok(JitReport {
        entry_function: entry_name,
        compiled_functions: declarations.len(),
        entry_return_value,
        executed,
        timings,
    })
}
