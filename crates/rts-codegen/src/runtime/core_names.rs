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
