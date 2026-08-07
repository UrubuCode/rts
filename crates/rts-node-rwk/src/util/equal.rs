//! `isDeepStrictEqual` — a structural, depth-bounded comparison.
//!
//! Ambient throughout, per [`super::values`]'s rule.

use rts_core_rwk::entry;

use super::values::{bool_value, get, kind_of, own_key_strings, string_of};

/// The cap the walk stops at.
///
/// Not the inspector's depth: a comparison is not a print, and a real program
/// compares structures deeper than two levels far more often than it prints one
/// that deep. The cap exists for the same reason the inspector's does — a cyclic
/// object is valid input — not to match a formatting default.
const EQUAL_DEPTH: u32 = 32;

/// `util.isDeepStrictEqual(a, b)`.
///
/// The third documented argument, `{ skipPrototype }`, is read but only to say
/// it changes nothing: this walk never compares prototypes, so it already
/// behaves as `skipPrototype: true` and the option is refused by name in the
/// module doc rather than accepted as if it selected between two behaviours.
pub(super) extern "C" fn is_deep_strict_equal(
    _e: u64,
    _this: u64,
    a: u64,
    b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    bool_value(deep_equal(a, b, EQUAL_DEPTH))
}

/// The recursive comparison itself.
fn deep_equal(a: u64, b: u64, depth: u32) -> bool {
    if a == b {
        return true;
    }
    // `a == b` above already caught two nulls; reaching here with either one
    // `null` means the other is not, and `typeof null` reporting `"object"`
    // would otherwise let it fall into the structural branch and compare equal
    // to `{}` by both having no keys.
    if a == entry::null_value() || b == entry::null_value() {
        return false;
    }
    let ta = kind_of(a);
    if ta != kind_of(b) {
        return false;
    }
    match ta.as_str() {
        // The language's own `===`, not a comparison of the text `described`
        // would coerce out of them: two values that are not the same thing can
        // share a description, and `===` is exactly the relation
        // `deepStrictEqual` is defined to use on a primitive.
        "string" | "boolean" | "symbol" => entry::strict_equals(a, b),
        // Not bit equality: `NaN !== NaN` in the language but
        // `isDeepStrictEqual(NaN, NaN)` is true, and `0 === -0` while the two
        // are NOT deep-strict-equal. Both are `Object.is`, which is what this
        // pair of comparisons spells.
        "number" => match (entry::number_of(a), entry::number_of(b)) {
            (Some(left), Some(right)) => {
                (left.is_nan() && right.is_nan()) || (left == right && left.is_sign_negative() == right.is_sign_negative())
            }
            _ => false,
        },
        "bigint" => entry::described(a) == entry::described(b),
        "undefined" => true,
        // Two functions are equal only by identity, which `a == b` already
        // decided.
        "function" => false,
        _ => {
            if depth == 0 {
                return false;
            }
            // An array and a plain object with the same keys are NOT equal, and
            // nothing below would have noticed: both walk the same key list.
            if entry::is_array(a) != entry::is_array(b) {
                return false;
            }
            let mut keys_a = own_key_strings(a);
            let mut keys_b = own_key_strings(b);
            keys_a.sort();
            keys_b.sort();
            if keys_a != keys_b {
                return false;
            }
            keys_a.iter().all(|key| deep_equal(get(a, key), get(b, key), depth - 1))
        }
    }
}

/// `util.toUSVString(string)`.
///
/// # Why this is close to the identity function here
///
/// A lone surrogate cannot reach this native: the host's text is UTF-8 and
/// `string_in` answers nothing for a string holding one, so the input is either
/// already a valid scalar-value sequence — which this must return unchanged —
/// or unreadable. The unreadable case answers `undefined`, which a program can
/// see, rather than the replacement-character string the specification would
/// have produced and this cannot construct from bytes it never got.
pub(super) extern "C" fn to_usv_string(
    _e: u64,
    _this: u64,
    value: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    match string_of(value) {
        Some(text) => super::values::string(&text),
        None => entry::undefined_value(),
    }
}
