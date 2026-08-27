//! The bounded repetition primitive for JavaScript strings.
//!
//! The implementation lives apart from the other basic methods because the
//! string module has a 500-line ceiling. Its result limit is shared with the
//! public `node:buffer` string constant through `entry` rather than copied.

use super::super::with_current;
use super::{answer, nothing, units_of};

/// `s.repeat(n)`.
pub(super) extern "C" fn repeat(
    _e: u64,
    this: u64,
    count: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    // `ToString(RequireObjectCoercible(this))`, before any borrow — see
    // `super::coerce_receiver`.
    let Some(this) = super::coerce_receiver(this) else {
        return super::refused();
    };
    // The count AFTER the receiver, which is the order the specification states
    // and the order a throwing `toString` observes — see the `order` rows of
    // `string/claude2-repeat-count-coercion-and-errors`.
    let Some(count) = super::number_arg(count) else {
        return super::refused();
    };
    let asked = with_current(|context| super::integer_arg(context, count));
    // A negative count, or one that never terminates, is a `RangeError` —
    // raising is possible now that rule 8's discipline is in place (`repeat`
    // calls no user code, so there is nothing to check first). Raised OUTSIDE
    // the borrow below: `range_error` takes the context's own `RefCell` borrow
    // to build the error object, and taking it a second time while the
    // first is still held is the nested borrow this crate's whole entry layer
    // is arranged to make unreachable — an `extern "C"` frame cannot unwind
    // the panic that follows, so it aborted the process instead of throwing.
    if asked < 0.0 || asked.is_infinite() {
        super::super::throw::range_error("Invalid count value");
        return with_current(|context| nothing(context));
    }
    let Some(units) = with_current(|context| units_of(context, this)) else {
        return super::refused();
    };
    // A result past the length a string can have is the SECOND `RangeError` the
    // specification names, and it was a silent empty string here: `asked > 4096`
    // answered `""` for `"ab".repeat(1e10)`, which every engine refuses. The
    // ceiling was also wrong in the other direction — `"x".repeat(10000)` is an
    // ordinary thing to write and answered nothing.
    //
    // The bound is on the RESULT, not on the count, which is what makes
    // `"".repeat(1e10)` the empty string rather than an error: an empty receiver
    // produces nothing however many times it is repeated, and the fixture asks
    // for exactly that.
    const LIMIT: f64 = super::super::BUFFER_MAX_STRING_LENGTH;
    // An empty receiver answers the empty string for ANY finite count, and the
    // check has to come before the loop rather than fall out of it: repeating
    // nothing 10^10 times is 10^10 iterations of copying nothing, which is a
    // hang and not a wrong answer.
    if units.is_empty() || asked == 0.0 {
        return with_current(|context| answer(context, &[]));
    }
    if asked * units.len() as f64 > LIMIT {
        super::super::throw::range_error("Invalid string length");
        return super::refused();
    }
    with_current(|context| {
        let mut out = Vec::with_capacity(units.len() * asked as usize);
        for _ in 0..asked as usize {
            out.extend_from_slice(&units);
        }
        answer(context, &out)
    })
}
