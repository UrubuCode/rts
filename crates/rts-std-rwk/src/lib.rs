//! The modules a program imports by name, for the new engine.
//!
//! # What decides membership here rather than in `rts-core-rwk`
//!
//! Availability, which is the only membership rule that crate states. `Math` is
//! on every target including wasm and lives there; `io.print` writes to a file
//! descriptor and does not. So this crate is where anything needing an operating
//! system goes, and it is a **client** of the runtime rather than part of it.
//!
//! # Why this exists at all, measured
//!
//! Every one of the 818 files in this repository's suite begins with
//! `import { describe, test } from "rts:test"`, and 224 of them also import
//! `rts`. Before this crate, 631 of them compiled as far as a name nothing
//! introduced — see `rts-host-rwk`'s `suite_coverage` test, which is what that
//! number comes from. Nothing here is a guess about what a program needs: the
//! surface is what the corpus actually reaches for, counted.
//!
//! # What it is NOT
//!
//! A test runner. This crate provides `describe`, `test` and `expect` as
//! functions a program can call, and records what they saw. Deciding what to do
//! with the record — print it, fail a build, compare it against another engine —
//! belongs to whatever runs the program.
//!
//! `node:*` is deliberately absent. 102 of the 818 files import one, and none of
//! them can be read yet; a module nothing can reach is a gap rather than a
//! feature, which is what the crate READMEs say about a structure with no
//! producer.

#![deny(missing_docs)]
#![deny(dead_code)]

pub mod io;
pub mod test;

use rts_core_rwk::entry::Context;

/// Registers every module this crate provides.
///
/// Called by a host before a program runs, and by that host rather than by a
/// constructor here: which modules exist is a decision about the environment the
/// program is given, and a crate that registered itself would be taking it.
pub fn install(context: &mut Context) {
    let harness = test::namespace(context);
    rts_core_rwk::entry::declare_module(context, "rts:test", harness);
    let standard = io::namespace(context);
    rts_core_rwk::entry::declare_module(context, "rts", standard);
}
