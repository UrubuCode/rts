/// Tipos primitivos suportados pelo RTS, cobrindo JS/TS nativos e
/// extensões de baixo nível para FFI e buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TsPrimitive {
    // JS/TS nativos
    Number,
    Boolean,
    String,
    // Extensões de baixo nível (FFI, buffers, WASM)
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
    F32,
    F64,
    // Void e never
    Void,
    Never,
}
