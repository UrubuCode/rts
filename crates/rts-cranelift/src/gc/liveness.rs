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
            if let Some(result) = data.result {
                live.remove(&result);
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
        if let Some(result) = inst.result {
            live.remove(&result);
        }
        live.extend(inst.inst.operands());
    }
    after
}
