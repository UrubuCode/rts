//! Where a bigint's digits live.
//!
//! # Why a primitive keeps its data on the heap
//!
//! A bigint is arbitrary precision, and forty-eight bits of payload is not
//! arbitrary. So the payload names a slot and the digits live in it — which is
//! what every engine does, and which does not make a bigint an object: `typeof`
//! answers `"bigint"` from the tag, `1n === 1n` is true because equality compares
//! the DIGITS rather than the slot, and nothing can be written to one.
//!
//! That last pair is the whole difference from a cell. A string is the same
//! shape one level down — two cells, equal text, `===` true — and this is that
//! rule applied to a value that is not a reference at all.
//!
//! # Why the slab is never freed
//!
//! Nothing collects yet, so a program that computes a million bigints keeps a
//! million entries. The same bet arrays and every `Aside` here already make, and
//! the note the eventual collector needs: this table is reachable only through
//! values, so tracing it means reading the words.

use super::Context;
use crate::bigint::BigInt;
use crate::heap::Slot;
use crate::value::Value;

impl Context {
    /// The digits a payload names, if the slab still holds them.
    pub(super) fn bigint_at(&self, payload: u64) -> Option<&BigInt> {
        self.bigints.at(Slot(payload as u32)).ok()
    }

    /// Puts digits on the heap and answers the primitive naming them.
    pub(super) fn bigint_value(&mut self, held: BigInt) -> u64 {
        let slot = self.bigints.insert(held).slot();
        Value::from_client(self.kinds.bigint, u64::from(slot.0)).bits()
    }
}

/// The digits of a value, when it is a bigint.
pub(super) fn digits_of(context: &Context, value: u64) -> Option<&BigInt> {
    context.bigint_at(Value(value).as_client(context.kinds.bigint)?)
}

/// Whether two values are the same bigint.
///
/// **By value, not by slot.** `1n === 1n` is true and two separately computed
/// bigints live in different slots, so comparing payloads would answer false for
/// the one case this exists to get right — exactly the mistake comparing two
/// string cells by index would be.
pub(super) fn same(context: &Context, left: u64, right: u64) -> bool {
    match (digits_of(context, left), digits_of(context, right)) {
        (Some(a), Some(b)) => a.cmp(b) == core::cmp::Ordering::Equal,
        _ => false,
    }
}
