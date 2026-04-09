use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use anyhow::{Context, Result};
use cranelift_codegen::ir::{AbiParam, InstBuilder, MemFlags, types, Value};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module};

/// Tracks whether a VReg holds a native value or an opaque handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VRegKind {
    Handle,     // i64 handle to ValueStore
    NativeF64,  // raw f64 bits stored as i64
    NativeI32,  // raw i32 value stored as i64
}

use crate::mir::{MirBinOp, MirInstruction, MirUnaryOp, SimdOp, SimdWidth, TypedMirFunction, VReg};

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
const RTS_IS_TRUTHY: &str = "__rts_is_truthy";

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

fn truthy_signature<M: Module>(module: &mut M) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64)); // handle
    sig.returns.push(AbiParam::new(types::I64)); // 0 or 1
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

        let entry_params = builder.block_params(entry_block).to_vec();
        let mut vreg_map = BTreeMap::<VReg, Value>::new();
        let mut vreg_kinds = BTreeMap::<VReg, VRegKind>::new();

        let instructions: Vec<_> = function
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter())
            .collect();

        // --- Pass 1: Create Cranelift blocks for all labels ---
        let mut label_blocks = BTreeMap::<String, cranelift_codegen::ir::Block>::new();
        for instruction in &instructions {
            if let MirInstruction::Label(name) = instruction {
                if !label_blocks.contains_key(name.as_str()) {
                    let block = builder.create_block();
                    label_blocks.insert(name.clone(), block);
                }
            }
        }

        // Also create a dedicated exit block for break statements
        let exit_block = builder.create_block();

        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        let mut default_return = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
        let mut default_return_is_native = false;
        let mut terminated = false;

        // --- Pass 2: Emit instructions with real control flow ---
        for instruction in &instructions {
            if terminated {
                // If we hit a label after termination, switch to that block
                if let MirInstruction::Label(name) = instruction {
                    if let Some(&target_block) = label_blocks.get(name.as_str()) {
                        builder.switch_to_block(target_block);
                        builder.seal_block(target_block);
                        terminated = false;
                    }
                }
                if terminated { continue; }
                // Re-check current instruction (Label was handled above, skip it)
                if matches!(instruction, MirInstruction::Label(_)) { continue; }
            }

            match instruction {
                MirInstruction::ConstNumber(dst, val) => {
                    let bits = i64::from_ne_bytes(val.to_ne_bytes());
                    let result = builder.ins().iconst(types::I64, bits);
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::NativeF64);
                    default_return = result;
                    default_return_is_native = true;
                }

                MirInstruction::ConstInt32(dst, val) => {
                    let result = builder.ins().iconst(types::I64, *val as i64);
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::NativeI32);
                    default_return = result;
                    default_return_is_native = true;
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
                    default_return_is_native = false;
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
                    default_return_is_native = false;
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
                    default_return_is_native = false;
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
                    let lhs_kind = vreg_kinds.get(lhs).copied().unwrap_or(VRegKind::Handle);
                    let rhs_kind = vreg_kinds.get(rhs).copied().unwrap_or(VRegKind::Handle);
                    let is_arith = matches!(op, MirBinOp::Add | MirBinOp::Sub | MirBinOp::Mul | MirBinOp::Div | MirBinOp::Mod);
                    let is_cmp = matches!(op, MirBinOp::Lt | MirBinOp::Lte | MirBinOp::Gt | MirBinOp::Gte | MirBinOp::Eq | MirBinOp::Ne);

                    if lhs_kind == VRegKind::NativeI32 && rhs_kind == VRegKind::NativeI32 && is_cmp {
                        // Native i32 comparison — returns 0 or 1 as i64
                        let lhs_val = resolve_vreg(&vreg_map, lhs, &mut builder);
                        let rhs_val = resolve_vreg(&vreg_map, rhs, &mut builder);
                        let lhs_i32 = builder.ins().ireduce(types::I32, lhs_val);
                        let rhs_i32 = builder.ins().ireduce(types::I32, rhs_val);
                        use cranelift_codegen::ir::condcodes::IntCC;
                        let cc = match op {
                            MirBinOp::Lt => IntCC::SignedLessThan,
                            MirBinOp::Lte => IntCC::SignedLessThanOrEqual,
                            MirBinOp::Gt => IntCC::SignedGreaterThan,
                            MirBinOp::Gte => IntCC::SignedGreaterThanOrEqual,
                            MirBinOp::Eq => IntCC::Equal,
                            MirBinOp::Ne => IntCC::NotEqual,
                            _ => unreachable!(),
                        };
                        let cmp = builder.ins().icmp(cc, lhs_i32, rhs_i32);
                        let result = builder.ins().uextend(types::I64, cmp);
                        vreg_map.insert(*dst, result);
                        vreg_kinds.insert(*dst, VRegKind::NativeI32);
                        default_return = result;
                        default_return_is_native = true;
                    } else if lhs_kind == VRegKind::NativeF64 && rhs_kind == VRegKind::NativeF64 && is_cmp {
                        // Native f64 comparison — returns 0 or 1 as i64
                        let lhs_val = resolve_vreg(&vreg_map, lhs, &mut builder);
                        let rhs_val = resolve_vreg(&vreg_map, rhs, &mut builder);
                        let lhs_f64 = builder.ins().bitcast(types::F64, MemFlags::new(), lhs_val);
                        let rhs_f64 = builder.ins().bitcast(types::F64, MemFlags::new(), rhs_val);
                        use cranelift_codegen::ir::condcodes::FloatCC;
                        let cc = match op {
                            MirBinOp::Lt => FloatCC::LessThan,
                            MirBinOp::Lte => FloatCC::LessThanOrEqual,
                            MirBinOp::Gt => FloatCC::GreaterThan,
                            MirBinOp::Gte => FloatCC::GreaterThanOrEqual,
                            MirBinOp::Eq => FloatCC::Equal,
                            MirBinOp::Ne => FloatCC::NotEqual,
                            _ => unreachable!(),
                        };
                        let cmp = builder.ins().fcmp(cc, lhs_f64, rhs_f64);
                        let result = builder.ins().uextend(types::I64, cmp);
                        vreg_map.insert(*dst, result);
                        vreg_kinds.insert(*dst, VRegKind::NativeI32);
                        default_return = result;
                        default_return_is_native = true;
                    } else if lhs_kind == VRegKind::NativeI32 && rhs_kind == VRegKind::NativeI32 && is_arith {
                        // Native i32 arithmetic path
                        let lhs_val = resolve_vreg(&vreg_map, lhs, &mut builder);
                        let rhs_val = resolve_vreg(&vreg_map, rhs, &mut builder);
                        let lhs_i32 = builder.ins().ireduce(types::I32, lhs_val);
                        let rhs_i32 = builder.ins().ireduce(types::I32, rhs_val);
                        let result_i32 = match op {
                            MirBinOp::Add => builder.ins().iadd(lhs_i32, rhs_i32),
                            MirBinOp::Sub => builder.ins().isub(lhs_i32, rhs_i32),
                            MirBinOp::Mul => builder.ins().imul(lhs_i32, rhs_i32),
                            MirBinOp::Div => builder.ins().sdiv(lhs_i32, rhs_i32),
                            MirBinOp::Mod => builder.ins().srem(lhs_i32, rhs_i32),
                            _ => unreachable!(),
                        };
                        let result = builder.ins().sextend(types::I64, result_i32);
                        vreg_map.insert(*dst, result);
                        vreg_kinds.insert(*dst, VRegKind::NativeI32);
                        default_return = result;
                        default_return_is_native = true;
                    } else if lhs_kind == VRegKind::NativeF64 && rhs_kind == VRegKind::NativeF64 && is_arith {
                        // Native f64 arithmetic path
                        let lhs_val = resolve_vreg(&vreg_map, lhs, &mut builder);
                        let rhs_val = resolve_vreg(&vreg_map, rhs, &mut builder);
                        let lhs_f64 = builder.ins().bitcast(types::F64, MemFlags::new(), lhs_val);
                        let rhs_f64 = builder.ins().bitcast(types::F64, MemFlags::new(), rhs_val);
                        let result_f64 = match op {
                            MirBinOp::Add => builder.ins().fadd(lhs_f64, rhs_f64),
                            MirBinOp::Sub => builder.ins().fsub(lhs_f64, rhs_f64),
                            MirBinOp::Mul => builder.ins().fmul(lhs_f64, rhs_f64),
                            MirBinOp::Div => builder.ins().fdiv(lhs_f64, rhs_f64),
                            MirBinOp::Mod => {
                                // f64 mod: a - floor(a/b) * b
                                let div = builder.ins().fdiv(lhs_f64, rhs_f64);
                                let floored = builder.ins().floor(div);
                                let product = builder.ins().fmul(floored, rhs_f64);
                                builder.ins().fsub(lhs_f64, product)
                            }
                            _ => unreachable!(),
                        };
                        let result = builder.ins().bitcast(types::I64, MemFlags::new(), result_f64);
                        vreg_map.insert(*dst, result);
                        vreg_kinds.insert(*dst, VRegKind::NativeF64);
                        default_return = result;
                        default_return_is_native = true;
                    } else {
                        // Fallback: box any native f64 operands, then call __rts_binop
                        let lhs_val = ensure_handle(&vreg_map, &vreg_kinds, lhs, module, func_declarations, &mut builder)?;
                        let rhs_val = ensure_handle(&vreg_map, &vreg_kinds, rhs, module, func_declarations, &mut builder)?;
                        let op_tag = binop_to_tag(op);
                        let op_val = builder.ins().iconst(types::I64, op_tag);
                        let sig = binop_signature(module);
                        let binop_fn =
                            ensure_import(module, func_declarations, RTS_BINOP, &sig)?;
                        let local = module.declare_func_in_func(binop_fn, builder.func);
                        let call = builder.ins().call(local, &[op_val, lhs_val, rhs_val]);
                        let result = builder.inst_results(call)[0];
                        vreg_map.insert(*dst, result);
                        vreg_kinds.insert(*dst, VRegKind::Handle);
                        default_return = result;
                        default_return_is_native = false;
                    }
                }

                MirInstruction::UnaryOp(dst, op, src) => {
                    let src_kind = vreg_kinds.get(src).copied().unwrap_or(VRegKind::Handle);
                    let src_val = resolve_vreg(&vreg_map, src, &mut builder);
                    let (result, result_kind) = match op {
                        MirUnaryOp::Negate if src_kind == VRegKind::NativeI32 => {
                            // Native i32 negate
                            let src_i32 = builder.ins().ireduce(types::I32, src_val);
                            let neg = builder.ins().ineg(src_i32);
                            let r = builder.ins().sextend(types::I64, neg);
                            (r, VRegKind::NativeI32)
                        }
                        MirUnaryOp::Negate if src_kind == VRegKind::NativeF64 => {
                            // Native fneg
                            let src_f64 = builder.ins().bitcast(types::F64, MemFlags::new(), src_val);
                            let neg = builder.ins().fneg(src_f64);
                            let r = builder.ins().bitcast(types::I64, MemFlags::new(), neg);
                            (r, VRegKind::NativeF64)
                        }
                        MirUnaryOp::Negate => {
                            // Fallback: -x == 0 - x via runtime
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
                            (builder.inst_results(call)[0], VRegKind::Handle)
                        }
                        MirUnaryOp::Not => {
                            // !x: box native numbers first if needed
                            let handle_val = match src_kind {
                                VRegKind::NativeF64 => {
                                    box_native_f64(module, func_declarations, &mut builder, src_val)?
                                }
                                VRegKind::NativeI32 => {
                                    box_native_i32(module, func_declarations, &mut builder, src_val)?
                                }
                                _ => src_val
                            };
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
                            let call = builder.ins().call(local, &[op_val, handle_val, false_handle]);
                            (builder.inst_results(call)[0], VRegKind::Handle)
                        }
                        MirUnaryOp::Positive => {
                            // +x is identity for numbers
                            (src_val, src_kind)
                        }
                    };
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, result_kind);
                    default_return = result;
                    default_return_is_native = matches!(result_kind, VRegKind::NativeF64 | VRegKind::NativeI32);
                }

                MirInstruction::Call(dst, callee, args) => {
                    let result = if func_declarations.contains_key(callee.as_str()) {
                        // Direct call to a known user function
                        let callee_id = func_declarations[callee.as_str()];
                        let mut call_args = Vec::with_capacity(ABI_PARAM_COUNT);
                        call_args
                            .push(builder.ins().iconst(types::I64, args.len() as i64));
                        for arg in args.iter().take(ABI_ARG_SLOTS) {
                            let val = ensure_handle(&vreg_map, &vreg_kinds, arg, module, func_declarations, &mut builder)?;
                            call_args.push(val);
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
                            let val = ensure_handle(&vreg_map, &vreg_kinds, arg, module, func_declarations, &mut builder)?;
                            dispatch_args.push(val);
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
                    vreg_kinds.insert(*dst, VRegKind::Handle);
                    default_return = result;
                    default_return_is_native = false;
                }

                MirInstruction::Bind(name, src, mutable) => {
                    let value_handle = ensure_handle(&vreg_map, &vreg_kinds, src, module, func_declarations, &mut builder)?;
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
                    default_return_is_native = false;
                }

                MirInstruction::WriteBind(name, src) => {
                    // Write to an existing mutable binding (uses same ABI as Bind with mutable=true)
                    let value_handle = ensure_handle(&vreg_map, &vreg_kinds, src, module, func_declarations, &mut builder)?;
                    let sig = bind_signature(module);
                    let bind_fn =
                        ensure_import(module, func_declarations, RTS_BIND, &sig)?;

                    let data_id =
                        declare_string_data(module, data_cache, name.as_str())?;
                    let data_ref = module.declare_data_in_func(data_id, builder.func);
                    let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
                    let name_len =
                        builder.ins().iconst(types::I64, name.len() as i64);
                    let mutable_flag = builder.ins().iconst(types::I64, 1i64);

                    let local = module.declare_func_in_func(bind_fn, builder.func);
                    let call = builder.ins().call(
                        local,
                        &[name_ptr, name_len, value_handle, mutable_flag],
                    );
                    let result = builder.inst_results(call)[0];
                    default_return = result;
                    default_return_is_native = false;
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
                    vreg_kinds.insert(*dst, VRegKind::Handle);
                    default_return = result;
                    default_return_is_native = false;
                }

                MirInstruction::Return(Some(vreg)) => {
                    let raw = resolve_vreg(&vreg_map, vreg, &mut builder);
                    let value = match vreg_kinds.get(vreg) {
                        Some(&VRegKind::NativeF64) => {
                            box_native_f64(module, func_declarations, &mut builder, raw)?
                        }
                        Some(&VRegKind::NativeI32) => {
                            box_native_i32(module, func_declarations, &mut builder, raw)?
                        }
                        _ => raw
                    };
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

                MirInstruction::Jump(label) => {
                    if let Some(&target_block) = label_blocks.get(label.as_str()) {
                        builder.ins().jump(target_block, &[]);
                        terminated = true;
                    }
                }

                MirInstruction::JumpIf(condition, label) => {
                    if let Some(&target_block) = label_blocks.get(label.as_str()) {
                        let cond_val = resolve_vreg(&vreg_map, condition, &mut builder);
                        let cond_kind = vreg_kinds.get(condition).copied().unwrap_or(VRegKind::Handle);
                        // For handles, call __rts_is_truthy to get 0/1
                        let bool_val = if cond_kind == VRegKind::Handle {
                            let sig = truthy_signature(module);
                            let truthy_fn = ensure_import(module, func_declarations, RTS_IS_TRUTHY, &sig)?;
                            let local = module.declare_func_in_func(truthy_fn, builder.func);
                            let call = builder.ins().call(local, &[cond_val]);
                            builder.inst_results(call)[0]
                        } else {
                            cond_val
                        };
                        let zero = builder.ins().iconst(types::I64, 0);
                        let cmp = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::NotEqual, bool_val, zero);
                        let fallthrough = builder.create_block();
                        builder.ins().brif(cmp, target_block, &[], fallthrough, &[]);
                        builder.switch_to_block(fallthrough);
                        builder.seal_block(fallthrough);
                    }
                }

                MirInstruction::JumpIfNot(condition, label) => {
                    if let Some(&target_block) = label_blocks.get(label.as_str()) {
                        let cond_val = resolve_vreg(&vreg_map, condition, &mut builder);
                        let cond_kind = vreg_kinds.get(condition).copied().unwrap_or(VRegKind::Handle);
                        let bool_val = if cond_kind == VRegKind::Handle {
                            let sig = truthy_signature(module);
                            let truthy_fn = ensure_import(module, func_declarations, RTS_IS_TRUTHY, &sig)?;
                            let local = module.declare_func_in_func(truthy_fn, builder.func);
                            let call = builder.ins().call(local, &[cond_val]);
                            builder.inst_results(call)[0]
                        } else {
                            cond_val
                        };
                        let zero = builder.ins().iconst(types::I64, 0);
                        let cmp = builder.ins().icmp(cranelift_codegen::ir::condcodes::IntCC::Equal, bool_val, zero);
                        let fallthrough = builder.create_block();
                        builder.ins().brif(cmp, target_block, &[], fallthrough, &[]);
                        builder.switch_to_block(fallthrough);
                        builder.seal_block(fallthrough);
                    }
                }

                MirInstruction::Label(label) => {
                    if let Some(&target_block) = label_blocks.get(label.as_str()) {
                        // Fall through from current block to label block
                        builder.ins().jump(target_block, &[]);
                        builder.switch_to_block(target_block);
                        builder.seal_block(target_block);
                    }
                }

                MirInstruction::Break => {
                    builder.ins().jump(exit_block, &[]);
                    terminated = true;
                }

                MirInstruction::Continue => {
                    // Continue jumps back to the nearest loop header
                    // For now, treat as no-op (requires loop tracking)
                }

                MirInstruction::SimdConst(dst, width, values) => {
                    let vec_type = simd_type(*width);
                    let lane_count = simd_lane_count(*width);
                    // Build the vector by splatting first value, then inserting others
                    let first = values.first().copied().unwrap_or(0.0);
                    let first_f64 = builder.ins().f64const(first);
                    let mut vec_val = builder.ins().splat(vec_type, first_f64);
                    for (i, &v) in values.iter().enumerate().skip(1).take(lane_count - 1) {
                        let lane_val = builder.ins().f64const(v);
                        vec_val = builder.ins().insertlane(vec_val, lane_val, i as u8);
                    }
                    // Store as i64 handle (pointer-width placeholder for the vector SSA value)
                    vreg_map.insert(*dst, vec_val);
                    vreg_kinds.insert(*dst, VRegKind::NativeF64);
                }

                MirInstruction::SimdOp(dst, op, width, lhs, rhs) => {
                    let lhs_val = resolve_vreg(&vreg_map, lhs, &mut builder);
                    let rhs_val = resolve_vreg(&vreg_map, rhs, &mut builder);

                    let result = match op {
                        SimdOp::Add => builder.ins().fadd(lhs_val, rhs_val),
                        SimdOp::Sub => builder.ins().fsub(lhs_val, rhs_val),
                        SimdOp::Mul => builder.ins().fmul(lhs_val, rhs_val),
                        SimdOp::Div => builder.ins().fdiv(lhs_val, rhs_val),
                        SimdOp::Max => builder.ins().fmax(lhs_val, rhs_val),
                        SimdOp::Min => builder.ins().fmin(lhs_val, rhs_val),
                        SimdOp::Sqrt => builder.ins().sqrt(lhs_val),
                        SimdOp::FMA => {
                            let mul = builder.ins().fmul(lhs_val, rhs_val);
                            builder.ins().fadd(mul, lhs_val)
                        }
                    };

                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::NativeF64);
                }

                MirInstruction::SimdLoad(dst, width, base, offset) => {
                    let vec_type = simd_type(*width);
                    let base_val = resolve_vreg(&vreg_map, base, &mut builder);
                    let addr = builder.ins().iadd_imm(base_val, *offset as i64);
                    let loaded = builder.ins().load(vec_type, MemFlags::new(), addr, 0);
                    vreg_map.insert(*dst, loaded);
                    vreg_kinds.insert(*dst, VRegKind::NativeF64);
                }

                MirInstruction::SimdStore(width, vec, base, offset) => {
                    let vec_val = resolve_vreg(&vreg_map, vec, &mut builder);
                    let base_val = resolve_vreg(&vreg_map, base, &mut builder);
                    let addr = builder.ins().iadd_imm(base_val, *offset as i64);
                    builder.ins().store(MemFlags::new(), vec_val, addr, 0);
                }

                MirInstruction::UnrollHint(factor) => {
                    // Add comment about unrolling for debugging
                    // In a full implementation, this would guide the instruction scheduler
                    // For now, we just acknowledge the hint
                }

                MirInstruction::LoopBegin(loop_id) => {
                    // Mark beginning of optimized loop region
                    // Could be used for register allocation hints or branch prediction
                }

                MirInstruction::LoopEnd(loop_id) => {
                    // Mark end of optimized loop region
                }

                MirInstruction::StrengthReduce(dst, op, lhs, rhs) => {
                    let lhs_val = resolve_vreg(&vreg_map, lhs, &mut builder);
                    let rhs_val = resolve_vreg(&vreg_map, rhs, &mut builder);

                    let result = match op {
                        MirBinOp::Mul => {
                            // Try to detect power-of-2 constant for shift optimization
                            if let Some(MirInstruction::ConstInt32(_, val)) = instructions.iter().find(|i| {
                                matches!(i, MirInstruction::ConstInt32(r, _) if *r == *rhs)
                            }) {
                                let v = *val as u64;
                                if v.is_power_of_two() && v > 0 {
                                    let shift = v.trailing_zeros() as i64;
                                    let shift_val = builder.ins().iconst(types::I64, shift);
                                    builder.ins().ishl(lhs_val, shift_val)
                                } else {
                                    builder.ins().imul(lhs_val, rhs_val)
                                }
                            } else {
                                builder.ins().imul(lhs_val, rhs_val)
                            }
                        }
                        MirBinOp::Div => {
                            if let Some(MirInstruction::ConstInt32(_, val)) = instructions.iter().find(|i| {
                                matches!(i, MirInstruction::ConstInt32(r, _) if *r == *rhs)
                            }) {
                                let v = *val as u64;
                                if v.is_power_of_two() && v > 0 {
                                    let shift = v.trailing_zeros() as i64;
                                    let shift_val = builder.ins().iconst(types::I64, shift);
                                    builder.ins().sshr(lhs_val, shift_val)
                                } else {
                                    builder.ins().sdiv(lhs_val, rhs_val)
                                }
                            } else {
                                builder.ins().sdiv(lhs_val, rhs_val)
                            }
                        }
                        MirBinOp::Add => builder.ins().iadd(lhs_val, rhs_val),
                        MirBinOp::Sub => builder.ins().isub(lhs_val, rhs_val),
                        _ => builder.ins().imul(lhs_val, rhs_val), // Fallback
                    };

                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::NativeI32);
                }

                MirInstruction::HoistInvariant(vreg, loop_id) => {
                    // Invariant hoisting hint - in a real implementation, this would
                    // inform the register allocator to keep this value in a register
                    // across loop iterations
                }

                MirInstruction::InlineCandidate(function_name) => {
                    // Mark that the following code was inlined from function_name
                    // This could be used for debugging or profiling information
                }

                MirInstruction::InlineCall(dst, function_name, args) => {
                    // This would contain the actual inlined function body
                    // For now, just emit a placeholder
                    let result = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
                    vreg_map.insert(*dst, result);
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
                    default_return_is_native = false;
                }
            }
        }

        if !terminated {
            let ret_val = if default_return_is_native {
                box_native_f64(module, func_declarations, &mut builder, default_return)?
            } else {
                default_return
            };
            builder.ins().return_(&[ret_val]);
        }

        // Seal and finalize the exit block (used by Break)
        builder.switch_to_block(exit_block);
        builder.seal_block(exit_block);
        let exit_ret = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
        builder.ins().return_(&[exit_ret]);

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

fn ensure_handle<M: Module>(
    vreg_map: &BTreeMap<VReg, Value>,
    vreg_kinds: &BTreeMap<VReg, VRegKind>,
    vreg: &VReg,
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
) -> Result<Value> {
    let val = resolve_vreg(vreg_map, vreg, builder);
    match vreg_kinds.get(vreg) {
        Some(&VRegKind::NativeF64) => {
            box_native_f64(module, func_declarations, builder, val)
        }
        Some(&VRegKind::NativeI32) => {
            box_native_i32(module, func_declarations, builder, val)
        }
        _ => Ok(val)
    }
}

fn box_native_f64<M: Module>(
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
    bits: Value,
) -> Result<Value> {
    let sig = box_number_signature(module);
    let box_fn = ensure_import(module, func_declarations, RTS_BOX_NUMBER, &sig)?;
    let local = module.declare_func_in_func(box_fn, builder.func);
    let call = builder.ins().call(local, &[bits]);
    Ok(builder.inst_results(call)[0])
}

fn box_native_i32<M: Module>(
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
    i32_val: Value,
) -> Result<Value> {
    // Convert i32 to f64, then to f64 bits
    let i32_reduced = builder.ins().ireduce(types::I32, i32_val);
    let f64_val = builder.ins().fcvt_from_sint(types::F64, i32_reduced);
    let f64_bits = builder.ins().bitcast(types::I64, MemFlags::new(), f64_val);

    let sig = box_number_signature(module);
    let box_fn = ensure_import(module, func_declarations, RTS_BOX_NUMBER, &sig)?;
    let local = module.declare_func_in_func(box_fn, builder.func);
    let call = builder.ins().call(local, &[f64_bits]);
    Ok(builder.inst_results(call)[0])
}

fn simd_type(width: SimdWidth) -> cranelift_codegen::ir::Type {
    match width {
        SimdWidth::V128 => types::F64X2,  // 128-bit = 2x f64
        SimdWidth::V256 => types::F64X2,  // Cranelift doesn't support 256-bit natively, fallback to 128
    }
}

fn simd_lane_count(width: SimdWidth) -> usize {
    match width {
        SimdWidth::V128 => 2,  // 2x f64
        SimdWidth::V256 => 2,  // Fallback to 128-bit
    }
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
