//! `ArrayBuffer` e `DataView` globais.
//!
//! Backing: `Entry::Buffer(Vec<u8>)` via o namespace `buffer`. `ArrayBuffer(n)`
//! aloca um buffer zerado; `DataView(buf)` eh uma view sobre esse handle
//! (byteOffset = 0). Os getters/setters de DataView seguem a spec JS
//! (big-endian por padrao). As impls extern "C" vivem em
//! `crate::namespaces::buffer` (`__RTS_FN_GL_DATAVIEW_*` / `__RTS_FN_GL_ARRAY_BUFFER_*`).
//!
//! Migrado do `#[rts_class]` (macro) pro modelo builder hand-written do
//! `rts-engine` (rumo à remoção da `rts-macro`). Padrao "all-external": cada
//! membro referencia o extern ja existente em `buffer`; nenhum extern eh
//! reemitido aqui (fn_ptr null, sem `#[no_mangle]`). Os overloads JS
//! (`getUint16` big-endian vs `_LE`) viram membros distintos com o mesmo
//! `name = "..."`, resolvidos por aridade no dispatch de codegen.

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Endereço real do extern `buffer` que implementa `symbol`. Os membros
/// ArrayBuffer/DataView são "external" (o extern vive em `crate::namespaces::buffer`),
/// mas o motor NOVO instala os símbolos JIT pelo HARVEST do Registry
/// (`all_jit_symbols`), que só colhe `fn_ptr` NÃO-null. Sem este `fn_ptr` real o
/// `call __RTS_FN_GL_ARRAY_BUFFER_NEW` emitido fica sem endereço → "can't resolve
/// symbol" no JIT. Mapa símbolo→fn num único lugar (o que o `#[rts_class]` fazia).
fn fp_for(symbol: &str) -> *const u8 {
    use crate::namespaces::buffer as b;
    match symbol {
        "__RTS_FN_GL_ARRAY_BUFFER_NEW" => b::__RTS_FN_GL_ARRAY_BUFFER_NEW as *const u8,
        "__RTS_FN_GL_ARRAY_BUFFER_SLICE" => b::__RTS_FN_GL_ARRAY_BUFFER_SLICE as *const u8,
        "__RTS_FN_GL_DATAVIEW_NEW" => b::__RTS_FN_GL_DATAVIEW_NEW as *const u8,
        "__RTS_FN_GL_DATAVIEW_BYTE_LENGTH" => b::__RTS_FN_GL_DATAVIEW_BYTE_LENGTH as *const u8,
        "__RTS_FN_GL_DATAVIEW_BYTE_OFFSET" => b::__RTS_FN_GL_DATAVIEW_BYTE_OFFSET as *const u8,
        "__RTS_FN_GL_DATAVIEW_SET_UINT8" => b::__RTS_FN_GL_DATAVIEW_SET_UINT8 as *const u8,
        "__RTS_FN_GL_DATAVIEW_GET_UINT8" => b::__RTS_FN_GL_DATAVIEW_GET_UINT8 as *const u8,
        "__RTS_FN_GL_DATAVIEW_SET_UINT16" => b::__RTS_FN_GL_DATAVIEW_SET_UINT16 as *const u8,
        "__RTS_FN_GL_DATAVIEW_GET_UINT16" => b::__RTS_FN_GL_DATAVIEW_GET_UINT16 as *const u8,
        "__RTS_FN_GL_DATAVIEW_SET_INT32" => b::__RTS_FN_GL_DATAVIEW_SET_INT32 as *const u8,
        "__RTS_FN_GL_DATAVIEW_GET_INT32" => b::__RTS_FN_GL_DATAVIEW_GET_INT32 as *const u8,
        "__RTS_FN_GL_DATAVIEW_SET_UINT16_LE" => b::__RTS_FN_GL_DATAVIEW_SET_UINT16_LE as *const u8,
        "__RTS_FN_GL_DATAVIEW_GET_UINT16_LE" => b::__RTS_FN_GL_DATAVIEW_GET_UINT16_LE as *const u8,
        "__RTS_FN_GL_DATAVIEW_SET_INT32_LE" => b::__RTS_FN_GL_DATAVIEW_SET_INT32_LE as *const u8,
        "__RTS_FN_GL_DATAVIEW_GET_INT32_LE" => b::__RTS_FN_GL_DATAVIEW_GET_INT32_LE as *const u8,
        "__RTS_FN_GL_DATAVIEW_SET_FLOAT64" => b::__RTS_FN_GL_DATAVIEW_SET_FLOAT64 as *const u8,
        "__RTS_FN_GL_DATAVIEW_GET_FLOAT64" => b::__RTS_FN_GL_DATAVIEW_GET_FLOAT64 as *const u8,
        "__RTS_FN_GL_DATAVIEW_SET_FLOAT32" => b::__RTS_FN_GL_DATAVIEW_SET_FLOAT32 as *const u8,
        "__RTS_FN_GL_DATAVIEW_GET_FLOAT32" => b::__RTS_FN_GL_DATAVIEW_GET_FLOAT32 as *const u8,
        "__RTS_FN_GL_DATAVIEW_SET_BIGINT64" => b::__RTS_FN_GL_DATAVIEW_SET_BIGINT64 as *const u8,
        "__RTS_FN_GL_DATAVIEW_GET_BIGINT64" => b::__RTS_FN_GL_DATAVIEW_GET_BIGINT64 as *const u8,
        "__RTS_FN_GL_DATAVIEW_SET_BIGUINT64" => b::__RTS_FN_GL_DATAVIEW_SET_BIGUINT64 as *const u8,
        "__RTS_FN_GL_DATAVIEW_GET_BIGUINT64" => b::__RTS_FN_GL_DATAVIEW_GET_BIGUINT64 as *const u8,
        _ => core::ptr::null(),
    }
}

/// Membro de classe global (helper hand-written, espelha `leak_class` da macro).
/// `fn_ptr` resolvido por [`fp_for`] (o motor novo instala o símbolo via harvest).
#[allow(clippy::too_many_arguments)]
fn m(
    name: &str,
    kind: MemberKind,
    sig: Sig,
    symbol: &str,
    ts: &str,
    doc: &str,
    pure: bool,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp_for(symbol)),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        intrinsic: None,
    }
}

/// Registra a classe global `ArrayBuffer` no motor (hand-written, sem macro).
/// Todos os membros sao `external` — os externs vivem em `buffer`.
pub fn register_array_buffer_class_spec(e: &mut Engine) {
    e.class("ArrayBuffer")
        .doc("Built-in ArrayBuffer class (raw byte buffer). Todos os membros sao `external` — os externs vivem em `buffer::ops`.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::I64], AbiType::Handle),
            "__RTS_FN_GL_ARRAY_BUFFER_NEW",
            "new ArrayBuffer(byteLength: number): ArrayBuffer",
            "",
            false,
        ))
        .member(m(
            "byteLength",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATAVIEW_BYTE_LENGTH",
            "byteLength: number",
            "",
            true,
        ))
        .member(m(
            "slice",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64, AbiType::I64], AbiType::Handle),
            "__RTS_FN_GL_ARRAY_BUFFER_SLICE",
            "slice(begin?: number, end?: number): ArrayBuffer",
            "",
            true,
        ))
        .done();
}

/// Registra a classe global `DataView` no motor (hand-written, sem macro).
/// Todos os membros sao `external` — os externs vivem em `buffer`.
pub fn register_data_view_class_spec(e: &mut Engine) {
    e.class("DataView")
        .doc("Built-in DataView class (big-endian accessors over an ArrayBuffer). Todos os membros sao `external` — os externs vivem em `buffer::ops`.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_DATAVIEW_NEW",
            "new DataView(buffer: ArrayBuffer): DataView",
            "",
            false,
        ))
        .member(m(
            "setUint8",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64, AbiType::I64], AbiType::Void),
            "__RTS_FN_GL_DATAVIEW_SET_UINT8",
            "setUint8(byteOffset: number, value: number): void",
            "",
            false,
        ))
        .member(m(
            "getUint8",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_DATAVIEW_GET_UINT8",
            "getUint8(byteOffset: number): number",
            "",
            true,
        ))
        .member(m(
            "setUint16",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64, AbiType::I64], AbiType::Void),
            "__RTS_FN_GL_DATAVIEW_SET_UINT16",
            "setUint16(byteOffset: number, value: number): void",
            "",
            false,
        ))
        .member(m(
            "getUint16",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_DATAVIEW_GET_UINT16",
            "getUint16(byteOffset: number): number",
            "",
            true,
        ))
        .member(m(
            "setInt32",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64, AbiType::I64], AbiType::Void),
            "__RTS_FN_GL_DATAVIEW_SET_INT32",
            "setInt32(byteOffset: number, value: number): void",
            "",
            false,
        ))
        .member(m(
            "getInt32",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::I64),
            "__RTS_FN_GL_DATAVIEW_GET_INT32",
            "getInt32(byteOffset: number): number",
            "",
            true,
        ))
        // ── Overloads com `littleEndian` (aridade maior) ──────────────────────
        .member(m(
            "setUint16",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64, AbiType::I64, AbiType::I32], AbiType::Void),
            "__RTS_FN_GL_DATAVIEW_SET_UINT16_LE",
            "setUint16(byteOffset: number, value: number, littleEndian: boolean): void",
            "",
            false,
        ))
        .member(m(
            "getUint16",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64, AbiType::I32], AbiType::I64),
            "__RTS_FN_GL_DATAVIEW_GET_UINT16_LE",
            "getUint16(byteOffset: number, littleEndian: boolean): number",
            "",
            true,
        ))
        .member(m(
            "setInt32",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64, AbiType::I64, AbiType::I32], AbiType::Void),
            "__RTS_FN_GL_DATAVIEW_SET_INT32_LE",
            "setInt32(byteOffset: number, value: number, littleEndian: boolean): void",
            "",
            false,
        ))
        .member(m(
            "getInt32",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64, AbiType::I32], AbiType::I64),
            "__RTS_FN_GL_DATAVIEW_GET_INT32_LE",
            "getInt32(byteOffset: number, littleEndian: boolean): number",
            "",
            true,
        ))
        .member(m(
            "setFloat64",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64, AbiType::F64, AbiType::I32], AbiType::Void),
            "__RTS_FN_GL_DATAVIEW_SET_FLOAT64",
            "setFloat64(byteOffset: number, value: number, littleEndian?: boolean): void",
            "",
            false,
        ))
        .member(m(
            "getFloat64",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64, AbiType::I32], AbiType::F64),
            "__RTS_FN_GL_DATAVIEW_GET_FLOAT64",
            "getFloat64(byteOffset: number, littleEndian?: boolean): number",
            "",
            true,
        ))
        .member(m(
            "setFloat32",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64, AbiType::F64, AbiType::I32], AbiType::Void),
            "__RTS_FN_GL_DATAVIEW_SET_FLOAT32",
            "setFloat32(byteOffset: number, value: number, littleEndian?: boolean): void",
            "",
            false,
        ))
        .member(m(
            "getFloat32",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64, AbiType::I32], AbiType::F64),
            "__RTS_FN_GL_DATAVIEW_GET_FLOAT32",
            "getFloat32(byteOffset: number, littleEndian?: boolean): number",
            "",
            true,
        ))
        .member(m(
            "setBigInt64",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64, AbiType::I64, AbiType::I32], AbiType::Void),
            "__RTS_FN_GL_DATAVIEW_SET_BIGINT64",
            "setBigInt64(byteOffset: number, value: bigint, littleEndian?: boolean): void",
            "",
            false,
        ))
        .member(m(
            "getBigInt64",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64, AbiType::I32], AbiType::I64),
            "__RTS_FN_GL_DATAVIEW_GET_BIGINT64",
            "getBigInt64(byteOffset: number, littleEndian?: boolean): bigint",
            "",
            true,
        ))
        .member(m(
            "setBigUint64",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64, AbiType::I64, AbiType::I32], AbiType::Void),
            "__RTS_FN_GL_DATAVIEW_SET_BIGUINT64",
            "setBigUint64(byteOffset: number, value: bigint, littleEndian?: boolean): void",
            "",
            false,
        ))
        .member(m(
            "getBigUint64",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::I64, AbiType::I32], AbiType::I64),
            "__RTS_FN_GL_DATAVIEW_GET_BIGUINT64",
            "getBigUint64(byteOffset: number, littleEndian?: boolean): bigint",
            "",
            true,
        ))
        .member(m(
            "byteLength",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATAVIEW_BYTE_LENGTH",
            "byteLength: number",
            "",
            true,
        ))
        .member(m(
            "byteOffset",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::I64),
            "__RTS_FN_GL_DATAVIEW_BYTE_OFFSET",
            "byteOffset: number",
            "",
            true,
        ))
        .done();
}
