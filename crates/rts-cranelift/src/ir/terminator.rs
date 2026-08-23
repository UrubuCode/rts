//! What a terminator says about control and about values.
//!
//! Apart from `inst.rs` — which holds the instruction and terminator
//! vocabularies themselves — because that file reached this crate's 1000-line
//! ceiling and rule 5 splits at it. The line between the two is the one a reader
//! already draws: `inst.rs` says what the operations ARE, and this says what
//! every one of them implies for the graph and for liveness.
//!
//! Both answers are exhaustive matches over one enum, which is the property that
//! makes this split safe: adding a terminator without deciding its successors or
//! its operands does not compile.

use super::inst::Terminator;
use super::{BlockId, ValueId};

impl Terminator {
    /// The blocks control may reach from here.
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Terminator::Jump(call) => vec![call.block],
            Terminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                vec![then_block.block, else_block.block]
            }
            Terminator::Guard { ok, fail, .. } | Terminator::GuardType { ok, fail, .. } => {
                vec![ok.block, fail.block]
            }
            Terminator::CachedGet { hit, miss, .. }
            | Terminator::CachedGetIndirect { hit, miss, .. }
            | Terminator::CachedGetKeyed { hit, miss, .. }
            | Terminator::CachedSet { hit, miss, .. } => {
                vec![hit.block, miss.block]
            }
            // A throw has no successor in this function's graph. Where it lands
            // is decided by the region tree, and may be in a caller; calling it
            // an edge here would claim a transfer this block does not perform.
            Terminator::Return(_)
            | Terminator::Throw { .. }
            | Terminator::TailCall { .. }
            | Terminator::TailCallIndirect { .. }
            | Terminator::CleanupDone
            | Terminator::Trap(_) => Vec::new(),
        }
    }

    /// The values this terminator reads, including branch arguments.
    pub fn operands(&self) -> Vec<ValueId> {
        let mut operands = Vec::new();
        match self {
            Terminator::Jump(call) => operands.extend_from_slice(&call.args),
            Terminator::Branch {
                cond,
                then_block,
                else_block,
            } => {
                operands.push(*cond);
                operands.extend_from_slice(&then_block.args);
                operands.extend_from_slice(&else_block.args);
            }
            Terminator::Guard {
                input, ok, fail, ..
            } => {
                operands.push(*input);
                operands.extend_from_slice(&ok.args);
                operands.extend_from_slice(&fail.args);
            }
            Terminator::GuardType {
                object, ok, fail, ..
            } => {
                operands.push(*object);
                operands.extend_from_slice(&ok.args);
                operands.extend_from_slice(&fail.args);
            }
            Terminator::CachedGet {
                object, hit, miss, ..
            }
            | Terminator::CachedGetIndirect {
                object, hit, miss, ..
            } => {
                operands.push(*object);
                operands.extend_from_slice(&hit.args);
                operands.extend_from_slice(&miss.args);
            }
            Terminator::CachedSet {
                object,
                value,
                hit,
                miss,
                ..
            } => {
                operands.push(*object);
                operands.push(*value);
                operands.extend_from_slice(&hit.args);
                operands.extend_from_slice(&miss.args);
            }
            // Two operands, like the store and unlike the other two reads: the
            // key is a value the program computed, so it is live into this
            // terminator and everything that reads operands — liveness, the
            // register allocator, the frame — has to see it.
            Terminator::CachedGetKeyed {
                object,
                key,
                hit,
                miss,
                ..
            } => {
                operands.push(*object);
                operands.push(*key);
                operands.extend_from_slice(&hit.args);
                operands.extend_from_slice(&miss.args);
            }
            Terminator::Return(values) => operands.extend_from_slice(values),
            Terminator::TailCall { args, .. } => operands.extend_from_slice(args),
            Terminator::TailCallIndirect { callee, args, .. } => {
                operands.push(*callee);
                operands.extend_from_slice(args);
            }
            Terminator::Throw { payload, .. } => operands.push(*payload),
            Terminator::CleanupDone | Terminator::Trap(_) => {}
        }
        operands
    }
}
