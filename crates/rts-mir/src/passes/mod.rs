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
/// Order: narrow (canonicaliza aritmética i8/i16 com masks pra
/// preservar overflow) → fold (constant folding + strength reduction)
/// → fma (FMA fusion `a*b+c → fma`) → cse (common subexpression
/// elimination) → dce (dead code elimination — limpa os IAddImm 0
/// alias deixados pelo CSE e FMul fundidos pelo FMA).
///
/// `narrow` roda primeiro pra que os masks que ele insere participem
/// do fold (ex: `(x+1) & 0xFF` com x const vira const direto). DCE
/// remove masks redundantes ao final quando o consumidor mascara de
/// novo.
pub fn optimize(mir: &mut crate::ir::MirFunc) {
    narrow(mir);
    fold(mir);
    fma(mir);
    cse(mir);
    dce(mir);
}
