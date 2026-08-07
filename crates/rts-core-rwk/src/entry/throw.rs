//! A thrown value that no handler in the throwing function caught.
//!
//! # What this used to do, and why it changed
//!
//! It ended the program. The reasoning was sound and its boundary was drawn
//! honestly: the machine computes where a throw lands from the region tree of
//! the function it is IN, which is complete for every handler in that function
//! and says nothing about a caller's — so a `try` protecting a call was refused
//! by name rather than compiled into a `catch` that silently never runs.
//!
//! What that reasoning assumed is that finding a caller's handler means
//! unwinding the native stack, which needs an exception table and a personality
//! routine. It does not. It needs the throw to leave ONE frame and the caller to
//! ask whether it did — and the caller is compiled code that can be made to ask.
//!
//! So a throw is now **recorded** rather than reported, the machine returns from
//! the throwing function instead of trapping, and every call site checks. A
//! throw that reaches a handler is caught by the machine's own region tree,
//! unchanged; a throw that reaches nobody arrives here still pending when the
//! program ends, and [`pending`] is what the host reads to report it exactly as
//! this file used to.
//!
//! # Why one slot and not a stack
//!
//! Because there is one throw in flight. A second cannot start before the first
//! is caught or ends the program — the only code that runs in between is
//! compiled code returning, and returning cannot throw. A stack would be
//! modelling a state the language cannot reach.
//!
//! A cleanup that throws while unwinding IS such a state, and it is named in
//! [`record`]: the second value replaces the first, which is what JavaScript
//! says a `finally` that throws does to the exception it was unwinding.

use super::with_current;

/// Records a thrown value that no handler in the throwing function caught.
///
/// The tag is carried and not interpreted: the machine compares tags for
/// equality and this is the escaping path, where there was nothing to compare
/// against. It is kept so that a value thrown with an unexpected tag can still
/// be reported with the tag it had.
///
/// # Why the second throw wins
///
/// A `finally` that throws while a value is already unwinding replaces it, which
/// is what the language says. So this overwrites rather than refusing, and the
/// alternative — keeping the first — would make a `finally`'s own failure
/// invisible.
#[rtse::entry("rts_throw")]
pub fn throw(tag: i64, payload: u64) {
    with_current(|context| context.thrown = Some((tag, payload)));
}

/// Whether a throw is in flight, as `1` or `0`.
///
/// # Why an integer and not a boolean
///
/// A Rust `bool` is one byte, and compiled code reading it as a word takes the
/// callee's leftover bits — the mistake that once made `===` answer true for two
/// different strings in release and false in debug. Every flag crossing this
/// boundary is an `i64` for that reason.
///
/// # Why this is a call at every call site
///
/// Because a throw has to leave the frame it was raised in, and the frame above
/// only learns that by asking. The alternative is unwinding the native stack
/// with an exception table and a personality routine, which is a campaign; this
/// is a load and a branch. It is not free and it is not measured yet — no claim
/// is made about its cost here, per this repository's rule about performance
/// claims.
#[rtse::entry("__rts_thrown")]
pub fn thrown() -> i64 {
    with_current(|context| i64::from(context.thrown.is_some()))
}

/// Takes the value in flight, clearing it.
///
/// Cleared by the take, because the caller is about to re-raise it: leaving it
/// set would make the re-raise look like a second throw to the next check, and
/// every call after a caught one would appear to be unwinding.
#[rtse::entry("__rts_take_thrown")]
pub fn take_thrown() -> u64 {
    with_current(|context| match context.thrown.take() {
        Some((_, payload)) => payload,
        // Reached only if compiled code asked without asking [`thrown`] first,
        // which the emitter does not do. `undefined` rather than a poison value:
        // there is no honest value here and the caller is about to throw it.
        None => super::objects::undefined_of(context),
    })
}

/// The throw still in flight when a program ended, if there was one.
///
/// Read by the host after the program returns. This is where an uncaught
/// exception is reported, which is what this module did inline until a throw
/// could leave a frame.
pub fn pending() -> Option<(i64, String)> {
    with_current(|context| {
        let (tag, payload) = context.thrown.take()?;
        // Whatever text the value has WITHOUT running user code.
        //
        // `ToPrimitive` on an object calls a `toString` an entry point cannot
        // call, and reaching back into the program that just failed is not worth
        // a diagnostic. But an error object needs neither: `name` and `message`
        // are ordinary data properties, so `throw new Error("boom")` reports
        // `"Error: boom"` from a read rather than from a call — the difference
        // between a message that names the fault and one that says "an object".
        let value = crate::value::Value(payload);
        let described = super::text::to_text(context, value)
            .and_then(|text| text.to_rust())
            .or_else(|| super::error::joined(context, value.as_slot()?));
        Some((tag, described.unwrap_or_else(|| "an object".to_owned())))
    })
}
