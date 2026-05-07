//! Bridge between `rts_hir::CraneliftTypeHint` and `cranelift_codegen::ir::Type`.
//!
//! Lives here (not in rts-hir) because rts-hir must stay free of cranelift
//! dependencies — see `RTS_REFACTOR.md` invariants.

use cranelift_codegen::ir::{types as cl, Type};
use rts_hir::ir::{HirType, VecKind};
use rts_hir::CraneliftTypeHint;

/// Mapeia `VecKind` para o `Type` SIMD do Cranelift.
pub fn veckind_to_cl(k: VecKind) -> Type {
    match k {
        VecKind::I8x16 => cl::I8X16,
        VecKind::I16x8 => cl::I16X8,
        VecKind::I32x4 => cl::I32X4,
        VecKind::I64x2 => cl::I64X2,
        VecKind::F32x4 => cl::F32X4,
        VecKind::F64x2 => cl::F64X2,
    }
}

/// Convert a HIR type hint to its Cranelift equivalent. Returns `None` for
/// `Void`, `StrPtr` (multi-slot), or compound types — caller decides how
/// to handle those.
pub fn hint_to_cl(hint: CraneliftTypeHint) -> Option<Type> {
    Some(match hint {
        CraneliftTypeHint::I8 => cl::I8,
        CraneliftTypeHint::I16 => cl::I16,
        CraneliftTypeHint::I32 => cl::I32,
        CraneliftTypeHint::I64 => cl::I64,
        CraneliftTypeHint::I128 => cl::I128,
        CraneliftTypeHint::F32 => cl::F32,
        CraneliftTypeHint::F64 => cl::F64,
        // Pointer width: assume 64-bit since RTS only targets 64-bit hosts
        CraneliftTypeHint::Ptr => cl::I64,
        CraneliftTypeHint::V128(k) => veckind_to_cl(k),
        CraneliftTypeHint::Void | CraneliftTypeHint::StrPtr => return None,
    })
}

/// Convert a HIR type directly to a Cranelift `Type`, falling back to `I64`
/// for compound/unknown types so the IR remains well-formed even when MIR
/// carries imprecise types.
///
/// **Narrow ints (I8/I16/U8/U16)** sao mapeados para `I64` — o ABI de
/// chamadas cross-fn AST<->MIR ja' viaja em i64, e shifts > width
/// (e.g. `ishl_imm 56` para sign-extend signed) seriam UB sobre `cl::I8`.
/// O pass `narrow` do MIR ja' insere mask + sign-extend explicitos,
/// preservando overflow em i64.
pub fn hir_to_cl(ty: &HirType) -> Type {
    use rts_hir::ir::HirType as H;
    match ty {
        H::I8 | H::I16 | H::U8 | H::U16 => cl::I64,
        _ => match ty.cranelift_hint() {
            Some(hint) => hint_to_cl(hint).unwrap_or(cl::I64),
            None => cl::I64,
        },
    }
}
