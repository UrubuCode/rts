//! `rts:rigid` — a parallel rigid-body solver, as the CPU fallback for a scene
//! whose default backend is a GPU compute kernel.
//!
//! # Why this is a crate of its own
//!
//! Availability, which is the only boundary rule `docs/engine/architecture.md`
//! accepts and the same one that put a window in `rts-ui`. This solver is
//! `rayon` over OS threads: there are none in wasm, and a target without them
//! should not carry a thread pool in its graph to find that out. `rts-core`'s
//! membership rule — "what every target has, including wasm" — excludes it by
//! construction, and `rts-std` is the surface a headless build always installs,
//! so putting it there would spend a thread pool on every program that never
//! simulates anything.
//!
//! # Reuse-check, in the sentence the rule asks for
//!
//! **Nothing in this workspace answers this.** The nearest are
//! `rts_cranelift`'s `rayon`, which parallelises compilation rather than a
//! simulation, and `rts-ui`'s `scene`, which moves geometry across the boundary
//! and computes nothing. The data channel, on the other hand, was NOT written
//! here and deliberately: `rts_core::entry::bytes_pointer` already answers "where
//! are this typed array's bytes", it was added for `rts-napi-rwk`, and a second
//! shared-memory scheme between TypeScript and Rust would be exactly the
//! duplicate the reuse rule exists to refuse.
//!
//! # What this is a port of, and why a port rather than a design
//!
//! `engine/rigid/gpurigid.ts` in the `rts-game` project — the WGSL kernel that is
//! the default backend. Its formulation is gather/Jacobi: each body reads its
//! neighbours and writes only itself. That is what makes it parallel with no
//! atomics and no partitioning, and porting it rather than writing a sequential
//! solver with locks is what keeps the two backends comparable — they are meant
//! to be checked against each other by final position, which only means something
//! while they compute the same thing.
//!
//! The one deliberate divergence is stated where it lives, in `solver`'s module
//! doc: neighbours here are read from a snapshot, where the GPU reads whatever a
//! racing thread left. It is a choice with a consequence for parity and is
//! written down rather than absorbed.
//!
//! # What no thread here may do
//!
//! Touch a `Context`, call user code, allocate in the engine's heap, or read a
//! cell. A `Context` is reached through a thread-local, so a worker doing any of
//! those is looking at a runtime that is not its own. The workers see `&[f32]`
//! and `&mut [f32]`, and the surface is shaped as buffers rather than as objects
//! for that reason. It is the same discipline the ten `thread::spawn` sites in
//! `rts-node` already keep.

#![deny(missing_docs)]
#![deny(dead_code)]

pub mod backend;
pub mod registry;
pub mod shape;
pub mod solver;
mod surface;

use rts_core::entry::Context;

/// Registers `rts:rigid`.
///
/// By the host and not by a constructor here, for the reason `rts-std` states:
/// which modules exist is a decision about the environment a program is given,
/// and a crate that registered itself would be taking it.
pub fn install(context: &mut Context) {
    let rigid = surface::namespace(context);
    rts_core::entry::declare_module(context, "rts:rigid", rigid);
}
