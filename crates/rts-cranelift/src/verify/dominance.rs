//! Which blocks dominate which, in time proportional to the program.
//!
//! # Why this is its own module and not the matrix it replaces
//!
//! Nothing in this workspace answered "does this block dominate that one" — the
//! search that rule 0b asks for found one private helper inside `rules.rs` and
//! nothing else. The nearest neighbour is `lower::reachable_first`, which orders
//! blocks for emission and computes no dominance at all.
//!
//! What was there was the textbook iterative bitmap: a `Vec<Vec<bool>>` of
//! `blocks × blocks`, rebuilt to a fixed point, with a fresh `Vec<bool>` of the
//! block count allocated per block per round. That is `rounds × blocks²` work
//! and `blocks²` memory, and it was **the entire compile cost** of any function
//! large enough to notice. Measured on this tree, one function whose body is `n`
//! property reads — no `try`, no `finally`, so nothing that reads dominance at
//! all:
//!
//! | statements | blocks | `prepare` |
//! |---:|---:|---:|
//! | 100 | 1 330 | 68 ms |
//! | 200 | 2 658 | 542 ms |
//! | 400 | 5 258 | 4 578 ms |
//!
//! Blocks double, time goes up eightfold: cubic, because the round count grows
//! with the block count too. Every other phase over the same programs is linear
//! — `emit` 0.7 → 2.1 ms, `place` 19 → 98 ms.
//!
//! # The algorithm, and what it costs instead
//!
//! Cooper, Harvey and Kennedy, *A Simple, Fast Dominance Algorithm* (2001): keep
//! one immediate dominator per block, walk the reverse postorder to a fixed
//! point, and intersect two blocks by climbing the two `idom` chains toward
//! whichever has the higher postorder number. One `u32` per block instead of a
//! row of bools, and a handful of passes rather than a number that scales with
//! the graph.
//!
//! Queries then have to stay cheap, because [`check_cleanups`] asks one per
//! candidate per protected block. Climbing the `idom` chain per query would be
//! `O(depth)`; instead the dominator tree is walked once and each block records
//! the interval `[enter, leave)` of that walk. `a` dominates `b` exactly when
//! `b`'s interval is inside `a`'s, which is two comparisons.
//!
//! [`check_cleanups`]: super::rules
//!
//! # The answer for a block the entry cannot reach, and why it is `true`
//!
//! A handler block and a cleanup entry have **no predecessor in this graph**:
//! the unwinder enters them, and the unwinder is not an edge. The matrix gave
//! them their initial value — dominated by everything — and that is load
//! bearing rather than incidental. `rules.rs` records what happened when an
//! earlier rule did otherwise: every `try`/`catch` in the corpus began reporting
//! `CleanupReadsOutsideItself` about the environment pointer, a value the entry
//! block defines.
//!
//! So this reproduces it exactly: a block unreachable from the entry is
//! dominated by every block, which admits more and refuses nothing. The same
//! answer covers genuinely unreachable code, where it is the conservative one
//! for the same reason — a block nothing reaches admits nothing extra.

use crate::ir::{BlockId, Function};

/// The dominator tree of one function, with constant-time queries.
/// The immediate dominators themselves are not kept: they are what the walk
/// below consumes, and a query is answered by the intervals rather than by
/// climbing a chain, so holding them would be state with no reader.
pub(super) struct Dominance {
    /// Position of each block in the walk of the dominator tree.
    enter: Vec<u32>,
    /// One past the last position of the block's subtree.
    leave: Vec<u32>,
    /// Whether the entry reaches the block at all.
    reached: Vec<bool>,
}

impl Dominance {
    /// Computes the dominator tree.
    pub(super) fn of(func: &Function) -> Self {
        let count = func.blocks().count();
        let entry = func.entry.index();

        // Postorder over the reachable subgraph, iteratively: a deeply nested
        // body must not overflow the compiler's own stack, which is the one
        // failure a compiler may not have — the same reason
        // `lower::reachable_first` is written this way.
        let mut order = Vec::with_capacity(count);
        let mut reached = vec![false; count];
        if entry < count {
            let mut stack = vec![(entry, false)];
            while let Some((index, expanded)) = stack.pop() {
                if expanded {
                    order.push(index);
                    continue;
                }
                if reached[index] {
                    continue;
                }
                reached[index] = true;
                stack.push((index, true));
                if let Some(block) = func.block(BlockId(index as u32))
                    && let Some(terminator) = &block.terminator
                {
                    for successor in terminator.successors() {
                        let next = successor.index();
                        if next < count && !reached[next] {
                            stack.push((next, false));
                        }
                    }
                }
            }
        }

        // `post[b]` is what `intersect` compares: climbing toward the higher
        // number walks toward the entry, which has the highest of all.
        let mut post = vec![0u32; count];
        for (position, &index) in order.iter().enumerate() {
            post[index] = position as u32;
        }

        // Predecessors of the reachable blocks only. An unreachable predecessor
        // contributes nothing to an intersection — the matrix said so by leaving
        // its row all `true` — so leaving it out is the same answer.
        let mut predecessors: Vec<Vec<u32>> = vec![Vec::new(); count];
        for (block, data) in func.blocks() {
            if !reached[block.index()] {
                continue;
            }
            let Some(terminator) = &data.terminator else {
                continue;
            };
            for successor in terminator.successors() {
                if let Some(slot) = predecessors.get_mut(successor.index()) {
                    slot.push(block.index() as u32);
                }
            }
        }

        let mut idom: Vec<Option<u32>> = vec![None; count];
        if entry < count {
            idom[entry] = Some(entry as u32);
        }

        // Reverse postorder, skipping the entry: its dominator is itself and
        // never moves.
        let mut changed = true;
        while changed {
            changed = false;
            for &index in order.iter().rev() {
                if index == entry {
                    continue;
                }
                let mut candidate: Option<u32> = None;
                for &predecessor in &predecessors[index] {
                    if idom[predecessor as usize].is_none() {
                        continue;
                    }
                    candidate = Some(match candidate {
                        None => predecessor,
                        Some(held) => intersect(&idom, &post, predecessor, held),
                    });
                }
                if candidate.is_some() && idom[index] != candidate {
                    idom[index] = candidate;
                    changed = true;
                }
            }
        }

        // The tree, walked once, so a query is an interval test rather than a
        // climb. Children in block order, because rule 13 asks for a walk that
        // does not change between builds — and this one feeds a `reaching` set
        // that a person reads in a diff.
        let mut children: Vec<Vec<u32>> = vec![Vec::new(); count];
        for index in 0..count {
            if index == entry {
                continue;
            }
            if let Some(parent) = idom[index] {
                children[parent as usize].push(index as u32);
            }
        }

        let mut enter = vec![0u32; count];
        let mut leave = vec![0u32; count];
        let mut clock = 0u32;
        if entry < count {
            let mut stack = vec![(entry as u32, false)];
            while let Some((index, closing)) = stack.pop() {
                match closing {
                    true => leave[index as usize] = clock,
                    false => {
                        enter[index as usize] = clock;
                        clock += 1;
                        stack.push((index, true));
                        for &child in children[index as usize].iter().rev() {
                            stack.push((child, false));
                        }
                    }
                }
            }
        }

        Dominance {
            enter,
            leave,
            reached,
        }
    }

    /// Whether `candidate` dominates `block`, `block` itself included.
    ///
    /// Answers `true` for every candidate when the entry cannot reach `block` —
    /// see this module's header for why that case is the conservative one and
    /// what regressed when it was answered the other way.
    pub(super) fn dominates(&self, candidate: BlockId, block: BlockId) -> bool {
        let (candidate, block) = (candidate.index(), block.index());
        if block >= self.reached.len() || !self.reached[block] {
            return true;
        }
        if candidate >= self.reached.len() || !self.reached[candidate] {
            return false;
        }
        self.enter[candidate] <= self.enter[block] && self.leave[block] <= self.leave[candidate]
    }
}

/// The nearest block that dominates both, by climbing the two chains.
fn intersect(idom: &[Option<u32>], post: &[u32], mut a: u32, mut b: u32) -> u32 {
    while a != b {
        while post[a as usize] < post[b as usize] {
            match idom[a as usize] {
                Some(next) if next != a => a = next,
                // Only the entry is its own dominator, and it has the highest
                // postorder number, so the loop above cannot reach here for a
                // graph this walked. Stopping rather than looping is what keeps
                // a malformed function a rejected one instead of a hang.
                _ => return b,
            }
        }
        while post[b as usize] < post[a as usize] {
            match idom[b as usize] {
                Some(next) if next != b => b = next,
                _ => return a,
            }
        }
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ConstDecl, FuncBuilder, Function, ScalarBits, Signature};
    use crate::repr::Repr;
    use crate::types::TypeRegistry;

    /// A function with one boolean constant available to branch on.
    fn start() -> (Function, TypeRegistry) {
        (Function::new(Signature::default()), TypeRegistry::new())
    }

    #[test]
    fn an_arm_of_a_branch_does_not_dominate_the_block_after_it() {
        let (mut func, types) = start();
        let entry = func.entry;
        let mut b = FuncBuilder::new(&mut func, &types, entry);
        let left = b.create_block();
        let right = b.create_block();
        let join = b.create_block();
        let cond = b.declare_const(ConstDecl::Scalar {
            repr: Repr::Bool,
            bits: ScalarBits(1),
        });
        let cond = b.use_const(cond);
        b.branch(cond, (left, &[]), (right, &[])).expect("a branch");
        let mut b = FuncBuilder::new(&mut func, &types, left);
        b.jump(join, &[]).expect("a jump");
        let mut b = FuncBuilder::new(&mut func, &types, right);
        b.jump(join, &[]).expect("a jump");
        let mut b = FuncBuilder::new(&mut func, &types, join);
        b.ret(&[]);

        let dominance = Dominance::of(&func);
        assert!(
            dominance.dominates(entry, join),
            "control cannot reach the join without passing the entry"
        );
        assert!(
            !dominance.dominates(left, join),
            "the other arm reaches the join without passing this one"
        );
        assert!(
            !dominance.dominates(right, join),
            "the other arm reaches the join without passing this one"
        );
        assert!(dominance.dominates(join, join), "a block dominates itself");
    }

    #[test]
    fn a_block_the_entry_cannot_reach_is_dominated_by_everything() {
        // The answer a handler and a cleanup entry rely on: the unwinder enters
        // them and the unwinder is not an edge, so nothing in this graph reaches
        // them. `rules.rs` records what the other answer cost — every try/catch
        // in the corpus reporting `CleanupReadsOutsideItself` about a value the
        // entry block defines.
        let (mut func, types) = start();
        let entry = func.entry;
        let mut b = FuncBuilder::new(&mut func, &types, entry);
        let orphan = b.create_block();
        b.ret(&[]);
        let mut b = FuncBuilder::new(&mut func, &types, orphan);
        b.ret(&[]);

        let dominance = Dominance::of(&func);
        assert!(
            dominance.dominates(entry, orphan),
            "an unreachable block admits every definition, which is what every \
             try/catch in the corpus depends on"
        );
        assert!(
            !dominance.dominates(orphan, entry),
            "a block nothing reaches dominates nothing"
        );
    }

    #[test]
    fn a_loop_body_is_dominated_by_the_head_and_not_the_other_way() {
        let (mut func, types) = start();
        let entry = func.entry;
        let mut b = FuncBuilder::new(&mut func, &types, entry);
        let head = b.create_block();
        let body = b.create_block();
        let done = b.create_block();
        b.jump(head, &[]).expect("a jump");
        let mut b = FuncBuilder::new(&mut func, &types, head);
        let cond = b.declare_const(ConstDecl::Scalar {
            repr: Repr::Bool,
            bits: ScalarBits(1),
        });
        let cond = b.use_const(cond);
        b.branch(cond, (body, &[]), (done, &[])).expect("a branch");
        let mut b = FuncBuilder::new(&mut func, &types, body);
        b.jump(head, &[]).expect("the back edge");
        let mut b = FuncBuilder::new(&mut func, &types, done);
        b.ret(&[]);

        let dominance = Dominance::of(&func);
        assert!(
            dominance.dominates(head, body),
            "the only way into the body is through the head"
        );
        assert!(
            !dominance.dominates(body, head),
            "the head runs once before the body ever does, so the back edge must \
             not make the body dominate it"
        );
        assert!(
            dominance.dominates(head, done),
            "the head dominates what leaves the loop as well"
        );
    }
}
