//! Liveness: which values are still needed at a program point.
//!
//! This is the analysis the root set is derived from, and deriving it is the
//! whole point. The alternative — a client marking its own live references — is
//! a discipline, and a discipline that must hold at every allocation in every
//! program is one that will not hold. There is no API here to forget, because
//! there is no API here at all: the layer computes it.
//!
//! Backward dataflow to a fixed point. A value is live entering a block if the
//! block reads it before defining it, or if it is live leaving the block.

use std::collections::{HashMap, HashSet};

use crate::ir::{BlockId, Function, ValueId};

/// Values live entering and leaving each block.
pub struct Liveness {
    live_in: HashMap<BlockId, HashSet<ValueId>>,
    live_out: HashMap<BlockId, HashSet<ValueId>>,
}

impl Liveness {
    /// Computes liveness over a whole function.
    pub fn compute(func: &Function) -> Self {
        let mut analysis = Liveness {
            live_in: HashMap::new(),
            live_out: HashMap::new(),
        };

        for (id, _) in func.blocks() {
            analysis.live_in.insert(id, HashSet::new());
            analysis.live_out.insert(id, HashSet::new());
        }

        // Iterate until nothing changes. Blocks are visited in reverse creation
        // order, which reaches the fixed point quickly for the forward-built
        // programs this layer produces without depending on it for correctness.
        let mut changed = true;
        while changed {
            changed = false;
            for (id, _) in func.blocks().collect::<Vec<_>>().into_iter().rev() {
                changed |= analysis.update_block(func, id);
            }
        }
        analysis
    }

    /// Values live leaving a block.
    pub fn live_out(&self, block: BlockId) -> &HashSet<ValueId> {
        self.live_out
            .get(&block)
            .expect("block belongs to this function")
    }

    /// Values live entering a block.
    pub fn live_in(&self, block: BlockId) -> &HashSet<ValueId> {
        self.live_in
            .get(&block)
            .expect("block belongs to this function")
    }

    /// Recomputes one block's sets; reports whether anything changed.
    fn update_block(&mut self, func: &Function, id: BlockId) -> bool {
        let block = func.block(id).expect("block belongs to this function");

        let mut out = HashSet::new();
        if let Some(terminator) = &block.terminator {
            for successor in terminator.successors() {
                out.extend(self.live_in[&successor].iter().copied());
            }
        }
        // The successors a TERMINATOR does not name. A block inside a protected
        // region can leave through a throw at any call in it, and where it goes
        // is the region's handler or its cleanup — an edge that exists in the
        // unwinder and in no terminator, so a walk over `successors()` alone
        // cannot see it.
        //
        // Missing them is not a missing optimisation, it is a wrong answer
        // twice over. `frame::plan_suspension` reads this to decide what a
        // parked frame must keep, so a value a HANDLER reads was left in a
        // register across a suspension, and the rewritten function used a
        // definition that no longer dominated it — Cranelift refused
        // `retry(fn, times)`, the ordinary shape of `try { return await f() }
        // catch {}` inside a loop, with "uses value from non-dominating". The
        // collector reads it too, so the same value was also absent from the
        // root set at every allocation in the region.
        //
        // Outward through the enclosing regions, because a tag no handler here
        // matches propagates to the one outside, and a cleanup on the way owes
        // its own code either way.
        if let Some(region) = func.region_of(id) {
            for (_, enclosing) in func.regions.enclosing(region) {
                for handler in &enclosing.handlers {
                    out.extend(self.live_in[&handler.block].iter().copied());
                }
                if let Some(cleanup) = enclosing.cleanup {
                    out.extend(self.live_in[&cleanup].iter().copied());
                }
            }
        }

        // Walk the block backwards: a value read here is live before this point,
        // a value defined here is not live before it.
        let mut live = out.clone();
        if let Some(terminator) = &block.terminator {
            live.extend(terminator.operands());
        }
        for &inst_id in block.insts.iter().rev() {
            let Some(data) = func.inst(inst_id) else {
                continue;
            };
            for result in &data.results {
                live.remove(result);
            }
            live.extend(data.inst.operands());
        }
        for param in &block.params {
            live.remove(param);
        }

        let changed = self.live_out[&id] != out || self.live_in[&id] != live;
        self.live_out.insert(id, out);
        self.live_in.insert(id, live);
        changed
    }
}

/// Values live immediately after each instruction of a block, in program order.
///
/// Computed by replaying the block backwards from its exit set. Kept separate
/// from the fixed point because it is only needed for the blocks that contain a
/// point worth describing, and computing it everywhere would be work thrown away.
pub fn live_after_each_inst(
    func: &Function,
    block: BlockId,
    liveness: &Liveness,
) -> Vec<HashSet<ValueId>> {
    let data = func.block(block).expect("block belongs to this function");

    let mut live = liveness.live_out(block).clone();
    if let Some(terminator) = &data.terminator {
        live.extend(terminator.operands());
    }

    let mut after = vec![HashSet::new(); data.insts.len()];
    for (position, &inst_id) in data.insts.iter().enumerate().rev() {
        after[position] = live.clone();

        let Some(inst) = func.inst(inst_id) else {
            continue;
        };
        for result in &inst.results {
            live.remove(result);
        }
        live.extend(inst.inst.operands());
    }
    after
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ConstDecl, FuncBuilder, Function, ScalarBits, Signature};
    use crate::repr::Repr;
    use crate::types::TypeRegistry;
    use crate::unwind::{Handler, Tag};

    /// A value only the HANDLER reads is live for every block the region covers.
    ///
    /// The edge that carries it is the throw, which no terminator names — so a
    /// walk over successors alone answers "dead" for a value the program is
    /// about to read. What that wrong answer costs is two things at once: the
    /// frame rewrite leaves it in a register across a suspension, and the
    /// collector leaves it out of the root set at every allocation inside the
    /// region.
    #[test]
    fn a_value_only_the_handler_reads_is_live_inside_the_region() {
        let mut func = Function::new(Signature {
            params: vec![Repr::Tagged],
            returns: vec![Repr::Tagged],
            ..Signature::default()
        });
        let entry = func.entry;
        let held = func.block(entry).expect("entry exists").params[0];
        let types = TypeRegistry::new();

        let mut b = FuncBuilder::new(&mut func, &types, entry);
        let handler = b.create_block();
        b.add_block_param(handler, Repr::Tagged);
        let protected = b.create_block();
        b.jump(protected, &[]).expect("an ordinary edge");

        // The protected block reads NOTHING that outlives it: what makes `held`
        // live here is only the possibility of leaving through the handler.
        let mut b = FuncBuilder::new(&mut func, &types, protected);
        b.open_region(
            vec![Handler {
                tag: Tag(1),
                block: handler,
            }],
            None,
        );
        let answer = b.declare_const(ConstDecl::Scalar {
            repr: Repr::Tagged,
            bits: ScalarBits(0),
        });
        let answer = b.use_const(answer);
        b.ret(&[answer]);
        b.close_region();

        let mut b = FuncBuilder::new(&mut func, &types, handler);
        b.ret(&[held]);

        let liveness = Liveness::compute(&func);
        assert!(
            liveness.live_out(protected).contains(&held),
            "the parameter is read by the handler, and the only way there from \
             this block is the throw — so it is live leaving it"
        );
    }
}
