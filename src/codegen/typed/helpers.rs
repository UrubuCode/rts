use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use anyhow::{Context, Result};
use cranelift_codegen::ir::{AbiParam, InstBuilder, MemFlags, StackSlot, Value, types};
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{DataDescription, DataId, FuncId, Linkage, Module};

use crate::mir::{MirBinOp, VReg};
use crate::namespaces::abi::{
    FN_BIND_IDENTIFIER, FN_BOX_NUMBER, FN_BOX_STRING, FN_PIN_HANDLE, FN_UNPIN_HANDLE,
};

use super::types::{ABI_UNDEFINED_HANDLE, BindingState, RTS_DISPATCH, VRegKind};
pub(super) fn resolve_vreg(
    vreg_map: &BTreeMap<VReg, Value>,
    vreg: &VReg,
    builder: &mut FunctionBuilder,
) -> Value {
    vreg_map
        .get(vreg)
        .copied()
        .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE))
}

pub(super) fn store_binding_slot(builder: &mut FunctionBuilder, slot: StackSlot, value: Value) {
    let addr = builder.ins().stack_addr(types::I64, slot, 0);
    builder.ins().store(MemFlags::new(), value, addr, 0);
}

pub(super) fn emit_shadow_writeback<M: Module>(
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    data_cache: &mut BTreeMap<String, DataId>,
    builder: &mut FunctionBuilder,
    shadow_names: &[String],
    local_bindings: &BTreeMap<String, BindingState>,
) -> Result<()> {
    for name in shadow_names {
        let Some(state) = local_bindings.get(name) else {
            continue;
        };
        let bits = load_binding_slot(builder, state.slot);
        let handle = emit_dispatch(module, func_declarations, builder, FN_BOX_NUMBER, &[bits])?;
        let data_id = declare_string_data(module, data_cache, name.as_str())?;
        let data_ref = module.declare_data_in_func(data_id, builder.func);
        let name_ptr = builder.ins().symbol_value(types::I64, data_ref);
        let name_len = builder.ins().iconst(types::I64, name.len() as i64);
        let mutable_flag = builder.ins().iconst(types::I64, 1);
        emit_dispatch(
            module,
            func_declarations,
            builder,
            FN_BIND_IDENTIFIER,
            &[name_ptr, name_len, handle, mutable_flag],
        )?;
    }
    Ok(())
}

pub(super) fn load_binding_slot(builder: &mut FunctionBuilder, slot: StackSlot) -> Value {
    let addr = builder.ins().stack_addr(types::I64, slot, 0);
    builder.ins().load(types::I64, MemFlags::new(), addr, 0)
}

pub(super) fn pin_live_handles_for_dynamic_call<M: Module>(
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
    local_bindings: &BTreeMap<String, BindingState>,
    call_args: &[Value],
    extra_handles: &[Value],
    param_handle_slots: &[StackSlot],
) -> Result<Vec<Value>> {
    let mut pinned_values = Vec::new();

    for state in local_bindings.values() {
        if state.kind == VRegKind::Handle {
            let handle = load_binding_slot(builder, state.slot);
            let _ = emit_dispatch(module, func_declarations, builder, FN_PIN_HANDLE, &[handle])?;
            pinned_values.push(handle);
        }
    }

    for &arg in call_args {
        let _ = emit_dispatch(module, func_declarations, builder, FN_PIN_HANDLE, &[arg])?;
        pinned_values.push(arg);
    }

    for &handle in extra_handles {
        let _ = emit_dispatch(module, func_declarations, builder, FN_PIN_HANDLE, &[handle])?;
        pinned_values.push(handle);
    }

    for &slot in param_handle_slots {
        let handle = load_binding_slot(builder, slot);
        let _ = emit_dispatch(module, func_declarations, builder, FN_PIN_HANDLE, &[handle])?;
        pinned_values.push(handle);
    }

    Ok(pinned_values)
}

pub(super) fn unpin_live_handles_after_dynamic_call<M: Module>(
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
    pinned_values: &[Value],
) -> Result<()> {
    for &handle in pinned_values {
        let _ = emit_dispatch(
            module,
            func_declarations,
            builder,
            FN_UNPIN_HANDLE,
            &[handle],
        )?;
    }
    Ok(())
}

pub(super) fn ensure_import<M: Module>(
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

pub(super) fn declare_string_data<M: Module>(
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

pub(super) fn stable_hash(input: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn dispatch_signature<M: Module>(module: &mut M) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    for _ in 0..7 {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

pub(super) fn emit_dispatch<M: Module>(
    module: &mut M,
    declarations: &mut BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
    fn_id: i64,
    args: &[Value],
) -> Result<Value> {
    let sig = dispatch_signature(module);
    let dispatch_fn = ensure_import(module, declarations, RTS_DISPATCH, &sig)?;
    let mut call_args: Vec<Value> = Vec::with_capacity(7);
    call_args.push(builder.ins().iconst(types::I64, fn_id));
    for &arg in args.iter().take(6) {
        call_args.push(arg);
    }
    while call_args.len() < 7 {
        call_args.push(builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE));
    }
    let local = module.declare_func_in_func(dispatch_fn, builder.func);
    let call = builder.ins().call(local, &call_args);
    Ok(builder
        .inst_results(call)
        .first()
        .copied()
        .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)))
}

pub(super) fn call_dispatch_signature<M: Module>(
    module: &mut M,
) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    for _ in 0..9 {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

pub(super) fn emit_call_dispatch<M: Module>(
    module: &mut M,
    declarations: &mut BTreeMap<String, FuncId>,
    data_cache: &mut BTreeMap<String, DataId>,
    builder: &mut FunctionBuilder,
    callee: &str,
    args: &[Value],
) -> Result<Value> {
    use crate::codegen::mir_parse::RTS_CALL_DISPATCH_SYMBOL;
    let sig = call_dispatch_signature(module);
    let fn_id = ensure_import(module, declarations, RTS_CALL_DISPATCH_SYMBOL, &sig)?;

    let data_id = declare_string_data(module, data_cache, callee)?;
    let data_ref = module.declare_data_in_func(data_id, builder.func);
    let ptr = builder.ins().symbol_value(types::I64, data_ref);
    let len = builder.ins().iconst(types::I64, callee.len() as i64);
    let argc = builder.ins().iconst(types::I64, args.len() as i64);

    let mut call_args: Vec<Value> = Vec::with_capacity(9);
    call_args.push(ptr);
    call_args.push(len);
    call_args.push(argc);
    for &arg in args.iter().take(6) {
        call_args.push(arg);
    }
    while call_args.len() < 9 {
        call_args.push(builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE));
    }

    let local = module.declare_func_in_func(fn_id, builder.func);
    let call = builder.ins().call(local, &call_args);
    Ok(builder
        .inst_results(call)
        .first()
        .copied()
        .unwrap_or_else(|| builder.ins().iconst(types::I64, ABI_UNDEFINED_HANDLE)))
}

pub(super) fn emit_box_string<M: Module>(
    module: &mut M,
    declarations: &mut BTreeMap<String, FuncId>,
    data_cache: &mut BTreeMap<String, DataId>,
    builder: &mut FunctionBuilder,
    text: &str,
) -> Result<Value> {
    let data_id = declare_string_data(module, data_cache, text)?;
    let data_ref = module.declare_data_in_func(data_id, builder.func);
    let ptr = builder.ins().symbol_value(types::I64, data_ref);
    let len = builder.ins().iconst(types::I64, text.len() as i64);
    emit_dispatch(module, declarations, builder, FN_BOX_STRING, &[ptr, len])
}

pub(super) fn adapt_to_kind<M: Module>(
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
    value: Value,
    from: VRegKind,
    to: VRegKind,
) -> Result<Value> {
    if from == to {
        return Ok(value);
    }
    match (from, to) {
        (VRegKind::NativeI32, VRegKind::NativeF64) => {
            let i32_val = builder.ins().ireduce(types::I32, value);
            let f64_val = builder.ins().fcvt_from_sint(types::F64, i32_val);
            Ok(builder.ins().bitcast(types::I64, MemFlags::new(), f64_val))
        }
        (VRegKind::NativeF64, VRegKind::NativeI32) => {
            let f64_val = builder.ins().bitcast(types::F64, MemFlags::new(), value);
            let i32_val = builder.ins().fcvt_to_sint(types::I32, f64_val);
            Ok(builder.ins().sextend(types::I64, i32_val))
        }
        (VRegKind::Handle, VRegKind::NativeF64) => {
            let bits = emit_dispatch(
                module,
                func_declarations,
                builder,
                crate::namespaces::abi::FN_UNBOX_NUMBER,
                &[value],
            )?;
            Ok(bits)
        }
        (VRegKind::Handle, VRegKind::NativeI32) => {
            let bits = emit_dispatch(
                module,
                func_declarations,
                builder,
                crate::namespaces::abi::FN_UNBOX_NUMBER,
                &[value],
            )?;
            let f64_val = builder.ins().bitcast(types::F64, MemFlags::new(), bits);
            let i32_val = builder.ins().fcvt_to_sint(types::I32, f64_val);
            Ok(builder.ins().sextend(types::I64, i32_val))
        }
        (VRegKind::NativeF64, VRegKind::Handle) => {
            box_native_f64(module, func_declarations, builder, value)
        }
        (VRegKind::NativeI32, VRegKind::Handle) => {
            box_native_i32(module, func_declarations, builder, value)
        }
        (VRegKind::Handle, VRegKind::Handle)
        | (VRegKind::NativeF64, VRegKind::NativeF64)
        | (VRegKind::NativeI32, VRegKind::NativeI32) => Ok(value),
    }
}

pub(super) fn ensure_handle<M: Module>(
    vreg_map: &BTreeMap<VReg, Value>,
    vreg_kinds: &BTreeMap<VReg, VRegKind>,
    vreg: &VReg,
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
) -> Result<Value> {
    let val = resolve_vreg(vreg_map, vreg, builder);
    match vreg_kinds.get(vreg) {
        Some(&VRegKind::NativeF64) => box_native_f64(module, func_declarations, builder, val),
        Some(&VRegKind::NativeI32) => box_native_i32(module, func_declarations, builder, val),
        _ => Ok(val),
    }
}

pub(super) fn box_native_f64<M: Module>(
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
    bits: Value,
) -> Result<Value> {
    emit_dispatch(module, func_declarations, builder, FN_BOX_NUMBER, &[bits])
}

pub(super) fn box_native_i32<M: Module>(
    module: &mut M,
    func_declarations: &mut BTreeMap<String, FuncId>,
    builder: &mut FunctionBuilder,
    i32_val: Value,
) -> Result<Value> {
    let i32_reduced = builder.ins().ireduce(types::I32, i32_val);
    let f64_val = builder.ins().fcvt_from_sint(types::F64, i32_reduced);
    let f64_bits = builder.ins().bitcast(types::I64, MemFlags::new(), f64_val);
    emit_dispatch(
        module,
        func_declarations,
        builder,
        FN_BOX_NUMBER,
        &[f64_bits],
    )
}

pub(super) fn binop_to_tag(op: &MirBinOp) -> i64 {
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
