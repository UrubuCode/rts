use std::collections::BTreeMap;
use std::env;
use std::hash::{Hash, Hasher};

use anyhow::{Context, Result, anyhow, bail};
use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types};
use cranelift_codegen::isa;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module, default_libcall_names};
use cranelift_object::{ObjectBuilder, ObjectModule};
use target_lexicon::Triple;

use crate::mir::{MirFunction, MirModule};

use super::mir_parse::{
    ABI_ARG_SLOTS, ABI_PARAM_COUNT, ABI_UNDEFINED_HANDLE, RTS_BIND_IDENTIFIER_SYMBOL,
    RTS_CALL_DISPATCH_SYMBOL, RTS_EVAL_EXPR_SYMBOL, RTS_EVAL_STMT_SYMBOL, is_valid_binding_name,
    parse_call_statement, parse_declaration_statement, parse_enter_parameters,
    parse_return_expression, parse_return_literal,
};

#[derive(Debug, Clone, Copy)]
pub struct ObjectBuildOptions {
    pub emit_entrypoint: bool,
    pub optimize_for_production: bool,
}

pub fn lower_to_native_object(mir: &MirModule) -> Result<Vec<u8>> {
    lower_to_native_object_with_options(
        mir,
        &ObjectBuildOptions {
            emit_entrypoint: true,
            optimize_for_production: false,
        },
    )
}

pub fn lower_to_native_object_with_options(
    mir: &MirModule,
    options: &ObjectBuildOptions,
) -> Result<Vec<u8>> {
    let mut object_module = initialize_object_module(options)?;
    let mut declarations = BTreeMap::<String, FuncId>::new();
    let mut data_cache = BTreeMap::<String, DataId>::new();

    let signature = function_signature(&mut object_module);
    for function in &mir.functions {
        let id = object_module
            .declare_function(&function.name, Linkage::Export, &signature)
            .with_context(|| format!("failed to declare AOT function '{}'", function.name))?;
        declarations.insert(function.name.clone(), id);
    }

    let needs_synthetic_start = options.emit_entrypoint && !declarations.contains_key("_start");
    if needs_synthetic_start {
        let start_id = object_module
            .declare_function("_start", Linkage::Export, &signature)
            .context("failed to declare AOT synthetic '_start'")?;
        declarations.insert("_start".to_string(), start_id);
    }

    for function in &mir.functions {
        let id = declarations
            .get(&function.name)
            .copied()
            .ok_or_else(|| anyhow!("missing declaration for AOT function '{}'", function.name))?;
        define_function(
            &mut object_module,
            &mut declarations,
            &mut data_cache,
            id,
            function,
        )?;
    }

    if needs_synthetic_start {
        define_synthetic_start(&mut object_module, &declarations)?;
    }

    let object = object_module.finish();
    object
        .emit()
        .map_err(|error| anyhow!("failed to emit native object bytes: {error}"))
}

pub fn build_namespace_dispatch_object(
    callees: &[String],
    optimize_for_production: bool,
) -> Result<Vec<u8>> {
    let mut module = initialize_object_module(&ObjectBuildOptions {
        emit_entrypoint: false,
        optimize_for_production,
    })?;

    let wrapper_signature = function_signature(&mut module);
    let dispatch_signature = runtime_dispatch_signature(&mut module);
    let dispatch_id = module
        .declare_function(
            RTS_CALL_DISPATCH_SYMBOL,
            Linkage::Import,
            &dispatch_signature,
        )
        .context("failed to declare runtime dispatch import for namespace wrappers")?;

    let mut declarations = BTreeMap::<String, FuncId>::new();
    for callee in callees {
        let id = module
            .declare_function(callee, Linkage::Export, &wrapper_signature)
            .with_context(|| format!("failed to declare namespace wrapper '{}'", callee))?;
        declarations.insert(callee.clone(), id);
    }

    let mut data_cache = BTreeMap::<String, DataId>::new();

    for callee in callees {
        let Some(function_id) = declarations.get(callee).copied() else {
            continue;
        };

        let mut context = module.make_context();
        context.func.signature = wrapper_signature.clone();
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

            let data_id = declare_string_data(
                &mut module,
                &mut data_cache,
                "__rts_callee_",
                callee.as_str(),
                callee.as_bytes(),
            )?;
            let callee_ref = module.declare_data_in_func(data_id, builder.func);
            let callee_ptr = builder.ins().symbol_value(types::I64, callee_ref);
            let callee_len = builder.ins().iconst(types::I64, callee.len() as i64);

            let local_dispatch = module.declare_func_in_func(dispatch_id, builder.func);
            let mut dispatch_args = Vec::with_capacity(3 + ABI_ARG_SLOTS);
            dispatch_args.push(callee_ptr);
            dispatch_args.push(callee_len);
            dispatch_args.push(argc);
            for index in 0..ABI_ARG_SLOTS {
                dispatch_args.push(
                    params
                        .get(index + 1)
                        .copied()
                        .unwrap_or_else(|| builder.ins().iconst(types::I64, 0)),
                );
            }

            let call = builder.ins().call(local_dispatch, &dispatch_args);
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
            .with_context(|| format!("failed to define namespace wrapper '{}'", callee))?;
        module.clear_context(&mut context);
    }

    let object = module.finish();
    object
        .emit()
        .map_err(|error| anyhow!("failed to emit namespace wrapper object: {error}"))
}

pub fn lower_typed_to_native_object(
    mir: &crate::mir::TypedMirModule,
    options: &ObjectBuildOptions,
) -> Result<Vec<u8>> {
    let mut object_module = initialize_object_module(options)?;
    let mut declarations = BTreeMap::<String, FuncId>::new();
    let mut data_cache = BTreeMap::<String, DataId>::new();

    let signature = super::typed_codegen::function_signature(&mut object_module);

    for function in &mir.functions {
        let id = object_module
            .declare_function(&function.name, Linkage::Export, &signature)
            .with_context(|| format!("failed to declare typed AOT function '{}'", function.name))?;
        declarations.insert(function.name.clone(), id);
    }

    let needs_start = options.emit_entrypoint && !declarations.contains_key("_start");
    if needs_start {
        let start_id = object_module
            .declare_function("_start", Linkage::Export, &signature)
            .context("failed to declare typed AOT synthetic '_start'")?;
        declarations.insert("_start".to_string(), start_id);
    }

    for function in &mir.functions {
        let id = declarations.get(&function.name).copied().ok_or_else(|| {
            anyhow!(
                "missing declaration for typed AOT function '{}'",
                function.name
            )
        })?;
        super::typed_codegen::define_typed_function(
            &mut object_module,
            &mut declarations,
            &mut data_cache,
            id,
            function,
        )?;
    }

    if needs_start {
        define_synthetic_start(&mut object_module, &declarations)?;
    }

    let object = object_module.finish();
    object
        .emit()
        .map_err(|e| anyhow!("failed to emit typed AOT object: {e}"))
}

fn initialize_object_module(options: &ObjectBuildOptions) -> Result<ObjectModule> {
    let isa = resolve_target_isa(options)?;
    let builder = ObjectBuilder::new(isa, "rts_aot".to_string(), default_libcall_names())
        .context("failed to initialize Cranelift object builder")?;
    Ok(ObjectModule::new(builder))
}

fn resolve_target_isa(options: &ObjectBuildOptions) -> Result<std::sync::Arc<dyn isa::TargetIsa>> {
    if let Some(target) = env::var("RTS_TARGET")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        let triple = target
            .parse::<Triple>()
            .map_err(|error| anyhow!("invalid RTS_TARGET triple '{}': {}", target, error))?;

        match isa::lookup(triple.clone()) {
            Ok(builder) => {
                let flags = build_cranelift_flags(options)?;
                return builder
                    .finish(flags)
                    .with_context(|| format!("failed to finalize AOT ISA for '{}'", triple));
            }
            Err(error) => {
                eprintln!(
                    "RTS AOT: target '{}' is unsupported by this Cranelift build ({}). Falling back to host ISA.",
                    target, error
                );
            }
        }
    }

    let flags = build_cranelift_flags(options)?;
    let isa_builder = cranelift_native::builder()
        .map_err(|error| anyhow!("failed to build host ISA for AOT: {error}"))?;
    isa_builder
        .finish(flags)
        .context("failed to finalize host ISA for AOT")
}

fn build_cranelift_flags(options: &ObjectBuildOptions) -> Result<settings::Flags> {
    let mut settings_builder = settings::builder();
    settings_builder
        .set("is_pic", "false")
        .context("failed to configure Cranelift setting 'is_pic' for AOT")?;
    settings_builder
        .set(
            "opt_level",
            if options.optimize_for_production {
                "speed_and_size"
            } else {
                "none"
            },
        )
        .context("failed to configure Cranelift setting 'opt_level' for AOT")?;
    Ok(settings::Flags::new(settings_builder))
}

fn function_signature(module: &mut ObjectModule) -> Signature {
    let mut signature = module.make_signature();
    for _ in 0..ABI_PARAM_COUNT {
        signature.params.push(AbiParam::new(types::I64));
    }
    signature.returns.push(AbiParam::new(types::I64));
    signature
}

fn runtime_eval_signature(module: &mut ObjectModule) -> Signature {
    let mut signature = module.make_signature();
    signature.params.push(AbiParam::new(types::I64));
    signature.params.push(AbiParam::new(types::I64));
    signature.returns.push(AbiParam::new(types::I64));
    signature
}

fn runtime_dispatch_signature(module: &mut ObjectModule) -> Signature {
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

fn runtime_bind_signature(module: &mut ObjectModule) -> Signature {
    let mut signature = module.make_signature();
    signature.params.push(AbiParam::new(types::I64)); // name ptr
    signature.params.push(AbiParam::new(types::I64)); // name len
    signature.params.push(AbiParam::new(types::I64)); // value handle
    signature.params.push(AbiParam::new(types::I64)); // mutable flag
    signature.returns.push(AbiParam::new(types::I64));
    signature
}

fn define_function(
    module: &mut ObjectModule,
    declarations: &mut BTreeMap<String, FuncId>,
    data_cache: &mut BTreeMap<String, DataId>,
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
                                declarations,
                                data_cache,
                                &mut builder,
                                function,
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
                            declarations,
                            data_cache,
                            &mut builder,
                            function,
                            expression.as_str(),
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
                                declarations,
                                data_cache,
                                &mut builder,
                                function,
                                initializer,
                            )?
                        } else {
                            builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)
                        };
                    let _ = lower_runtime_binding(
                        module,
                        declarations,
                        data_cache,
                        &mut builder,
                        function,
                        declaration.name.as_str(),
                        initializer_handle,
                        declaration.mutable,
                    )?;
                    continue;
                }

                if let Some(call) = parse_call_statement(text) {
                    let call_signature = function_signature(module);
                    let callee_id = resolve_or_declare_import(
                        module,
                        declarations,
                        call.callee.as_str(),
                        &call_signature,
                        function,
                    )?;

                    let mut args = Vec::with_capacity(ABI_PARAM_COUNT);
                    args.push(builder.ins().iconst(types::I64, call.args.len() as i64));
                    for expression in call.args.iter().take(ABI_ARG_SLOTS) {
                        let value = lower_call_argument(
                            module,
                            declarations,
                            data_cache,
                            &mut builder,
                            function,
                            expression,
                        )?;
                        args.push(value);
                    }

                    if call.args.len() > ABI_ARG_SLOTS {
                        bail!(
                            "function '{}' called '{}' with {} arguments, but RTS ABI supports up to {} arguments per call",
                            function.name,
                            call.callee,
                            call.args.len(),
                            ABI_ARG_SLOTS
                        );
                    }

                    while args.len() < ABI_PARAM_COUNT {
                        args.push(builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE));
                    }

                    let local = module.declare_func_in_func(callee_id, builder.func);
                    let call_inst = builder.ins().call(local, &args);
                    if let Some(value) = builder.inst_results(call_inst).first().copied() {
                        default_return = value;
                    }
                    continue;
                }

                let value = lower_runtime_statement(
                    module,
                    declarations,
                    data_cache,
                    &mut builder,
                    function,
                    text,
                )?;
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
        .with_context(|| format!("failed to define AOT function '{}'", function.name))?;
    module.clear_context(&mut context);
    Ok(())
}

fn lower_runtime_expression(
    module: &mut ObjectModule,
    declarations: &mut BTreeMap<String, FuncId>,
    data_cache: &mut BTreeMap<String, DataId>,
    builder: &mut FunctionBuilder,
    function: &MirFunction,
    expression: &str,
) -> Result<cranelift_codegen::ir::Value> {
    let eval_signature = runtime_eval_signature(module);
    let eval_id = resolve_or_declare_import(
        module,
        declarations,
        RTS_EVAL_EXPR_SYMBOL,
        &eval_signature,
        function,
    )?;

    let expression = expression.trim();
    let data_id = declare_string_data(
        module,
        data_cache,
        "__rts_expr_",
        expression,
        expression.as_bytes(),
    )?;
    let data_ref = module.declare_data_in_func(data_id, builder.func);
    let expression_ptr = builder.ins().symbol_value(types::I64, data_ref);
    let expression_len = builder.ins().iconst(types::I64, expression.len() as i64);

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
    module: &mut ObjectModule,
    declarations: &mut BTreeMap<String, FuncId>,
    data_cache: &mut BTreeMap<String, DataId>,
    builder: &mut FunctionBuilder,
    function: &MirFunction,
    expression: &str,
) -> Result<cranelift_codegen::ir::Value> {
    let text = expression.trim();
    if let Some(nested_call) = parse_call_statement(text) {
        if let Some(callee_id) = declarations.get(nested_call.callee.as_str()).copied() {
            if nested_call.args.len() > ABI_ARG_SLOTS {
                bail!(
                    "function argument call '{}' has {} arguments, but RTS ABI supports up to {} arguments per call",
                    nested_call.callee,
                    nested_call.args.len(),
                    ABI_ARG_SLOTS
                );
            }

            let mut args = Vec::with_capacity(ABI_PARAM_COUNT);
            args.push(
                builder
                    .ins()
                    .iconst(types::I64, nested_call.args.len() as i64),
            );
            for nested_arg in nested_call.args.iter().take(ABI_ARG_SLOTS) {
                let lowered_nested = lower_call_argument(
                    module,
                    declarations,
                    data_cache,
                    builder,
                    function,
                    nested_arg,
                )?;
                args.push(lowered_nested);
            }

            while args.len() < ABI_PARAM_COUNT {
                args.push(builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE));
            }

            let local = module.declare_func_in_func(callee_id, builder.func);
            let call_inst = builder.ins().call(local, &args);
            return Ok(builder
                .inst_results(call_inst)
                .first()
                .copied()
                .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)));
        }
    }

    lower_runtime_expression(module, declarations, data_cache, builder, function, text)
}

fn lower_runtime_binding(
    module: &mut ObjectModule,
    declarations: &mut BTreeMap<String, FuncId>,
    data_cache: &mut BTreeMap<String, DataId>,
    builder: &mut FunctionBuilder,
    function: &MirFunction,
    name: &str,
    value_handle: cranelift_codegen::ir::Value,
    mutable: bool,
) -> Result<cranelift_codegen::ir::Value> {
    let bind_signature = runtime_bind_signature(module);
    let bind_id = resolve_or_declare_import(
        module,
        declarations,
        RTS_BIND_IDENTIFIER_SYMBOL,
        &bind_signature,
        function,
    )?;

    let data_id = declare_string_data(
        module,
        data_cache,
        "__rts_bind_name_",
        name,
        name.as_bytes(),
    )?;
    let data_ref = module.declare_data_in_func(data_id, builder.func);
    let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
    let name_len = builder.ins().iconst(types::I64, name.len() as i64);
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
    module: &mut ObjectModule,
    declarations: &mut BTreeMap<String, FuncId>,
    data_cache: &mut BTreeMap<String, DataId>,
    builder: &mut FunctionBuilder,
    function: &MirFunction,
    statement: &str,
) -> Result<cranelift_codegen::ir::Value> {
    let eval_signature = runtime_eval_signature(module);
    let eval_id = resolve_or_declare_import(
        module,
        declarations,
        RTS_EVAL_STMT_SYMBOL,
        &eval_signature,
        function,
    )?;

    let statement = statement.trim();
    let data_id = declare_string_data(
        module,
        data_cache,
        "__rts_stmt_",
        statement,
        statement.as_bytes(),
    )?;
    let data_ref = module.declare_data_in_func(data_id, builder.func);
    let statement_ptr = builder.ins().symbol_value(types::I64, data_ref);
    let statement_len = builder.ins().iconst(types::I64, statement.len() as i64);

    let local_eval = module.declare_func_in_func(eval_id, builder.func);
    let call = builder
        .ins()
        .call(local_eval, &[statement_ptr, statement_len]);
    Ok(builder
        .inst_results(call)
        .first()
        .copied()
        .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)))
}

fn resolve_or_declare_import(
    module: &mut ObjectModule,
    declarations: &mut BTreeMap<String, FuncId>,
    name: &str,
    signature: &Signature,
    function: &MirFunction,
) -> Result<FuncId> {
    if let Some(existing) = declarations.get(name).copied() {
        return Ok(existing);
    }

    let id = module
        .declare_function(name, Linkage::Import, signature)
        .with_context(|| {
            format!(
                "failed to declare imported callee '{}' for function '{}'",
                name, function.name
            )
        })?;
    declarations.insert(name.to_string(), id);
    Ok(id)
}

fn declare_string_data(
    module: &mut ObjectModule,
    data_cache: &mut BTreeMap<String, DataId>,
    prefix: &str,
    symbol_seed: &str,
    payload: &[u8],
) -> Result<DataId> {
    let key = format!("{prefix}{symbol_seed}");
    if let Some(existing) = data_cache.get(&key).copied() {
        return Ok(existing);
    }

    let symbol = format!("{prefix}{:016x}", stable_hash(symbol_seed));
    let name = sanitize_symbol_name(symbol.as_str());
    let id = module
        .declare_data(name.as_str(), Linkage::Local, false, false)
        .with_context(|| format!("failed to declare data symbol '{}'", name))?;

    let mut description = DataDescription::new();
    description.define(payload.to_vec().into_boxed_slice());
    module
        .define_data(id, &description)
        .with_context(|| format!("failed to define data payload for '{}'", name))?;

    data_cache.insert(key, id);
    Ok(id)
}

fn stable_hash(input: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}

fn sanitize_symbol_name(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$' | '.') {
            output.push(ch);
        } else {
            output.push('_');
        }
    }
    output
}

fn define_synthetic_start(
    module: &mut ObjectModule,
    declarations: &BTreeMap<String, FuncId>,
) -> Result<()> {
    let Some(start_id) = declarations.get("_start").copied() else {
        return Ok(());
    };

    let mut context = module.make_context();
    context.func.signature = function_signature(module);
    let mut builder_context = FunctionBuilderContext::new();

    {
        let mut builder = FunctionBuilder::new(&mut context.func, &mut builder_context);
        let entry_block = builder.create_block();
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        // Reset runtime state before executing main (like JIT does)
        // This ensures consistent state between JIT and AOT execution
        let reset_sig = module.make_signature(); // () -> ()
        let reset_id = module
            .declare_function("__rts_reset_thread_state", Linkage::Import, &reset_sig)
            .context("failed to declare reset_thread_state for AOT")?;
        let reset_local = module.declare_func_in_func(reset_id, builder.func);
        builder.ins().call(reset_local, &[]);

        let default_return = if let Some(main_id) = declarations.get("main").copied() {
            let local = module.declare_func_in_func(main_id, builder.func);
            let mut args = Vec::with_capacity(ABI_PARAM_COUNT);
            args.push(builder.ins().iconst(types::I64, 0));
            for _ in 0..ABI_ARG_SLOTS {
                args.push(builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE));
            }
            let call = builder.ins().call(local, &args);
            builder
                .inst_results(call)
                .first()
                .copied()
                .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE))
        } else {
            builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)
        };

        builder.ins().return_(&[default_return]);
        builder.finalize();
    }

    module
        .define_function(start_id, &mut context)
        .context("failed to define AOT synthetic '_start'")?;
    module.clear_context(&mut context);
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::mir::cfg::{BasicBlock, Terminator};
    use crate::mir::{MirFunction, MirModule, MirStatement};

    use super::{build_namespace_dispatch_object, lower_to_native_object, parse_call_statement};

    #[test]
    fn emits_non_empty_native_object() {
        let module = MirModule {
            functions: vec![MirFunction {
                name: "main".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![MirStatement {
                        text: "ret 3".to_string(),
                    }],
                    terminator: Terminator::Return,
                }],
            }],
        };

        let bytes = lower_to_native_object(&module).expect("AOT object must compile");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn parses_direct_call_with_arguments() {
        let parsed = parse_call_statement(r#"io.print("hello", 123)"#).expect("call parse");
        assert_eq!(parsed.callee, "io.print");
        assert_eq!(parsed.args, vec![r#""hello""#, "123"]);
    }

    #[test]
    fn lowers_imported_namespace_call_without_panicking() {
        let module = MirModule {
            functions: vec![MirFunction {
                name: "main".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![MirStatement {
                        text: r#"io.print("hello")"#.to_string(),
                    }],
                    terminator: Terminator::Return,
                }],
            }],
        };

        let bytes = lower_to_native_object(&module).expect("AOT object should compile");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn builds_namespace_wrapper_object() {
        let bytes = build_namespace_dispatch_object(
            &[String::from("io.print"), String::from("process.arch")],
            false,
        )
        .expect("namespace wrapper object should compile");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn lowers_typed_variable_declaration_without_panicking() {
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

        let bytes = lower_to_native_object(&module).expect("declaration should lower to AOT");
        assert!(!bytes.is_empty());
    }

    #[test]
    fn lowers_if_else_statement_via_runtime_evaluator() {
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

        let bytes = lower_to_native_object(&module).expect("if/else should lower to AOT");
        assert!(!bytes.is_empty());
    }
}
