pub use rts_abi::signature::*;

use cranelift_codegen::ir::Type as ClType;
use cranelift_codegen::ir::types as cl_types;
use rts_abi::AbiType;

pub fn scalar_to_cl(ty: AbiType) -> ClType {
    match ty {
        AbiType::Bool | AbiType::I64 | AbiType::U64 | AbiType::Handle => cl_types::I64,
        AbiType::I32 => cl_types::I32,
        AbiType::F64 => cl_types::F64,
        AbiType::Void | AbiType::StrPtr => unreachable!("compound/void handled by caller"),
    }
}

pub fn sig_params_cl(params: &[AbiType]) -> Vec<ClType> {
    params.iter().copied().map(scalar_to_cl).collect()
}

pub fn sig_ret_cl(ret: Option<AbiType>) -> Option<ClType> {
    ret.map(scalar_to_cl)
}
