use std::collections::BTreeMap;

use anyhow::{Context, Result};
use cranelift_codegen::ir::{
    InstBuilder, MemFlags, StackSlot, StackSlotData, StackSlotKind, Value, types,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{DataId, FuncId, Module};

use crate::mir::{MirBinOp, MirInstruction, MirUnaryOp, TypedMirFunction, VReg};
use crate::namespaces::abi::{
    FN_BIND_IDENTIFIER, FN_BINOP, FN_BOX_BOOL, FN_BOX_NATIVE_FN, FN_BOX_NUMBER, FN_CALL_BY_HANDLE,
    FN_COMPACT_EXCLUDING, FN_EVAL_STMT, FN_IS_TRUTHY, FN_LOAD_FIELD, FN_NEW_INSTANCE,
    FN_READ_IDENTIFIER, FN_STORE_FIELD, FN_UNBOX_NUMBER,
};

use super::control_flow::rewrite_loop_control;
use super::helpers::{
    adapt_to_kind, binop_to_tag, box_native_f64, box_native_i32, declare_string_data,
    emit_box_string, emit_call_dispatch, emit_dispatch, emit_shadow_writeback, ensure_handle,
    load_binding_slot, pin_live_handles_for_dynamic_call, resolve_vreg, store_binding_slot,
    unpin_live_handles_after_dynamic_call,
};
use super::shadow::ShadowGlobalPlan;
use super::shadow::analyze_shadow_globals;
use super::signatures::function_signature;
use super::types::{
    ABI_ARG_SLOTS, ABI_PARAM_COUNT, ABI_UNDEFINED_HANDLE, BindingState, CALLEE_FN_IDS, VRegKind,
};
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
        let mut param_handle_entries: Vec<(usize, Value)> = Vec::new();
        for index in 0..function.param_count {
            let kind = function
                .param_kinds
                .get(index)
                .copied()
                .unwrap_or(crate::mir::NumericKind::Any);
            if kind != crate::mir::NumericKind::Any {
                continue;
            }
            if let Some(value) = entry_params.get(index + 1).copied() {
                param_handle_entries.push((index, value));
            }
        }
        let mut vreg_map = BTreeMap::<VReg, Value>::new();
        let mut vreg_kinds = BTreeMap::<VReg, VRegKind>::new();
        let mut const_string_vregs = BTreeMap::<VReg, String>::new();
        let mut local_bindings = BTreeMap::<String, BindingState>::new();
        let use_local_bindings = function.name != "main";

        let raw_instructions: Vec<MirInstruction> = function
            .blocks
            .iter()
            .flat_map(|block| block.instructions.iter().cloned())
            .collect();
        let instructions = rewrite_loop_control(&raw_instructions);

        let mut label_blocks = BTreeMap::<String, cranelift_codegen::ir::Block>::new();
        for instruction in &instructions {
            if let MirInstruction::Label(name) = instruction {
                if !label_blocks.contains_key(name.as_str()) {
                    let block = builder.create_block();
                    label_blocks.insert(name.clone(), block);
                }
            }
        }

        let exit_block = builder.create_block();

        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        let mut handle_param_slots: Vec<StackSlot> = Vec::new();
        for (_index, value) in &param_handle_entries {
            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            store_binding_slot(&mut builder, slot, *value);
            handle_param_slots.push(slot);
        }

        if function.name == "main" {
            let user_fn_names: Vec<String> = func_declarations
                .keys()
                .filter(|n| !n.starts_with("__"))
                .cloned()
                .collect();
            for fn_name in user_fn_names {
                let name_data_id = declare_string_data(module, data_cache, fn_name.as_str())?;
                let name_data_ref = module.declare_data_in_func(name_data_id, builder.func);
                let name_ptr = builder.ins().symbol_value(types::I64, name_data_ref);
                let name_len = builder.ins().iconst(types::I64, fn_name.len() as i64);
                let not_mutable = builder.ins().iconst(types::I64, 0);
                let fn_handle = emit_dispatch(
                    module,
                    func_declarations,
                    &mut builder,
                    FN_BOX_NATIVE_FN,
                    &[name_ptr, name_len],
                )?;
                emit_dispatch(
                    module,
                    func_declarations,
                    &mut builder,
                    FN_BIND_IDENTIFIER,
                    &[name_ptr, name_len, fn_handle, not_mutable],
                )?;
            }
        }

        let shadow_plan = if use_local_bindings {
            analyze_shadow_globals(&instructions, function.name.as_str())
        } else {
            ShadowGlobalPlan::default()
        };
        for name in &shadow_plan.names {
            let data_id = declare_string_data(module, data_cache, name.as_str())?;
            let data_ref = module.declare_data_in_func(data_id, builder.func);
            let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
            let name_len = builder.ins().iconst(types::I64, name.len() as i64);
            let handle = emit_dispatch(
                module,
                func_declarations,
                &mut builder,
                FN_READ_IDENTIFIER,
                &[name_ptr, name_len],
            )?;
            let bits = emit_dispatch(
                module,
                func_declarations,
                &mut builder,
                FN_UNBOX_NUMBER,
                &[handle],
            )?;
            let slot = builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                8,
                3,
            ));
            store_binding_slot(&mut builder, slot, bits);
            local_bindings.insert(
                name.clone(),
                BindingState {
                    slot,
                    mutable: true,
                    kind: VRegKind::NativeF64,
                },
            );
        }

        let mut default_return = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
        let mut default_return_is_native = false;
        let mut terminated = false;

        for instruction in &instructions {
            if terminated {
                if let MirInstruction::Label(name) = instruction {
                    if let Some(&target_block) = label_blocks.get(name.as_str()) {
                        builder.switch_to_block(target_block);
                        default_return = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
                        default_return_is_native = false;
                        terminated = false;
                    }
                }
                if terminated {
                    continue;
                }
                if matches!(instruction, MirInstruction::Label(_)) {
                    continue;
                }
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
                    let result = emit_box_string(
                        module,
                        func_declarations,
                        data_cache,
                        &mut builder,
                        s.as_str(),
                    )?;
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::Handle);
                    const_string_vregs.insert(*dst, s.clone());
                    default_return = result;
                    default_return_is_native = false;
                }

                MirInstruction::ConstBool(dst, b) => {
                    let flag = builder.ins().iconst(types::I64, if *b { 1 } else { 0 });
                    let result = emit_dispatch(
                        module,
                        func_declarations,
                        &mut builder,
                        FN_BOX_BOOL,
                        &[flag],
                    )?;
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::Handle);
                    default_return = result;
                    default_return_is_native = false;
                }

                MirInstruction::ConstNull(dst) => {
                    let result = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
                    vreg_map.insert(*dst, result);
                    default_return = result;
                    default_return_is_native = false;
                }

                MirInstruction::ConstUndef(dst) => {
                    let result = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
                    vreg_map.insert(*dst, result);
                }

                MirInstruction::LoadParam(dst, index) => {
                    let handle = entry_params
                        .get(index + 1)
                        .copied()
                        .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE));
                    let kind = function
                        .param_kinds
                        .get(*index)
                        .copied()
                        .unwrap_or(crate::mir::NumericKind::Any);
                    match kind {
                        crate::mir::NumericKind::Float64 => {
                            let bits = emit_dispatch(
                                module,
                                func_declarations,
                                &mut builder,
                                FN_UNBOX_NUMBER,
                                &[handle],
                            )?;
                            vreg_map.insert(*dst, bits);
                            vreg_kinds.insert(*dst, VRegKind::NativeF64);
                        }
                        crate::mir::NumericKind::Int32 => {
                            // Unbox como f64 bits, converte para i32 nativo
                            // (fcvt_to_sint clampa/arredonda out-of-range).
                            // Resultado fica em vreg como i64 com sign-extend
                            // de i32, casando com VRegKind::NativeI32.
                            let bits = emit_dispatch(
                                module,
                                func_declarations,
                                &mut builder,
                                FN_UNBOX_NUMBER,
                                &[handle],
                            )?;
                            let f64_val = builder.ins().bitcast(
                                types::F64,
                                cranelift_codegen::ir::MemFlags::new(),
                                bits,
                            );
                            let i32_val = builder.ins().fcvt_to_sint(types::I32, f64_val);
                            let sext = builder.ins().sextend(types::I64, i32_val);
                            vreg_map.insert(*dst, sext);
                            vreg_kinds.insert(*dst, VRegKind::NativeI32);
                        }
                        crate::mir::NumericKind::Any => {
                            vreg_map.insert(*dst, handle);
                        }
                    }
                }

                MirInstruction::BinOp(dst, op, lhs, rhs) => {
                    let mut lhs_kind = vreg_kinds.get(lhs).copied().unwrap_or(VRegKind::Handle);
                    let mut rhs_kind = vreg_kinds.get(rhs).copied().unwrap_or(VRegKind::Handle);
                    let is_arith = matches!(
                        op,
                        MirBinOp::Add
                            | MirBinOp::Sub
                            | MirBinOp::Mul
                            | MirBinOp::Div
                            | MirBinOp::Mod
                    );
                    let is_cmp = matches!(
                        op,
                        MirBinOp::Lt
                            | MirBinOp::Lte
                            | MirBinOp::Gt
                            | MirBinOp::Gte
                            | MirBinOp::Eq
                            | MirBinOp::Ne
                    );

                    let mut lhs_val = resolve_vreg(&vreg_map, lhs, &mut builder);
                    let mut rhs_val = resolve_vreg(&vreg_map, rhs, &mut builder);
                    let has_handle_operand =
                        lhs_kind == VRegKind::Handle || rhs_kind == VRegKind::Handle;
                    let skip_numeric_promotion = matches!(op, MirBinOp::Add) && has_handle_operand;
                    if (is_arith || is_cmp) && lhs_kind != rhs_kind && !skip_numeric_promotion {
                        let target = match (lhs_kind, rhs_kind) {
                            (VRegKind::NativeI32, VRegKind::NativeF64)
                            | (VRegKind::NativeF64, VRegKind::NativeI32)
                            | (VRegKind::Handle, VRegKind::NativeF64)
                            | (VRegKind::NativeF64, VRegKind::Handle)
                            | (VRegKind::Handle, VRegKind::NativeI32)
                            | (VRegKind::NativeI32, VRegKind::Handle) => VRegKind::NativeF64,
                            _ => VRegKind::Handle,
                        };
                        if target != VRegKind::Handle {
                            if lhs_kind != target {
                                lhs_val = adapt_to_kind(
                                    module,
                                    func_declarations,
                                    &mut builder,
                                    lhs_val,
                                    lhs_kind,
                                    target,
                                )?;
                                lhs_kind = target;
                            }
                            if rhs_kind != target {
                                rhs_val = adapt_to_kind(
                                    module,
                                    func_declarations,
                                    &mut builder,
                                    rhs_val,
                                    rhs_kind,
                                    target,
                                )?;
                                rhs_kind = target;
                            }
                        }
                    }

                    if lhs_kind == VRegKind::NativeI32 && rhs_kind == VRegKind::NativeI32 && is_cmp
                    {
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
                    } else if lhs_kind == VRegKind::NativeF64
                        && rhs_kind == VRegKind::NativeF64
                        && is_cmp
                    {
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
                    } else if lhs_kind == VRegKind::NativeI32
                        && rhs_kind == VRegKind::NativeI32
                        && is_arith
                    {
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
                    } else if lhs_kind == VRegKind::NativeF64
                        && rhs_kind == VRegKind::NativeF64
                        && is_arith
                    {
                        let lhs_f64 = builder.ins().bitcast(types::F64, MemFlags::new(), lhs_val);
                        let rhs_f64 = builder.ins().bitcast(types::F64, MemFlags::new(), rhs_val);
                        let result_f64 = match op {
                            MirBinOp::Add => builder.ins().fadd(lhs_f64, rhs_f64),
                            MirBinOp::Sub => builder.ins().fsub(lhs_f64, rhs_f64),
                            MirBinOp::Mul => builder.ins().fmul(lhs_f64, rhs_f64),
                            MirBinOp::Div => builder.ins().fdiv(lhs_f64, rhs_f64),
                            MirBinOp::Mod => {
                                let div = builder.ins().fdiv(lhs_f64, rhs_f64);
                                let truncated = builder.ins().trunc(div);
                                let product = builder.ins().fmul(truncated, rhs_f64);
                                builder.ins().fsub(lhs_f64, product)
                            }
                            _ => unreachable!(),
                        };
                        let result = builder
                            .ins()
                            .bitcast(types::I64, MemFlags::new(), result_f64);
                        vreg_map.insert(*dst, result);
                        vreg_kinds.insert(*dst, VRegKind::NativeF64);
                        default_return = result;
                        default_return_is_native = true;
                    } else {
                        let lhs_val = ensure_handle(
                            &vreg_map,
                            &vreg_kinds,
                            lhs,
                            module,
                            func_declarations,
                            &mut builder,
                        )?;
                        let rhs_val = ensure_handle(
                            &vreg_map,
                            &vreg_kinds,
                            rhs,
                            module,
                            func_declarations,
                            &mut builder,
                        )?;
                        let op_tag = binop_to_tag(op);
                        let op_val = builder.ins().iconst(types::I64, op_tag);
                        let result = emit_dispatch(
                            module,
                            func_declarations,
                            &mut builder,
                            FN_BINOP,
                            &[op_val, lhs_val, rhs_val],
                        )?;
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
                            let src_i32 = builder.ins().ireduce(types::I32, src_val);
                            let neg = builder.ins().ineg(src_i32);
                            let r = builder.ins().sextend(types::I64, neg);
                            (r, VRegKind::NativeI32)
                        }
                        MirUnaryOp::Negate if src_kind == VRegKind::NativeF64 => {
                            let src_f64 =
                                builder.ins().bitcast(types::F64, MemFlags::new(), src_val);
                            let neg = builder.ins().fneg(src_f64);
                            let r = builder.ins().bitcast(types::I64, MemFlags::new(), neg);
                            (r, VRegKind::NativeF64)
                        }
                        MirUnaryOp::Negate => {
                            let zero_bits = i64::from_ne_bytes(0.0f64.to_ne_bytes());
                            let zero_raw = builder.ins().iconst(types::I64, zero_bits);
                            let zero_handle = emit_dispatch(
                                module,
                                func_declarations,
                                &mut builder,
                                FN_BOX_NUMBER,
                                &[zero_raw],
                            )?;
                            let op_val = builder
                                .ins()
                                .iconst(types::I64, binop_to_tag(&MirBinOp::Sub));
                            let result = emit_dispatch(
                                module,
                                func_declarations,
                                &mut builder,
                                FN_BINOP,
                                &[op_val, zero_handle, src_val],
                            )?;
                            (result, VRegKind::Handle)
                        }
                        MirUnaryOp::Not => {
                            let handle_val = match src_kind {
                                VRegKind::NativeF64 => box_native_f64(
                                    module,
                                    func_declarations,
                                    &mut builder,
                                    src_val,
                                )?,
                                VRegKind::NativeI32 => box_native_i32(
                                    module,
                                    func_declarations,
                                    &mut builder,
                                    src_val,
                                )?,
                                _ => src_val,
                            };
                            let false_i32 = builder.ins().iconst(types::I64, 0);
                            let false_handle =
                                box_native_i32(module, func_declarations, &mut builder, false_i32)?;
                            let op_val = builder
                                .ins()
                                .iconst(types::I64, binop_to_tag(&MirBinOp::Eq));
                            let result = emit_dispatch(
                                module,
                                func_declarations,
                                &mut builder,
                                FN_BINOP,
                                &[op_val, handle_val, false_handle],
                            )?;
                            (result, VRegKind::Handle)
                        }
                        MirUnaryOp::Positive => (src_val, src_kind),
                    };
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, result_kind);
                    default_return = result;
                    default_return_is_native =
                        matches!(result_kind, VRegKind::NativeF64 | VRegKind::NativeI32);
                }

                MirInstruction::Call(dst, callee, args) => {
                    for (vreg, text) in const_string_vregs
                        .iter()
                        .map(|(v, s)| (*v, s.clone()))
                        .collect::<Vec<_>>()
                    {
                        if !vreg_map.contains_key(&vreg) {
                            continue;
                        }
                        let refreshed = emit_box_string(
                            module,
                            func_declarations,
                            data_cache,
                            &mut builder,
                            text.as_str(),
                        )?;
                        vreg_map.insert(vreg, refreshed);
                    }

                    let result = if func_declarations.contains_key(callee.as_str()) {
                        let callee_id = func_declarations[callee.as_str()];
                        let mut call_args = Vec::with_capacity(ABI_PARAM_COUNT);
                        call_args.push(builder.ins().iconst(types::I64, args.len() as i64));
                        for arg in args.iter().take(ABI_ARG_SLOTS) {
                            let val = ensure_handle(
                                &vreg_map,
                                &vreg_kinds,
                                arg,
                                module,
                                func_declarations,
                                &mut builder,
                            )?;
                            call_args.push(val);
                        }
                        while call_args.len() < ABI_PARAM_COUNT {
                            call_args.push(builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE));
                        }
                        let local = module.declare_func_in_func(callee_id, builder.func);
                        let call = builder.ins().call(local, &call_args);
                        builder.inst_results(call)[0]
                    } else if let Some(&(_, fn_id_val)) = CALLEE_FN_IDS
                        .iter()
                        .find(|(name, _)| *name == callee.as_str())
                    {
                        let mut handle_args: Vec<Value> = Vec::with_capacity(args.len());
                        for arg in args.iter().take(6) {
                            let val = ensure_handle(
                                &vreg_map,
                                &vreg_kinds,
                                arg,
                                module,
                                func_declarations,
                                &mut builder,
                            )?;
                            handle_args.push(val);
                        }
                        emit_dispatch(
                            module,
                            func_declarations,
                            &mut builder,
                            fn_id_val,
                            &handle_args,
                        )?
                    } else if crate::namespaces::is_catalog_callee(callee.as_str()) {
                        let mut handle_args: Vec<Value> = Vec::with_capacity(args.len());
                        for arg in args.iter().take(6) {
                            let val = ensure_handle(
                                &vreg_map,
                                &vreg_kinds,
                                arg,
                                module,
                                func_declarations,
                                &mut builder,
                            )?;
                            handle_args.push(val);
                        }

                        let pinned_values = pin_live_handles_for_dynamic_call(
                            module,
                            func_declarations,
                            &mut builder,
                            &local_bindings,
                            &handle_args,
                            &[],
                            &handle_param_slots,
                        )?;
                        let result = emit_call_dispatch(
                            module,
                            func_declarations,
                            data_cache,
                            &mut builder,
                            callee.as_str(),
                            &handle_args,
                        )?;
                        unpin_live_handles_after_dynamic_call(
                            module,
                            func_declarations,
                            &mut builder,
                            &pinned_values,
                        )?;
                        result
                    } else {
                        let fn_handle = if use_local_bindings {
                            if let Some(state) = local_bindings.get(callee.as_str()) {
                                builder.ins().stack_load(types::I64, state.slot, 0)
                            } else {
                                let data_id =
                                    declare_string_data(module, data_cache, callee.as_str())?;
                                let data_ref = module.declare_data_in_func(data_id, builder.func);
                                let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
                                let name_len =
                                    builder.ins().iconst(types::I64, callee.len() as i64);
                                emit_dispatch(
                                    module,
                                    func_declarations,
                                    &mut builder,
                                    FN_READ_IDENTIFIER,
                                    &[name_ptr, name_len],
                                )?
                            }
                        } else {
                            let data_id = declare_string_data(module, data_cache, callee.as_str())?;
                            let data_ref = module.declare_data_in_func(data_id, builder.func);
                            let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
                            let name_len = builder.ins().iconst(types::I64, callee.len() as i64);
                            emit_dispatch(
                                module,
                                func_declarations,
                                &mut builder,
                                FN_READ_IDENTIFIER,
                                &[name_ptr, name_len],
                            )?
                        };
                        let argc = builder.ins().iconst(types::I64, args.len() as i64);
                        emit_dispatch(
                            module,
                            func_declarations,
                            &mut builder,
                            FN_CALL_BY_HANDLE,
                            &[fn_handle, argc],
                        )?
                    };
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::Handle);
                    default_return = result;
                    default_return_is_native = false;
                }

                MirInstruction::Bind(name, src, mutable) => {
                    if use_local_bindings {
                        let src_kind = vreg_kinds.get(src).copied().unwrap_or(VRegKind::Handle);
                        let src_val = resolve_vreg(&vreg_map, src, &mut builder);

                        if let Some(state) = local_bindings.get_mut(name) {
                            let adapted = adapt_to_kind(
                                module,
                                func_declarations,
                                &mut builder,
                                src_val,
                                src_kind,
                                state.kind,
                            )?;
                            store_binding_slot(&mut builder, state.slot, adapted);
                            state.mutable = *mutable;
                            continue;
                        }

                        let slot = builder.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            8,
                            3,
                        ));
                        store_binding_slot(&mut builder, slot, src_val);
                        local_bindings.insert(
                            name.clone(),
                            BindingState {
                                slot,
                                mutable: *mutable,
                                kind: src_kind,
                            },
                        );
                        continue;
                    }

                    let value_handle = ensure_handle(
                        &vreg_map,
                        &vreg_kinds,
                        src,
                        module,
                        func_declarations,
                        &mut builder,
                    )?;
                    let data_id = declare_string_data(module, data_cache, name.as_str())?;
                    let data_ref = module.declare_data_in_func(data_id, builder.func);
                    let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
                    let name_len = builder.ins().iconst(types::I64, name.len() as i64);
                    let mutable_flag = builder
                        .ins()
                        .iconst(types::I64, if *mutable { 1 } else { 0 });
                    let result = emit_dispatch(
                        module,
                        func_declarations,
                        &mut builder,
                        FN_BIND_IDENTIFIER,
                        &[name_ptr, name_len, value_handle, mutable_flag],
                    )?;
                    default_return = result;
                    default_return_is_native = false;
                }

                MirInstruction::WriteBind(name, src) => {
                    if use_local_bindings {
                        if let Some(state) = local_bindings.get(name).copied() {
                            if state.mutable {
                                let src_kind =
                                    vreg_kinds.get(src).copied().unwrap_or(VRegKind::Handle);
                                let src_val = resolve_vreg(&vreg_map, src, &mut builder);
                                let adapted = adapt_to_kind(
                                    module,
                                    func_declarations,
                                    &mut builder,
                                    src_val,
                                    src_kind,
                                    state.kind,
                                )?;
                                store_binding_slot(&mut builder, state.slot, adapted);
                                continue;
                            }
                        }
                    }

                    let value_handle = ensure_handle(
                        &vreg_map,
                        &vreg_kinds,
                        src,
                        module,
                        func_declarations,
                        &mut builder,
                    )?;
                    let data_id = declare_string_data(module, data_cache, name.as_str())?;
                    let data_ref = module.declare_data_in_func(data_id, builder.func);
                    let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
                    let name_len = builder.ins().iconst(types::I64, name.len() as i64);
                    let mutable_flag = builder.ins().iconst(types::I64, 1i64);
                    let result = emit_dispatch(
                        module,
                        func_declarations,
                        &mut builder,
                        FN_BIND_IDENTIFIER,
                        &[name_ptr, name_len, value_handle, mutable_flag],
                    )?;
                    default_return = result;
                    default_return_is_native = false;
                }

                MirInstruction::LoadBinding(dst, name) => {
                    if use_local_bindings {
                        if let Some(state) = local_bindings.get(name) {
                            let result = load_binding_slot(&mut builder, state.slot);
                            vreg_map.insert(*dst, result);
                            vreg_kinds.insert(*dst, state.kind);
                            default_return = result;
                            default_return_is_native =
                                matches!(state.kind, VRegKind::NativeF64 | VRegKind::NativeI32);
                            continue;
                        }
                    }

                    let data_id = declare_string_data(module, data_cache, name.as_str())?;
                    let data_ref = module.declare_data_in_func(data_id, builder.func);
                    let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
                    let name_len = builder.ins().iconst(types::I64, name.len() as i64);
                    let result = emit_dispatch(
                        module,
                        func_declarations,
                        &mut builder,
                        FN_READ_IDENTIFIER,
                        &[name_ptr, name_len],
                    )?;
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
                        _ => raw,
                    };
                    emit_shadow_writeback(
                        module,
                        func_declarations,
                        data_cache,
                        &mut builder,
                        &shadow_plan.names,
                        &local_bindings,
                    )?;
                    builder.ins().return_(&[value]);
                    terminated = true;
                }

                MirInstruction::Return(None) => {
                    let value = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
                    emit_shadow_writeback(
                        module,
                        func_declarations,
                        data_cache,
                        &mut builder,
                        &shadow_plan.names,
                        &local_bindings,
                    )?;
                    builder.ins().return_(&[value]);
                    terminated = true;
                }

                MirInstruction::Import { .. } => {}

                MirInstruction::Jump(label) => {
                    if !terminated {
                        if let Some(&target_block) = label_blocks.get(label.as_str()) {
                            // Antes disparava FN_COMPACT_EXCLUDING em cada back-edge de
                            // while/do-while. Em loops aritmeticos apertados isto domina
                            // o tempo de execucao (medido: 98k compactions / 85ms em
                            // bench_simple de 200k iter). GC compaction deve rodar em
                            // pontos de quiescencia (return/scope-exit), nao por iter.
                            builder.ins().jump(target_block, &[]);
                            terminated = true;
                        }
                    }
                }

                MirInstruction::JumpIf(condition, label) => {
                    if let Some(&target_block) = label_blocks.get(label.as_str()) {
                        let cond_val = resolve_vreg(&vreg_map, condition, &mut builder);
                        let cond_kind = vreg_kinds
                            .get(condition)
                            .copied()
                            .unwrap_or(VRegKind::Handle);
                        let bool_val = if cond_kind == VRegKind::Handle {
                            emit_dispatch(
                                module,
                                func_declarations,
                                &mut builder,
                                FN_IS_TRUTHY,
                                &[cond_val],
                            )?
                        } else {
                            cond_val
                        };
                        let zero = builder.ins().iconst(types::I64, 0);
                        let cmp = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::NotEqual,
                            bool_val,
                            zero,
                        );
                        let fallthrough = builder.create_block();
                        builder.ins().brif(cmp, target_block, &[], fallthrough, &[]);
                        builder.switch_to_block(fallthrough);
                        builder.seal_block(fallthrough);
                    }
                }

                MirInstruction::JumpIfNot(condition, label) => {
                    if let Some(&target_block) = label_blocks.get(label.as_str()) {
                        let cond_val = resolve_vreg(&vreg_map, condition, &mut builder);
                        let cond_kind = vreg_kinds
                            .get(condition)
                            .copied()
                            .unwrap_or(VRegKind::Handle);
                        let bool_val = if cond_kind == VRegKind::Handle {
                            emit_dispatch(
                                module,
                                func_declarations,
                                &mut builder,
                                FN_IS_TRUTHY,
                                &[cond_val],
                            )?
                        } else {
                            cond_val
                        };
                        let zero = builder.ins().iconst(types::I64, 0);
                        let cmp = builder.ins().icmp(
                            cranelift_codegen::ir::condcodes::IntCC::Equal,
                            bool_val,
                            zero,
                        );
                        let fallthrough = builder.create_block();
                        builder.ins().brif(cmp, target_block, &[], fallthrough, &[]);
                        builder.switch_to_block(fallthrough);
                        builder.seal_block(fallthrough);
                    }
                }

                MirInstruction::Label(label) => {
                    if let Some(&target_block) = label_blocks.get(label.as_str()) {
                        builder.ins().jump(target_block, &[]);
                        builder.switch_to_block(target_block);
                        default_return = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
                        default_return_is_native = false;
                    }
                }

                MirInstruction::Break => {
                    builder.ins().jump(exit_block, &[]);
                    terminated = true;
                }

                MirInstruction::Continue => {}

                MirInstruction::RuntimeEval(dst, text) => {
                    let data_id = declare_string_data(module, data_cache, text.as_str())?;
                    let data_ref = module.declare_data_in_func(data_id, builder.func);
                    let text_ptr = builder.ins().symbol_value(types::I64, data_ref);
                    let text_len = builder.ins().iconst(types::I64, text.len() as i64);
                    let result = emit_dispatch(
                        module,
                        func_declarations,
                        &mut builder,
                        FN_EVAL_STMT,
                        &[text_ptr, text_len],
                    )?;
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::Handle);
                    default_return = result;
                    default_return_is_native = false;
                }
                MirInstruction::NewInstance(dst, class_name) => {
                    let data_id = declare_string_data(module, data_cache, class_name.as_str())?;
                    let data_ref = module.declare_data_in_func(data_id, builder.func);
                    let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
                    let name_len = builder.ins().iconst(types::I64, class_name.len() as i64);
                    let result = emit_dispatch(
                        module,
                        func_declarations,
                        &mut builder,
                        FN_NEW_INSTANCE,
                        &[name_ptr, name_len],
                    )?;
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::Handle);
                    default_return = result;
                    default_return_is_native = false;
                }
                MirInstruction::LoadField(dst, obj_vreg, field_name) => {
                    let obj_value = resolve_vreg(&vreg_map, obj_vreg, &mut builder);
                    let obj_handle = adapt_to_kind(
                        module,
                        func_declarations,
                        &mut builder,
                        obj_value,
                        vreg_kinds
                            .get(obj_vreg)
                            .copied()
                            .unwrap_or(VRegKind::Handle),
                        VRegKind::Handle,
                    )?;
                    let data_id = declare_string_data(module, data_cache, field_name.as_str())?;
                    let data_ref = module.declare_data_in_func(data_id, builder.func);
                    let field_ptr = builder.ins().symbol_value(types::I64, data_ref);
                    let field_len = builder.ins().iconst(types::I64, field_name.len() as i64);
                    let result = emit_dispatch(
                        module,
                        func_declarations,
                        &mut builder,
                        FN_LOAD_FIELD,
                        &[obj_handle, field_ptr, field_len],
                    )?;
                    vreg_map.insert(*dst, result);
                    vreg_kinds.insert(*dst, VRegKind::Handle);
                    default_return = result;
                    default_return_is_native = false;
                }
                MirInstruction::StoreField(obj_vreg, field_name, value_vreg) => {
                    let obj_value = resolve_vreg(&vreg_map, obj_vreg, &mut builder);
                    let obj_handle = adapt_to_kind(
                        module,
                        func_declarations,
                        &mut builder,
                        obj_value,
                        vreg_kinds
                            .get(obj_vreg)
                            .copied()
                            .unwrap_or(VRegKind::Handle),
                        VRegKind::Handle,
                    )?;
                    let value_raw = resolve_vreg(&vreg_map, value_vreg, &mut builder);
                    let value_handle = adapt_to_kind(
                        module,
                        func_declarations,
                        &mut builder,
                        value_raw,
                        vreg_kinds
                            .get(value_vreg)
                            .copied()
                            .unwrap_or(VRegKind::Handle),
                        VRegKind::Handle,
                    )?;
                    let data_id = declare_string_data(module, data_cache, field_name.as_str())?;
                    let data_ref = module.declare_data_in_func(data_id, builder.func);
                    let field_ptr = builder.ins().symbol_value(types::I64, data_ref);
                    let field_len = builder.ins().iconst(types::I64, field_name.len() as i64);
                    let _ = emit_dispatch(
                        module,
                        func_declarations,
                        &mut builder,
                        FN_STORE_FIELD,
                        &[obj_handle, field_ptr, field_len, value_handle],
                    )?;
                }
            }
        }

        if !terminated {
            let ret_val = if default_return_is_native {
                box_native_f64(module, func_declarations, &mut builder, default_return)?
            } else {
                default_return
            };
            emit_shadow_writeback(
                module,
                func_declarations,
                data_cache,
                &mut builder,
                &shadow_plan.names,
                &local_bindings,
            )?;
            let _ = emit_dispatch(
                module,
                func_declarations,
                &mut builder,
                FN_COMPACT_EXCLUDING,
                &[ret_val],
            )?;
            builder.ins().return_(&[ret_val]);
        }

        builder.switch_to_block(exit_block);
        let exit_ret = builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE);
        emit_shadow_writeback(
            module,
            func_declarations,
            data_cache,
            &mut builder,
            &shadow_plan.names,
            &local_bindings,
        )?;
        let _ = emit_dispatch(
            module,
            func_declarations,
            &mut builder,
            FN_COMPACT_EXCLUDING,
            &[exit_ret],
        )?;
        builder.ins().return_(&[exit_ret]);

        builder.seal_all_blocks();
        builder.finalize();
    }

    module
        .define_function(function_id, &mut context)
        .with_context(|| format!("failed to define typed function '{}'", function.name))?;
    module.clear_context(&mut context);
    Ok(())
}
