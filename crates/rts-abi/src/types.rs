//! Primitive ABI types used in namespace signatures.
//!
//! Every argument and return value at the `extern "C"` boundary is encoded
//! using one of these primitives. `StrPtr` is the only compound variant: it
//! expands into two Cranelift slots, `(ptr: i64, len: i64)`. Tagged or
//! polymorphic values never appear here — `any`-typed values are unboxed at
//! the call site by codegen into concrete primitives before the call.

/// Scalar types that survive the C ABI with zero wrapping.
///
/// Width is fixed regardless of host target so codegen can derive Cranelift
/// signatures deterministically. Runtime handles (string table, object table,
/// buffer table) are `u64` values produced by the GC namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AbiType {
    /// No value. Only legal as a function return type.
    Void,
    /// One-bit boolean transmitted as `i64` with value in `{0, 1}`.
    Bool,
    /// Signed 32-bit integer.
    I32,
    /// Signed 64-bit integer.
    I64,
    /// Unsigned 64-bit integer. Typical carrier for GC handles.
    U64,
    /// IEEE-754 double precision.
    F64,
    /// Static UTF-8 slice. Expands into two slots: `(*const u8, i64)`.
    StrPtr,
    /// Opaque runtime handle (`u64`) produced by the GC namespace.
    Handle,
}

impl AbiType {
    /// Number of Cranelift slots this type occupies in a signature.
    ///
    /// Most primitives take a single slot; `StrPtr` splits into two.
    pub const fn slot_count(self) -> usize {
        match self {
            AbiType::Void => 0,
            AbiType::StrPtr => 2,
            _ => 1,
        }
    }

    /// True when the type may appear as a return value.
    pub const fn is_returnable(self) -> bool {
        !matches!(self, AbiType::StrPtr)
    }
}
