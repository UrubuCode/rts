//! Cranelift IR that marshals at the REAL-symbol call boundaries.
//!
//! The adapter is a TOTAL function of (Repr + tag): every box/unbox/marshal is
//! explicit IR emitted at a proven boundary, never a side-table of meaning. This
//! module is the IR half — it emits the `call`s to the real `__RTS_FN_*` symbols
//! (and the `__rtsadp_*` table trampolines), shuffling representations so each
//! call's Cranelift signature is EXACTLY right (param count, f64 vs i64, the
//! `StrPtr` ptr+len split). It defines no symbol; it only emits `call`s.

use cranelift_codegen::ir::{types, InstBuilder, Value};
use cranelift_module::{Linkage, Module};
use cranelift_frontend::FunctionBuilder;

use super::abi_sig::sig_of;
use super::PAYLOAD_MASK;

/// Declare-import `name` (resolving its real [`super::abi_sig::SymSig`]) and emit
/// the `call` with `args` (already-marshaled Cranelift values, one per Cranelift
/// slot — StrPtr already split into ptr+len). Returns the single result value, or
/// `None` for a void symbol.
///
/// Panics (a codegen bug, not a user error) if `name` is unknown or the arg count
/// does not match the symbol's slot count — the lowering must marshal correctly.
pub fn emit_call(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    name: &str,
    args: &[Value],
) -> Option<Value> {
    let sig = sig_of(name)
        .unwrap_or_else(|| panic!("emit_call: unknown runtime symbol `{name}`"));
    assert_eq!(
        sig.param_slot_count(),
        args.len(),
        "emit_call `{name}`: expected {} slots, got {} marshaled args",
        sig.param_slot_count(),
        args.len()
    );
    let cl_sig = sig.to_cranelift(module);
    let callee = module
        .declare_function(name, Linkage::Import, &cl_sig)
        .unwrap_or_else(|e| panic!("declare runtime symbol `{name}`: {e}"));
    let func_ref = module.declare_func_in_func(callee, builder.func);
    let call = builder.ins().call(func_ref, args);
    if sig.returns() {
        Some(builder.inst_results(call)[0])
    } else {
        None
    }
}

/// `__rtsadp_load(idx)` — table idx (the 48-bit PolyValue payload) → full real
/// runtime handle. `idx_word` is the raw string-PolyValue word; we mask off the
/// tag/header to isolate the 48-bit payload first.
pub fn emit_table_load(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    poly_word: Value,
) -> Value {
    let mask = builder.ins().iconst(types::I64, PAYLOAD_MASK as i64);
    let idx = builder.ins().band(poly_word, mask);
    emit_call(module, builder, "__rtsadp_load", &[idx])
        .expect("__rtsadp_load returns a value")
}

/// `__rtsadp_store(real_handle)` → idx, then box that idx as a string PolyValue.
/// Returns the raw string-PolyValue word.
pub fn emit_box_real_string(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    real_handle: Value,
) -> Value {
    let idx = emit_call(module, builder, "__rtsadp_store", &[real_handle])
        .expect("__rtsadp_store returns a value");
    // box: BOX_BASE | (TAG_STR<<48) | (idx & PAYLOAD_MASK).
    let header = super::encode(super::TAG_STR, 0) as i64; // BOX_BASE | TAG_STR<<48
    let mask = builder.ins().iconst(types::I64, PAYLOAD_MASK as i64);
    let payload = builder.ins().band(idx, mask);
    let header_v = builder.ins().iconst(types::I64, header);
    builder.ins().bor(payload, header_v)
}

/// From a real string handle, emit `STRING_PTR(h)` and `STRING_LEN(h)`, returning
/// `(ptr, len)` as two i64 Cranelift values — the `StrPtr` 2-slot ABI shape.
pub fn emit_string_ptr_len(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    real_handle: Value,
) -> (Value, Value) {
    let ptr = emit_call(module, builder, "__RTS_FN_NS_GC_STRING_PTR", &[real_handle])
        .expect("STRING_PTR returns a value");
    let len = emit_call(module, builder, "__RTS_FN_NS_GC_STRING_LEN", &[real_handle])
        .expect("STRING_LEN returns a value");
    (ptr, len)
}

/// Emit a `console.log`-style line print of one string PolyValue: table-load to
/// the real handle, STRING_PTR + STRING_LEN (the real pool), then
/// `__rtsadp_print_line(ptr, len)` — the marshaling trampoline that forwards to
/// the REAL `__RTS_FN_NS_IO_PRINT(ptr, len)` (newline appended by the runtime),
/// or buffers in capture mode for the in-process tests. The `(ptr, len)` pair is
/// the `StrPtr` 2-slot ABI, computed in IR — the proof that boundary is wired
/// correctly through real codegen.
pub fn emit_print_string_poly(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    string_poly_word: Value,
) {
    let handle = emit_table_load(module, builder, string_poly_word);
    let (ptr, len) = emit_string_ptr_len(module, builder, handle);
    // StrPtr = two slots; pass (ptr, len) in order.
    emit_call(module, builder, "__rtsadp_print_line", &[ptr, len]);
}
