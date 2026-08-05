//! The names the runtime provides, and how one is read.
//!
//! # What this is not
//!
//! It is **not** the global object. A real one is an object every unbound name
//! resolves against, with `globalThis`, with writes that create properties, and
//! with `typeof undeclared` answering `"undefined"` instead of throwing. None of
//! that is here, and none of it is faked: a name not in the list below is still
//! [`super::EmitError::UnboundName`], so a typo is still a program that does not
//! compile rather than one that runs.
//!
//! What this is: a fixed set of names whose values the runtime holds, read by
//! key. It exists because `RegExp` is not a constant the way `NaN` is — it is an
//! object with a `prototype`, allocated once, and a program can write properties
//! to it. So the emitter cannot produce it, and a call is what reaches it.
//!
//! # Why the list lives in this crate
//!
//! Because which names the global object has is a fact about **JavaScript** —
//! ECMA-262 §19 enumerates them — and this crate is the one that knows the
//! language. The runtime decides what it can supply, which is a different
//! question, and answers `undefined` for a name it does not have.
//!
//! That asymmetry is deliberate rather than sloppy: a name listed here and
//! missing there is a value a program can see, where the alternative — the
//! runtime naming the set — would make the compiler ask permission from
//! whichever runtime it happened to be built against, which is the boundary
//! rule 1 draws.

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::expr::{call, key_constant};
use super::{Ctx, EmitResult};
use crate::names::Name;
use crate::runtime::RuntimeOp;

/// The names this engine supplies as values rather than as constants.
///
/// Short on purpose. Every entry is a name a program may read without declaring
/// it, so a name added here stops being a `ReferenceError` — which is a language
/// decision and not a convenience.
const PROVIDED: &[&str] = &["RegExp"];

/// Emits a read of one, if it is one.
///
/// `None` means the name is not provided, which the caller turns back into the
/// unbound-name refusal rather than into `undefined`.
pub(super) fn read(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    name: Name,
) -> Option<EmitResult<ValueId>> {
    if !PROVIDED.contains(&ctx.names.text(name)) {
        return None;
    }
    // By key, not by index into the list above: the runtime holds these as
    // properties of an object, so the number that crosses is the one the key
    // registry issued — the same numbering every other property read uses. A
    // position in this list would be a second numbering for the same names.
    let key = key_constant(builder, ctx, name);
    Some(call(builder, ctx, RuntimeOp::GlobalGet, &[key]).map(|answered| answered[0]))
}
