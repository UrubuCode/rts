//! Unwinding.
//!
//! Throwing terminates a frame, runs whatever cleanup that frame declared, and
//! transfers to a handler that may be many frames away. All three parts require
//! knowing the frame layout, which this layer owns and a client does not — which
//! is why unwinding is here and not in a language.
//!
//! # What travels, and what this layer knows about it
//!
//! The thrown value is opaque. Languages disagree about what may be thrown, and
//! none of that disagreement reaches here: the value is carried in the generic
//! form, and the tag beside it is compared for equality against handlers and
//! otherwise not interpreted. A layer that decided which values match a handler
//! would be deciding what may be thrown.
//!
//! # Why not a check after every call
//!
//! The arrangement this replaces signals errors through a slot examined after
//! each call, with no real unwinding. That pays on every call forever, including
//! the overwhelming majority that never throw. An exception table costs nothing
//! on the ordinary path and is paid only when something is actually thrown.
//!
//! # Why the search is computed here
//!
//! Where a throw lands is a function of the region tree and the tag, both known
//! at build time. Computing it here means the emitted code contains the answer
//! instead of the search — and, more importantly, means the cleanup chain is
//! derived rather than remembered. See [`plan_unwind`].
//!
//! # One consequence a client can see
//!
//! A call in tail position discards its frame before control transfers, so it
//! cannot also be the call a handler is installed around. Returning a call's
//! result directly and catching that call's exception are mutually exclusive.
//! Better said now than discovered later.
//!
//! # Why this shares the frame descriptor
//!
//! A point that can collect, inside a protected region, is one program point in
//! both concerns. The region a point belongs to is recorded on the same record
//! its roots are, so that a value spilled for the collector and a value that must
//! survive cleanup are never two answers to one question — and so that resuming a
//! parked frame re-establishes the cleanup chain it suspended inside.

mod plan;
mod region;

pub use plan::{UnwindPlan, plan_normal_exit, plan_unwind};
pub use region::{Enclosing, Handler, Region, RegionId, RegionTree, Tag};

use crate::ir::{BlockId, Function, Terminator};

/// What a throw in this function does, for every throw it contains.
///
/// Returned in program order, one entry per throwing block.
pub fn plan_all_throws(func: &Function) -> Vec<(BlockId, UnwindPlan)> {
    let mut plans = Vec::new();
    for (id, block) in func.blocks() {
        let Some(Terminator::Throw { tag, .. }) = &block.terminator else {
            continue;
        };

        let plan = match func.region_of(id) {
            Some(region) => plan_unwind(&func.regions, region, *tag),
            // Outside every region there is nothing to clean up and nothing to
            // match, so the value leaves immediately. That is a plan, not a gap.
            None => UnwindPlan {
                cleanups: Vec::new(),
                handler: None,
            },
        };
        plans.push((id, plan));
    }
    plans
}
