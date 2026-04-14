use cranelift_codegen::ir::{AbiParam, types};
use cranelift_module::Module;

use super::types::ABI_PARAM_COUNT;

pub(crate) fn function_signature<M: Module>(module: &mut M) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    for _ in 0..ABI_PARAM_COUNT {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));
    sig
}
