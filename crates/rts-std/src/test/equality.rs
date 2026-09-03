//! Identity, truthiness, and nullish matchers — `toBe`, `toEqual`,
//! `toBeTruthy`, `toBeFalsy`, `toBeNull`, `toBeUndefined`, `toBeDefined`.
//!
//! What is common to every matcher in this crate lives in the parent module:
//! [`super::received_of`], [`super::negate_if`], [`super::settle`]. This file
//! is only the seven answers that decide `held`.

/// `expect(a).toBe(b)`.
///
/// Jest's `toBe` is `Object.is`, not `===`: `expect(0/0).toBe(NaN)` passes and
/// `expect(-0).toBe(0)` fails in the real harness, which `===` gets backwards on
/// both. `rts_core::entry::same_value` is `SameValue` for exactly that
/// reason — this used to call `strict_equals` and silently graded both cases
/// the wrong way.
///
/// `toEqual` is installed as the same function. They differ in the language's
/// harness — one is identity and one is deep equality — and a deep comparison
/// that ran on identity would report passes it did not earn, so the two share
/// the same-value answer and the divergence is named here rather than guessed
/// at.
pub(super) extern "C" fn to_be(_e: u64, this: u64, expected: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = super::received_of(this);
    let held = rts_core::entry::same_value(received, expected);
    super::settle(super::negate_if(this, held), super::is_negated(this), received, expected)
}

/// `expect(x).toBeTruthy()`.
pub(super) extern "C" fn to_be_truthy(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = super::received_of(this);
    let held = rts_core::entry::to_boolean(received);
    super::settle(super::negate_if(this, held), super::is_negated(this), received, received)
}

/// `expect(x).toBeFalsy()`.
pub(super) extern "C" fn to_be_falsy(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = super::received_of(this);
    let held = !rts_core::entry::to_boolean(received);
    super::settle(super::negate_if(this, held), super::is_negated(this), received, received)
}

/// `expect(x).toBeNull()`.
pub(super) extern "C" fn to_be_null(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = super::received_of(this);
    let null = rts_core::entry::null_value();
    super::settle(super::negate_if(this, received == null), super::is_negated(this), received, null)
}

/// `expect(x).toBeUndefined()`.
pub(super) extern "C" fn to_be_undefined(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = super::received_of(this);
    let absent = rts_core::entry::undefined_value();
    super::settle(super::negate_if(this, received == absent), super::is_negated(this), received, absent)
}

/// `expect(x).toBeDefined()`.
pub(super) extern "C" fn to_be_defined(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = super::received_of(this);
    let absent = rts_core::entry::undefined_value();
    super::settle(super::negate_if(this, received != absent), super::is_negated(this), received, absent)
}
