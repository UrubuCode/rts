//! `rts-symbol-baker` — the RTS symbol table, generated statically and ordered.
//!
//! Owner directive (2026-07-27): `rt.rs` and the hand-declared symbol system are
//! removed; `rts-macro`'s authoring surface is the single source of truth for
//! symbol NAMES, and a baker emits a `.rs` file — for the JIT and for the AOT —
//! producing all RTS symbols **statically** (a Rust-static table beats a runtime
//! Registry harvest) and **ordered** (so a later resolver can avoid matching or
//! scanning). This crate is that baker; it is the `.rs` analogue of
//! `rts-prelude-baker`'s `.ts` bake, and shares its determinism discipline.
//!
//! # What it replaces
//!
//! `rts-codegen-new`'s `adapter_symbols` assembles the JIT table from THREE
//! sources: a runtime Registry harvest plus two hand-written lists
//! (`list_a.rs` + `list_b.rs`, ~1380 lines of `sym("__RTS_…", ptr)`). Its own
//! doc-comment confesses the harvest "cannot supply" the engine-internal
//! primitives, because there is no Registry member to hold their address. A
//! generated table collapses all three: a symbol is in the table because it
//! EXISTS in the source, not because someone remembered to list it. The same
//! collapse deletes hand-maintained pairings elsewhere, e.g. `rts-node`'s
//! `path::syms()` table of `("__RTS_FN_NODE_PATH_…", fn as *const u8)` rows.
//!
//! # Why a source-scanning binary (mechanism (a))
//!
//! A proc-macro cannot build the table: each expansion sees only its own item,
//! there is no cross-crate state, and `#[rtse::abi]` therefore cannot know its
//! neighbours. The two remaining options were a distributed slice
//! (`linkme`/`inventory`) and this scanner. A distributed slice collects at LINK
//! time, in an order the linker chooses — non-deterministic across platforms,
//! link modes and section-GC settings. That directly contradicts the owner's
//! "ordered" requirement and would make the table's contents unreviewable in a
//! diff. A scanner produces a checked-in artefact whose order is ours, whose
//! diff is readable, and which a CI drift check can verify byte-for-byte.
//!
//! # The ordering invariant
//!
//! Strictly ascending byte order of the symbol name, no duplicates (except
//! `#[cfg]`-disjoint alternatives, which are mutually exclusive at compile
//! time). The consequences — `O(log n)` lookup with no hashing, and every scope
//! being one contiguous range so a resolver selects candidates by prefix instead
//! of matching — are documented on [`rts_abi::table`].
//!
//! # How an address is obtained in generated code
//!
//! The generated file declares each symbol in an `unsafe extern "C"` block with
//! `#[link_name = "…"]` and takes its address. It does NOT name a Rust module
//! path, which is what the hand lists do (`rt_str::__RTS_FN_NS_GC_STRING_NEW`)
//! and which only works for symbols that happen to be `pub`-reachable through a
//! public module chain — the reason engine-internal primitives had to be listed
//! by hand at all. Going through the LINKER name instead reaches every
//! `#[no_mangle]` symbol regardless of Rust visibility, and turns a missing
//! symbol into a build-time link error rather than the link-OK / runtime-SIGILL
//! class of bug.

pub mod emit;
pub mod scan;

pub use emit::render;
pub use scan::{Declaration, scan_workspace};

/// The crates scanned for exported symbols, in scan order.
///
/// These are the crates whose `#[no_mangle] extern "C"` functions end up in the
/// linked image — the JIT binary and, via the `rts-adapters` staticlib, the AOT
/// archive. A crate absent from this list contributes no symbols; a crate
/// present but not linked would make the generated extern declarations fail to
/// resolve, which is a loud link error by design.
pub const SCANNED_CRATES: &[&str] = &[
    "rts-adapters",
    "rts-egui",
    "rts-engine",
    "rts-napi",
    "rts-node",
    "rts-primitives",
    "rts-runtime",
    "rts-shared",
    "rts-std",
];

/// Default output path, relative to the workspace root.
///
/// The artefact is CHECKED IN (not `OUT_DIR`): it must be reviewable in a diff,
/// greppable when debugging a missing symbol, and identical for every consumer
/// of the workspace. `--check` is the drift guard that keeps a checked-in file
/// honest.
pub const DEFAULT_OUT: &str = "crates/rts-symbol-baker/generated/symbol_table.rs";
