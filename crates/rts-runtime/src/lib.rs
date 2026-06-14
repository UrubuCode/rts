pub mod abi {
    pub use rts_engine::abi::*;
}

pub use rts_std::runtime;
/// Embedded TS stdlib sources (engine includes), re-exported from `rts-shared`
/// so the new engine reaches them through the facade (`rts_runtime::stdlib`).
pub use rts_shared::stdlib;
pub mod namespaces;
