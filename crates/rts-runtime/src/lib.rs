pub mod abi {
    pub use rts_engine::abi::*;
}

pub use rts_std::runtime;
pub mod namespaces;

/// N-API (.node native addons). Re-exporta o crate `rts-napi`: os símbolos
/// `napi_*` (export-table do bin) e o loader `__RTS_FN_NS_NAPI_LOAD_ADDON`
/// (registrado no JIT). Ver docs/specs/napi-implementation.md.
pub use rts_napi as napi;
