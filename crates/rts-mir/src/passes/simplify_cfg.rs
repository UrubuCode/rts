//! A branch whose condition is a constant is a jump, and a block nothing reaches is gone.
//!
//! The constant folder answers `1 === 1` with the constant `true`; this is what a graph
//! does with that. It is the SAFE half of CFG simplification and stops there on purpose:
//! it never merges a block into its single predecessor and never rewrites a block
//! parameter, because both need dominance to be sure no value is read where it was not
//! yet defined -- and this crate has the order-based check of `verify` and no dominator
//! tree. Taking an edge away needs neither: every surviving edge keeps its own argument
//! list, and a parameter is only ever supplied by edges, so a parameter of a block that
//! lost a predecessor is still supplied by each predecessor that is left.
//!
//! # What counts as a root
//!
//! The entry, and every block a region names -- its handler, its cleanup, its resumption
//! target. Nothing jumps to those (an exception edge is not a jump, see
//! [`crate::cfg::Terminator::Raise`]), so a reachability walk over successors alone would
//! delete every handler of the function. Keeping a handler whose protected body was
//! deleted costs one block; deleting one a raise can still reach costs the program.

use std::collections::BTreeMap;

use crate::cfg::{BlockId, Const, Func, Op, Terminator, ValueId};

/// Turns every branch over a constant truth value into a jump to the taken side, then
/// deletes the blocks that no longer have a way in. Answers how many blocks it removed
/// and how many branches it turned into jumps, summed.
pub fn simplify_cfg(func: &mut Func) -> usize {
    let mut truths: BTreeMap<ValueId, bool> = BTreeMap::new();
    for block in func.block_ids() {
        for inst in &func.block(block).insts {
            if let Op::Const(Const::Bool(truth)) = &func.inst(*inst).op {
                truths.insert(func.inst(*inst).result, *truth);
            }
        }
    }
    let mut changed = 0;
    for block in &mut func.blocks {
        let Some(Terminator::Branch {
            condition,
            then_block,
            then_args,
            else_block,
            else_args,
        }) = &block.terminator
        else {
            continue;
        };
        let Some(truth) = truths.get(condition) else {
            continue;
        };
        block.terminator = Some(match truth {
            true => Terminator::Jump {
                target: *then_block,
                args: then_args.clone(),
            },
            false => Terminator::Jump {
                target: *else_block,
                args: else_args.clone(),
            },
        });
        changed += 1;
    }
    changed + remove_unreachable(func)
}

/// Deletes every block no path from a root reaches, renumbering the survivors in their
/// order and rewriting each edge and region that named one.
fn remove_unreachable(func: &mut Func) -> usize {
    let count = func.blocks.len();
    let mut live = vec![false; count];
    let mut work = vec![func.entry()];
    for region in &func.regions {
        work.extend(region.handler);
        work.extend(region.cleanup);
        work.extend(region.resume_return);
    }
    while let Some(block) = work.pop() {
        if std::mem::replace(&mut live[block.0 as usize], true) {
            continue;
        }
        if let Some(end) = &func.block(block).terminator {
            work.extend(end.successors());
        }
    }
    let gone = live.iter().filter(|held| !**held).count();
    if gone == 0 {
        return 0;
    }

    let mut renumber = vec![None; count];
    let mut next = 0;
    for (at, held) in live.iter().enumerate() {
        if *held {
            renumber[at] = Some(BlockId(next));
            next += 1;
        }
    }
    let to = |block: BlockId| renumber[block.0 as usize].expect("a live block names a live block");

    let mut at = 0;
    func.blocks.retain(|_| {
        at += 1;
        live[at - 1]
    });
    let mut at = 0;
    func.block_regions.retain(|_| {
        at += 1;
        live.get(at - 1).copied().unwrap_or(true)
    });
    for block in &mut func.blocks {
        match block.terminator.as_mut() {
            Some(Terminator::Jump { target, .. }) => *target = to(*target),
            Some(Terminator::Branch {
                then_block,
                else_block,
                ..
            }) => {
                *then_block = to(*then_block);
                *else_block = to(*else_block);
            }
            _ => {}
        }
    }
    for region in &mut func.regions {
        // A region may name a block that went (a handler is a root, so this is the
        // none-at-all case rather than a dangling one): kept as it was would be an index
        // past the end.
        for slot in [&mut region.handler, &mut region.cleanup, &mut region.resume_return] {
            *slot = slot.map(to);
        }
    }
    gone
}
