//! Declare an entry point; the shape is derived.
//!
//! # Why a second crate rather than a second attribute
//!
//! The first attempt added `#[rtse::entry]` to `rts-macro`. The emission was
//! right and the dependency was not: `rts-macro` depends on `rts-abi`, so every
//! crate reaching for the new attribute reached — through its build graph — for
//! the interface `rts_cranelift::abi` was built to replace.
//!
//! `cargo tree` said so plainly: `rts-core-rwk → rts-macro → rts-abi`. Source
//! that looks independent and a graph that is not is worse than an honest
//! coupling, because nothing shows it until the day `rts-abi` is deleted and the
//! new runtime stops building with it.
//!
//! So this is separate, and depends on `syn`, `quote` and `proc-macro2` and
//! nothing else. When `rts-abi` goes, `rts-macro` goes with it and this does not
//! notice.
//!
//! # How a crate chooses which one it gets
//!
//! Cargo renames a dependency, so both are spelled `rtse` at the use site and
//! each crate picks in one line:
//!
//! ```toml
//! rtse = { package = "rts-macro-rwk", path = "../rts-macro-rwk" }  # new engine
//! rtse = { package = "rts-macro",     path = "../rts-macro" }      # old engine
//! ```
//!
//! No conditional compilation, no feature flag, no shared body carrying two
//! shapes forever. A crate belongs to one world and says which.
//!
//! # What is deliberately not here
//!
//! Everything `rts-macro` has that the new engine has no use for: Registry
//! members, class declarations, the `__rtsm_`/`__rtsn_` scope-naming convention
//! that exists to organise a table of thousands, and constants exposed as
//! globals.
//!
//! That is most of it, and its absence is the point. A rework that copied the
//! old surface across would be a rename, and the parts not copied are the ones
//! whose reason for existing left with the engine that needed them.

#![deny(missing_docs)]

use proc_macro::TokenStream;

mod entry;

/// Declare a runtime entry point: a function compiled code calls rather than
/// inlines.
///
/// Rewrites the function to `extern "C"` under a derived linker name, and emits
/// a `const` describing its ABI shape **derived from the Rust signature**. Edit a
/// parameter and the descriptor changes in the same edit, or the crate does not
/// compile.
///
/// ```ignore
/// #[rtse::entry]
/// pub fn add(left: u64, right: u64) -> u64 { … }
/// // exports `__rts_add`, and emits `ADD_ENTRY: EntryDesc`
///
/// #[rtse::entry("__rts_legacy_name")]
/// pub fn something(value: f64) -> u64 { … }
/// ```
///
/// It does not decide which *number* an entry has. A declaration cannot see its
/// neighbours, so the set is assembled somewhere that can.
///
/// See the `entry` module for the type mapping and what it refuses.
#[proc_macro_attribute]
pub fn entry(args: TokenStream, item: TokenStream) -> TokenStream {
    match entry::expand(args.into(), item.into()) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}
