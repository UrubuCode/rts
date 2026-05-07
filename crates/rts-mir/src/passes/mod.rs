//! MIR optimization passes.
//!
//! Each submodule exposes a free function that takes `&mut MirFunc` and
//! transforms it in place. Order of application matters: typically run
//! `fold` before `dce` so newly-dead consts (replaced by their folded
//! equivalents) get cleaned up.

pub mod cse;
pub mod dce;
pub mod fma;
pub mod fold;
pub mod inline;
pub mod narrow;
pub mod verify;

pub use cse::cse;
pub use dce::dce;
pub use fma::fma;
pub use fold::fold;
pub use inline::{inline, INLINE_BUDGET};
pub use narrow::narrow;
pub use verify::{verify, VerifyError};

/// Convenience: run the standard pass pipeline on a function.
/// Order: fold (constant folding + strength reduction) → fma (FMA
/// fusion `a*b+c → fma`) → cse (common subexpression elimination) →
/// dce (dead code elimination — limpa os IAddImm 0 alias deixados
/// pelo CSE e FMul fundidos pelo FMA).
pub fn optimize(mir: &mut crate::ir::MirFunc) {
    fold(mir);
    fma(mir);
    cse(mir);
    dce(mir);
}
