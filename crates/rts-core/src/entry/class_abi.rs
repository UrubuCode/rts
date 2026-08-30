//! The surface `#[rtse::class]` expands into, and the only one it may name.
//!
//! # Why this module exists
//!
//! The attribute expanded to `crate::entry::native::…`, `crate::entry::
//! class_support::…` and `crate::entry::objects::…`, and those three modules are
//! private. So the expansion compiled in exactly one crate — this one — and
//! every other surface hand-wrote its table instead: `rts-std`, `rts-node` and
//! `rts-ui` each carry a `MEMBERS: &[(&str, Provided)]` list beside a set of
//! `extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64`.
//!
//! That is not a style difference. A hand-written table has no Rust signature to
//! derive anything FROM, so `rts emit-types` has nothing to say about those
//! names — and a caller in TypeScript is contradicted by nothing. The defect
//! that proved it: `egui.isOpen` and `input.key` answer a BOOLEAN, roughly
//! twenty `.ts` callers test them with `!== 0`, and `false !== 0` is true. The
//! mini-browser's keyboard was wired ON in every frame and nothing said so.
//!
//! With the attribute reachable, `fn is_open(…) -> bool` derives `boolean` and
//! that comparison stops compiling. The generated declaration is not a
//! convenience here — it is the only thing standing between a Rust signature and
//! a caller's guess.
//!
//! # Why a facade rather than making the three modules public
//!
//! Because what the attribute needs is a dozen and a half items and those
//! modules hold hundreds. Publishing them would make every internal of the
//! object model part of this crate's contract, and the next person to change one
//! would be changing an interface without knowing it. The items below are `pub`
//! and their modules are still private, so this path is the ONLY way in — which
//! is what makes "what the attribute may name" a list someone can read.
//!
//! # What is deliberately not re-exported
//!
//! Anything an author's BODY needs rather than the wrapper. A member's body
//! reaches the runtime the way any native does, through the rest of `entry`;
//! this is the wrapper's vocabulary alone. Adding to it because a body wanted
//! something is how a facade becomes a second copy of the crate.

pub use super::class_support::{constants, made, record, to_boolean, to_number, Constant};
pub use super::native::{
    callable, hidden, install_with_arity, install_with_arity_and_prototypes, length_of, name_of,
    pinned, plain, tagged, Native,
};
pub use super::objects::{put, read_property, undefined_of};
