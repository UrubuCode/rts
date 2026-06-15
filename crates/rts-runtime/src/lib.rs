pub mod abi {
    pub use rts_engine::abi::*;
}

pub use rts_std::runtime;
/// Embedded TS stdlib sources (engine includes), re-exported from `rts-shared`
/// so the new engine reaches them through the facade (`rts_runtime::stdlib`).
pub use rts_shared::stdlib;
/// Embedded TS source of the PRIMORDIAL `Error` family, re-exported from
/// `rts-primitives` (Error is a primordial, so its `.ts` lives there) so the new
/// engine reaches it through the facade (`rts_runtime::ERROR_TS`). Included by the
/// engine ahead of the Map/Set stdlib prelude.
pub use rts_primitives::ERROR_TS;
/// Embedded TS source of the PRIMORDIAL `Boolean.prototype` methods, re-exported
/// from `rts-primitives` so the new engine reaches it through the facade
/// (`rts_runtime::BOOLEAN_TS`). Included by the engine as a declarations-only
/// prelude; a primitive-bool method call routes into its ambient `class Boolean`.
pub use rts_primitives::BOOLEAN_TS;
pub mod namespaces;
