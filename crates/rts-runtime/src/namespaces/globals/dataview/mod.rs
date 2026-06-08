//! `ArrayBuffer` e `DataView` globais.
//!
//! Backing: `Entry::Buffer(Vec<u8>)` via o namespace `buffer`. `ArrayBuffer(n)`
//! aloca um buffer zerado; `DataView(buf)` eh uma view sobre esse handle
//! (byteOffset = 0). Os getters/setters de DataView seguem a spec JS
//! (big-endian por padrao). As impls extern "C" vivem em
//! `crate::namespaces::buffer::ops` (`__RTS_FN_GL_DATAVIEW_*`).
//!
//! Migrado ao modelo `#[rts_class]` (stage 5, `docs/specs/rts-core-engine.md`)
//! no padrao "all-external": cada membro referencia o extern ja existente em
//! `buffer::ops`; o macro deriva apenas os CLASS_SPEC, sem re-emitir externs.
//! Os overloads JS (`getUint16` big-endian vs `_LE`) viram fns Rust distintas
//! com o mesmo `name = "..."`, resolvidas por aridade no dispatch de codegen.

#[allow(unused_imports)]
use rts_abi::ty::{F64, Handle, I32, I64};
use rts_macro::rts_class;

/// Built-in ArrayBuffer class (raw byte buffer). Todos os membros sao
/// `external` — os externs vivem em `buffer::ops`.
#[rts_class(ArrayBuffer, spec = "ARRAY_BUFFER_CLASS_SPEC")]
impl ArrayBufferClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_ARRAY_BUFFER_NEW",
        ts = "new ArrayBuffer(byteLength: number): ArrayBuffer"
    )]
    pub fn new(_byte_length: I64) -> Handle {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "byteLength",
        symbol = "__RTS_FN_GL_DATAVIEW_BYTE_LENGTH",
        ts = "byteLength: number",
        pure
    )]
    pub fn byte_length(_h: Handle) -> I64 {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "slice",
        symbol = "__RTS_FN_GL_ARRAY_BUFFER_SLICE",
        ts = "slice(begin?: number, end?: number): ArrayBuffer",
        pure
    )]
    pub fn slice(_h: Handle, _start: I64, _end: I64) -> Handle {
        unreachable!()
    }
}

/// Built-in DataView class (big-endian accessors over an ArrayBuffer). Todos os
/// membros sao `external` — os externs vivem em `buffer::ops`.
#[rts_class(DataView, spec = "DATA_VIEW_CLASS_SPEC")]
impl DataViewClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_DATAVIEW_NEW",
        ts = "new DataView(buffer: ArrayBuffer): DataView"
    )]
    pub fn new(_buffer: Handle) -> Handle {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "setUint8",
        symbol = "__RTS_FN_GL_DATAVIEW_SET_UINT8",
        ts = "setUint8(byteOffset: number, value: number): void"
    )]
    pub fn set_uint8(_h: Handle, _byte_offset: I64, _value: I64) {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "getUint8",
        symbol = "__RTS_FN_GL_DATAVIEW_GET_UINT8",
        ts = "getUint8(byteOffset: number): number",
        pure
    )]
    pub fn get_uint8(_h: Handle, _byte_offset: I64) -> I64 {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "setUint16",
        symbol = "__RTS_FN_GL_DATAVIEW_SET_UINT16",
        ts = "setUint16(byteOffset: number, value: number): void"
    )]
    pub fn set_uint16(_h: Handle, _byte_offset: I64, _value: I64) {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "getUint16",
        symbol = "__RTS_FN_GL_DATAVIEW_GET_UINT16",
        ts = "getUint16(byteOffset: number): number",
        pure
    )]
    pub fn get_uint16(_h: Handle, _byte_offset: I64) -> I64 {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "setInt32",
        symbol = "__RTS_FN_GL_DATAVIEW_SET_INT32",
        ts = "setInt32(byteOffset: number, value: number): void"
    )]
    pub fn set_int32(_h: Handle, _byte_offset: I64, _value: I64) {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "getInt32",
        symbol = "__RTS_FN_GL_DATAVIEW_GET_INT32",
        ts = "getInt32(byteOffset: number): number",
        pure
    )]
    pub fn get_int32(_h: Handle, _byte_offset: I64) -> I64 {
        unreachable!()
    }

    // ── Overloads com `littleEndian` (aridade maior) ──────────────────────
    #[rts_method(
        external,
        name = "setUint16",
        symbol = "__RTS_FN_GL_DATAVIEW_SET_UINT16_LE",
        ts = "setUint16(byteOffset: number, value: number, littleEndian: boolean): void"
    )]
    pub fn set_uint16_le(_h: Handle, _byte_offset: I64, _value: I64, _le: I32) {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "getUint16",
        symbol = "__RTS_FN_GL_DATAVIEW_GET_UINT16_LE",
        ts = "getUint16(byteOffset: number, littleEndian: boolean): number",
        pure
    )]
    pub fn get_uint16_le(_h: Handle, _byte_offset: I64, _le: I32) -> I64 {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "setInt32",
        symbol = "__RTS_FN_GL_DATAVIEW_SET_INT32_LE",
        ts = "setInt32(byteOffset: number, value: number, littleEndian: boolean): void"
    )]
    pub fn set_int32_le(_h: Handle, _byte_offset: I64, _value: I64, _le: I32) {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "getInt32",
        symbol = "__RTS_FN_GL_DATAVIEW_GET_INT32_LE",
        ts = "getInt32(byteOffset: number, littleEndian: boolean): number",
        pure
    )]
    pub fn get_int32_le(_h: Handle, _byte_offset: I64, _le: I32) -> I64 {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "setFloat64",
        symbol = "__RTS_FN_GL_DATAVIEW_SET_FLOAT64",
        ts = "setFloat64(byteOffset: number, value: number, littleEndian?: boolean): void"
    )]
    pub fn set_float64(_h: Handle, _byte_offset: I64, _value: F64, _le: I32) {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "getFloat64",
        symbol = "__RTS_FN_GL_DATAVIEW_GET_FLOAT64",
        ts = "getFloat64(byteOffset: number, littleEndian?: boolean): number",
        pure
    )]
    pub fn get_float64(_h: Handle, _byte_offset: I64, _le: I32) -> F64 {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "setFloat32",
        symbol = "__RTS_FN_GL_DATAVIEW_SET_FLOAT32",
        ts = "setFloat32(byteOffset: number, value: number, littleEndian?: boolean): void"
    )]
    pub fn set_float32(_h: Handle, _byte_offset: I64, _value: F64, _le: I32) {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "getFloat32",
        symbol = "__RTS_FN_GL_DATAVIEW_GET_FLOAT32",
        ts = "getFloat32(byteOffset: number, littleEndian?: boolean): number",
        pure
    )]
    pub fn get_float32(_h: Handle, _byte_offset: I64, _le: I32) -> F64 {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "setBigInt64",
        symbol = "__RTS_FN_GL_DATAVIEW_SET_BIGINT64",
        ts = "setBigInt64(byteOffset: number, value: bigint, littleEndian?: boolean): void"
    )]
    pub fn set_bigint64(_h: Handle, _byte_offset: I64, _value: I64, _le: I32) {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "getBigInt64",
        symbol = "__RTS_FN_GL_DATAVIEW_GET_BIGINT64",
        ts = "getBigInt64(byteOffset: number, littleEndian?: boolean): bigint",
        pure
    )]
    pub fn get_bigint64(_h: Handle, _byte_offset: I64, _le: I32) -> I64 {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "setBigUint64",
        symbol = "__RTS_FN_GL_DATAVIEW_SET_BIGUINT64",
        ts = "setBigUint64(byteOffset: number, value: bigint, littleEndian?: boolean): void"
    )]
    pub fn set_biguint64(_h: Handle, _byte_offset: I64, _value: I64, _le: I32) {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "getBigUint64",
        symbol = "__RTS_FN_GL_DATAVIEW_GET_BIGUINT64",
        ts = "getBigUint64(byteOffset: number, littleEndian?: boolean): bigint",
        pure
    )]
    pub fn get_biguint64(_h: Handle, _byte_offset: I64, _le: I32) -> I64 {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "byteLength",
        symbol = "__RTS_FN_GL_DATAVIEW_BYTE_LENGTH",
        ts = "byteLength: number",
        pure
    )]
    pub fn byte_length(_h: Handle) -> I64 {
        unreachable!()
    }

    #[rts_method(
        external,
        name = "byteOffset",
        symbol = "__RTS_FN_GL_DATAVIEW_BYTE_OFFSET",
        ts = "byteOffset: number",
        pure
    )]
    pub fn byte_offset(_h: Handle) -> I64 {
        unreachable!()
    }
}
