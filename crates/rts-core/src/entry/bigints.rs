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

use super::{Context, with_current};
use crate::bigint::BigInt;
use crate::heap::Slot;
use crate::value::Value;

impl Context {
    /// The digits a payload names, if the slab still holds them.
    pub(super) fn bigint_at(&self, payload: u64) -> Option<&BigInt> {
        self.bigints.at(Slot(payload as u32)).ok()
    }

    /// Puts digits on the heap and answers the primitive naming them.
    pub(in crate::entry) fn bigint_value(&mut self, held: BigInt) -> u64 {
        let slot = self.bigints.insert(held).slot();
        Value::from_client(self.kinds.bigint, u64::from(slot.0)).bits()
    }
}

/// The digits of a value, when it is a bigint.
pub(super) fn digits_of(context: &Context, value: u64) -> Option<&BigInt> {
    context.bigint_at(Value(value).as_client(context.kinds.bigint)?)
}

/// A bigint from a sign and base-2^64 words.
///
/// The words spelling rather than [`Context::bigint_value`] because that one
/// takes a [`BigInt`], which is this crate's own type: a caller outside it —
/// `rts-napi`, which is the reason this exists — cannot name one, and should
/// not have to depend on the representation to hand over an integer.
pub fn bigint_from_words(negative: bool, words: &[u64]) -> u64 {
    with_current(|context| {
        let held = BigInt::from_words(negative, words);
        context.bigint_value(held)
    })
}

/// The sign and base-2^64 words of a value, when it is a bigint.
///
/// `None` for anything else, which is the caller's `napi_bigint_expected`.
pub fn bigint_words(value: u64) -> Option<(bool, Vec<u64>)> {
    with_current(|context| Some(digits_of(context, value)?.to_words()))
}

/// A bigint as an `i64`, and whether it fitted.
///
/// The pair rather than an `Option` because the ABI asks for both: a value that
/// does not fit is still converted (truncated to the low sixty-four bits) and
/// reported as lossy, so refusing would lose the answer an addon expects.
pub fn bigint_i64(value: u64) -> Option<(i64, bool)> {
    with_current(|context| {
        let held = digits_of(context, value)?;
        Some(match held.as_i64() {
            Some(exact) => (exact, false),
            None => (truncate(held) as i64, true),
        })
    })
}

/// A bigint as a `u64`, and whether it fitted.
///
/// Lossy covers two cases the ABI does not distinguish: too large, and
/// negative. Both are "the `u64` is not this value".
pub fn bigint_u64(value: u64) -> Option<(u64, bool)> {
    with_current(|context| {
        let held = digits_of(context, value)?;
        Some(match held.as_u64() {
            Some(exact) => (exact, false),
            None => (truncate(held), true),
        })
    })
}

/// The low sixty-four bits of a magnitude, negated when the value is.
///
/// Two's complement of the truncation, which is what a C caller assigning a
/// negative bigint to a `uint64_t` gets — the same wrap the language's
/// `BigInt.asUintN(64, x)` performs.
fn truncate(held: &BigInt) -> u64 {
    let (negative, words) = held.to_words();
    let low = words.first().copied().unwrap_or(0);
    match negative {
        true => low.wrapping_neg(),
        false => low,
    }
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
