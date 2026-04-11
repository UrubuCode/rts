use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow, bail};
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{DataId, FuncId, Linkage, Module, default_libcall_names};

use crate::mir::{MirFunction, MirModule};

use super::mir_parse::{
    ABI_ARG_SLOTS, ABI_PARAM_COUNT, ABI_UNDEFINED_HANDLE, RTS_BIND_IDENTIFIER_SYMBOL,
    RTS_CALL_DISPATCH_SYMBOL, RTS_EVAL_EXPR_SYMBOL, RTS_EVAL_STMT_SYMBOL, is_valid_binding_name,
    parse_call_statement, parse_declaration_statement, parse_enter_parameters,
    parse_return_expression, parse_return_literal,
};

#[derive(Debug, Clone)]
pub struct JitReport {
    pub entry_function: String,
    pub compiled_functions: usize,
    pub entry_return_value: i64,
    pub executed: bool,
}

pub fn execute(module: &MirModule, entry_function: &str) -> Result<JitReport> {
    crate::namespaces::abi::reset_thread_state();
    let mut jit = initialize_jit_module()?;
    let mut declarations = BTreeMap::<String, FuncId>::new();
    let mut synthetic_stubs = Vec::<String>::new();

    let signature = function_signature(&mut jit);
    for function in &module.functions {
        let id = jit
            .declare_function(&function.name, Linkage::Export, &signature)
            .with_context(|| format!("failed to declare function '{}'", function.name))?;
        declarations.insert(function.name.clone(), id);
    }

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

    let eval_id = declare_helper_import(&mut jit, &mut declarations, RTS_EVAL_EXPR_SYMBOL)?;
    let eval_stmt_id = declare_helper_import(&mut jit, &mut declarations, RTS_EVAL_STMT_SYMBOL)?;
    let bind_id = declare_bind_import(&mut jit, &mut declarations)?;
    let dispatch_id = declare_dispatch_import(&mut jit, &mut declarations)?;

    for function in &module.functions {
        let id = declarations
            .get(&function.name)
            .copied()
            .ok_or_else(|| anyhow!("missing declaration for function '{}'", function.name))?;
        define_function(
            &mut jit,
            &declarations,
            eval_id,
            eval_stmt_id,
            bind_id,
            id,
            function,
        )?;
    }

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
            dispatch_id,
            leaked_symbol.as_ptr() as i64,
            leaked_symbol.len() as i64,
        )?;
    }

    jit.finalize_definitions()
        .context("failed to finalize JIT definitions")?;

    let selected_entry = if declarations.contains_key(entry_function) {
        Some(entry_function.to_string())
    } else if entry_function != "main" && declarations.contains_key("main") {
        Some("main".to_string())
    } else {
        None
    };

    let (entry_name, entry_return_value, executed) = if let Some(entry_name) = selected_entry {
        let entry_id = declarations
            .get(&entry_name)
            .copied()
            .ok_or_else(|| anyhow!("failed to resolve JIT entry '{}'", entry_name))?;

        let address = jit.get_finalized_function(entry_id);
        let entry = unsafe {
            // SAFETY: RTS JIT emits every lowered function with ABI `(argc, a0..a5) -> i64`.
            std::mem::transmute::<*const u8, extern "C" fn(i64, i64, i64, i64, i64, i64, i64) -> i64>(
                address,
            )
        };
        (entry_name, entry(0, 0, 0, 0, 0, 0, 0), true)
    } else {
        (entry_function.to_string(), ABI_UNDEFINED_HANDLE, false)
    };

    Ok(JitReport {
        entry_function: entry_name,
        compiled_functions: declarations.len(),
        entry_return_value,
        executed,
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
        RTS_EVAL_EXPR_SYMBOL,
        crate::namespaces::abi::__rts_eval_expr as *const u8,
    );
    builder.symbol(
        RTS_CALL_DISPATCH_SYMBOL,
        crate::namespaces::abi::__rts_call_dispatch as *const u8,
    );
    builder.symbol(
        RTS_EVAL_STMT_SYMBOL,
        crate::namespaces::abi::__rts_eval_stmt as *const u8,
    );
    builder.symbol(
        RTS_BIND_IDENTIFIER_SYMBOL,
        crate::namespaces::abi::__rts_bind_identifier as *const u8,
    );
    builder.symbol(
        "__rts_read_identifier",
        crate::namespaces::abi::__rts_read_identifier as *const u8,
    );
    builder.symbol(
        "__rts_binop",
        crate::namespaces::abi::__rts_binop as *const u8,
    );
    builder.symbol(
        "__rts_box_number",
        crate::namespaces::abi::__rts_box_number as *const u8,
    );
    builder.symbol(
        "__rts_unbox_number",
        crate::namespaces::abi::__rts_unbox_number as *const u8,
    );
    builder.symbol(
        "__rts_is_truthy",
        crate::namespaces::abi::__rts_is_truthy as *const u8,
    );
    builder.symbol(
        "__rts_box_string",
        crate::namespaces::abi::__rts_box_string as *const u8,
    );
    builder.symbol(
        "__rts_box_bool",
        crate::namespaces::abi::__rts_box_bool as *const u8,
    );
    builder.symbol(
        "__rts_reset_thread_state",
        crate::namespaces::abi::__rts_reset_thread_state as *const u8,
    );
    builder.symbol(
        "__rts_io_print",
        crate::namespaces::rust::__rts_io_print as *const u8,
    );
    builder.symbol(
        "__rts_io_stdout_write",
        crate::namespaces::rust::__rts_io_stdout_write as *const u8,
    );
    builder.symbol(
        "__rts_io_stderr_write",
        crate::namespaces::rust::__rts_io_stderr_write as *const u8,
    );
    builder.symbol(
        "__rts_io_panic",
        crate::namespaces::rust::__rts_io_panic as *const u8,
    );
    builder.symbol(
        "__rts_crypto_sha256",
        crate::namespaces::rust::__rts_crypto_sha256 as *const u8,
    );
    builder.symbol(
        "__rts_process_exit",
        crate::namespaces::rust::__rts_process_exit as *const u8,
    );
    builder.symbol(
        "__rts_global_set",
        crate::namespaces::rust::__rts_global_set as *const u8,
    );
    builder.symbol(
        "__rts_global_get",
        crate::namespaces::rust::__rts_global_get as *const u8,
    );
    builder.symbol(
        "__rts_global_has",
        crate::namespaces::rust::__rts_global_has as *const u8,
    );
    builder.symbol(
        "__rts_global_delete",
        crate::namespaces::rust::__rts_global_delete as *const u8,
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

fn eval_signature(module: &mut JITModule) -> Signature {
    let mut signature = module.make_signature();
    signature.params.push(AbiParam::new(types::I64));
    signature.params.push(AbiParam::new(types::I64));
    signature.returns.push(AbiParam::new(types::I64));
    signature
}

fn dispatch_signature(module: &mut JITModule) -> Signature {
    let mut signature = module.make_signature();
    signature.params.push(AbiParam::new(types::I64)); // callee ptr
    signature.params.push(AbiParam::new(types::I64)); // callee len
    signature.params.push(AbiParam::new(types::I64)); // argc
    for _ in 0..ABI_ARG_SLOTS {
        signature.params.push(AbiParam::new(types::I64));
    }
    signature.returns.push(AbiParam::new(types::I64));
    signature
}

fn bind_signature(module: &mut JITModule) -> Signature {
    let mut signature = module.make_signature();
    signature.params.push(AbiParam::new(types::I64)); // name ptr
    signature.params.push(AbiParam::new(types::I64)); // name len
    signature.params.push(AbiParam::new(types::I64)); // value handle
    signature.params.push(AbiParam::new(types::I64)); // mutable flag
    signature.returns.push(AbiParam::new(types::I64));
    signature
}

fn declare_helper_import(
    module: &mut JITModule,
    declarations: &mut BTreeMap<String, FuncId>,
    name: &str,
) -> Result<FuncId> {
    if let Some(existing) = declarations.get(name).copied() {
        return Ok(existing);
    }

    let signature = eval_signature(module);
    let id = module
        .declare_function(name, Linkage::Import, &signature)
        .with_context(|| format!("failed to declare JIT helper '{}'", name))?;
    declarations.insert(name.to_string(), id);
    Ok(id)
}

fn declare_bind_import(
    module: &mut JITModule,
    declarations: &mut BTreeMap<String, FuncId>,
) -> Result<FuncId> {
    if let Some(existing) = declarations.get(RTS_BIND_IDENTIFIER_SYMBOL).copied() {
        return Ok(existing);
    }

    let signature = bind_signature(module);
    let id = module
        .declare_function(RTS_BIND_IDENTIFIER_SYMBOL, Linkage::Import, &signature)
        .context("failed to declare JIT bind helper")?;
    declarations.insert(RTS_BIND_IDENTIFIER_SYMBOL.to_string(), id);
    Ok(id)
}

fn declare_dispatch_import(
    module: &mut JITModule,
    declarations: &mut BTreeMap<String, FuncId>,
) -> Result<FuncId> {
    if let Some(existing) = declarations.get(RTS_CALL_DISPATCH_SYMBOL).copied() {
        return Ok(existing);
    }

    let signature = dispatch_signature(module);
    let id = module
        .declare_function(RTS_CALL_DISPATCH_SYMBOL, Linkage::Import, &signature)
        .context("failed to declare JIT dispatch helper")?;
    declarations.insert(RTS_CALL_DISPATCH_SYMBOL.to_string(), id);
    Ok(id)
}

fn define_function(
    module: &mut JITModule,
    declarations: &BTreeMap<String, FuncId>,
    eval_id: FuncId,
    eval_stmt_id: FuncId,
    bind_id: FuncId,
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
                                bind_id,
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
                        lower_runtime_expression(module, &mut builder, eval_id, &expression)?
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
                            lower_runtime_expression(module, &mut builder, eval_id, initializer)?
                        } else {
                            builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)
                        };
                    let _ = lower_runtime_binding(
                        module,
                        &mut builder,
                        bind_id,
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
                            eval_id,
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

                let value = lower_runtime_statement(module, &mut builder, eval_stmt_id, text)?;
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

fn lower_runtime_expression(
    module: &mut JITModule,
    builder: &mut FunctionBuilder,
    eval_id: FuncId,
    expression: &str,
) -> Result<cranelift_codegen::ir::Value> {
    let bytes = Box::leak(expression.as_bytes().to_vec().into_boxed_slice());
    let expression_ptr = builder.ins().iconst(types::I64, bytes.as_ptr() as i64);
    let expression_len = builder.ins().iconst(types::I64, bytes.len() as i64);

    let local_eval = module.declare_func_in_func(eval_id, builder.func);
    let call = builder
        .ins()
        .call(local_eval, &[expression_ptr, expression_len]);
    Ok(builder
        .inst_results(call)
        .first()
        .copied()
        .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)))
}

fn lower_call_argument(
    module: &mut JITModule,
    declarations: &BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
    eval_id: FuncId,
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
                    lower_call_argument(module, declarations, builder, eval_id, nested_arg)?;
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

    lower_runtime_expression(module, builder, eval_id, text)
}

fn lower_runtime_binding(
    module: &mut JITModule,
    builder: &mut FunctionBuilder,
    bind_id: FuncId,
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

    let local_bind = module.declare_func_in_func(bind_id, builder.func);
    let call = builder.ins().call(
        local_bind,
        &[name_ptr, name_len, value_handle, mutable_flag],
    );
    Ok(builder
        .inst_results(call)
        .first()
        .copied()
        .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)))
}

fn lower_runtime_statement(
    module: &mut JITModule,
    builder: &mut FunctionBuilder,
    eval_stmt_id: FuncId,
    statement: &str,
) -> Result<cranelift_codegen::ir::Value> {
    let bytes = Box::leak(statement.as_bytes().to_vec().into_boxed_slice());
    let statement_ptr = builder.ins().iconst(types::I64, bytes.as_ptr() as i64);
    let statement_len = builder.ins().iconst(types::I64, bytes.len() as i64);

    let local_eval_stmt = module.declare_func_in_func(eval_stmt_id, builder.func);
    let call = builder
        .ins()
        .call(local_eval_stmt, &[statement_ptr, statement_len]);
    Ok(builder
        .inst_results(call)
        .first()
        .copied()
        .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)))
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
    crate::namespaces::abi::reset_thread_state();
    let mut jit = initialize_jit_module()?;
    let mut declarations = BTreeMap::<String, FuncId>::new();
    let mut data_cache = BTreeMap::<String, DataId>::new();

    let signature = super::typed_codegen::function_signature(&mut jit);
    for function in &module.functions {
        let id = jit
            .declare_function(&function.name, Linkage::Export, &signature)
            .with_context(|| format!("failed to declare typed JIT function '{}'", function.name))?;
        declarations.insert(function.name.clone(), id);
    }

    for function in &module.functions {
        let id = declarations.get(&function.name).copied().ok_or_else(|| {
            anyhow!(
                "missing declaration for typed JIT function '{}'",
                function.name
            )
        })?;
        super::typed_codegen::define_typed_function(
            &mut jit,
            &mut declarations,
            &mut data_cache,
            id,
            function,
        )?;
    }

    jit.finalize_definitions()
        .context("failed to finalize typed JIT definitions")?;

    let selected_entry = if declarations.contains_key(entry_function) {
        Some(entry_function.to_string())
    } else if entry_function != "main" && declarations.contains_key("main") {
        Some("main".to_string())
    } else {
        None
    };

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

    Ok(JitReport {
        entry_function: entry_name,
        compiled_functions: declarations.len(),
        entry_return_value,
        executed,
    })
}

#[cfg(test)]
mod tests {
    use crate::mir::cfg::{BasicBlock, Terminator};
    use crate::mir::{MirFunction, MirModule, MirStatement};

    use super::execute;

    #[test]
    fn jit_executes_main_and_returns_literal() {
        let module = MirModule {
            functions: vec![MirFunction {
                name: "main".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![MirStatement {
                        text: "ret 7".to_string(),
                    }],
                    terminator: Terminator::Return,
                }],
            }],
        };

        let report = execute(&module, "main").expect("jit should compile");
        assert_eq!(report.entry_function, "main");
        assert!(report.compiled_functions >= 1);
        assert_eq!(report.entry_return_value, 7);
        assert!(report.executed);
    }

    #[test]
    fn jit_entry_can_call_another_function() {
        let module = MirModule {
            functions: vec![
                MirFunction {
                    name: "helper".to_string(),
                    blocks: vec![BasicBlock {
                        label: "entry".to_string(),
                        statements: vec![MirStatement {
                            text: "ret 42".to_string(),
                        }],
                        terminator: Terminator::Return,
                    }],
                },
                MirFunction {
                    name: "main".to_string(),
                    blocks: vec![BasicBlock {
                        label: "entry".to_string(),
                        statements: vec![MirStatement {
                            text: "call helper".to_string(),
                        }],
                        terminator: Terminator::Return,
                    }],
                },
            ],
        };

        let report = execute(&module, "main").expect("jit should compile");
        assert_eq!(report.entry_return_value, 42);
        assert!(report.executed);
    }

    #[test]
    fn jit_skips_execution_when_entry_is_missing() {
        let module = MirModule {
            functions: vec![MirFunction {
                name: "helper".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![MirStatement {
                        text: "ret 42".to_string(),
                    }],
                    terminator: Terminator::Return,
                }],
            }],
        };

        let report = execute(&module, "main").expect("jit should compile");
        assert_eq!(report.entry_function, "main");
        assert_eq!(report.entry_return_value, 0);
        assert!(!report.executed);
    }

    #[test]
    fn jit_unknown_namespace_call_falls_back_to_runtime_dispatch() {
        let module = MirModule {
            functions: vec![MirFunction {
                name: "main".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![MirStatement {
                        text: r#"io.print("hello from jit")"#.to_string(),
                    }],
                    terminator: Terminator::Return,
                }],
            }],
        };

        let report = execute(&module, "main").expect("jit should compile");
        assert_eq!(report.entry_return_value, 0);
        assert!(report.executed);
    }

    #[test]
    fn jit_accepts_typed_variable_declaration_statement() {
        let module = MirModule {
            functions: vec![MirFunction {
                name: "main".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![MirStatement {
                        text: "const valor: i32 = 2 * 60 * 60 * 1000;".to_string(),
                    }],
                    terminator: Terminator::Return,
                }],
            }],
        };

        let report = execute(&module, "main").expect("jit should compile declarations");
        assert_eq!(report.entry_return_value, 0);
        assert!(report.executed);
    }

    #[test]
    fn jit_executes_if_else_statement_via_runtime_evaluator() {
        let module = MirModule {
            functions: vec![MirFunction {
                name: "main".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![MirStatement {
                        text: "if (true) { io.print(1); } else { io.print(2); }".to_string(),
                    }],
                    terminator: Terminator::Return,
                }],
            }],
        };

        let report = execute(&module, "main").expect("jit should evaluate if/else statement");
        assert_eq!(report.entry_return_value, 0);
        assert!(report.executed);
    }

    #[test]
    fn jit_supports_nested_user_function_call_arguments() {
        let module = MirModule {
            functions: vec![
                MirFunction {
                    name: "helper".to_string(),
                    blocks: vec![BasicBlock {
                        label: "entry".to_string(),
                        statements: vec![MirStatement {
                            text: "return 4;".to_string(),
                        }],
                        terminator: Terminator::Return,
                    }],
                },
                MirFunction {
                    name: "id".to_string(),
                    blocks: vec![BasicBlock {
                        label: "entry".to_string(),
                        statements: vec![
                            MirStatement {
                                text: "enter id(n)".to_string(),
                            },
                            MirStatement {
                                text: "return n;".to_string(),
                            },
                        ],
                        terminator: Terminator::Return,
                    }],
                },
                MirFunction {
                    name: "main".to_string(),
                    blocks: vec![BasicBlock {
                        label: "entry".to_string(),
                        statements: vec![MirStatement {
                            text: "id(helper())".to_string(),
                        }],
                        terminator: Terminator::Return,
                    }],
                },
            ],
        };

        let report = execute(&module, "main").expect("jit should support nested call arguments");
        assert_ne!(report.entry_return_value, 0);
        assert!(report.executed);
    }
}
