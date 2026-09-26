//! Passes over a lowered MIR graph that need to know what this LANGUAGE's operations
//! mean.
//!
//! `rts_mir::passes` holds the ones that do not -- they read effects and the domain's
//! answers, never a primitive's meaning, by that crate's rule 2. A pass that must know
//! that `NewObject` makes an object whose own property `k` is the value written under
//! `k` is this crate's, and lives here.
//!
//! Each is run by whoever lowers a function, after the lowering and before the types
//! are inferred, so the inference sees the graph the machine will.

mod scalar;

pub use scalar::{Replaced, replace_scalars};
