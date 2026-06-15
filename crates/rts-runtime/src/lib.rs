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
/// Embedded TS source of the PRIMORDIAL `Number.prototype` methods, re-exported
/// from `rts-primitives` so the new engine reaches it through the facade
/// (`rts_runtime::NUMBER_TS`). Included as a declarations-only prelude; a
/// primitive-number method call routes into its ambient `class Number`. The
/// irreducible numeric formatting stays in Rust and is bridged via the private
/// `engine.num_*` helpers the `.ts` bodies call.
pub use rts_primitives::NUMBER_TS;
/// Embedded TS source of the PRIMORDIAL `String.prototype` methods, re-exported
/// from `rts-primitives` so the new engine reaches it through the facade
/// (`rts_runtime::STRING_TS`). Included as a declarations-only prelude; a
/// primitive-string method call routes into its ambient `class String`. The
/// irreducible Unicode string logic stays in Rust and is bridged via the private
/// `engine.str_*` helpers the `.ts` bodies call.
pub use rts_primitives::STRING_TS;
pub mod namespaces;
