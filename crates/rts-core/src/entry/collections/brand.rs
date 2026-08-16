//! Which class a table belongs to, and the refusals that follow from it.
//!
//! # Why the brand is a module rather than three lines in `mod.rs`
//!
//! Because it is one decision with four call sites of its own — the check, the
//! name-to-brand mapping, the `new` requirement and the two `size` getters —
//! and `mod.rs` was at its 500-line ceiling before any of them existed. Keeping
//! it here also puts the whole answer to "how does this engine tell a `Set` from
//! a `Map`" in one file, which is the question a reader arrives with.
//!
//! [`super::table::Brand`] is the value itself; it lives beside the table
//! because the table IS the internal slot and the two must not be settable
//! apart.

use super::{Brand, Table, with_current};
use crate::value::Value;

/// The cell of a receiver that really is one of these collections, of the brand
/// asked for — and the `TypeError` the language raises when it is not.
///
/// # Why this exists now and did not before
///
/// Every method used to answer for a foreign receiver rather than refuse:
/// `Map.prototype.get.call({})` was `undefined`, `Set.prototype.has.call(map)`
/// read the map's table, and `Map.prototype.get.call(set)` answered a `Set`'s
/// member as a value. The comment above `super::sized` called that "the
/// tolerance every refusal in this module settles on **while the brand check has
/// no handler to reach**" — and that clause expired: a native raises a catchable
/// error now, under rule 8 of the crate's README, so the check has somewhere to
/// go.
///
/// Refusing matters beyond conformance. A `Set` stores each member as both key
/// and value precisely so one table type serves four classes, which means a
/// `Map` method reading a `Set`'s table gets a plausible answer rather than a
/// wrong-looking one — the expensive kind.
///
/// The borrow is taken and given back before the raise, because building the
/// error object takes one of its own.
pub(in crate::entry) fn branded(this: u64, brand: Brand) -> Option<u32> {
    let cell = with_current(|context| {
        let cell = Value(this).as_slot()?;
        (context.table_at(cell)?.brand() == brand).then_some(cell)
    });
    if cell.is_none() {
        let named = brand.named();
        crate::entry::throw::type_error(&format!(
            "a {named} method needs a {named} — the receiver has no such internal slot"
        ));
    }
    cell
}

/// Whether the constructor was reached with `new`, raising if it was not.
///
/// # Why a plain call IS refused now
///
/// `Map()` without `new` is a `TypeError`, and `super::built` used to make an
/// object anyway — the note there said raising "would end the program", because
/// `crate::entry::throw` had no handler to reach. That is no longer true (rule 8
/// of the crate's README), so the four collection constructors refuse the way
/// the language does. `Error("x")` keeps the old tolerance because the language
/// genuinely allows it; these four do not.
///
/// Asked BEFORE the borrow a constructor's body takes, and before the argument
/// is read: the order is observable — a fixture asserts that
/// `Map.call(undefined, iterable)` throws without touching the iterable.
pub(in crate::entry) fn requires_new(this: u64, class: &str) -> bool {
    if Value(this).as_slot().is_some() {
        return true;
    }
    crate::entry::throw::type_error(&format!("constructor {class} requires 'new'"));
    false
}

/// The brand a class name stands for.
///
/// Derived from the name the caller already passes rather than added as a second
/// argument at every call site: `super::fresh` and `super::built` both take the
/// class so they can find its prototype, and a second parameter saying the same
/// thing is the pair that comes to disagree.
pub(super) fn of_class(class: &str) -> Brand {
    match class {
        "Set" => Brand::Set,
        "WeakMap" => Brand::WeakMap,
        "WeakSet" => Brand::WeakSet,
        "Map" => Brand::Map,
        // `WeakRef` and `FinalizationRegistry` keep a table without being a
        // collection any of the four sets of methods walks.
        _ => Brand::Other,
    }
}

/// The getter `size` installs on a class's prototype.
///
/// TWO of them rather than one, and that is the brand showing up where it is
/// least obvious: `Map.prototype` and `Set.prototype` each own an accessor of
/// their own in the language, and
/// `Object.getOwnPropertyDescriptor(Map.prototype, "size").get.call(new Set())`
/// is a `TypeError`. One shared getter answered the count instead — the
/// plausible wrong answer [`branded`] exists to stop.
pub(super) fn size_getter(class: &str) -> crate::entry::native::Native {
    match of_class(class) {
        Brand::Set => set_size as crate::entry::native::Native,
        _ => map_size as crate::entry::native::Native,
    }
}

/// `m.size`.
extern "C" fn map_size(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    counted(this, Brand::Map)
}

/// `s.size`.
extern "C" fn set_size(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    counted(this, Brand::Set)
}

/// The shared body of both, once the brand is decided.
fn counted(this: u64, brand: Brand) -> u64 {
    let Some(cell) = branded(this, brand) else {
        return super::undefined();
    };
    with_current(
        |context| match context.table_at(cell).map(Table::len) {
            Some(count) => Value::from_f64(count as f64).bits(),
            None => crate::entry::objects::undefined_of(context),
        },
    )
}
