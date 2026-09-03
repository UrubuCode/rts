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
pub(super) extern "C" fn to_be(_e: u64, this: u64, expected: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = super::received_of(this);
    let held = rts_core::entry::same_value(received, expected);
    super::settle(super::negate_if(this, held), super::is_negated(this), received, expected)
}

/// The recursion in [`to_equal`] refuses to go deeper than this — the same
/// floor `node:util`'s `isDeepStrictEqual` uses
/// (`crates/rts-node/src/util/equal.rs`, `EQUAL_DEPTH`), for the identical
/// reason stated there: a cyclic object is legal input to
/// `expect(x).toEqual(x)`, and a walk with no floor recurses on the native
/// stack rather than the program's.
const EQUAL_DEPTH: u32 = 32;

/// `expect(a).toEqual(b)` — Jest's DEEP structural equality.
///
/// # What this used to be, and why that was the wrong direction to be wrong in
///
/// This was registered as the identical function as [`to_be`] — `SameValue`,
/// which is reference identity for anything that is not a primitive. The
/// module doc that stood here argued the risk ran one way: a deep comparison
/// that ran on identity would report passes it did not earn. That is
/// backwards. Two distinct arrays or objects are NEVER `SameValue`-equal to
/// each other, so identity standing in for `toEqual` does not pass things it
/// should not — it FAILS every one of them, unconditionally.
/// `expect([3,4,5]).toEqual([3,4,5])` failed on every pair of arrays or
/// objects this repository ever built, for as long as this line stood, which
/// is what a fixture in this round had to route around with a manual
/// `JSON.stringify` comparison to see its own real findings.
///
/// # The comparison itself
///
/// Structural and recursive: `Object.is` on every primitive (so
/// `toEqual(NaN)` matches `NaN` and `toEqual(-0)` does not match `0`, same as
/// [`to_be`]); an array against an array, index by index, and length must
/// agree; an object against an object, key by key. A property whose value is
/// `undefined` is skipped on BOTH sides before the key sets are compared, so
/// `{a:1}` equals `{a:1,b:undefined}` — this is Jest's own stated rule for
/// `toEqual` (`toStrictEqual` is the matcher that keeps the distinction).
///
/// # Where this logic already exists once, and why it is written a second
/// time here rather than shared
///
/// `node:util`'s `isDeepStrictEqual` (`crates/rts-node/src/util/equal.rs`) is
/// the same shape of walk over the same primitives —
/// `same_value`/`is_object`/`is_array`/`own_keys`/member access — and
/// `rts-std` cannot depend on `rts-node` to reach it (the dependency runs the
/// other way: `node:test` itself would have to live in `rts-node`, and
/// `rts:test` does not). The two are not one algorithm wearing two names,
/// though: Node's `assert.deepStrictEqual` treats `{a:undefined}` and `{}` as
/// UNEQUAL — an `undefined` property is a property there — while Jest's
/// `toEqual` treats them as equal, which is the rule this function follows.
/// A shared skeleton parameterised by that one policy difference belongs in
/// `rts-core`, where both `rts-std` and `rts-node` could reach it without
/// creating a dependency between them — that move is proposed, not made
/// here: it touches a crate every other engine agent is also editing this
/// round, for a payoff (removing one ~30-line recursion) too small to justify
/// the risk without agreement first.
pub(super) extern "C" fn to_equal(_e: u64, this: u64, expected: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = super::received_of(this);
    let held = deep_equal(received, expected, EQUAL_DEPTH);
    super::settle(super::negate_if(this, held), super::is_negated(this), received, expected)
}

/// The recursive walk [`to_equal`] runs. See its doc for the rule; this is the
/// mechanism.
fn deep_equal(a: u64, b: u64, depth: u32) -> bool {
    if rts_core::entry::same_value(a, b) {
        return true;
    }
    // `same_value` already separated every primitive pair that was not
    // literally the same value, INCLUDING two of different `typeof` (a number
    // is never `same_value` to a string, etc) and `null`/`undefined` against
    // anything else. What is left here that can still be equal is two
    // DIFFERENT objects with the same shape — so anything that is not an
    // object at this point (a value that reached here only because it was not
    // equal to the other side) can never become equal by recursing.
    //
    // # Why `is_object` alone is not that question, and it was measured
    //
    // [`rts_core::entry::is_object`] answers whether a value occupies a slot
    // in the region, and its own doc says what it is for: telling `new C()`
    // from `C()`. A primitive string and a function each occupy a slot, so
    // each passes it — and each then offers an EMPTY enumerable key list, so
    // the walk below declared them equal to one another and to `{}`. Measured
    // before this guard existed: `expect("42").not.toEqual("43")` failed, and
    // so did `expect(f).not.toEqual(g)` for two distinct functions. Both are
    // pairs `same_value` had already separated CORRECTLY one line above, so
    // the walk was turning a right answer into a wrong one — which is the one
    // direction a deep comparison must never fail in.
    //
    // The exclusions use the narrow predicates rather than a `typeof` spelled
    // again here: `utf8_bytes_if_string` refuses a non-string by construction
    // (it does not coerce, unlike `text_of`) and `is_callable_in` is the same
    // question `type_of` itself asks. A second spelling of the nine type
    // names is the drift `TYPE_NAMES` exists one crate down to prevent.
    let walkable = rts_core::entry::with_runtime(|context| {
        [a, b].iter().all(|&v| {
            rts_core::entry::is_object(context, v) && !rts_core::entry::is_callable_in(context, v)
        })
    }) && rts_core::entry::utf8_bytes_if_string(a).is_none()
        && rts_core::entry::utf8_bytes_if_string(b).is_none();
    if !walkable || depth == 0 {
        return false;
    }
    // An array and a plain object with the same keys are not `toEqual`, and
    // the generic key walk below would not have noticed on its own — both
    // would offer the identical enumerable key list.
    let array_a = rts_core::entry::is_array(a);
    if array_a != rts_core::entry::is_array(b) {
        return false;
    }
    if array_a {
        // Elements only, through `element_at` — NOT `defined_keys`/`member`
        // below, which read a NAME-keyed property (`get_member`) and answer
        // `undefined` for an array's own indices in this engine (they live in
        // a separate elements store; `rts-node`'s own `util::values::get` doc
        // states the identical split). A named property added onto an array
        // (`arr.extra = 1`) is therefore NOT compared — a stated gap, not a
        // silent one: this corpus's `toEqual` calls are over plain arrays,
        // and Jest's own array handling is index-and-length besides.
        let length = rts_core::entry::array_length(a);
        if length != rts_core::entry::array_length(b) {
            return false;
        }
        let mut at = 0.0;
        while at < length {
            let index = rts_core::entry::make_number(at);
            let left = rts_core::entry::element_at(a, index);
            let right = rts_core::entry::element_at(b, index);
            if !deep_equal(left, right, depth - 1) {
                return false;
            }
            at += 1.0;
        }
        return true;
    }
    let mut keys_a = defined_keys(a);
    let mut keys_b = defined_keys(b);
    keys_a.sort();
    keys_b.sort();
    if keys_a != keys_b {
        return false;
    }
    keys_a
        .iter()
        .all(|key| deep_equal(member(a, key), member(b, key), depth - 1))
}

/// An object's own enumerable keys, minus the ones whose value is `undefined`
/// — the filter that makes `{a:1}` and `{a:1,b:undefined}` compare equal. Read
/// straight off `object` rather than the array `own_keys` answers, because a
/// symbol key `described` cannot round-trip through `member` the way a string
/// one does; this corpus's `toEqual` calls are over string-keyed objects and
/// arrays, and a symbol-keyed one is a gap named here rather than guessed at.
fn defined_keys(object: u64) -> Vec<String> {
    let raw = rts_core::entry::own_keys(object);
    let length = rts_core::entry::array_length(raw);
    let mut keys = Vec::new();
    let mut at = 0.0;
    while at < length {
        let index = rts_core::entry::make_number(at);
        let item = rts_core::entry::element_at(raw, index);
        at += 1.0;
        let Some(name) = rts_core::entry::described(item) else {
            continue;
        };
        if member(object, &name) != rts_core::entry::undefined_value() {
            keys.push(name);
        }
    }
    keys
}

/// One named property, from outside a borrow — the shape every ambient reader
/// in this crate's `node:` sibling uses (`rts-node`'s `util::values::get`),
/// kept here rather than shared for the same reason [`to_equal`]'s doc gives.
fn member(object: u64, key: &str) -> u64 {
    rts_core::entry::with_runtime(|context| rts_core::entry::get_member(context, object, key))
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
