//! Types by abstract interpretation, to a fixed point.
//!
//! The pass that could not exist before this crate did. A syntax-directed
//! traversal can compute a type for `x + 1`; it cannot compute one for `x` inside
//! a loop, because the answer depends on the back edge and the back edge has not
//! been walked yet. Iterating until nothing changes is the whole difference, and
//! it needs a graph to iterate over.
//!
//! # Termination
//!
//! A worklist over blocks, joining into each block's parameters from every
//! predecessor, stopping when no block's answer changed. It terminates because
//! [`Domain::join`] never narrows: each value's type only moves up the lattice, so
//! the number of rounds is bounded by the lattice's height.
//!
//! # What a change re-queues
//!
//! The successors of the block where it happened, AND every block that reads the value
//! that moved -- an instruction's operand, or an argument a terminator hands to another
//! block's parameter. Successors alone are not enough, and it was measured: a loop's
//! carried value joined as the entry's narrow type on the first pass, the exit block
//! received it through a block that only forwarded it and so never changed, and the exit
//! kept that narrow type for a value the back edge had widened. The machine was then
//! asked to narrow a wide representation to pass it -- 26 functions of a front end's
//! test suite refused, and a lattice that is WRONG rather than merely coarse is a wrong
//! answer the day something trusts it. `tests/toy_domain.rs` pins the graph.
//!
//! # What a branch teaches
//!
//! A block entered by exactly ONE edge, from a branch, knows the branch's condition went
//! the way that edge says -- on every execution, because there is no other way in. So
//! [`Domain::refine`] may narrow the condition's operands FOR THAT BLOCK (and for any
//! block it alone enters, in turn): `i < 1000` held, so `i` is below 1000 here. The
//! narrowing is a VIEW and never a store: `of[i]` stays what holds everywhere, and only
//! what this block computes from `i`, and what it hands its successors, reads the view.
//! The values are SSA, so the fact cannot be invalidated by a later write.
//!
//! # Widening
//!
//! A tall lattice (an integer range) needs the climb cut short. After a block has been
//! analysed several times, [`Domain::widen`] replaces a parameter's new type by one at
//! least as wide as BOTH the old and the new -- so a parameter's type never narrows once
//! widening applies, and a bound that is still moving jumps to the extreme.
//!
//! A domain whose `join` is not monotone does not terminate, and that is the
//! front end's bug rather than this pass's. `ROUNDS` bounds it anyway — not to
//! paper over such a domain but so the failure is a panic naming the cause instead
//! of a compiler that hangs, which is what `lost-roots.md` calls the difference
//! between a defect and a mystery.

use crate::cfg::{Func, Op, Prim, Terminator, ValueId};
use crate::domain::{Domain, Forward};

/// What is known about every value of a function.
pub struct Types<T> {
    of: Vec<T>,
}

impl<T: Clone> Types<T> {
    /// What is known about one value.
    pub fn of(&self, value: ValueId) -> &T {
        &self.of[value.0 as usize]
    }
}

/// How many times a block may be re-analysed before this is called a bug.
///
/// Generous rather than tight: a legitimate lattice is a handful of levels deep,
/// so a hundred rounds over one block means `join` narrowed somewhere.
const ROUNDS: usize = 100;

/// How many levels a value may climb, counting the widening's jumps -- the lattice's height
/// as the fixed point sees it: bottom, a range, a half-widened range, the whole `Int32`,
/// `Double`, the top, with room to spare.
const CLIMB: usize = 8;

/// How many times one block may be re-analysed in a function of `values` values.
///
/// `ROUNDS` alone called a LONG CHAIN a bug. A block is re-queued each time a value it
/// reads moves, and a value that flows round a loop one link per lap (`a = b + 1; b = c +
/// 1; ...` with the reads before the writes, so every link is carried by the back edge)
/// moves its front one link per visit of the head however monotone the join is: a
/// minified bundle's hundred-variable loop needed a hundred laps and was refused as a
/// narrowing domain. The honest bound is the one the argument gives: every re-analysis is
/// caused by some value having moved, a value moves at most `CLIMB` times, so no block can
/// be re-analysed more than `CLIMB * values` times without a value moving DOWN. `ROUNDS`
/// stays as the floor, for the small function where that product is the smaller number.
fn round_limit(values: usize) -> usize {
    ROUNDS.max(CLIMB.saturating_mul(values))
}

/// How many analyses of a block pass before its parameters are widened.
///
/// Small, because a loop's climb is cut by the widening and not by waiting; not zero,
/// because the first rounds see a merge with only some predecessors computed, and
/// widening THOSE would give up a precision no loop was costing.
const WIDEN_AFTER: usize = 3;

/// How many analyses of a block pass before EVERY parameter of it is widened, loop head or
/// not -- the backstop under `loop_heads`, which sees only cycles of edges. Well under
/// `ROUNDS`, so the widening has room to settle what it cuts.
const WIDEN_EVERYWHERE: usize = 32;

/// How far up a chain of single-entry blocks a fact is inherited from.
const INHERIT: usize = 8;

/// One thing a block knows by the way it was entered: `prim(args)` answered `truth`.
struct Fact {
    prim: Prim,
    args: Vec<ValueId>,
    truth: bool,
}

/// The type of every value, computed to a fixed point.
///
/// Values a block's parameters receive are joined from every predecessor's
/// arguments; everything else follows from its operation. A guard narrows.
pub fn infer<D: Domain>(func: &Func, domain: &D) -> Types<D::Type> {
    let mut of = vec![domain.bottom(); func.values as usize];
    // The entry block's parameters are the function's own, and nothing in this
    // function supplies them: they are as unknown as the domain allows.
    for param in &func.block(func.entry()).params {
        of[param.0 as usize] = domain.top();
    }
    // SO ARE A HANDLER'S, and a resumed return's: no edge of the graph reaches either --
    // an exception edge is not an edge here, which `Terminator::Raise` states -- so
    // joining over predecessors found none and left the parameter at the BOTTOM. A
    // caught value typed as nothing then joined with anything answered the other side,
    // and a `catch (e) { e && ... }` compiled `e` as a boolean it never was.
    let entered_aside: Vec<crate::cfg::BlockId> = func
        .regions
        .iter()
        .flat_map(|region| [region.handler, region.resume_return])
        .flatten()
        .collect();
    for block in &entered_aside {
        for param in &func.block(*block).params {
            of[param.0 as usize] = domain.top();
        }
    }
    // WHO LOOKS AGAIN when a value moves -- see the module's note.
    let mut dependents: Vec<Vec<crate::cfg::BlockId>> = vec![Vec::new(); func.values as usize];
    for block in func.block_ids() {
        for inst in &func.block(block).insts {
            for read in func.reads(*inst) {
                dependents[read.0 as usize].push(block);
            }
        }
        if let Some(end) = &func.block(block).terminator {
            for read in end.reads() {
                dependents[read.0 as usize].extend(end.successors());
            }
        }
    }
    // WHAT EACH BLOCK KNOWS BY HOW IT WAS ENTERED -- see the module's note on branches.
    let facts = edge_facts(func, domain, &entered_aside);
    for block in func.block_ids() {
        // A fact reads its operands' types, so a move of one re-queues the block that
        // views through it AND the successors it hands the view to (an argument it
        // forwards may not itself have moved).
        let successors = func
            .block(block)
            .terminator
            .as_ref()
            .map(|end| end.successors())
            .unwrap_or_default();
        for fact in &facts[block.0 as usize] {
            for source in &fact.args {
                dependents[source.0 as usize].push(block);
                dependents[source.0 as usize].extend(successors.iter().copied());
            }
        }
    }
    let heads = loop_heads(func);
    let mut visits = vec![0usize; func.blocks.len()];
    let mut worklist: Vec<_> = func.block_ids().collect();
    worklist.reverse();
    let limit = round_limit(func.values as usize);
    while let Some(block) = worklist.pop() {
        visits[block.0 as usize] += 1;
        assert!(
            visits[block.0 as usize] <= limit,
            "a block was re-analysed {limit} times, which means the domain's join narrows somewhere"
        );
        let mut changed = false;
        let mut moved = Vec::new();
        // The parameters, joined from every predecessor that supplies them.
        if block != func.entry() && !entered_aside.contains(&block) {
            let params = func.block(block).params.clone();
            for (at, param) in params.iter().enumerate() {
                let mut joined = domain.bottom();
                for predecessor in func.predecessors(block) {
                    let Some(end) = &func.block(predecessor).terminator else {
                        continue;
                    };
                    for supplied in arguments_to(end, block, at) {
                        let seen = viewed(domain, &of, &facts[predecessor.0 as usize], supplied);
                        joined = domain.join(&joined, &seen);
                    }
                }
                // WIDENED at a loop head, once the block has been seen a few times. Only at
                // a head: every cycle of the graph passes through one, which is all
                // termination needs, and a parameter that is merely a function of the
                // loop's (a block inside its body) must be left exact -- widening it
                // would put the loop's own bound beyond what its guard allows. And not
                // before a few rounds, so a merge outside any loop keeps the precision
                // the first rounds give it.
                //
                // OR ANYWHERE, once a block has been seen `WIDEN_EVERYWHERE` times: a cycle
                // of VALUES that no cycle of EDGES explains (a handler is entered by no edge
                // and reads what dominates the region it guards, so a loop closed through
                // one has no back edge to find) would otherwise climb until `ROUNDS`. A
                // block seen that often is on such a cycle or in a very long chain, and
                // either way losing a little precision is the sound answer.
                let widening = (heads[block.0 as usize] && visits[block.0 as usize] > WIDEN_AFTER)
                    || visits[block.0 as usize] > WIDEN_EVERYWHERE;
                if widening {
                    joined = domain.widen(&of[param.0 as usize], &joined);
                }
                if joined != of[param.0 as usize] {
                    of[param.0 as usize] = joined;
                    changed = true;
                    moved.push(*param);
                }
            }
        }
        // The instructions, in order, each from what it reads.
        for inst in &func.block(block).insts {
            let held = func.inst(*inst);
            let computed = match &held.op {
                Op::Const(value) => domain.of_const(value),
                Op::Prim { prim, args } => {
                    let of_args: Vec<_> = args
                        .iter()
                        .map(|held| viewed(domain, &of, &facts[block.0 as usize], *held))
                        .collect();
                    domain.transfer(*prim, &of_args)
                }
                // WHAT WAS SENT IN, which nothing in this function decides: whoever
                // resumed the frame chose it. So it is the top of the lattice and not
                // the type of what was handed out -- a yield answers the argument of
                // the NEXT resumption, never its own operand, and reading the operand
                // here would be an unsound narrowing rather than a missing one.
                Op::Suspend { .. } => domain.top(),
                Op::Call { callee, .. } => match callee {
                    crate::cfg::Callee::Entry(entry) => domain.of_entry(*entry),
                    // A call to a function of this program or to whatever a value
                    // holds: nothing is known until an interprocedural pass says
                    // so, and answering `top` is the sound half of not knowing.
                    crate::cfg::Callee::Func(_) | crate::cfg::Callee::Dynamic(_) => domain.top(),
                },
                Op::Guard { assertion, on, .. } => domain.narrow(
                    *assertion,
                    &viewed(domain, &of, &facts[block.0 as usize], *on),
                ),
            };
            if computed != of[held.result.0 as usize] {
                of[held.result.0 as usize] = computed;
                changed = true;
                moved.push(held.result);
            }
        }
        if changed {
            let successors = func
                .block(block)
                .terminator
                .as_ref()
                .map(|end| end.successors())
                .unwrap_or_default();
            let readers = moved
                .iter()
                .flat_map(|value| dependents[value.0 as usize].iter().copied());
            for again in successors.into_iter().chain(readers) {
                if !worklist.contains(&again) {
                    worklist.push(again);
                }
            }
        }
    }
    Types { of }
}

/// The values a terminator supplies to `target`'s parameter at `at`.
///
/// A vector rather than an `Option` because a branch may take both arms to the
/// same block with different arguments, and joining only one of them would answer
/// something no path produces.
fn arguments_to(end: &Terminator, target: crate::cfg::BlockId, at: usize) -> Vec<ValueId> {
    let mut found = Vec::new();
    match end {
        Terminator::Jump { target: to, args } if *to == target => {
            found.extend(args.get(at).copied());
        }
        Terminator::Branch {
            then_block,
            then_args,
            else_block,
            else_args,
            ..
        } => {
            if *then_block == target {
                found.extend(then_args.get(at).copied());
            }
            if *else_block == target {
                found.extend(else_args.get(at).copied());
            }
        }
        _ => {}
    }
    found
}

/// What each block knows from how it is entered, by block index.
///
/// A block is entered by ONE edge when exactly one terminator names it, once -- so a
/// block both arms of a branch reach, or one with two predecessors, learns nothing. Such
/// a block, entered from a branch, knows the condition took that arm; entered by a
/// jump, it knows whatever its predecessor does (the predecessor ran, and ran to its
/// end). A handler or a resumed return is entered aside, by no edge at all, so it knows
/// nothing.
fn edge_facts<D: Domain>(
    func: &Func,
    domain: &D,
    entered_aside: &[crate::cfg::BlockId],
) -> Vec<Vec<Fact>> {
    // Who defines each value, when a primitive does.
    let mut defined: Vec<Option<(Prim, Vec<ValueId>)>> = vec![None; func.values as usize];
    for block in func.block_ids() {
        for inst in &func.block(block).insts {
            let held = func.inst(*inst);
            if let Op::Prim { prim, args } = &held.op {
                defined[held.result.0 as usize] = Some((*prim, args.clone()));
            }
        }
    }
    // How many edges enter each block, and from where.
    let mut entries: Vec<Vec<crate::cfg::BlockId>> = vec![Vec::new(); func.blocks.len()];
    for block in func.block_ids() {
        if let Some(end) = &func.block(block).terminator {
            for to in end.successors() {
                entries[to.0 as usize].push(block);
            }
        }
    }
    let mut own: Vec<Vec<Fact>> = Vec::new();
    for block in func.block_ids() {
        let mut found = Vec::new();
        if block != func.entry() && !entered_aside.contains(&block) {
            if let [from] = entries[block.0 as usize].as_slice() {
                if let Some(Terminator::Branch {
                    condition,
                    then_block,
                    else_block,
                    ..
                }) = &func.block(*from).terminator
                {
                    if then_block != else_block {
                        let mut truth = *then_block == block;
                        let mut at = *condition;
                        // Through the primitives that only restate their operand's truth.
                        for _ in 0..INHERIT {
                            let Some((prim, args)) = &defined[at.0 as usize] else {
                                break;
                            };
                            found.push(Fact {
                                prim: *prim,
                                args: args.clone(),
                                truth,
                            });
                            match (domain.forwards_truth(*prim), args.as_slice()) {
                                (Some(how), [only]) => {
                                    if how == Forward::Negated {
                                        truth = !truth;
                                    }
                                    at = *only;
                                }
                                _ => break,
                            }
                        }
                    }
                }
            }
        }
        own.push(found);
    }
    // INHERITED down a chain of blocks each entered by one edge: what the predecessor
    // knew, this knows. Bounded, and cut at the first block that is entered otherwise.
    let mut all: Vec<Vec<Fact>> = Vec::new();
    for block in func.block_ids() {
        let mut chain = Vec::new();
        let mut at = block;
        for _ in 0..INHERIT {
            for fact in &own[at.0 as usize] {
                chain.push(Fact {
                    prim: fact.prim,
                    args: fact.args.clone(),
                    truth: fact.truth,
                });
            }
            let back = &entries[at.0 as usize];
            if at == func.entry() || entered_aside.contains(&at) || back.len() != 1 {
                break;
            }
            at = back[0];
        }
        all.push(chain);
    }
    all
}

/// What `value` is, as a block that knows `facts` reads it.
///
/// `of[value]` narrowed by each fact that mentions it; with no fact, `of[value]` itself.
/// The domain's answer is used only when it exists, and a fact teaching nothing leaves
/// the type as it was.
fn viewed<D: Domain>(domain: &D, of: &[D::Type], facts: &[Fact], value: ValueId) -> D::Type {
    let mut held = of[value.0 as usize].clone();
    for fact in facts {
        for (at, arg) in fact.args.iter().enumerate() {
            if *arg != value {
                continue;
            }
            let mut types: Vec<D::Type> = fact
                .args
                .iter()
                .map(|other| of[other.0 as usize].clone())
                .collect();
            types[at] = held.clone();
            if let Some(narrower) = domain.refine(fact.prim, &types, fact.truth, at) {
                held = narrower;
            }
        }
    }
    held
}

/// The blocks a back edge enters: where a loop's parameters are widened.
///
/// By depth-first search from the entry, an edge to a block still on the search stack is
/// a retreating edge, and its target a loop head. EVERY cycle of a graph contains at least
/// one retreating edge (the first of its blocks the search reached is on the stack when the
/// cycle returns to it), so widening at these alone cuts every cycle -- which is what the
/// termination argument needs, and fewer widening points than "every parameter" is what
/// keeps a loop's body exact.
fn loop_heads(func: &Func) -> Vec<bool> {
    let mut heads = vec![false; func.blocks.len()];
    // 0 unseen, 1 on the stack, 2 finished.
    let mut state = vec![0u8; func.blocks.len()];
    let mut stack: Vec<(crate::cfg::BlockId, Vec<crate::cfg::BlockId>)> = Vec::new();
    let successors = |block: crate::cfg::BlockId| {
        func.block(block)
            .terminator
            .as_ref()
            .map(|end| end.successors())
            .unwrap_or_default()
    };
    // FROM EVERY BLOCK NOT YET SEEN, the entry first: a handler, a resumed return and a
    // copied cleanup are entered by no edge, and a loop inside one is a cycle the entry's
    // search never reaches -- left unwidened it would climb until `ROUNDS` called it a bug,
    // which is exactly what a `finally` containing a loop did. A depth-first FOREST still
    // has the property: every cycle holds a retreating edge of the tree it lies in.
    let roots = std::iter::once(func.entry()).chain(func.block_ids());
    for root in roots {
        if state[root.0 as usize] != 0 {
            continue;
        }
        state[root.0 as usize] = 1;
        stack.push((root, successors(root)));
        while let Some((block, pending)) = stack.last_mut() {
            match pending.pop() {
                Some(next) => match state[next.0 as usize] {
                    0 => {
                        state[next.0 as usize] = 1;
                        let after = successors(next);
                        stack.push((next, after));
                    }
                    1 => heads[next.0 as usize] = true,
                    _ => {}
                },
                None => {
                    state[block.0 as usize] = 2;
                    stack.pop();
                }
            }
        }
    }
    heads
}
