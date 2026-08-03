//! Attributing an address to a place in the program.
//!
//! A profiler that cannot say which part of a program time was spent in is
//! guessing, and a stack trace that cannot name a frame is a list of numbers.
//! Both questions arrive the same way — here is an address, what is it — and only
//! the thing that emitted the code can answer.
//!
//! # Why this is here and not above
//!
//! The correspondence exists during lowering and nowhere else. A layer above
//! could keep its own table of what it *asked* for, but not of what was emitted:
//! instructions move, merge and disappear, and an address is a fact about the
//! result rather than about the request.
//!
//! # What it does not do
//!
//! It does not sample, count, or decide what is interesting. Those need a policy
//! — how often, of what, at whose expense — and a policy chosen here would be one
//! every client inherited. What this provides is the answer a profiler needs;
//! being a profiler is not a machine-level capability.

mod code;
mod positions;

pub use code::{CodeMap, CodeRange};
pub use positions::PositionMap;
