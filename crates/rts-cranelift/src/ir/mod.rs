//! The intermediate representation.
//!
//! Clients lower into this and never into a code generator directly. That
//! indirection is the whole point: it is where a decision can be written once,
//! where a verifier can reject what should not exist, and where a program can be
//! tested with no client present.
//!
//! # Calls, and why they arrived late
//!
//! There were no call instructions for several phases, deliberately: a call is
//! inseparable from the convention it uses, from the safepoint it implies, and
//! from whether its callee may suspend, and adding the node before those
//! existed would have meant choosing their answers implicitly and
//! rediscovering them later.
//!
//! All three now exist — [`crate::abi::Convention`], the frame descriptors, and
//! the suspension form — so [`Inst::Call`] and [`Inst::CallIndirect`] are here,
//! each carrying the convention rather than assuming one.

pub mod builder;
pub mod consts;
pub mod entity;
pub mod func;
pub mod funcs;
pub mod inst;
pub mod text;

pub use builder::{BuildError, BuildResult, FuncBuilder};
pub use consts::{ConstDecl, ScalarBits, SymbolRef};
pub use entity::{BlockId, CacheId, ConstId, InstId, ValueId};
pub use func::{Function, Signature, ValueData, ValueOrigin};
pub use funcs::{FuncDecl, FuncId, FuncRegistry, SigId};
pub use inst::{
    BitOp, BlockCall, BlockData, CmpOp, GenericOp, Inst, InstData, NumOp, Region, Terminator,
    TrapCode,
};
