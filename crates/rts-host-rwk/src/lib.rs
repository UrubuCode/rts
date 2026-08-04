//! Running a program.
//!
//! Three crates each hold half of something. `rts-codegen` knows JavaScript and
//! no machine; `rts-cranelift` knows the machine and no language;
//! `rts-core-rwk` implements what the language calls out for and never decides
//! what to call. None of them can run a program, by construction.
//!
//! This is the crate that may name all three, so it is where one runs.
//!
//! # What it is not allowed to do
//!
//! Hold semantics, and depend on `cranelift-*`. The second was broken by the
//! first version of this crate, which named `cranelift-module` and
//! `cranelift-jit` and asserted in its own README that it was entitled to —
//! a sentence written to justify a manifest rather than read from any rule.
//!
//! The machine's rule 1 is unconditional: *"No other crate in the workspace may
//! depend on `cranelift-*`."* What made an exception look necessary was that the
//! machine's placement surface spoke the code generator's vocabulary, so the
//! rule and the API contradicted each other. The machine now says placement in
//! its own words, and there is nothing left to except.

#![deny(missing_docs)]

mod entries;
mod link;
mod run;

pub use link::{HostError, singletons_for};
pub use run::{Compiled, compile};
