//! The verifier.
//!
//! Every rule this layer states as an invariant is only real if something
//! rejects the program that breaks it. Documentation that is not enforced
//! degrades into documentation that is wrong.
//!
//! The verifier is deliberately redundant with the builder. The builder makes a
//! mistake awkward to write; the verifier makes it impossible to ship, including
//! for a program that did not come from the builder — a fixture read from disk,
//! a transformation applied after construction, a future client that builds the
//! tables directly.
//!
//! It reports every violation it finds rather than stopping at the first,
//! because a program with two mistakes should take one pass to diagnose.

mod calls;
mod dominance;
mod error;
mod rules;

pub use calls::knows_signature;
pub use error::{CallSite, VerifyError};

use crate::ir::{FuncRegistry, Function};
use crate::types::TypeRegistry;

/// Checks a function against every structural rule.
///
/// Returns the violations found, empty when the function is well formed.
pub fn verify(func: &Function, types: &TypeRegistry, funcs: &FuncRegistry) -> Vec<VerifyError> {
    let mut errors = Vec::new();
    rules::check_blocks_terminate(func, &mut errors);
    rules::check_terminators(func, &mut errors);
    rules::check_instructions(func, types, &mut errors);
    rules::check_returns(func, &mut errors);
    rules::check_unwind(func, &mut errors);
    calls::check_calls(func, funcs, &mut errors);
    errors
}

/// Whether a function is well formed.
pub fn is_valid(func: &Function, types: &TypeRegistry, funcs: &FuncRegistry) -> bool {
    verify(func, types, funcs).is_empty()
}
