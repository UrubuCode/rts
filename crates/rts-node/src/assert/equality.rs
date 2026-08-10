//! The one comparison walker every `deep*Equal` in this module is a mode of.
//!
//! # Why one walker and not five functions
//!
//! Because Node's five — `deepEqual`, `notDeepEqual`, `deepStrictEqual`,
//! `notDeepStrictEqual`, `partialDeepStrictEqual` — differ in exactly three
//! bits: which leaf comparison, whether `actual` may carry extra keys, and
//! whether `[[Prototype]]` is compared. Five walks would be five places for the
//! cycle guard to be got wrong, and the previous version of this module had the
//! guard wrong in the only walk it had (a depth cap, which turns a deep
//! structure into a wrong answer instead of a hang).
//!
//! # What the reuse search found
//!
//! `node:util`'s `isDeepStrictEqual` has a walker of the same shape in
//! `util.rs`, and `rts_core::entry` exports none. That walker is private to
//! `node:util` and is NOT this one: it compares leaves by their rendered text
//! (so `1` and `"1"` compare equal), caps depth at 32 rather than guarding
//! cycles, and never looks at a prototype. Calling it would make
//! `deepStrictEqual` weaker than it is here, and widening its visibility is
//! another module's file. Named as the near miss rather than silently
//! duplicated: if `node:util` ever needs real `Object.is` leaves, this is the
//! walker it should reach for and `values.rs` the seam.

use super::values::{invoke, is_callable, kind_of, member, own_key_strings, string_of};
use rts_core::entry;

/// Which of Node's five comparisons a walk is.
#[derive(Clone, Copy)]
pub(super) struct Mode {
    /// `Object.is` leaves rather than `==` leaves.
    pub strict: bool,
    /// Only `expected`'s own keys are checked; `actual` may carry more.
    pub partial: bool,
    /// `[[Prototype]]` must match.
    pub prototypes: bool,
}

/// `deepStrictEqual` / `notDeepStrictEqual`.
pub(super) const STRICT: Mode = Mode { strict: true, partial: false, prototypes: true };

/// `deepStrictEqual` under `new Assert({ skipPrototype: true })`.
pub(super) const STRICT_OPEN: Mode = Mode { strict: true, partial: false, prototypes: false };

/// Legacy `deepEqual` / `notDeepEqual` — no prototype comparison, by Node's
/// own definition rather than as a shortcut.
pub(super) const LOOSE: Mode = Mode { strict: false, partial: false, prototypes: false };

/// `partialDeepStrictEqual` — strict leaves, subset keys, and Node never
/// compares `[[Prototype]]` here even without `skipPrototype`.
pub(super) const PARTIAL: Mode = Mode { strict: true, partial: true, prototypes: false };

/// `Object.is(a, b)`.
///
/// # Why this is not `strict_equals`
///
/// `strict_equals` is `===`, and `===` disagrees with `Object.is` on exactly
/// the two values assertions are most often written about: `NaN === NaN` is
/// false where `Object.is(NaN, NaN)` is true, and `0 === -0` is true where
/// `Object.is(0, -0)` is false. The previous module documented
/// `deepStrictEqual(0, -0)` wrongly passing as a known divergence; nothing on
/// the host surface was missing, only the four lines below.
///
/// [`entry::number_of`] is a type test and not a coercion — it answers `None`
/// for a string, a boolean and an object alike — so this cannot mistake `"1"`
/// for `1` the way a `text_of` comparison would.
pub(super) fn same_value(a: u64, b: u64) -> bool {
    if let (Some(left), Some(right)) = (entry::number_of(a), entry::number_of(b)) {
        return left.to_bits() == right.to_bits() || (left.is_nan() && right.is_nan());
    }
    entry::strict_equals(a, b)
}

/// Legacy `==`, with the one deviation Node documents: `NaN` equals `NaN`.
pub(super) fn loose_equal(a: u64, b: u64) -> bool {
    if let (Some(left), Some(right)) = (entry::number_of(a), entry::number_of(b)) {
        if left.is_nan() && right.is_nan() {
            return true;
        }
    }
    entry::loose_equals(a, b)
}

/// The entry point the five `deep*Equal` checks share.
pub(super) fn deep_equal(actual: u64, expected: u64, mode: Mode) -> bool {
    walk(actual, expected, mode, &mut Vec::new())
}

/// One level of the comparison.
///
/// `seen` holds the `(actual, expected)` pairs currently on the path. Meeting a
/// pair again is a cycle, and Node's rule since v24 is to stop and treat it as
/// equal-so-far rather than to recurse or to throw — which also means a
/// structure can be arbitrarily deep without a cap, so the depth limit here is
/// the native stack, exactly as it is in Node.
fn walk(actual: u64, expected: u64, mode: Mode, seen: &mut Vec<(u64, u64)>) -> bool {
    let leaves = match mode.strict {
        true => same_value(actual, expected),
        false => loose_equal(actual, expected),
    };
    if leaves {
        return true;
    }
    // `null`'s `typeof` is `"object"`, so without this it would reach the
    // structural branch and compare equal to `{}` — both have no keys.
    let null = entry::null_value();
    if actual == null || expected == null {
        return false;
    }
    if kind_of(actual) != "object" || kind_of(expected) != "object" {
        // Two functions are equal only by identity, which the leaf test above
        // already decided; so is a primitive that failed it.
        return false;
    }
    if seen.iter().any(|(left, right)| *left == actual && *right == expected) {
        return true;
    }
    if mode.prototypes && entry::get_prototype(actual) != entry::get_prototype(expected) {
        return false;
    }
    if identity_only(actual) || identity_only(expected) {
        return false;
    }
    if !time_equal(actual, expected) || !pattern_equal(actual, expected) {
        return false;
    }
    seen.push((actual, expected));
    let answer = members_equal(actual, expected, mode, seen);
    seen.pop();
    answer
}

/// Whether a value is one this walker refuses to compare structurally.
///
/// # Why a refusal and not a comparison
///
/// A `Map`'s entries and a `Set`'s items are internal slots: they are not own
/// enumerable properties, so the key walk below sees NOTHING in them and every
/// pair of `Map`s compares equal — a wrong answer that runs, and the exact
/// defect class this repository refuses. Comparing them properly needs to
/// iterate them, and iterating means calling user-reachable methods for each
/// entry from inside a comparison; the honest stopping point is to compare by
/// identity and say so.
///
/// For `Promise`, `WeakMap` and `WeakSet` identity IS Node's answer, so those
/// are correct rather than conservative. For `Map`, `Set` and anything else
/// caught by the duck test this is a false NEGATIVE — an assertion that should
/// pass reports a failure — which is loud, unlike the silent pass it replaces.
///
/// Duck-typed because the host surface exposes no class identity: a `then`
/// method (thenables, `Promise`), or `has` **and** `delete` together (`Map`,
/// `Set`, `WeakMap`, `WeakSet`). A plain object carrying that pair is caught
/// too, and that is named in the module's refusal list.
fn identity_only(value: u64) -> bool {
    if is_callable(member(value, "then")) {
        return true;
    }
    is_callable(member(value, "has")) && is_callable(member(value, "delete"))
}

/// The `Date` rule, applied when either side looks like one.
///
/// `getTime()` compared with [`same_value`], which gives Node 25's
/// invalid-Date-equals-invalid-Date behavior for free: both answer `NaN`, and
/// `Object.is(NaN, NaN)` is true.
///
/// Additive rather than terminal — a value passing this still has its own
/// properties compared below — so a plain object that happens to carry a
/// `getTime` method can only be judged MORE unequal by it, never wrongly equal.
fn time_equal(actual: u64, expected: u64) -> bool {
    if !is_callable(member(actual, "getTime")) && !is_callable(member(expected, "getTime")) {
        return true;
    }
    same_value(invoke(actual, "getTime"), invoke(expected, "getTime"))
}

/// The `RegExp` rule: `source`, `flags` and `lastIndex`, the last of which Node
/// has compared since v18. Additive for the same reason [`time_equal`] is.
fn pattern_equal(actual: u64, expected: u64) -> bool {
    if string_of(member(actual, "source")).is_none() && string_of(member(expected, "source")).is_none() {
        return true;
    }
    ["source", "flags", "lastIndex"]
        .iter()
        .all(|name| same_value(member(actual, name), member(expected, name)))
}

/// The own-enumerable-property comparison, which is where the mode's `partial`
/// bit is spent.
fn members_equal(actual: u64, expected: u64, mode: Mode, seen: &mut Vec<(u64, u64)>) -> bool {
    let mut expected_keys = own_key_strings(expected);
    if !mode.partial {
        let mut actual_keys = own_key_strings(actual);
        actual_keys.sort();
        expected_keys.sort();
        if actual_keys != expected_keys {
            return false;
        }
    }
    expected_keys
        .iter()
        .all(|key| walk(member(actual, key), member(expected, key), mode, seen))
}
