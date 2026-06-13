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

/// `__RTS_FN_NS_GC_POLY_TO_HANDLE(payload)` — the 48-bit PolyValue payload
/// (slot+shard) → full real runtime handle, with the 16-bit generation
/// reconstructed from the live slot. `poly_word` is the raw heap-PolyValue word;
/// we mask off the tag/header to isolate the 48-bit payload first.
pub fn emit_table_load(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    poly_word: Value,
) -> Value {
    let mask = builder.ins().iconst(types::I64, PAYLOAD_MASK as i64);
    let payload = builder.ins().band(poly_word, mask);
    emit_call(module, builder, "__RTS_FN_NS_GC_POLY_TO_HANDLE", &[payload])
        .expect("POLY_TO_HANDLE returns a value")
}

/// `__RTS_FN_NS_GC_POLY_FROM_HANDLE(real_handle)` → bare 48-bit slot+shard
/// payload, then box that payload as a string PolyValue. Returns the raw
/// string-PolyValue word.
pub fn emit_box_real_string(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    real_handle: Value,
) -> Value {
    let payload48 = emit_call(module, builder, "__RTS_FN_NS_GC_POLY_FROM_HANDLE", &[real_handle])
        .expect("POLY_FROM_HANDLE returns a value");
    // box: BOX_BASE | (TAG_STR<<48) | (payload & PAYLOAD_MASK).
    let header = super::encode(super::TAG_STR, 0) as i64; // BOX_BASE | TAG_STR<<48
    let mask = builder.ins().iconst(types::I64, PAYLOAD_MASK as i64);
    let payload = builder.ins().band(payload48, mask);
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

/// `__RTS_FN_NS_COLLECTIONS_VEC_NEW()` → a fresh real Vec handle, then box it as
/// a `TAG_OBJECT` PolyValue word (the new engine's object/array representation:
/// an `Entry::Vec` of PolyValue words reached through a NaN-boxed object handle —
/// exactly the strings/handles bridge, with `TAG_OBJECT` instead of `TAG_STR`).
/// Returns the raw object-PolyValue word.
pub fn emit_new_vec_object(module: &mut dyn Module, builder: &mut FunctionBuilder) -> Value {
    let handle = emit_call(module, builder, "__RTS_FN_NS_COLLECTIONS_VEC_NEW", &[])
        .expect("VEC_NEW returns a value");
    box_handle_as(module, builder, handle, super::TAG_OBJECT)
}

/// Box a real runtime handle as a PolyValue of `tag` (`TAG_OBJECT`/`TAG_STR`/…):
/// `POLY_FROM_HANDLE` drops the generation to the bare 48-bit slot+shard, then
/// `BOX_BASE | (tag<<48) | (payload & PAYLOAD_MASK)` assembles the word.
fn box_handle_as(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    real_handle: Value,
    tag: u64,
) -> Value {
    let payload48 = emit_call(module, builder, "__RTS_FN_NS_GC_POLY_FROM_HANDLE", &[real_handle])
        .expect("POLY_FROM_HANDLE returns a value");
    let header = super::encode(tag, 0) as i64; // BOX_BASE | tag<<48
    let mask = builder.ins().iconst(types::I64, PAYLOAD_MASK as i64);
    let payload = builder.ins().band(payload48, mask);
    let header_v = builder.ins().iconst(types::I64, header);
    builder.ins().bor(payload, header_v)
}

/// `VEC_PUSH(realHandleOf(obj_word), value_word)` — append one PolyValue slot
/// word to the inline slot array behind `obj_word`.
pub fn emit_vec_push(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    obj_word: Value,
    value_word: Value,
) {
    let handle = emit_table_load(module, builder, obj_word);
    emit_call(module, builder, "__RTS_FN_NS_COLLECTIONS_VEC_PUSH", &[handle, value_word]);
}

/// `VEC_GET(realHandleOf(obj_word), index)` → the i64 slot word (a PolyValue
/// word, or `0` out of range — the caller maps that to `undefined`).
pub fn emit_vec_get(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    obj_word: Value,
    index: Value,
) -> Value {
    let handle = emit_table_load(module, builder, obj_word);
    emit_call(module, builder, "__RTS_FN_NS_COLLECTIONS_VEC_GET", &[handle, index])
        .expect("VEC_GET returns a value")
}

/// `VEC_SET(realHandleOf(obj_word), index, value_word)` — overwrite one slot.
pub fn emit_vec_set(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    obj_word: Value,
    index: Value,
    value_word: Value,
) {
    let handle = emit_table_load(module, builder, obj_word);
    emit_call(
        module,
        builder,
        "__RTS_FN_NS_COLLECTIONS_VEC_SET",
        &[handle, index, value_word],
    );
}

/// `VEC_LEN(realHandleOf(obj_word))` → element count (i64).
pub fn emit_vec_len(
    module: &mut dyn Module,
    builder: &mut FunctionBuilder,
    obj_word: Value,
) -> Value {
    let handle = emit_table_load(module, builder, obj_word);
    emit_call(module, builder, "__RTS_FN_NS_COLLECTIONS_VEC_LEN", &[handle])
        .expect("VEC_LEN returns a value")
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
