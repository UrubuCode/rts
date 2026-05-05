//! Re-exports from `rts_abi::signature` plus the Cranelift conversion helpers.
//!
//! `scalar_to_cl`, `sig_params_cl`, and `sig_ret_cl` live here (not in
//! `rts-abi`) because they depend on `cranelift_codegen`, which `rts-abi`
//! intentionally avoids.

pub use rts_abi::signature::*;

use cranelift_codegen::ir::Type as ClType;
use cranelift_codegen::ir::types as cl_types;

use crate::abi::types::AbiType;

/// Converts an `AbiType` scalar to its Cranelift IR `Type`.
///
/// `Void` and `StrPtr` are compound/absent — callers must handle them
/// before invoking this function.
pub fn scalar_to_cl(ty: AbiType) -> ClType {
    match ty {
        AbiType::Bool | AbiType::I64 | AbiType::U64 | AbiType::Handle => cl_types::I64,
        AbiType::I32 => cl_types::I32,
        AbiType::F64 => cl_types::F64,
        AbiType::Void | AbiType::StrPtr => {
            unreachable!("compound/void handled by caller")
        }
    }
}

/// Converts the `params` of a `LoweredSignature` to Cranelift IR types.
///
/// Call sites that build a Cranelift `Signature` use this instead of
/// iterating `lowered.params` directly.
pub fn sig_params_cl(params: &[AbiType]) -> Vec<ClType> {
    params.iter().copied().map(scalar_to_cl).collect()
}

/// Converts the `ret` of a `LoweredSignature` to a Cranelift IR type.
pub fn sig_ret_cl(ret: Option<AbiType>) -> Option<ClType> {
    ret.map(scalar_to_cl)
}
