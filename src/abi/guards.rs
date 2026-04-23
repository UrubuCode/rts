//! Call-site type-guard emission plan.
//!
//! When codegen emits a call to an ABI member with a statically typed TS
//! caller, no guard is needed: the argument is already in the right shape.
//! When the caller uses `any` (or any unsound unknown), codegen inserts a
//! guard that traps or coerces before the call, keeping the callee ABI
//! strict. This module centralises the decision table so different codegen
//! backends (JIT / object) stay consistent.
//!
//! Implementation is intentionally minimal at this stage: enums and the
//! policy matrix are defined, but the actual IR emission is deferred until
//! a namespace is migrated onto the new ABI. That migration wires the enums
//! into the codegen pipeline without having to redesign the decision logic.

use crate::abi::types::AbiType;

/// Describes what the codegen can infer about an argument at the call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallerShape {
    /// Caller's static type matches the callee's expected `AbiType`.
    Known(AbiType),
    /// Caller has an unknown or polymorphic value (`any`, union, etc).
    Unknown,
    /// Caller is a known-nullish constant (`null` / `undefined`).
    Nullish,
}

/// Action codegen must take before handing the value to the callee.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardAction {
    /// Pass the value through unchanged. No instructions emitted.
    Passthrough,
    /// Emit a runtime coercion that unboxes a polymorphic value into the
    /// target type. On mismatch, emits a trap with a type error.
    Coerce(AbiType),
    /// Emit a trap immediately: the call is statically known to be invalid.
    Trap(&'static str),
}

/// Computes the guard action for a single argument slot.
///
/// The policy is conservative: statically-matching shapes skip guards;
/// unknown shapes require a coercion; nullish against a non-nullable slot
/// traps at codegen time with a diagnostic-friendly message.
pub fn guard_for(expected: AbiType, caller: CallerShape) -> GuardAction {
    match (expected, caller) {
        (exp, CallerShape::Known(got)) if exp == got => GuardAction::Passthrough,
        (exp, CallerShape::Known(_)) => GuardAction::Coerce(exp),
        (exp, CallerShape::Unknown) => GuardAction::Coerce(exp),
        (AbiType::Handle, CallerShape::Nullish)
        | (AbiType::StrPtr, CallerShape::Nullish) => {
            GuardAction::Trap("nullish value cannot be converted to handle")
        }
        (_, CallerShape::Nullish) => GuardAction::Coerce(expected),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_static_type_passes_through() {
        assert_eq!(
            guard_for(AbiType::I64, CallerShape::Known(AbiType::I64)),
            GuardAction::Passthrough
        );
    }

    #[test]
    fn unknown_shape_always_coerces() {
        assert_eq!(
            guard_for(AbiType::F64, CallerShape::Unknown),
            GuardAction::Coerce(AbiType::F64)
        );
    }

    #[test]
    fn nullish_into_handle_traps() {
        assert!(matches!(
            guard_for(AbiType::Handle, CallerShape::Nullish),
            GuardAction::Trap(_)
        ));
    }
}
