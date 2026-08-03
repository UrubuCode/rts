//! The machine layer.
//!
//! This crate owns everything that is true of the machine and nothing that is
//! true of a source language. It knows about representations, layouts, frames,
//! references and calling conventions. It does not know about `undefined`,
//! `nil`, prototypes, metatables or `typeof`, and it records nothing about which
//! client declared what.
//!
//! The deciding rule for what belongs here: a capability belongs inside if it
//! can be specified, implemented and tested without naming a source-language
//! construct. Everything in this crate is testable with no front-end present.
//!
//! # Why an IR and not a helper library
//!
//! A helper library over a code generator moves the decisions behind nicer
//! names; the decisions are still taken at each call site. A representation in
//! the middle buys three things a helper cannot: a verifier, so illegal states
//! are unrepresentable rather than discouraged; one lowering, so a decision is
//! written once and serves every client; and isolated tests, so a slow program
//! can be attributed to the layer above rather than to this one.
//!
//! # Current scope
//!
//! The IR and its verifier, the type and tag registries, the ABI, and the three
//! capabilities that share one frame model: root reporting, unwinding and
//! suspension.
//!
//! Sharing that model is the point rather than a convenience. A program point
//! that can collect, inside a protected region, inside a function that may park
//! its frame, is *one* point in all three concerns, so it is one record. That is
//! what lets a parked frame — occupying no position on any call stack — still be
//! read as a frame, with its roots findable and the cleanup chain it suspended
//! inside intact.
//!
//! On top of that model sits the concurrency substrate: promises, continuations,
//! and the order they run in. A continuation is not a metaphor for a parked
//! frame — it is one, plus the promise it waits on, which is why coroutines,
//! generators, asynchronous functions and promise waiters are one mechanism here
//! rather than four.
//!
//! Calls sit on top of all of it, and arrived last on purpose: a call is
//! inseparable from the convention it uses, from the safepoint it implies, and
//! from where the frame is when control leaves.
//!
//! Still to come: lowering to machine code and the two output paths, guard
//! failure paths, faults and observability.

#![deny(missing_docs)]
#![deny(dead_code)]

pub mod abi;
pub mod frame;
pub mod gc;
pub mod ir;
pub mod repr;
pub mod sched;
pub mod tags;
pub mod types;
pub mod unwind;
pub mod verify;

pub use abi::{AbiType, Convention, Signature as AbiSignature, TargetAbi, lower_signature};
pub use frame::{ResumeLabel, SpillLayout, SuspendPlan, plan_suspension};
pub use gc::{FrameDescriptor, FrameTable, describe_frames};
pub use repr::{RefKind, Repr};
pub use sched::{PromiseTable, Scheduler, SchedulerId};
pub use types::{FieldLayout, TypeId, TypeRegistry};
pub use unwind::{RegionId, RegionTree, Tag, UnwindPlan, plan_unwind};
pub use verify::{VerifyError, verify};
