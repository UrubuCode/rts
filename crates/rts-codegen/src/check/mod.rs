//! Early errors: what a program may not be, said once.
//!
//! # Why this is not the parser, and not the emitter
//!
//! An early error is a rule no grammar production encodes and no tree node can
//! hold. `let x; let x;` parses perfectly — two declarations, both well formed —
//! and is not a program. So the rule cannot live in the grammar, and it cannot
//! live in a node, because there is no node whose existence is the mistake.
//!
//! It is also not the emitter's. The specification calls these *syntax* errors:
//! a program with one is refused before any of it runs, so a checker that ran at
//! emission would already have decided a program exists that does not. That is
//! the difference between refusing `let x; let x;` and refusing it *after* the
//! first `let` has been lowered.
//!
//! So: after the tree, before anything reads it, and reported as a syntax error.
//!
//! # Why it walks rather than checks in place
//!
//! Every rule here is about a *set* — the names a scope declares, the clauses a
//! switch has — and a set is not a node either. The walk builds the set the rule
//! is about and asks its question once, which is why each rule is stated in one
//! function and none of them is spread across the bridge.

mod scope;
mod walk;

use crate::names::Names;
use crate::syntax::Program;

pub(crate) use walk::Checker;

/// What a program may not be, and where it said it.
pub(crate) struct EarlyError {
    /// The rule, phrased as what is wrong rather than what was expected.
    pub message: String,
}

/// Check a program, and return the first thing wrong with it.
///
/// The first, not all of them: this feeds `ParseError::Syntax`, which carries
/// one message, and a program with an early error is refused whole. Collecting
/// the rest would be work whose only consumer is a diagnostic renderer that does
/// not exist yet — and adding it later is a change to this signature, not to any
/// rule below it.
pub(crate) fn check(program: &Program, names: &Names) -> Result<(), EarlyError> {
    let mut checker = Checker::new(names);
    checker.program(program);
    match checker.finish() {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
