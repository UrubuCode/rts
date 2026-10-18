//! Running a program.
//!
//! Three crates each hold half of something. `rts-codegen` knows JavaScript and
//! no machine; `rts-cranelift` knows the machine and no language;
//! `rts-core` implements what the language calls out for and never decides
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
//!
//! # Running on several threads
//!
//! [`compile_for`] takes the number of regions and [`Compiled::run_on`] runs the
//! program on that many threads, each over one of them. The count belongs to
//! compilation because the addressing is in the code: with one region the base
//! is an immediate, with several it is loaded from a table, and which of the two
//! was emitted cannot be revisited afterwards. [`compile`] stays at one, so the
//! single-threaded path pays nothing for the existence of the other.
//!
//! This gives each thread a heap of its own and nothing further. There is no
//! way to publish a value from one thread to another, no runtime write barrier
//! and no collector — a program that arranges to share is protected by nothing.

#![deny(missing_docs)]

pub mod describe;
pub mod prove;
mod compile_stats;
mod entries;
#[cfg(feature = "inspector")]
pub mod inspector;
pub mod graph;
pub mod jsonc;
mod link;
mod live;
pub mod object;
mod run;
mod stack;
mod wrap_script;

pub use graph::names_any_file;
pub use link::{HostError, singletons_for};
pub use live::install_compiler;
pub use run::{Compiled, DEFAULT_RESERVED_CELLS, MAX_CELLS_VAR, compile, compile_for, compile_graph};

/// Turns on the compiled-code cache for the rest of this process.
///
/// A function compiled by an earlier run, with the same body and the same
/// surroundings, is replayed instead of compiled again; see
/// `rts_cranelift::target::enable_code_cache` and its module for the key and
/// for why a hit is byte-for-byte what a compile would have produced.
/// `RTS_NO_CODE_CACHE` keeps it off, and `RTS_CODE_CACHE_DIR` moves it. By
/// default it lives in the system temp directory under `.rts/jit`, which
/// `rts clean` removes.
pub fn enable_code_cache() {
    let directory = std::env::var_os("RTS_CODE_CACHE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(".rts").join("jit"));
    rts_cranelift::target::enable_code_cache(directory);
}
