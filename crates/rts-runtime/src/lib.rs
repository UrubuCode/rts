//! The archive an AOT-compiled program links against.
//!
//! # What this crate is for
//!
//! A program the new engine compiles ahead of time is an object file full of
//! calls to named symbols: allocate, the write barrier, every entry point the
//! lowering could not express as instructions. Something has to hand the linker
//! those symbols, and a `staticlib` is what cargo produces that can. See this
//! crate's manifest for why it is ONE facade rather than one archive per crate.
//!
//! # Why it holds no code of its own
//!
//! Because a facade that implements anything is a second place to look for it.
//! What it does is depend on the three crates whose `#[rtse::entry]` symbols the
//! object needs, and keep them reachable so cargo does not drop them.
//!
//! # The part that is not obvious, and the reason [`keep`] exists
//!
//! An archive is not a promise that a symbol survived. Rust minted every entry
//! with `#[unsafe(export_name = …)]`, which names the symbol, but a `staticlib`
//! is still built from a dependency graph — and a dependency nothing in the crate
//! root reaches is a dependency cargo is free to leave out entirely. That failure
//! is quiet in the worst way: the archive builds, the link succeeds against
//! whatever it did carry, and the binary dies at the first call to a symbol that
//! was never there.
//!
//! So the three crates are TOUCHED here, and the touch is checked by
//! `dumpbin`/`nm` rather than trusted.

pub mod aot;

/// Reaches each dependency so its symbols reach the archive.
///
/// Called by nothing. It exists to be a use of all three crates from this crate's
/// root, which is what makes them part of this compilation rather than an
/// unreferenced edge in the manifest.
///
/// A `pub` function rather than a `#[used]` static because the thing that must
/// survive is the dependency's whole object, not one byte of data: naming a
/// function from each is what pulls its translation unit in.
pub fn keep() -> usize {
    // `install` is the right name to reach in each: it is what the host calls,
    // so it transitively touches everything the crate registers, which is
    // exactly the set an AOT program can call.
    let core = rts_core::entry::CORE_ENTRY_COUNT;
    let std_install = rts_std::install as usize;
    let node_install = rts_node::install as usize;
    core + (std_install & 1) + (node_install & 1)
}
