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
    /// A raw 64-bit PolyValue word (NaN-boxed) carried across the ABI UNCHANGED —
    /// no coercion, no unbox. Distinct from `Handle` (table-loads to a slot) and
    /// `U64` (coerces to an integer): a `PolyValue` arg is the boxed word as-is.
    /// Lets a backend receive a function-VALUE listener / an arbitrary value (the
    /// EventEmitter/Proxy callback bridge) without losing its tag.
    PolyValue,
}

/// A runtime symbol's NAME and ABI shape as ONE compile-time constant.
///
/// This is the single-source-of-truth carrier described in
/// `docs/specs/rts-macro-single-source.md`. A symbol used to be spelled out in
/// four independent places — the `extern "C"` definition, the JIT fn-ptr table
/// (`adapter_symbols`), the Cranelift signature table (`value::abi_sig`), and a
/// string literal at every lowering call site — with nothing checking that the
/// signature table agreed with the definition. `abi_sig`'s own doc-comment says
/// it exists because "mis-marshaling a single-slot string where the ABI expects
/// two → SIGILL", so the table that prevents a SIGILL was itself hand-synced.
///
/// A `SymbolDesc` is emitted BY `#[rtse::abi]` next to the function it describes,
/// with `params`/`ret` DERIVED FROM THE RUST SIGNATURE. Drift stops being a thing
/// a test has to catch and becomes unrepresentable: changing the Rust parameter
/// list changes the descriptor in the same edit, or the crate does not compile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SymbolDesc {
    /// The linker symbol (`__rtsadp_obj_get`, `__RTS_FN_NS_IO_PRINT`, …).
    pub name: &'static str,
    /// Parameter types in declaration order. A `StrPtr` counts as ONE entry here
    /// and expands to two Cranelift slots downstream.
    pub params: &'static [AbiType],
    /// Return type; `Void` for a function that returns nothing.
    pub ret: AbiType,
}

impl SymbolDesc {
    /// Total Cranelift slot count of the parameter list (`StrPtr` counts twice).
    pub fn param_slot_count(&self) -> usize {
        self.params.iter().map(|t| t.slot_count()).sum()
    }

    /// Whether this symbol returns a value.
    pub fn returns(&self) -> bool {
        !matches!(self.ret, AbiType::Void)
    }
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
