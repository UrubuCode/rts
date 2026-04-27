//! Namespace member metadata.
//!
//! Each `NamespaceMember` is consumed by codegen to emit direct
//! `call <symbol>` instructions and by the TypeScript declaration generator
//! to produce `rts.d.ts`. The layout intentionally mirrors what both
//! consumers need so no additional lookup structure is required.

use crate::abi::types::AbiType;

/// One exported entry inside a namespace.
///
/// Lifetime is `'static`: all entries live in `const` tables compiled into
/// the binary. No heap allocation occurs for metadata.
#[derive(Debug, Clone, Copy)]
pub struct NamespaceMember {
    /// Public name as written in TypeScript, e.g. `"print"`.
    pub name: &'static str,

    /// Whether the member is a callable function or a constant value.
    pub kind: MemberKind,

    /// Canonical C symbol. Must satisfy `abi::symbols::validate_symbol`.
    pub symbol: &'static str,

    /// Argument types, in order, as they appear in the function signature.
    /// `StrPtr` contributes two slots — codegen expands them automatically.
    pub args: &'static [AbiType],

    /// Return type. `AbiType::Void` indicates no return value.
    pub returns: AbiType,

    /// Doc comment used by `rts.d.ts` generation.
    pub doc: &'static str,

    /// TypeScript signature text, e.g. `"print(msg: string): void"`.
    pub ts_signature: &'static str,

    /// When present, codegen emits this operation inline at the call site
    /// instead of a call to `symbol`. Reserved for trivial hot-path members
    /// (single native Cranelift op, or a short inline sequence). The `symbol`
    /// is still exported so callers that do not know the member statically
    /// (e.g. reflection, FFI) keep working.
    pub intrinsic: Option<Intrinsic>,

    /// True if this member is pure: no I/O, no shared mutable state, no
    /// non-determinism. Pure members are eligible for automatic parallelisation
    /// (e.g. `for...of` body analysis in the purity pass). Conservative:
    /// false-negative is safe (falls back to sequential path).
    pub pure: bool,
}

/// Inlinable operations recognised by codegen.
///
/// Keep this list small: every variant forces codegen to carry hand-written
/// Cranelift IR that mirrors the extern implementation. Add a new variant
/// only when there's a measurable win in a real benchmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intrinsic {
    /// `f64::sqrt` → Cranelift `sqrt`.
    Sqrt,
    /// `f64::abs` → Cranelift `fabs`.
    AbsF64,
    /// `f64::min` → Cranelift `fmin`.
    MinF64,
    /// `f64::max` → Cranelift `fmax`.
    MaxF64,
    /// `i64::wrapping_abs` → sign-based abs using `iconst`, `ineg`, `select`.
    AbsI64,
    /// `i64::min` → signed integer min.
    MinI64,
    /// `i64::max` → signed integer max.
    MaxI64,
    /// Xorshift64 PRNG: load global state, mutate, store, convert to f64.
    /// State symbol is `__RTS_DATA_NS_MATH_RNG_STATE` (u64).
    RandomF64,
}

/// Whether a member is a function or a constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    /// Callable extern "C" function. `args`/`returns` describe its signature.
    Function,
    /// Constant value resolved once at program startup. `args` must be empty
    /// and `returns` holds the value type.
    Constant,
}

/// A registered namespace exposed through the new ABI.
#[derive(Debug, Clone, Copy)]
pub struct NamespaceSpec {
    /// Public name, e.g. `"io"`, `"fs"`, `"net"`.
    pub name: &'static str,
    /// Summary shown in `rts apis` and in generated declarations.
    pub doc: &'static str,
    /// Members of this namespace. Order is stable and significant for
    /// reproducible codegen.
    pub members: &'static [NamespaceMember],
}
