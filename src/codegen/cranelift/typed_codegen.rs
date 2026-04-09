use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use anyhow::{Context, Result};
use cranelift_codegen::ir::{AbiParam, InstBuilder, types, Value};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module};

use crate::mir::{MirBinOp, MirInstruction, MirUnaryOp, TypedMirFunction, VReg};

const ABI_ARG_SLOTS: usize = 6;
const ABI_PARAM_COUNT: usize = ABI_ARG_SLOTS + 1;
const ABI_UNDEFINED_HANDLE: i64 = 0;

const RTS_EVAL_EXPR: &str = "__rts_eval_expr";
const RTS_EVAL_STMT: &str = "__rts_eval_stmt";
const RTS_BIND: &str = "__rts_bind_identifier";
const RTS_READ: &str = "__rts_read_identifier";
const RTS_BINOP: &str = "__rts_binop";
const RTS_BOX_NUMBER: &str = "__rts_box_number";
const RTS_CALL_DISPATCH: &str = "__rts_call_dispatch";

pub fn function_signature<M: Module>(module: &mut M) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    for _ in 0..ABI_PARAM_COUNT {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

fn eval_signature<M: Module>(module: &mut M) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64)); // ptr
    sig.params.push(AbiParam::new(types::I64)); // len
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

fn bind_signature<M: Module>(module: &mut M) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64)); // name ptr
    sig.params.push(AbiParam::new(types::I64)); // name len
    sig.params.push(AbiParam::new(types::I64)); // value handle
    sig.params.push(AbiParam::new(types::I64)); // mutable flag
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

fn read_signature<M: Module>(module: &mut M) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64)); // name ptr
    sig.params.push(AbiParam::new(types::I64)); // name len
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

fn binop_signature<M: Module>(module: &mut M) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64)); // op tag
    sig.params.push(AbiParam::new(types::I64)); // lhs handle
    sig.params.push(AbiParam::new(types::I64)); // rhs handle
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

fn box_number_signature<M: Module>(module: &mut M) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64)); // f64 bits as i64
    sig.returns.push(AbiParam::new(types::I64)); // handle
    sig
}

fn dispatch_signature<M: Module>(module: &mut M) -> cranelift_codegen::ir::Signature {
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

pub fn define_typed_function<M: Module>(
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    data_cache: &mut BTreeMap<String, DataId>,
    function_id: FuncId,
    function: &TypedMirFunction,
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

        let entry_params = builder.block_params(entry_block).to_vec();
        let mut vreg_map = BTreeMap::<VReg, Value>::new();
        let mut default_return = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
        let mut terminated = false;

        let instructions: Vec<_> = function
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter())
            .collect();

        for instruction in &instructions {
            if terminated {
                break;
            }

            match instruction {
                MirInstruction::ConstNumber(dst, val) => {
                    let bits = i64::from_ne_bytes(val.to_ne_bytes());
                    let bits_val = builder.ins().iconst(types::I64, bits);
                    let sig = box_number_signature(module);
                    let box_fn = ensure_import(module, func_declarations, RTS_BOX_NUMBER, &sig)?;
                    let local = module.declare_func_in_func(box_fn, builder.func);
                    let call = builder.ins().call(local, &[bits_val]);
                    let result = builder.inst_results(call)[0];
                    vreg_map.insert(*dst, result);
                    default_return = result;
                }

                MirInstruction::ConstString(dst, s) => {
                    let quoted = format!(
                        "\"{}\"",
                        s.replace('\\', "\\\\").replace('"', "\\\"")
                    );
                    let result = emit_eval_expr(
                        module,
                        func_declarations,
                        data_cache,
                        &mut builder,
                        &quoted,
                    )?;
                    vreg_map.insert(*dst, result);
                    default_return = result;
                }

                MirInstruction::ConstBool(dst, b) => {
                    let text = if *b { "true" } else { "false" };
                    let result = emit_eval_expr(
                        module,
                        func_declarations,
                        data_cache,
                        &mut builder,
                        text,
                    )?;
                    vreg_map.insert(*dst, result);
                    default_return = result;
                }

                MirInstruction::ConstNull(dst) => {
                    let result = emit_eval_expr(
                        module,
                        func_declarations,
                        data_cache,
                        &mut builder,
                        "null",
                    )?;
                    vreg_map.insert(*dst, result);
                    default_return = result;
                }

                MirInstruction::ConstUndef(dst) => {
                    let result = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
                    vreg_map.insert(*dst, result);
                }

                MirInstruction::LoadParam(dst, index) => {
                    let value = entry_params
                        .get(index + 1)
                        .copied()
                        .unwrap_or_else(|| {
                            builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)
                        });
                    vreg_map.insert(*dst, value);
                }

                MirInstruction::BinOp(dst, op, lhs, rhs) => {
                    let op_tag = binop_to_tag(op);
                    let lhs_val = resolve_vreg(&vreg_map, lhs, &mut builder);
                    let rhs_val = resolve_vreg(&vreg_map, rhs, &mut builder);
                    let op_val = builder.ins().iconst(types::I64, op_tag);
                    let sig = binop_signature(module);
                    let binop_fn =
                        ensure_import(module, func_declarations, RTS_BINOP, &sig)?;
                    let local = module.declare_func_in_func(binop_fn, builder.func);
                    let call = builder.ins().call(local, &[op_val, lhs_val, rhs_val]);
                    let result = builder.inst_results(call)[0];
                    vreg_map.insert(*dst, result);
                    default_return = result;
                }

                MirInstruction::UnaryOp(dst, op, src) => {
                    let src_val = resolve_vreg(&vreg_map, src, &mut builder);
                    let result = match op {
                        MirUnaryOp::Negate => {
                            // -x == 0 - x
                            let zero_bits =
                                i64::from_ne_bytes(0.0f64.to_ne_bytes());
                            let zero_val = builder.ins().iconst(types::I64, zero_bits);
                            let sig = box_number_signature(module);
                            let box_fn = ensure_import(
                                module,
                                func_declarations,
                                RTS_BOX_NUMBER,
                                &sig,
                            )?;
                            let local = module.declare_func_in_func(box_fn, builder.func);
                            let call = builder.ins().call(local, &[zero_val]);
                            let zero_handle = builder.inst_results(call)[0];

                            let op_val = builder.ins().iconst(types::I64, binop_to_tag(&MirBinOp::Sub));
                            let sig = binop_signature(module);
                            let binop_fn = ensure_import(
                                module,
                                func_declarations,
                                RTS_BINOP,
                                &sig,
                            )?;
                            let local = module.declare_func_in_func(binop_fn, builder.func);
                            let call =
                                builder
                                    .ins()
                                    .call(local, &[op_val, zero_handle, src_val]);
                            builder.inst_results(call)[0]
                        }
                        MirUnaryOp::Not => {
                            // !x: compare x === false via Eq(x, false_handle)
                            let false_handle = emit_eval_expr(
                                module,
                                func_declarations,
                                data_cache,
                                &mut builder,
                                "false",
                            )?;
                            let op_val = builder.ins().iconst(types::I64, binop_to_tag(&MirBinOp::Eq));
                            let sig = binop_signature(module);
                            let binop_fn = ensure_import(
                                module,
                                func_declarations,
                                RTS_BINOP,
                                &sig,
                            )?;
                            let local = module.declare_func_in_func(binop_fn, builder.func);
                            let call = builder.ins().call(local, &[op_val, src_val, false_handle]);
                            builder.inst_results(call)[0]
                        }
                        MirUnaryOp::Positive => {
                            // +x is identity for numbers
                            src_val
                        }
                    };
                    vreg_map.insert(*dst, result);
                    default_return = result;
                }

                MirInstruction::Call(dst, callee, args) => {
                    let result = if func_declarations.contains_key(callee.as_str()) {
                        // Direct call to a known user function
                        let callee_id = func_declarations[callee.as_str()];
                        let mut call_args = Vec::with_capacity(ABI_PARAM_COUNT);
                        call_args
                            .push(builder.ins().iconst(types::I64, args.len() as i64));
                        for arg in args.iter().take(ABI_ARG_SLOTS) {
                            call_args.push(resolve_vreg(&vreg_map, arg, &mut builder));
                        }
                        while call_args.len() < ABI_PARAM_COUNT {
                            call_args.push(
                                builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE),
                            );
                        }
                        let local = module.declare_func_in_func(callee_id, builder.func);
                        let call = builder.ins().call(local, &call_args);
                        builder.inst_results(call)[0]
                    } else {
                        // Unknown callee -> use __rts_call_dispatch
                        let sig = dispatch_signature(module);
                        let dispatch_fn = ensure_import(
                            module,
                            func_declarations,
                            RTS_CALL_DISPATCH,
                            &sig,
                        )?;

                        let data_id =
                            declare_string_data(module, data_cache, callee.as_str())?;
                        let data_ref =
                            module.declare_data_in_func(data_id, builder.func);
                        let callee_ptr =
                            builder.ins().symbol_value(types::I64, data_ref);
                        let callee_len =
                            builder.ins().iconst(types::I64, callee.len() as i64);

                        let mut dispatch_args = Vec::with_capacity(3 + ABI_ARG_SLOTS);
                        dispatch_args.push(callee_ptr);
                        dispatch_args.push(callee_len);
                        dispatch_args
                            .push(builder.ins().iconst(types::I64, args.len() as i64));
                        for arg in args.iter().take(ABI_ARG_SLOTS) {
                            dispatch_args
                                .push(resolve_vreg(&vreg_map, arg, &mut builder));
                        }
                        while dispatch_args.len() < 3 + ABI_ARG_SLOTS {
                            dispatch_args.push(
                                builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE),
                            );
                        }

                        let local =
                            module.declare_func_in_func(dispatch_fn, builder.func);
                        let call = builder.ins().call(local, &dispatch_args);
                        builder.inst_results(call)[0]
                    };
                    vreg_map.insert(*dst, result);
                    default_return = result;
                }

                MirInstruction::Bind(name, src, mutable) => {
                    let value_handle = resolve_vreg(&vreg_map, src, &mut builder);
                    let sig = bind_signature(module);
                    let bind_fn =
                        ensure_import(module, func_declarations, RTS_BIND, &sig)?;

                    let data_id =
                        declare_string_data(module, data_cache, name.as_str())?;
                    let data_ref = module.declare_data_in_func(data_id, builder.func);
                    let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
                    let name_len =
                        builder.ins().iconst(types::I64, name.len() as i64);
                    let mutable_flag = builder
                        .ins()
                        .iconst(types::I64, if *mutable { 1 } else { 0 });

                    let local = module.declare_func_in_func(bind_fn, builder.func);
                    let call = builder.ins().call(
                        local,
                        &[name_ptr, name_len, value_handle, mutable_flag],
                    );
                    let result = builder.inst_results(call)[0];
                    default_return = result;
                }

                MirInstruction::LoadBinding(dst, name) => {
                    let sig = read_signature(module);
                    let read_fn =
                        ensure_import(module, func_declarations, RTS_READ, &sig)?;

                    let data_id =
                        declare_string_data(module, data_cache, name.as_str())?;
                    let data_ref = module.declare_data_in_func(data_id, builder.func);
                    let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
                    let name_len =
                        builder.ins().iconst(types::I64, name.len() as i64);

                    let local = module.declare_func_in_func(read_fn, builder.func);
                    let call = builder.ins().call(local, &[name_ptr, name_len]);
                    let result = builder.inst_results(call)[0];
                    vreg_map.insert(*dst, result);
                    default_return = result;
                }

                MirInstruction::Return(Some(vreg)) => {
                    let value = resolve_vreg(&vreg_map, vreg, &mut builder);
                    builder.ins().return_(&[value]);
                    terminated = true;
                }

                MirInstruction::Return(None) => {
                    let value =
                        builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
                    builder.ins().return_(&[value]);
                    terminated = true;
                }

                MirInstruction::Import { .. } => {
                    // No-op: imports are resolved at link time
                }

                MirInstruction::RuntimeEval(dst, text) => {
                    let result = emit_eval_stmt(
                        module,
                        func_declarations,
                        data_cache,
                        &mut builder,
                        text.as_str(),
                    )?;
                    vreg_map.insert(*dst, result);
                    default_return = result;
                }
            }
        }

        if !terminated {
            builder.ins().return_(&[default_return]);
        }
        builder.finalize();
    }

    module
        .define_function(function_id, &mut context)
        .with_context(|| {
            format!(
                "failed to define typed function '{}'",
                function.name
            )
        })?;
    module.clear_context(&mut context);
    Ok(())
}

fn resolve_vreg(
    vreg_map: &BTreeMap<VReg, Value>,
    vreg: &VReg,
    builder: &mut FunctionBuilder,
) -> Value {
    vreg_map
        .get(vreg)
        .copied()
        .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE))
}

fn ensure_import<M: Module>(
    module: &mut M,
    declarations: &mut BTreeMap<String, FuncId>,
    name: &str,
    signature: &cranelift_codegen::ir::Signature,
) -> Result<FuncId> {
    if let Some(id) = declarations.get(name).copied() {
        return Ok(id);
    }
    let id = module
        .declare_function(name, Linkage::Import, signature)
        .with_context(|| format!("failed to declare imported helper '{}'", name))?;
    declarations.insert(name.to_string(), id);
    Ok(id)
}

fn declare_string_data<M: Module>(
    module: &mut M,
    data_cache: &mut BTreeMap<String, DataId>,
    text: &str,
) -> Result<DataId> {
    if let Some(id) = data_cache.get(text).copied() {
        return Ok(id);
    }
    let symbol = format!("__rts_typed_{:016x}", stable_hash(text));
    let id = module
        .declare_data(&symbol, Linkage::Local, false, false)
        .with_context(|| format!("failed to declare typed data symbol '{}'", symbol))?;
    let mut desc = DataDescription::new();
    desc.define(text.as_bytes().to_vec().into_boxed_slice());
    module
        .define_data(id, &desc)
        .with_context(|| format!("failed to define typed data payload for '{}'", symbol))?;
    data_cache.insert(text.to_string(), id);
    Ok(id)
}

fn stable_hash(input: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}

fn emit_eval_expr<M: Module>(
    module: &mut M,
    declarations: &mut BTreeMap<String, FuncId>,
    data_cache: &mut BTreeMap<String, DataId>,
    builder: &mut FunctionBuilder,
    text: &str,
) -> Result<Value> {
    let sig = eval_signature(module);
    let eval_fn = ensure_import(module, declarations, RTS_EVAL_EXPR, &sig)?;
    let data_id = declare_string_data(module, data_cache, text)?;
    let data_ref = module.declare_data_in_func(data_id, builder.func);
    let ptr = builder.ins().symbol_value(types::I64, data_ref);
    let len = builder.ins().iconst(types::I64, text.len() as i64);
    let local = module.declare_func_in_func(eval_fn, builder.func);
    let call = builder.ins().call(local, &[ptr, len]);
    Ok(builder.inst_results(call)[0])
}

fn emit_eval_stmt<M: Module>(
    module: &mut M,
    declarations: &mut BTreeMap<String, FuncId>,
    data_cache: &mut BTreeMap<String, DataId>,
    builder: &mut FunctionBuilder,
    text: &str,
) -> Result<Value> {
    let sig = eval_signature(module);
    let eval_fn = ensure_import(module, declarations, RTS_EVAL_STMT, &sig)?;
    let data_id = declare_string_data(module, data_cache, text)?;
    let data_ref = module.declare_data_in_func(data_id, builder.func);
    let ptr = builder.ins().symbol_value(types::I64, data_ref);
    let len = builder.ins().iconst(types::I64, text.len() as i64);
    let local = module.declare_func_in_func(eval_fn, builder.func);
    let call = builder.ins().call(local, &[ptr, len]);
    Ok(builder.inst_results(call)[0])
}

fn binop_to_tag(op: &MirBinOp) -> i64 {
    match op {
        MirBinOp::Add => 0,
        MirBinOp::Sub => 1,
        MirBinOp::Mul => 2,
        MirBinOp::Div => 3,
        MirBinOp::Mod => 4,
        MirBinOp::Gt => 5,
        MirBinOp::Gte => 6,
        MirBinOp::Lt => 7,
        MirBinOp::Lte => 8,
        MirBinOp::Eq => 9,
        MirBinOp::Ne => 10,
        MirBinOp::LogicAnd => 11,
        MirBinOp::LogicOr => 12,
    }
}
