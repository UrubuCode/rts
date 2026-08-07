//! The `rts:` surface, and the globals a program reaches with no import.
//!
//! # What decides membership here rather than in `rts-core-rwk`
//!
//! Availability, which is the only membership rule that crate states. `Math` is
//! on every target including wasm and lives there; `io.print` writes to a file
//! descriptor and does not. So this crate is where anything needing an operating
//! system goes, and it is a **client** of the runtime rather than part of it.
//!
//! # What `rts:` is, and what it deliberately is not
//!
//! It is this engine's own surface — what RTS offers, in its own words. It is
//! **not** a second spelling of Node: `node:*` lives in `rts-node-rwk` and
//! answers a different question, which is what a program written for another
//! runtime expects to find. A module belongs here when it exists because of how
//! this engine works.
//!
//! | specifier | what it is |
//! |---|---|
//! | `rts:test` | `describe`/`test`/`expect`, and the record they leave |
//! | `rts:runtime` | the module table, from inside a running program |
//! | `rts:egui` | **not registered** — see below |
//!
//! # `rts:io` is gone, and the measurement that removed it
//!
//! It was a module exporting a nested `io` namespace: `import { io } from "rts"`
//! then `io.print(x)`. 224 of the 818 files in this repository's suite imported
//! it and exactly ONE called `io.print` — 139 of them never used the binding at
//! all, and the rest reached for namespaces of the OLD engine that were never
//! built here. What those files do instead is declare a local `print`.
//!
//! So `print`, `println`, `write` and `prompt` are globals now (`globals::output`),
//! which is the shape a program was already writing, and the module is not
//! replaced by a thinner one — a specifier kept alive for one caller is a
//! surface to keep in agreement for no reader.
//!
//! # `rts:egui` is absent, and why that is not an omission
//!
//! The `rts-egui` crate exists and is real, and it was written against the OLD
//! engine: its natives are linker symbols resolved through `rts-abi`, which is
//! the interface `rts_cranelift::abi` replaced. A native in the new engine is a
//! function pointer stored beside a cell — see
//! `docs/engine/authoring-natives.md` — so wiring the two together is a port,
//! not a registration.
//!
//! Registering a shell that refused every member was considered and rejected: it
//! would make `import { … } from "rts:egui"` compile and then draw nothing,
//! which for a UI surface is the failure mode that wastes the most time before
//! it is understood. An unresolved specifier says the same thing at the import
//! line.
//!
//! # What this crate is NOT
//!
//! A test runner. It provides `describe`, `test` and `expect` as functions a
//! program can call, and records what they saw. Deciding what to do with the
//! record — print it, fail a build, compare it against another engine — belongs
//! to whatever runs the program.

#![deny(missing_docs)]
#![deny(dead_code)]

pub mod console;
pub mod globals;
pub mod runtime;
pub mod test;

use rts_core_rwk::entry::Context;

/// Registers every module and global this crate provides.
///
/// Called by a host before a program runs, and by that host rather than by a
/// constructor here: which modules exist is a decision about the environment the
/// program is given, and a crate that registered itself would be taking it.
pub fn install(context: &mut Context) {
    let harness = test::namespace(context);
    rts_core_rwk::entry::declare_module(context, "rts:test", harness);

    let modules = runtime::namespace(context);
    rts_core_rwk::entry::declare_module(context, "rts:runtime", modules);

    // Not modules: a program writes `console.log` and `new TextEncoder()` with
    // no import line, so both have to be reachable by name.
    console::install(context);
    globals::install(context);
}
