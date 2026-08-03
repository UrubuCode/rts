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
//! The IR, the type and tag registries, and the verifier. Lowering to machine
//! code, the garbage-collection contract, the scheduler and the ABI arrive in
//! later modules; each is designed against the frame model this IR establishes.

#![deny(missing_docs)]
#![deny(dead_code)]

pub mod ir;
pub mod repr;
pub mod tags;
pub mod types;
pub mod verify;

pub use repr::{RefKind, Repr};
pub use types::{FieldLayout, TypeId, TypeRegistry};
pub use verify::{VerifyError, verify};
