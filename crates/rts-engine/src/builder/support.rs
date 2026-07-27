//! Shared helpers used by every builder flavor (fluent + closure-scoped):
//! symbol-stem derivation and the simple-member constructor.

use crate::Sig;
use crate::abi::{AbiType, MemberFlags, MemberKind};
use crate::member::{FnPtr, Member};

/// Stem de símbolo de um módulo: `NS_<NAME>`.
pub(crate) fn module_stem(name: &str) -> String {
    format!("NS_{}", name.to_uppercase())
}
/// Stem de símbolo de uma classe: `GL_<NAME>`.
pub(crate) fn class_stem(name: &str) -> String {
    format!("GL_{}", name.to_uppercase())
}
/// Símbolo canônico `__RTS_FN_<STEM>_<MEMBER>`.
pub(crate) fn fn_symbol(stem: &str, member: &str) -> String {
    format!("__RTS_FN_{stem}_{}", member.to_uppercase())
}

/// Constrói um [`Member`] simples (fn/const) com símbolo derivado.
pub(crate) fn simple_member(
    stem: &str,
    name: &str,
    kind: MemberKind,
    sig: Sig,
    ptr: *const u8,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        symbol: fn_symbol(stem, name),
        sig,
        fn_ptr: FnPtr(ptr),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: String::new(),
        doc: String::new(),
        ret_class: None,
        pure: false,
        
        emit: None,
    }
}

/// Tipo TS de um `AbiType` (para `ts_signature` derivado).
pub(crate) fn ts_of(ty: AbiType) -> &'static str {
    match ty {
        AbiType::Bool => "boolean",
        AbiType::StrPtr => "string",
        AbiType::Void => "void",
        _ => "number",
    }
}
