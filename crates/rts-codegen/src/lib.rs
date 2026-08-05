//! The language layer.
//!
//! JavaScript and TypeScript as semantics: what `a + b` means, what a scope is,
//! what a type annotation is evidence of, and what happens when it turns out to
//! be wrong.
//!
//! It is a client of the machine layer and knows nothing below it. Not what a
//! register is, not where a field sits, not which convention a call uses, not
//! that Cranelift exists. The mirror of that crate's rule, and only useful
//! together with it: one of them alone is a preference, both at once is a
//! boundary.
//!
//! # The rule that does the most work here
//!
//! **A type annotation is evidence, not proof.** TypeScript types are erased and
//! unchecked at every boundary a program does not control, so an annotation is a
//! claim: usable to prove a representation where the language can check it, and a
//! guard where it cannot.
//!
//! Treating a claim as a proof is the most tempting mistake available in this
//! crate, because it makes the generated code faster and the program wrong.
//!
//! # Current scope
//!
//! The tree a program is written in, the identifiers it names things by, and what
//! a value is. Scopes, lowering and what a claim is worth come next; each is
//! designed against the vocabulary here rather than beside it.

#![deny(missing_docs)]
#![deny(dead_code)]

mod check;
pub mod names;
pub mod parse;
pub mod syntax;
pub mod values;

pub mod emit;
pub mod runtime;

pub use names::{Name, Names};
pub use syntax::{Expr, Program, Stmt};
pub use values::{Singleton, ValueModel};
