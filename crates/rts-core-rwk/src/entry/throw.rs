//! A thrown value that no handler in the throwing function caught.
//!
//! # Why this ends the program rather than searching for a handler
//!
//! Because there is nothing yet to search. The machine computes where a throw
//! lands from the region tree of the function it is in, which is complete for
//! every handler in that function and says nothing about handlers in a caller.
//! Finding one of those means unwinding the native stack through frames Cranelift
//! emitted, and that needs an exception table and a personality routine — a
//! campaign, not a branch here.
//!
//! So the boundary is drawn where it can be drawn honestly: a throw that no
//! handler in its own function catches ends the program, with the value
//! reported. That is exactly right for an uncaught exception, and the language
//! layer refuses the one case where it would be wrong — a `try` protecting a
//! call, where a handler *does* exist and lives one frame up. Refused by name,
//! rather than compiled into a `catch` that silently never runs.
//!
//! # Why it does not return
//!
//! The machine emits a trap after the call, because the signature says nothing
//! comes back. Returning would reach that trap, which is a crash with no
//! diagnostic — the opposite of what an uncaught exception should look like.

use super::with_current;

/// Ends the program with an uncaught thrown value.
///
/// The tag is carried and not interpreted: the machine compares tags for
/// equality and this is the escaping path, where there was nothing to compare
/// against. It is reported because a value thrown with an unexpected tag is
/// worth seeing, not because anything here decides on it.
#[rtse::entry("rts_throw")]
pub fn throw(tag: i64, payload: u64) {
    // Whatever text the value has without running user code. An object answers
    // nothing, because `ToPrimitive` on one calls a `toString` an entry point
    // cannot call — and the diagnostic for an uncaught exception is not worth
    // reaching back into the program that just failed.
    let described = with_current(|context| {
        super::text::to_text(context, crate::value::Value(payload)).and_then(|text| text.to_rust())
    });
    match described {
        Some(text) => eprintln!("rts: uncaught exception (tag {tag}): {text}"),
        None => eprintln!("rts: uncaught exception (tag {tag}): an object"),
    }
    // `exit`, not `abort`: this is a program ending because of something the
    // program did, and a core dump describes the engine rather than the fault.
    std::process::exit(1);
}
