//! The one table that binds a name imported from `"rts:core"` to a neutral
//! [`CorePrimitive`], shared by both emitters (`emit/core_call.rs` for the
//! running emitter, `lower/intrinsic.rs::core_intrinsic` for the MIR stage) so
//! they cannot disagree about which names exist.
//!
//! `rts:core` is NOT a runtime module: an `import { imul } from "rts:core"` is
//! compile-time sugar. The import emits nothing, and every call of the bound
//! name lowers to the direct op for the primitive.
//!
//! Scaffolding: `imul` duplicates `Math.imul`. The point is the mechanism, which
//! later (unsafe) primitives reuse.
//!
//! TODO: each entry carries `public`, from `CorePrimitive::is_public`. When an
//! unsafe primitive is added, gate it by module path (std only) here; no path
//! gate exists yet because every entry is public.

use rts_corelib::CorePrimitive;

/// The module specifier whose imports are resolved at compile time.
pub const SPECIFIER: &str = "rts:core";

/// What a name binds.
#[derive(Clone, Copy, Debug)]
pub struct CoreName {
    /// The neutral primitive.
    pub primitive: CorePrimitive,
    /// Written arguments.
    pub arity: usize,
    /// Whether any module may use it (`CorePrimitive::is_public`).
    pub public: bool,
}

/// The second specifier of the same mechanism: the NATIVE-HOST layer. Its names
/// bind to host entries that read OS state, and exist only on a native target
/// (a browser has no OS), unlike the universal `rts:core`.
pub const SYS_SPECIFIER: &str = "rts:sys";

/// Whether `specifier` is resolved at compile time by this table.
pub fn is_compile_time(specifier: &str) -> bool {
    specifier == SPECIFIER || specifier == SYS_SPECIFIER
}

/// What an imported local name is bound to: one mechanism, two specifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bound {
    /// A neutral `rts:core` primitive.
    Core(CorePrimitive),
    /// A host entry from `rts:sys`.
    Sys(Sys),
}

/// A `rts:sys` host query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sys {
    /// Physical memory installed, in bytes.
    TotalMem,
    /// Physical memory available, in bytes.
    FreeMem,
    /// Seconds since boot.
    Uptime,
    /// Logical CPU count.
    CpuCount,
}

impl Sys {
    /// The selector `RuntimeOp::SysMemory` takes.
    pub fn selector(self) -> i64 {
        match self {
            Sys::TotalMem => 0,
            Sys::FreeMem => 1,
            Sys::Uptime => 2,
            Sys::CpuCount => 3,
        }
    }

    /// Written arguments.
    pub fn arity(self) -> usize {
        0
    }
}

/// Whether the target being compiled for has a host (an operating system).
///
/// TODO: the codegen has no target concept yet; this reads the architecture the
/// COMPILER was built for, which is the target today because there is only a
/// native backend. When a wasm backend lands, thread the real target here --
/// `rts:sys` must be refused for it.
pub fn host_available() -> bool {
    !cfg!(target_arch = "wasm32")
}

/// The host query a `rts:sys` export name means, or `None` for an unknown name.
pub fn lookup_sys(name: &str) -> Option<Sys> {
    match name {
        "totalmem" => Some(Sys::TotalMem),
        "freemem" => Some(Sys::FreeMem),
        "uptime" => Some(Sys::Uptime),
        "cpuCount" => Some(Sys::CpuCount),
        _ => None,
    }
}

/// The primitive a `rts:core` export name means, or `None` for an unknown name.
pub fn lookup(name: &str) -> Option<CoreName> {
    let (primitive, arity) = match name {
        "imul" => (CorePrimitive::I32Mul, 2),
        _ => return None,
    };
    Some(CoreName {
        primitive,
        arity,
        public: primitive.is_public(),
    })
}
