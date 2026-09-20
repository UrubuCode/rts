//! The shared mid-level IR: a CFG in SSA form, parameterised by a type domain.
//!
//! The stage between a language's tree and the machine. `README.md` carries the
//! eleven rules that are binding for changes here; the two that decide the shape
//! of everything else are that this crate never names a language, and that what
//! is semantic arrives through [`domain::Domain`] and the primitive table rather
//! than being built in.
//!
//! # Why this crate exists at all
//!
//! The pipeline was `AST → machine IR`, so every decision that needs a **fixed
//! point** was being taken in one syntax-directed pass while emitting — and a
//! fixed point cannot be iterated to during emission. What filled the gap was a
//! family of whole-program proofs over the *spelling* of the program, each
//! weaker than the question it stood in for, in both directions at once.
//! `docs/engine/four-stages.md` has the two measurements.
//!
//! # Why it is a crate and not a module of the language
//!
//! Because there are meant to be two languages, and a MIR inside `rts-codegen`
//! would force the second to depend on the first or to reimplement a CFG. That
//! is the argument that put the shape tree in the machine rather than in the
//! language, applied one layer up.

pub mod cfg;
pub mod domain;
pub mod effect;
pub mod guard;
pub mod infer;
pub mod lower;
pub mod passes;
pub mod text;
pub mod verify;

pub use cfg::{BlockId, Const, Func, FuncBuilder, InstId, Op, Prim, Terminator, ValueId};
pub use domain::Domain;
pub use effect::Effect;
pub use guard::{Assertion, PointId, Tier};
pub use verify::{Malformed, verify};
