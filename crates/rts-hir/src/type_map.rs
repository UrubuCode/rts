use crate::ir::{HirType, VecKind};

/// Hint for the Cranelift type to use — does not import cranelift directly.
/// `rts-codegen` converts this to `cranelift_codegen::ir::Type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CraneliftTypeHint {
    I8,
    I16,
    I32,
    I64,
    I128,
    F32,
    F64,
    /// Pointer-width integer (I32 on 32-bit targets, I64 on 64-bit).
    Ptr,
    Void,
    /// Two slots: ptr:I64 + len:I64 — caller must expand.
    StrPtr,
    /// 128-bit SIMD vector with lane configuration. Cranelift mapeia para
    /// `types::I8X16` etc. via `hint_to_cl` no `rts-codegen`.
    V128(VecKind),
}

impl HirType {
    /// Map this HIR type to a Cranelift type hint.
    ///
    /// Returns `None` for types that require multi-slot expansion (`Str`,
    /// `Any`, `Unknown`) — caller is responsible for the expansion.
    pub fn cranelift_hint(&self) -> Option<CraneliftTypeHint> {
        match self {
            HirType::I8 => Some(CraneliftTypeHint::I8),
            HirType::I16 => Some(CraneliftTypeHint::I16),
            HirType::I32 | HirType::U32 => Some(CraneliftTypeHint::I32),
            HirType::I64
            | HirType::U64
            | HirType::Bool
            | HirType::Handle(_) => Some(CraneliftTypeHint::I64),
            HirType::U8 => Some(CraneliftTypeHint::I8),
            HirType::U16 => Some(CraneliftTypeHint::I16),
            HirType::I128 | HirType::U128 => Some(CraneliftTypeHint::I128),
            HirType::F32 => Some(CraneliftTypeHint::F32),
            HirType::F64 | HirType::Number => Some(CraneliftTypeHint::F64),
            HirType::Void => Some(CraneliftTypeHint::Void),
            HirType::Str => Some(CraneliftTypeHint::StrPtr),
            HirType::V128(k) => Some(CraneliftTypeHint::V128(*k)),
            HirType::Any
            | HirType::Unknown
            | HirType::Array(_)
            | HirType::Function { .. }
            | HirType::Class(_)
            | HirType::Object => None,
        }
    }

    /// Returns the single-slot Cranelift hint for integer/handle types that
    /// travel as i64 in the ABI. Used for deciding whether to emit `iadd_imm`
    /// vs an extern call.
    pub fn is_direct_i64_abi(&self) -> bool {
        matches!(
            self,
            HirType::I64 | HirType::U64 | HirType::Bool | HirType::Handle(_)
        )
    }

    /// True when this type can be represented as a plain integer in a Cranelift
    /// variable — i.e., no GC tracking needed and fits one slot.
    pub fn is_scalar_int(&self) -> bool {
        matches!(
            self,
            HirType::I8
                | HirType::I16
                | HirType::I32
                | HirType::I64
                | HirType::U8
                | HirType::U16
                | HirType::U32
                | HirType::U64
                | HirType::Bool
        )
    }
}
