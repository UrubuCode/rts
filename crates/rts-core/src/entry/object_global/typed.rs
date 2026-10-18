//! `[[DefineOwnProperty]]` where the receiver is a **typed array**.
//!
//! An integer-indexed exotic object answers for every CanonicalNumericIndexString
//! itself (`"0"`, `"5"`, `"-1"`, `"1.5"`, `"-0"`, `"NaN"`, `"Infinity"`):
//!
//! - not a valid in-range element: refused (`Object.defineProperty` raises
//!   "Invalid typed array index", `Reflect.defineProperty` answers `false`);
//! - an element: refused when the descriptor asks for `configurable: false`,
//!   `enumerable: false`, `writable: false` or an accessor — an element is
//!   always a writable, enumerable, configurable data property — and otherwise
//!   a `value` is stored through the element coercion.
//!
//! Any other key (`"01"`, `"tag"`, a symbol) is an ordinary property and falls
//! through to the generic definition.

use super::super::buffers;
use super::super::with_current;
use super::descriptor::{Descriptor, Verdict};
use crate::value::Value;

pub(super) fn define(object: u64, name: u64, wanted: &Descriptor) -> Option<Verdict> {
    // `None` (not a typed array / not an element key), or the element state:
    // `Some(in_range)`.
    let found = with_current(|context| {
        let cell = Value(object).as_slot()?;
        if !buffers::is_element_key(context, cell, Value(name)) {
            return None;
        }
        let at = super::super::array::as_index(context, Value(name))?;
        let count = context.view_at(cell).map_or(0, |view| view.count());
        Some(at < count)
    });
    let in_range = match found {
        None => {
            // A canonical key that is not even an index (negative, fractional,
            // "-0", NaN, Infinity) is still the view's: refuse it.
            let canonical = with_current(|context| {
                Value(object)
                    .as_slot()
                    .is_some_and(|cell| buffers::is_element_key(context, cell, Value(name)))
            });
            return canonical.then_some(Verdict::BadIndex);
        }
        Some(in_range) => in_range,
    };
    if !in_range {
        return Some(Verdict::BadIndex);
    }
    if wanted.configurable == Some(false)
        || wanted.enumerable == Some(false)
        || wanted.writable == Some(false)
        || wanted.get.is_some()
        || wanted.set.is_some()
    {
        return Some(Verdict::Refused);
    }
    let Some(stated) = wanted.value else {
        return Some(Verdict::Done);
    };
    // The coercion can run user code, so outside every borrow; a throw leaves
    // the element alone.
    let kind = buffers::store_kind(object)?;
    let Some(coerced) = buffers::coerce_for_store(stated, kind) else {
        return Some(Verdict::Done);
    };
    let mut refusal: Option<String> = None;
    with_current(|context| {
        let cell = Value(object).as_slot()?;
        buffers::indexed_set(context, cell, Value(name), coerced, &mut refusal);
        Some(())
    });
    if let Some(why) = refusal {
        super::super::throw::type_error(&why);
    }
    Some(Verdict::Done)
}
