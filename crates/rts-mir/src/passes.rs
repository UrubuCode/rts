//! Passes over a finished graph.
//!
//! Generic over the domain, by rule 2: a pass here reads effects, dominance and
//! the language's own answers through [`Domain`], and never a primitive's meaning.
//!
//! The first one is the one the stage was built to make possible at all. Every
//! other pass on the list — guard hoisting, CSE, LICM, scalar replacement — needs
//! the same two things this one needs: a graph to iterate over, and an effect
//! summary to say what may move.

use crate::cfg::{Func, Op, Terminator};
use crate::domain::Domain;
use crate::effect::Effect;
use crate::infer::infer;

/// What a pass did, so that a caller can say so instead of claiming it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Refined {
    /// How many instructions carry a narrower effect than they did.
    pub narrowed: usize,
    /// How many were left alone because the domain answered a wider effect from
    /// narrower types.
    ///
    /// Not zero-or-a-bug: a domain may legitimately know less about a joined type
    /// than the lowering knew about the two it was joined from. What it may not do
    /// is answer a wider effect for a NARROWER type, and this counts the cases the
    /// pass refused to apply so a front end can look at them rather than wonder.
    pub refused: usize,
}

/// Recomputes every operation's effect from what inference proved, narrowing only.
///
/// # Why this exists as a pass and not as better lowering
///
/// A lowering decides an instruction's effect when it pushes it, from what it knows
/// there. Inside a loop that is systematically less than the truth: a header's
/// parameter type is the join of what arrives from before the loop and what arrives
/// across the back edge, and the second is unknown until the body has been lowered
/// — which needs the parameter to already exist. So the lowering records the
/// domain's top and every operation over a carried value comes out pessimistic.
///
/// Measured on this shape before the pass existed, with `rts mir`:
///
/// ```text
/// b1(v3, v4):
///   v5 = lessthan(v4, v0)   ; calls|throws
///   v7 = add(v3, v4)        ; calls|throws
/// ```
///
/// Every operation of a loop whose values are all numbers, marked as possibly
/// reaching code the program wrote — which pins each of them where it is for every
/// pass that would move it.
///
/// Inference answers the same question over the FINISHED graph, iterating to a
/// fixed point, so the answer it gives for a header parameter is the join that
/// could not be computed in one pass. Asking the domain again with those types is
/// the whole of this.
///
/// # Narrowing only, and checked rather than trusted
///
/// The pass applies an effect only when it is a subset of the one already there. A
/// wider answer is discarded and counted: a domain whose `effect_of` is not monotone
/// in its types would otherwise make an operation look movable when it is not, and
/// that is the silent direction. Sound in the other direction costs nothing but a
/// missed narrowing.
pub fn refine_effects<D: Domain>(func: &mut Func, domain: &D) -> Refined {
    let types = infer(func, domain);
    let mut out = Refined::default();
    // The instruction list is flat, and an instruction's operands are defined
    // before it, so one walk in order is enough — there is nothing here that
    // changes a type, only what is recorded about an effect.
    for at in 0..func.insts.len() {
        let Op::Prim { prim, args } = &func.insts[at].op else {
            continue;
        };
        let of_args: Vec<D::Type> = args.iter().map(|held| types.of(*held).clone()).collect();
        let asked = domain.effect_of(*prim, &of_args);
        let held = func.insts[at].effect;
        if asked == held {
            continue;
        }
        // ASKED BY NAME, and this was the same question written twice: `held.has(asked)`
        // inline here, and `claims_no_more` defined below with nothing calling it. Rule 3
        // -- a rule stated once, where it is decided -- and a function with no caller is
        // rule 10's gap rather than a feature.
        match claims_no_more(asked, held) {
            // A subset: everything the new answer claims was already claimed, and
            // it claims less.
            true => {
                func.insts[at].effect = asked;
                out.narrowed += 1;
            }
            false => out.refused += 1,
        }
    }
    out
}

/// Whether an effect claims nothing the other does not.
///
/// Here rather than on [`Effect`] because subset is only a question a pass asks,
/// and a method on the type would invite a client to treat it as an ordering —
/// which `effect.rs` records these flags as deliberately not having.
pub fn claims_no_more(inner: Effect, outer: Effect) -> bool {
    outer.has(inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{Const, FuncBuilder, Prim, Terminator};
    use crate::guard::{Assertion, Tier};
    use crate::verify::verify;

    /// A domain with two types and one primitive, whose effect depends on whether
    /// both operands are known numbers.
    struct Pair;

    #[derive(Clone, PartialEq, Eq, Debug)]
    enum Known {
        Nothing,
        Number,
        Anything,
    }

    const COMBINE: Prim = Prim(0);

    impl Domain for Pair {
        type Type = Known;

        fn top(&self) -> Known {
            Known::Anything
        }
        fn bottom(&self) -> Known {
            Known::Nothing
        }
        fn join(&self, left: &Known, right: &Known) -> Known {
            match (left, right) {
                (Known::Nothing, other) | (other, Known::Nothing) => other.clone(),
                (one, two) if one == two => one.clone(),
                _ => Known::Anything,
            }
        }
        fn of_const(&self, _value: &Const) -> Known {
            Known::Number
        }
        fn transfer(&self, _prim: Prim, args: &[Known]) -> Known {
            match args.iter().all(|held| *held == Known::Number) {
                true => Known::Number,
                false => Known::Anything,
            }
        }
        fn of_entry(&self, _entry: crate::cfg::EntryId) -> Known {
            Known::Anything
        }
        fn narrow(&self, _assertion: Assertion, _of: &Known) -> Known {
            Known::Number
        }
        fn effect_of(&self, _prim: Prim, args: &[Known]) -> Effect {
            match args.iter().all(|held| *held == Known::Number) {
                true => Effect::PURE,
                false => Effect::CALLS_USER.and(Effect::THROWS),
            }
        }
        fn truth_of(&self, _of: &Known) -> Option<bool> {
            None
        }
    }

    /// The loop shape the pass exists for: a header parameter the lowering could
    /// only call unknown, and an operation over it that inference proves numeric.
    #[test]
    fn a_loop_carried_value_loses_its_pessimistic_effect() {
        let mut build = FuncBuilder::new(Tier::Generic);
        let header = build.block();
        let carried = build.param(header);
        let start = build.push(Op::Const(Const::Int(0)), Effect::PURE, Default::default());
        build.end(Terminator::Jump {
            target: header,
            args: vec![start],
        });
        build.switch_to(header);
        // As the lowering would: the header's type was unknown here, so the
        // pessimistic effect is what gets recorded.
        let next = build.push(
            Op::Prim {
                prim: COMBINE,
                args: vec![carried],
            },
            Effect::CALLS_USER.and(Effect::THROWS),
            Default::default(),
        );
        build.end(Terminator::Jump {
            target: header,
            args: vec![next],
        });
        let mut func = build.finish();
        assert_eq!(verify(&func), Ok(()));

        let refined = refine_effects(&mut func, &Pair);
        assert_eq!(refined.narrowed, 1);
        assert_eq!(refined.refused, 0);
        assert!(func.insts[1].effect.is_pure());
        // And the graph is still well formed, which a pass must leave true.
        assert_eq!(verify(&func), Ok(()));
    }

    /// An operation whose operands really are unknown keeps its effect.
    #[test]
    fn an_operation_over_a_parameter_keeps_what_it_had() {
        let mut build = FuncBuilder::new(Tier::Generic);
        let entry = build.current();
        let given = build.param(entry);
        let held = build.push(
            Op::Prim {
                prim: COMBINE,
                args: vec![given],
            },
            Effect::CALLS_USER.and(Effect::THROWS),
            Default::default(),
        );
        build.end(Terminator::Return(Some(held)));
        let mut func = build.finish();

        let refined = refine_effects(&mut func, &Pair);
        assert_eq!(refined.narrowed, 0);
        assert!(func.insts[0].effect.has(Effect::CALLS_USER));
    }

    /// A wider answer is discarded and counted, which is the check that stops an
    /// inconsistent domain making an operation look movable.
    #[test]
    fn a_wider_answer_is_refused_rather_than_applied() {
        let mut build = FuncBuilder::new(Tier::Generic);
        let entry = build.current();
        let given = build.param(entry);
        // Recorded as PURE although the operand is unknown — which is what an
        // inconsistent domain, or a hand-built graph, can produce.
        let held = build.push(
            Op::Prim {
                prim: COMBINE,
                args: vec![given],
            },
            Effect::PURE,
            Default::default(),
        );
        build.end(Terminator::Return(Some(held)));
        let mut func = build.finish();

        let refined = refine_effects(&mut func, &Pair);
        assert_eq!(refined.refused, 1);
        assert_eq!(refined.narrowed, 0);
        assert!(func.insts[0].effect.is_pure(), "the pass may not widen");
    }

    #[test]
    fn a_graph_with_nothing_to_narrow_reports_nothing() {
        let mut build = FuncBuilder::new(Tier::Generic);
        let one = build.push(Op::Const(Const::Int(1)), Effect::PURE, Default::default());
        build.end(Terminator::Return(Some(one)));
        let mut func = build.finish();
        assert_eq!(refine_effects(&mut func, &Pair), Refined::default());
    }
}

/// How many guards a pass removed for proving nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Dropped {
    /// Guards removed.
    pub guards: usize,
}

/// Removes every guard whose operand is already proved what it asserts.
///
/// # Why a pass and not a decision at the lowering
///
/// Because a lowering cannot know. It records what it knows as each instruction is
/// PUSHED, and a loop header's parameter is created before its back edge exists — so its
/// type is the top of the lattice there, whatever the two predecessors will turn out to
/// agree about. Inference runs over a finished graph and knows better.
///
/// The case that made this necessary: `while (a < b) { a = a - 1; }` over a parameter
/// guarded at the entry. The header's parameter joins a proved double with a
/// subtraction's result, which is a double, so nothing about it needs checking — and the
/// lowering had already emitted a guard on it, in the header, where a side exit cannot be
/// built. One redundant guard turned the whole function away.
///
/// This is the shape README rule 6 exists for: a guard is a value in the dataflow, so a
/// pass reaches it like anything else. A guard decided at emission could only ever be
/// removed by the emitter that wrote it.
///
/// # What it does to the instruction, and why it is not deleted
///
/// The instruction is unlisted from its block and its uses are pointed at the operand.
/// It stays in the function's flat list unreferenced, because the list is indexed by
/// [`crate::cfg::InstId`] and compacting it would renumber every instruction after it —
/// for no gain, since nothing walks the list expecting every entry to be live.
///
/// Its deoptimisation point goes unused, which is allowed: `guard::pair` is
/// one-directional precisely so a pass may remove a guard without the other tier having
/// to be rewritten.
pub fn drop_proved_guards<D: Domain>(func: &mut Func, domain: &D) -> Dropped {
    let types = infer(func, domain);
    let mut out = Dropped::default();
    let mut replace: std::collections::BTreeMap<crate::cfg::ValueId, crate::cfg::ValueId> =
        std::collections::BTreeMap::new();

    for at in 0..func.insts.len() {
        let Op::Guard { assertion, on, .. } = func.insts[at].op else {
            continue;
        };
        let of = types.of(on).clone();
        // PROVES NOTHING NEW, which is the exact test: narrowing an already-narrow type
        // answers the same type. A guard that would narrow is kept, and one over a value
        // nothing is known about narrows and is therefore kept too.
        if domain.narrow(assertion, &of) != of {
            continue;
        }
        let result = func.insts[at].result;
        // THROUGH ANY CHAIN, because two guards over one value leave the second reading
        // the first's result: dropping both has to send the uses all the way back.
        let mut target = on;
        while let Some(held) = replace.get(&target) {
            target = *held;
        }
        replace.insert(result, target);
        out.guards += 1;
    }
    if replace.is_empty() {
        return out;
    }

    let dropped: std::collections::BTreeSet<crate::cfg::InstId> = (0..func.insts.len())
        .map(|at| crate::cfg::InstId(at as u32))
        .filter(|held| replace.contains_key(&func.inst(*held).result))
        .collect();
    for block in 0..func.blocks.len() {
        func.blocks[block]
            .insts
            .retain(|held| !dropped.contains(held));
    }

    let of = |held: crate::cfg::ValueId| replace.get(&held).copied().unwrap_or(held);
    for inst in &mut func.insts {
        match &mut inst.op {
            Op::Prim { args, .. } => {
                for held in args.iter_mut() {
                    *held = of(*held);
                }
            }
            Op::Call { receiver, args, .. } => {
                if let Some(held) = receiver.as_mut() {
                    *held = of(*held);
                }
                for held in args.iter_mut() {
                    *held = of(*held);
                }
            }
            Op::Suspend { value } => {
                if let Some(held) = value.as_mut() {
                    *held = of(*held);
                }
            }
            Op::Guard { on, .. } => *on = of(*on),
            Op::Const(_) => {}
        }
    }
    for block in &mut func.blocks {
        let Some(end) = block.terminator.as_mut() else {
            continue;
        };
        match end {
            Terminator::Jump { args, .. } => {
                for held in args.iter_mut() {
                    *held = of(*held);
                }
            }
            Terminator::Branch {
                condition,
                then_args,
                else_args,
                ..
            } => {
                *condition = of(*condition);
                for held in then_args.iter_mut().chain(else_args.iter_mut()) {
                    *held = of(*held);
                }
            }
            Terminator::Return(Some(held)) | Terminator::Raise(held) => *held = of(*held),
            Terminator::Return(None)
            | Terminator::Fall(_)
            | Terminator::CleanupDone
            | Terminator::Unreachable => {}
        }
    }
    out
}
