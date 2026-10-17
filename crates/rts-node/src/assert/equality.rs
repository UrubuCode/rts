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

use super::values::{global_member, invoke, is_callable, kind_of, member, own_key_strings, string_of};
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
    // Typed arrays and Buffer instances expose their indexed elements through
    // internal view state rather than enumerable properties. Compare that state
    // directly so two equal slices do not depend on the underlying ArrayBuffer
    // identity or on the implementation-only shape fields.
    let (actual_bytes, expected_bytes) = entry::with_runtime(|context| {
        (entry::bytes_of(context, actual), entry::bytes_of(context, expected))
    });
    match (actual_bytes, expected_bytes) {
        (Some(left), Some(right)) => return left == right,
        (Some(_), None) | (None, Some(_)) => return false,
        (None, None) => {}
    }
    if identity_only(actual) || identity_only(expected) {
        return false;
    }
    if !time_equal(actual, expected) || !pattern_equal(actual, expected) {
        return false;
    }
    // A boxed primitive (`new Number(1)`) keeps its value in an internal slot.
    match (entry::boxed_primitive(actual), entry::boxed_primitive(expected)) {
        (Some(left), Some(right)) => {
            if !walk(left, right, mode, seen) {
                return false;
            }
        }
        (None, None) => {}
        _ => return false,
    }
    seen.push((actual, expected));
    let answer = collection_equal(actual, expected, mode, seen)
        && error_equal(actual, expected, mode, seen)
        && (!mode.strict || symbols_equal(actual, expected, mode, seen))
        && members_equal(actual, expected, mode, seen);
    seen.pop();
    answer
}

/// Whether a value is one this walker refuses to compare structurally.
///
/// # Why a refusal and not a comparison
///
/// A `WeakMap`'s and `WeakSet`'s entries are not enumerable by design, and a
/// `Promise` has no comparable state, so identity IS Node's answer for all
/// three and for any thenable. `Map` and `Set` are NOT here: their entries are
/// internal slots, but `Array.from` reads them out ([`collection_equal`]), so
/// they are compared structurally and order-insensitively as Node does.
///
/// Duck-typed because the host surface exposes no class identity: a `then`
/// method (thenables, `Promise`), or `has` **and** `delete` together on
/// something that is neither a `Map` nor a `Set`. A plain object carrying that
/// pair is caught too, and that is named in the module's refusal list.
fn identity_only(value: u64) -> bool {
    if is_callable(member(value, "then")) {
        return true;
    }
    collection_tag(value).is_none()
        && is_callable(member(value, "has"))
        && is_callable(member(value, "delete"))
}

/// Which keyed collection a value is, by `Object.prototype.toString`.
fn collection_tag(value: u64) -> Option<&'static str> {
    let object = global_member("Object");
    let to_string = member(member(object, "prototype"), "toString");
    if !is_callable(to_string) {
        return None;
    }
    let absent = entry::undefined_value();
    let tag = string_of(entry::call(to_string, value, absent, absent, absent, absent))?;
    match tag.as_str() {
        "[object Map]" => Some("Map"),
        "[object Set]" => Some("Set"),
        _ => None,
    }
}

/// `Array.from(collection)` as a vector — a `Set`'s items, or a `Map`'s
/// `[key, value]` pairs.
fn items_of(collection: u64) -> Vec<u64> {
    let array = global_member("Array");
    let from = member(array, "from");
    let absent = entry::undefined_value();
    let list = entry::call(from, array, collection, absent, absent, absent);
    let length = entry::number_of(member(list, "length")).map(|n| n as usize).unwrap_or(0);
    (0..length)
        .map(|index| entry::get_indexed(list, entry::make_number(index as f64)))
        .collect()
}

/// The `Map` / `Set` rule: same kind, same size, and every item of `actual`
/// matched to a DISTINCT item of `expected` by structural equality — position
/// is not part of the answer, and object items / object keys recurse.
///
/// Greedy matching is exact here because the comparison is an equivalence
/// relation: an item that equals one candidate equals the same set of them.
fn collection_equal(actual: u64, expected: u64, mode: Mode, seen: &mut Vec<(u64, u64)>) -> bool {
    let (left, right) = (collection_tag(actual), collection_tag(expected));
    if left != right {
        return false;
    }
    let Some(kind) = left else {
        return true;
    };
    let mine = items_of(actual);
    let theirs = items_of(expected);
    if mine.len() != theirs.len() {
        return false;
    }
    let mut taken = vec![false; theirs.len()];
    for item in &mine {
        let found = (0..theirs.len()).find(|&at| {
            if taken[at] {
                return false;
            }
            match kind {
                "Map" => {
                    walk(member(*item, "0"), member(theirs[at], "0"), mode, seen)
                        && walk(member(*item, "1"), member(theirs[at], "1"), mode, seen)
                }
                _ => walk(*item, theirs[at], mode, seen),
            }
        });
        match found {
            Some(at) => taken[at] = true,
            None => return false,
        }
    }
    true
}

/// The own enumerable symbol keys of a value.
fn symbols_of(value: u64) -> Vec<u64> {
    let object = global_member("Object");
    let get = member(object, "getOwnPropertySymbols");
    if !is_callable(get) {
        return Vec::new();
    }
    let absent = entry::undefined_value();
    let list = entry::call(get, object, value, absent, absent, absent);
    let length = entry::number_of(member(list, "length")).map(|n| n as usize).unwrap_or(0);
    let enumerable = member(member(object, "prototype"), "propertyIsEnumerable");
    (0..length)
        .map(|index| entry::get_indexed(list, entry::make_number(index as f64)))
        .filter(|symbol| {
            entry::to_boolean(entry::call(enumerable, value, *symbol, absent, absent, absent))
        })
        .collect()
}

/// The symbol-keyed rule: every enumerable own symbol of `expected` is on
/// `actual` with an equal value, and (unless partial) there are no others.
fn symbols_equal(actual: u64, expected: u64, mode: Mode, seen: &mut Vec<(u64, u64)>) -> bool {
    let wanted = symbols_of(expected);
    let held = symbols_of(actual);
    if !mode.partial && wanted.len() != held.len() {
        return false;
    }
    wanted.iter().all(|symbol| {
        held.iter().any(|own| entry::strict_equals(*own, *symbol))
            && walk(
                entry::get_indexed(actual, *symbol),
                entry::get_indexed(expected, *symbol),
                mode,
                seen,
            )
    })
}

/// The `Error` rule: `name`, `message`, `cause` and `errors` are
/// non-enumerable, so the key walk never sees them; Node compares them anyway.
fn error_equal(actual: u64, expected: u64, mode: Mode, seen: &mut Vec<(u64, u64)>) -> bool {
    let class = global_member("Error");
    if !is_callable(class) {
        return true;
    }
    let (left, right) = (entry::instance_of(actual, class), entry::instance_of(expected, class));
    if left != right {
        return false;
    }
    if !left {
        return true;
    }
    ["message", "name"]
        .iter()
        .all(|key| same_value(member(actual, key), member(expected, key)))
        && ["cause", "errors"]
            .iter()
            .all(|key| walk(member(actual, key), member(expected, key), mode, seen))
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
