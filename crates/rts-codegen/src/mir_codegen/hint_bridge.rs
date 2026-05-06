//! Bridge between `rts_hir::CraneliftTypeHint` and `cranelift_codegen::ir::Type`.
//!
//! Lives here (not in rts-hir) because rts-hir must stay free of cranelift
//! dependencies — see `RTS_REFACTOR.md` invariants.

use cranelift_codegen::ir::{types as cl, Type};
use rts_hir::ir::HirType;
use rts_hir::CraneliftTypeHint;

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
        CraneliftTypeHint::Void | CraneliftTypeHint::StrPtr => return None,
    })
}

/// Convert a HIR type directly to a Cranelift `Type`, falling back to `I64`
/// for compound/unknown types so the IR remains well-formed even when MIR
/// carries imprecise types.
pub fn hir_to_cl(ty: &HirType) -> Type {
    match ty.cranelift_hint() {
        Some(hint) => hint_to_cl(hint).unwrap_or(cl::I64),
        None => cl::I64, // Any/Unknown/Object/Array/Class — opaque i64
    }
}
