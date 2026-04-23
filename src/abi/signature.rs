//! Conversion from high-level `AbiType` slots to Cranelift IR types.
//!
//! Codegen consumes these helpers when declaring extern functions so the
//! Cranelift signature matches the Rust `extern "C"` function produced by
//! the runtime. The conversion is intentionally value-level (returns plain
//! `ir::Type`) so callers can attach their own calling convention and module
//! without this module depending on a specific Cranelift backend.

use cranelift_codegen::ir::types as cl_types;
use cranelift_codegen::ir::Type as ClType;

use crate::abi::member::{MemberKind, NamespaceMember};
use crate::abi::types::AbiType;

/// Expands an `AbiType` list into the flat Cranelift parameter types.
///
/// `StrPtr` contributes `[I64, I64]`; every other variant contributes a
/// single `ClType`. `Void` in a parameter position is rejected.
pub fn lower_params(args: &[AbiType]) -> Vec<ClType> {
    let mut out = Vec::with_capacity(args.len() + 1);
    for ty in args {
        match *ty {
            AbiType::Void => panic!("Void is not valid as a parameter type"),
            AbiType::StrPtr => {
                out.push(cl_types::I64);
                out.push(cl_types::I64);
            }
            other => out.push(scalar_to_cl(other)),
        }
    }
    out
}

/// Lowers the return type. `Void` returns `None`.
pub fn lower_return(ret: AbiType) -> Option<ClType> {
    match ret {
        AbiType::Void => None,
        AbiType::StrPtr => panic!("StrPtr is not a valid return type"),
        other => Some(scalar_to_cl(other)),
    }
}

/// Full lowering of a member's signature.
///
/// For constants, returns empty params and the declared return type.
pub fn lower_member(member: &NamespaceMember) -> LoweredSignature {
    match member.kind {
        MemberKind::Function => LoweredSignature {
            params: lower_params(member.args),
            ret: lower_return(member.returns),
        },
        MemberKind::Constant => LoweredSignature {
            params: Vec::new(),
            ret: lower_return(member.returns),
        },
    }
}

/// Pre-lowered Cranelift signature pieces, ready to attach to a
/// `cranelift_codegen::ir::Signature`.
#[derive(Debug, Clone)]
pub struct LoweredSignature {
    pub params: Vec<ClType>,
    pub ret: Option<ClType>,
}

fn scalar_to_cl(ty: AbiType) -> ClType {
    match ty {
        AbiType::Bool | AbiType::I64 | AbiType::U64 | AbiType::Handle => cl_types::I64,
        AbiType::I32 => cl_types::I32,
        AbiType::F64 => cl_types::F64,
        AbiType::Void | AbiType::StrPtr => {
            unreachable!("compound/void handled by caller")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strptr_expands_into_two_slots() {
        let lowered = lower_params(&[AbiType::StrPtr]);
        assert_eq!(lowered, vec![cl_types::I64, cl_types::I64]);
    }

    #[test]
    fn mixed_args_preserve_order() {
        let lowered = lower_params(&[AbiType::I32, AbiType::StrPtr, AbiType::F64]);
        assert_eq!(
            lowered,
            vec![cl_types::I32, cl_types::I64, cl_types::I64, cl_types::F64]
        );
    }

    #[test]
    fn void_return_is_none() {
        assert_eq!(lower_return(AbiType::Void), None);
        assert_eq!(lower_return(AbiType::Handle), Some(cl_types::I64));
        assert_eq!(lower_return(AbiType::F64), Some(cl_types::F64));
    }
}
