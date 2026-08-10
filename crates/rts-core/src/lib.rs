//! The runtime every target has.
//!
//! Values, objects, text, memory and scheduling: the operations a compiled
//! program performs that are not instructions. Present on every target,
//! including wasm, which is what decides membership — anything that needs an
//! operating system lives in `rts-host`, anything a browser provides lives in
//! `rts-browser`, and neither is a dependency of this.
//!
//! # Why the crate boundary is where it is
//!
//! A crate boundary answers **"does this exist here?"**, not "may I mention
//! this?". The layering being replaced used the second question — the engine
//! could not name a class it could not depend on — and a codegen that reaches
//! its runtime by index cannot name a class at all, so that boundary has no job
//! left. What remains is availability, which is a build fact and belongs in the
//! graph: a browser has no `node:fs`, and a cross-compile has to know that
//! before it links.
//!
//! Inside, modules are organised by what the code does rather than by class.
//! `String.prototype.indexOf` and `Array.prototype.indexOf` do related work on
//! different representations, and filing them apart because one is "a string
//! class" is filing by taxonomy rather than by where a reader will look.
//!
//! Full reasoning: `docs/engine/architecture.md`.
//!
//! # The name
//!
//! It was `rts-core-rwk` while `rts-primitives` and the portable half of
//! `rts-shared` still existed and cargo would not have two crates under one
//! name. Both were deleted on 2026-08-10 and the suffix went with them, which
//! is what its own doc comment promised would happen.

#![deny(missing_docs)]
#![deny(dead_code)]

pub mod bigint;
pub mod coerce;
pub mod collect;
pub mod entry;
pub mod heap;
pub mod object;
pub mod schedule;
pub mod text;
pub mod value;

pub use coerce::{Hint, Side};
pub use collect::{Marks, Trace};
pub use entry::Context;
pub use heap::{Handle, Slab, Slot};
pub use object::{Key, Object};
pub use schedule::Settlements;
pub use text::{Interner, Str};
pub use value::{Kind, Kinds, Value};
