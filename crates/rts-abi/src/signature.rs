//! Lowering of ABI signatures to flat parameter/return type lists.
//!
//! This module has zero Cranelift dependencies. It expands `AbiType` lists
//! into flat `Vec<AbiType>` sequences (StrPtr becomes two I64 entries).
//! Codegen converts these to Cranelift IR types via `scalar_to_cl` in
//! `src/abi/signature.rs`.

use crate::member::{MemberKind, NamespaceMember};
use crate::types::AbiType;

/// Pre-lowered signature pieces in terms of `AbiType`.
///
/// `StrPtr` has already been expanded into two `AbiType::I64` entries in
/// `params`. Codegen converts each entry to a Cranelift IR type via
/// `scalar_to_cl` before constructing a Cranelift `Signature`.
#[derive(Debug, Clone)]
pub struct LoweredSignature {
    pub params: Vec<AbiType>,
    pub ret: Option<AbiType>,
}

/// Expands an `AbiType` list into the flat lowered parameter types.
///
/// `StrPtr` contributes `[I64, I64]`; every other variant contributes a
/// single entry. `Void` in a parameter position is rejected.
pub fn lower_params(args: &[AbiType]) -> Vec<AbiType> {
    let mut out = Vec::with_capacity(args.len() + 1);
    for ty in args {
        match *ty {
            AbiType::Void => panic!("Void is not valid as a parameter type"),
            AbiType::StrPtr => {
                out.push(AbiType::I64);
                out.push(AbiType::I64);
            }
            other => out.push(other),
        }
    }
    out
}

/// Lowers the return type. `Void` returns `None`.
pub fn lower_return(ret: AbiType) -> Option<AbiType> {
    match ret {
        AbiType::Void => None,
        AbiType::StrPtr => panic!("StrPtr is not a valid return type"),
        other => Some(other),
    }
}

/// Full lowering of a member's signature.
///
/// For constants, returns empty params and the declared return type.
/// For `Constructor` and `InstanceMethod`, `args` is used as-is —
/// the Handle receiver is already included in the args list for
/// `InstanceMethod` (slot 0), and constructors carry their own params.
pub fn lower_member(member: &NamespaceMember) -> LoweredSignature {
    match member.kind {
        MemberKind::Function
        | MemberKind::Constructor
        | MemberKind::InstanceMethod
        | MemberKind::StaticMethod
        | MemberKind::InstanceGetter => LoweredSignature {
            params: lower_params(member.args),
            ret: lower_return(member.returns),
        },
        MemberKind::Constant => LoweredSignature {
            params: Vec::new(),
            ret: lower_return(member.returns),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strptr_expands_into_two_slots() {
        let lowered = lower_params(&[AbiType::StrPtr]);
        assert_eq!(lowered, vec![AbiType::I64, AbiType::I64]);
    }

    #[test]
    fn mixed_args_preserve_order() {
        let lowered = lower_params(&[AbiType::I32, AbiType::StrPtr, AbiType::F64]);
        assert_eq!(
            lowered,
            vec![AbiType::I32, AbiType::I64, AbiType::I64, AbiType::F64]
        );
    }

    #[test]
    fn void_return_is_none() {
        assert_eq!(lower_return(AbiType::Void), None);
        assert_eq!(lower_return(AbiType::Handle), Some(AbiType::Handle));
        assert_eq!(lower_return(AbiType::F64), Some(AbiType::F64));
    }
}
