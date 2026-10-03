//! What a program site actually saw, and how often.
//!
//! A guard is only worth emitting where something is likely. The mechanism that
//! emits one exists — `rts_mir`'s guard, its two tiers and the lateral fall —
//! and so does the cached access the machine builds. Neither has any idea what
//! to bet on. This crate is the record that answers that, and
//! `docs/engine/profile-oracle.md` is the design.
//!
//! # The one rule: this crate counts, it does not type
//!
//! An [`Observation`] says *"of 9 740 arrivals at this site, 9 731 brought these
//! names"*. It never says *"this is a float"*, and there is deliberately no
//! `should_speculate` here, no threshold and no verdict. Turning a frequency
//! into an assumption is a judgement about what a language's semantics make safe
//! — `rts_mir::domain::Domain` is where that belongs, and its own header says
//! why a shared lattice is worse than a parameter.
//!
//! That is what keeps this crate neutral, and it is checkable rather than
//! asserted: **if a field of a record could only be filled in by a language, the
//! field is in the wrong crate.** `tests/toy_oracle.rs` is that check — a second
//! client with three types, integer distinct from float, writing and reading the
//! same records with no front end present. `tests/neutrality.rs` is the other
//! half, and it caught this very sentence naming a language on its first run.
//!
//! # Why the key is not a number this workspace mints
//!
//! The obvious key is what the compiler already holds, and all three candidates
//! are local to one compilation: `CacheId` is a `pub(crate)` index into a dense
//! table, `KeyRegistry` issues keys in whatever order a compilation asks for
//! them, and a shape id is minted as the program grows. A file keyed by any of
//! them is valid only for the compilation that wrote it.
//!
//! **And that failure is silent**, which is why it decides the design. The
//! numbers would match *other* sites, every guard would still pass, and the
//! speculation would simply be pointed somewhere else — nothing asserts an
//! answer that could be wrong. It is the second silent class the honesty floor
//! names: a rule applied to the wrong thing rather than a wrong result.
//!
//! So a [`SiteKey`] is a [`ModuleId`] and a `Position`, and
//! [`Witness`] carries names rather than numbers. See each type for the rest.
//!
//! # What was searched before this was written
//!
//! `rts_cranelift::observe` is the nearest existing answer and it declines this
//! job in writing: *"it does not sample, count, or decide what is interesting.
//! Those need a policy — how often, of what, at whose expense — and a policy
//! chosen here would be one every client inherited… being a profiler is not a
//! machine-level capability."* It answers which `Position` an address belongs
//! to, which is the other half and is reused rather than mirrored. Nothing in
//! `rts-core`, `rts-codegen` or `rts-cranelift` counts anything per site.
//!
//! This crate mints no numbers at all, so there is no registry it must mint
//! from.

#![deny(missing_docs)]
#![deny(dead_code)]

mod module;
mod observation;
mod profile;
mod text;
mod witness;

pub use module::ModuleId;
pub use observation::{Majority, Observation, Recorder, WITNESS_WIDTH};
pub use profile::{Profile, SiteKey};
pub use text::{FORMAT_VERSION, FormatError, read, write};
pub use witness::Witness;
