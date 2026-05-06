//! MIR → Cranelift IR lowering — the final layer of the rts-mir pipeline.
//!
//! Converts a `MirFunc` into a Cranelift `Function` via `FunctionBuilder`.
//! Each `Inst` variant maps 1:1 to `builder.ins().*` calls; each `Terminator`
//! maps to a Cranelift terminator. This module is intentionally isolated
//! from `crate::codegen::lower::*` (the AST-driven path) so the two can
//! coexist while the MIR path matures.
//!
//! Status: Fase 3 etapa 3.9 — esqueleto inicial. Cobre o subset que o
//! `rts_mir::lower` já produz hoje (literais, aritmética, casts,
//! comparações, branches, return). Não plugado no pipeline ainda.

pub mod hint_bridge;
pub mod lower;

pub use lower::lower_mir_func;

use rts_abi::AbiType;
use rts_hir::ir::HirType;

/// Map an `AbiType` (rts-abi enum) to the closest `HirType` for embedding
/// in `Inst::CallExtern`. `StrPtr` is two-slot — caller layer must split
/// args/returns; for now we collapse to `I64` and let the codegen fall
/// back to the AST path when string args are involved.
fn abi_type_to_hir(t: AbiType) -> HirType {
    match t {
        AbiType::Void => HirType::Void,
        AbiType::Bool => HirType::Bool,
        AbiType::I32 => HirType::I32,
        AbiType::I64 => HirType::I64,
        AbiType::U64 => HirType::U64,
        AbiType::F64 => HirType::F64,
        AbiType::Handle => HirType::Handle(rts_hir::ir::HandleKind::Opaque),
        // StrPtr is two-slot; callers must bail before reaching CallExtern
        // with a StrPtr signature.
        AbiType::StrPtr => HirType::I64,
    }
}

/// Build an `ExternResolver` closure that consults the workspace's
/// `crate::abi::SPECS` to resolve `(ns, method)` pairs to extern symbols.
/// Returns `None` for namespaces with `StrPtr` in their signature (the MIR
/// layer doesn't model two-slot params yet).
pub fn extern_resolver_default() -> impl Fn(&str, &str) -> Option<(String, Vec<HirType>, HirType)> {
    |ns: &str, method: &str| {
        let qualified = format!("{ns}.{method}");
        let (_spec, member) = crate::abi::lookup(&qualified)?;
        // Skip members that take or return StrPtr — caller falls back.
        if member.args.iter().any(|t| matches!(t, AbiType::StrPtr))
            || matches!(member.returns, AbiType::StrPtr)
        {
            return None;
        }
        let param_tys: Vec<HirType> = member.args.iter().copied().map(abi_type_to_hir).collect();
        let ret_ty = abi_type_to_hir(member.returns);
        Some((member.symbol.to_string(), param_tys, ret_ty))
    }
}

#[cfg(test)]
mod tests;
