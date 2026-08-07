//! What a program reaches with no import line.
//!
//! # Why these are here and not in `rts-core-rwk`
//!
//! Availability, which is that crate's only membership rule. `Math` is on every
//! target including wasm and lives there; a `TextDecoder` over an encoding table
//! and an `EventTarget` holding listeners are host furniture, in no ECMA-262
//! section, and every non-browser JavaScript host supplies them. That is the
//! same line `console` was already on.
//!
//! # Why they are not in `rts-node-rwk` either
//!
//! Because they are not Node's. `TextEncoder`, `Event` and `AbortController` are
//! WHATWG, and a program using them is not asking for Node compatibility. What
//! IS Node's — `process`, `Buffer`, the timer family, `URL` — is installed by
//! that crate from the module that already owns each one, so there is one class
//! per name rather than two that fail an `instanceof`.
//!
//! # The rule the compiler holds up the other end
//!
//! `rts-codegen`'s `emit/globals.rs` decides which NAMES resolve, and its own
//! comment makes it a rule that every name in that list has a real value in this
//! workspace. This module is what makes that true for the WHATWG half; a name
//! added there without a value here turns a refusal at compile time into an
//! `undefined` at run time.

pub mod events;
pub mod output;
pub mod text;
pub mod timing;

use rts_core_rwk::entry::Context;

/// Installs every global this crate provides.
pub fn install(context: &mut Context) {
    output::install(context);
    text::install(context);
    events::install(context);
    timing::install(context);
}
