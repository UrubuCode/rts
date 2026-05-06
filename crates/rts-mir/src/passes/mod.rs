//! MIR optimization passes.
//!
//! Each submodule exposes a free function that takes `&mut MirFunc` and
//! transforms it in place. Order of application matters: typically run
//! `fold` before `dce` so newly-dead consts (replaced by their folded
//! equivalents) get cleaned up.

pub mod dce;
pub mod fold;
pub mod narrow;
pub mod verify;

pub use dce::dce;
pub use fold::fold;
pub use narrow::narrow;
pub use verify::{verify, VerifyError};

/// Convenience: run the standard pass pipeline on a function.
pub fn optimize(mir: &mut crate::ir::MirFunc) {
    fold(mir);
    dce(mir);
}
