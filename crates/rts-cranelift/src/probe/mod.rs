//! Measuring this layer, with no client present.
//!
//! The claim this crate makes is that after it exists, slowness is attributable
//! to the layer above rather than to the machine. That claim is only meaningful
//! if the machine can be measured alone — otherwise "the compiler is slow" stays
//! one undifferentiated blob, which is the situation this crate was built to end.
//!
//! So every number here comes from a program written directly in this crate's
//! representation, compiled through the ordinary pipeline, and called. No
//! front-end, no language, nothing to blame but us.
//!
//! # These numbers are a contract
//!
//! A regression in them is a regression in the machine layer. A slow program
//! whose probe numbers are unchanged is a problem somewhere else. That is the
//! whole point of having them, and it only works if they are honest:
//!
//! - **A debug number is not a number.** The harness says which profile produced
//!   it, and a reader who ignores that will draw conclusions from an
//!   unoptimized build.
//! - **What is not measured is not claimed.** There is nothing here about
//!   allocation, because allocation is a runtime entry point and what a stand-in
//!   allocator costs says nothing about what a real one will.
//! - **A measurement of nothing measures nothing.** Every fixture returns a value
//!   derived from its input, and the harness consumes the result, so that an
//!   optimizer removing the work would show up as a number too good to be true
//!   rather than as a fast one.
//!
//! # The floor, and why the difference is the number
//!
//! Each fixture is reached through a function pointer, which costs something
//! before any of the work happens. Measured rather than assumed: the first
//! fixture is one addition and a return, and it comes out at roughly the same
//! figure as reading a field. That is not a claim that reading a field is free —
//! it is the harness saying that its own call dominates both.
//!
//! So the first fixture is the floor, and what a primitive costs is its distance
//! from it. Reporting the totals alone would let a reader conclude that every
//! primitive costs three nanoseconds, which is a statement about calling a
//! function pointer and not about this layer at all.

mod fixtures;
mod harness;
mod phase;

pub use fixtures::{Fixture, all};
pub use harness::{Measurement, Profile, measure};
pub use phase::Phase;
