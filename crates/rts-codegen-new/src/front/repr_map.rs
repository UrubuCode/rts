//! `HirType` → [`Repr`] — the representation lattice (design pilar 2) in action.
//!
//! This is the single rule that decides whether a typed HIR value lives unboxed
//! in a register or as a [`crate::value::PolyValue`]. The numeric-subset lowering
//! ([`super::hir_lower`]) keeps `Int32`/`Float64`/`Bool` fully unboxed (native
//! `iadd`/`fadd`/`icmp`/…) and refuses anything that maps to `Tagged`.
//!
//! ## The mapping (and why)
//!
//! JS `number` is an IEEE-754 **double**, so `number` → [`Repr::Float64`]. The
//! explicit narrow float aliases (`f64`, `f32`) also map to `Float64` (f32 is
//! widened to f64 for the fast path; a true 32-bit float lane is a later
//! increment). The explicit small-integer annotations that fit in 32 bits
//! (`i8/i16/i32/u8/u16/u32`) map to [`Repr::Int32`] — the JS small-int fast path.
//! `boolean` → [`Repr::Bool`].
//!
//! `i64`/`u64` map to [`Repr::Int64`] — a proven monomorphic native i64 in a
//! register (lossless; NOT the 48-bit tagged-int payload, which stays `Int32`).
//! This also catches integer-valued numeric code the HIR narrows to `I64`: a
//! Fibonacci loop over `0`/`1` literals types as `I64`, because swc lowers
//! `0.0`/`1.0` (no fractional part) to integer literals.
//!
//! Everything else is widened to [`Repr::Tagged`] and the lowering bails:
//! - `i128/u128` do NOT fit a 64-bit register losslessly → a later increment.
//! - `Str/Void/Handle/Array/Object/Function/Class/Any/Unknown` are
//!   objects/strings/closures/`any` — later increments.

use rts_hir::HirType;

use crate::repr::Repr;

/// Map an [`HirType`] to its [`Repr`] under the increment-3 numeric subset.
///
/// `Number`/`F64`/`F32` → `Float64`; the i32-fitting integer annotations →
/// `Int32`; `i64`/`u64` → `Int64`; `Bool` → `Bool`; everything else (incl.
/// 128-bit ints, `Unknown`, `Any`, and every non-numeric kind) → `Tagged`. The
/// lowering treats a `Tagged` param/return/value as out of scope and returns an
/// explicit `Unsupported`.
pub fn repr_of(t: &HirType) -> Repr {
    match t {
        // JS number is a double; narrow float aliases ride the same fast path.
        HirType::Number | HirType::F64 | HirType::F32 => Repr::Float64,

        // Integer annotations that fit losslessly in an i32 register.
        HirType::I8 | HirType::I16 | HirType::I32 | HirType::U8 | HirType::U16 | HirType::U32 => {
            Repr::Int32
        }

        // 64-bit integers: a proven monomorphic native i64 register (lossless).
        HirType::I64 | HirType::U64 => Repr::Int64,

        HirType::Bool => Repr::Bool,

        // Out of the numeric subset → uniform tagged value; the lowering bails.
        // (128-bit ints need a wider/tagged rep; objects/strings/closures/any
        // are later increments.)
        _ => Repr::Tagged,
    }
}

/// Whether `t` is in the numeric subset (maps to an unboxed register repr).
pub fn is_numeric_subset(t: &HirType) -> bool {
    repr_of(t).is_unboxed()
}
