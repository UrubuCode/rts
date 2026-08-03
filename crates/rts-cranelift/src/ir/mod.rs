//! The intermediate representation.
//!
//! Clients lower into this and never into a code generator directly. That
//! indirection is the whole point: it is where a decision can be written once,
//! where a verifier can reject what should not exist, and where a program can be
//! tested with no client present.
//!
//! # What is deliberately absent
//!
//! There are no call instructions yet. A call is inseparable from the calling
//! convention it uses, from the safepoint it implies, and from whether its
//! callee may suspend — none of which exist in this phase. Adding a call node
//! now would mean choosing those answers implicitly and rediscovering them
//! later, which is the failure this layer is built to avoid.

pub mod builder;
pub mod consts;
pub mod entity;
pub mod func;
pub mod funcs;
pub mod inst;

pub use builder::{BuildError, BuildResult, FuncBuilder};
pub use consts::{ConstDecl, ScalarBits, SymbolRef};
pub use entity::{BlockId, ConstId, InstId, ValueId};
pub use func::{Function, Signature, ValueData, ValueOrigin};
pub use funcs::{FuncDecl, FuncId, FuncRegistry, SigId};
pub use inst::{
    BitOp, BlockCall, BlockData, CmpOp, GenericOp, Inst, InstData, NumOp, Region, Terminator,
    TrapCode,
};
