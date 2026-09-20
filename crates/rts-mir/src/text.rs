//! Printing a graph, so that what a pass changed can be read.
//!
//! `rts ir` exists for the machine's representation and it is the command that
//! found a string literal crossing into the runtime on every pass of a loop. This
//! is the same instrument one stage earlier, where the decisions a type domain and
//! a guard make are still visible — after lowering they are gone into offsets.
//!
//! # Why a legend rather than names
//!
//! Rule 4: a [`Prim`] is an opaque index here, so printing `prim#3` is the whole
//! of what this crate can honestly say. That would make the output nearly useless,
//! so the language supplies a [`Legend`] and the printer asks it. A caller with no
//! legend gets the indices, which is what [`Indices`] is.
//!
//! The alternative — a name on the instruction — was rejected because it is a
//! second copy of the language's table living in the IR, and the day the two
//! disagree the printer is the one that looks right.

use std::fmt::Write;

use crate::cfg::{Callee, Const, EntryId, Func, Op, Terminator};
use crate::guard::Assertion;
use crate::{Prim, ValueId};

/// What the language calls the things this crate only numbers.
pub trait Legend {
    /// What a primitive is called.
    fn prim(&self, prim: Prim) -> String;
    /// What an assertion asserts.
    fn assertion(&self, assertion: Assertion) -> String;
    /// What an entry point is called.
    fn entry(&self, entry: EntryId) -> String;
    /// What a declared constant is.
    fn declared(&self, index: u32) -> String;
}

/// The legend for a caller that has none: every index printed as itself.
pub struct Indices;

impl Legend for Indices {
    fn prim(&self, prim: Prim) -> String {
        format!("prim#{}", prim.0)
    }
    fn assertion(&self, assertion: Assertion) -> String {
        format!("assert#{}", assertion.0)
    }
    fn entry(&self, entry: EntryId) -> String {
        format!("entry#{}", entry.0)
    }
    fn declared(&self, index: u32) -> String {
        format!("const#{index}")
    }
}

/// One function, printed.
///
/// The effect summary is printed beside every instruction that has one, because
/// which motions are legal is the question this form exists to answer and a reader
/// cannot derive it: `PURE` and `calls|throws` are the difference between an
/// operation a pass may hoist and one it may not, and they look identical in the
/// source the program was written in.
pub fn print(func: &Func, legend: &impl Legend) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{:?} tier, {} values", func.tier, func.values);
    if !func.points.is_empty() {
        let points: Vec<String> = func
            .points
            .iter()
            .map(|held| format!("p{}", held.0))
            .collect();
        let _ = writeln!(out, "deopt points: {}", points.join(" "));
    }
    for block in func.block_ids() {
        let held = func.block(block);
        let params: Vec<String> = held.params.iter().map(value).collect();
        let _ = match params.is_empty() {
            true => writeln!(out, "\nb{}:", block.0),
            false => writeln!(out, "\nb{}({}):", block.0, params.join(", ")),
        };
        // WHICH REGION PROTECTS IT, because an exception edge has no jump to print --
        // a graph without this line looks as though nothing can leave the block except
        // through its terminator, which is the one thing a protected block does not do.
        if let Some(region) = func.region_of(block) {
            let held = func.region(region);
            let handler = match held.handler {
                Some(block) => format!("b{}", block.0),
                None => "none".to_owned(),
            };
            let _ = writeln!(out, "  ; protected by r{} -> {handler}", region.0);
        }
        // The predecessors, because reading a join means knowing what arrives and
        // a graph printed without them has to be read twice to find out.
        let from = func.predecessors(block);
        if !from.is_empty() {
            let from: Vec<String> = from.iter().map(|held| format!("b{}", held.0)).collect();
            let _ = writeln!(out, "  ; from {}", from.join(", "));
        }
        for inst in &held.insts {
            let inst = func.inst(*inst);
            let _ = writeln!(
                out,
                "  {} = {}{}",
                value(&inst.result),
                operation(&inst.op, legend),
                effect(inst.effect)
            );
        }
        let _ = match &held.terminator {
            Some(end) => writeln!(out, "  {}", terminator(end)),
            None => writeln!(out, "  ; NOT TERMINATED"),
        };
    }
    out
}

fn operation(op: &Op, legend: &impl Legend) -> String {
    match op {
        Op::Const(Const::Int(held)) => format!("{held}"),
        Op::Const(Const::Float(held)) => format!("{held}"),
        Op::Const(Const::Bool(held)) => format!("{held}"),
        Op::Const(Const::Declared(index)) => legend.declared(*index),
        Op::Prim { prim, args } => format!("{}({})", legend.prim(*prim), values(args)),
        Op::Suspend { value } => match value {
            Some(held) => format!("suspend v{}", held.0),
            None => "suspend".to_string(),
        },
        Op::Call {
            callee,
            receiver,
            args,
        } => {
            let callee = match callee {
                Callee::Entry(entry) => legend.entry(*entry),
                Callee::Func(func) => format!("f{}", func.0),
                Callee::Dynamic(value) => value_of(*value),
            };
            // The receiver is printed as part of the callee -- `v3.call` reads the
            // way the program was written, and a reader who sees no dot knows there
            // is no receiver rather than having to count arguments.
            match receiver {
                Some(held) => format!("call {}.{callee}({})", value_of(*held), values(args)),
                None => format!("call {callee}({})", values(args)),
            }
        }
        Op::Guard {
            assertion,
            on,
            point,
        } => format!(
            "guard {} of {} else p{}",
            legend.assertion(*assertion),
            value_of(*on),
            point.0
        ),
    }
}

fn terminator(end: &Terminator) -> String {
    match end {
        Terminator::Jump { target, args } => match args.is_empty() {
            true => format!("jump b{}", target.0),
            false => format!("jump b{}({})", target.0, values(args)),
        },
        Terminator::Branch {
            condition,
            then_block,
            then_args,
            else_block,
            else_args,
        } => format!(
            "branch {} -> b{}({}) else b{}({})",
            value_of(*condition),
            then_block.0,
            values(then_args),
            else_block.0,
            values(else_args)
        ),
        Terminator::Return(Some(held)) => format!("return {}", value_of(*held)),
        Terminator::Return(None) => "return".to_owned(),
        Terminator::Fall(point) => format!("fall p{}", point.0),
        Terminator::Unreachable => "unreachable".to_owned(),
    }
}

/// The effect, or nothing at all where it is pure.
///
/// Printed as a suffix rather than a column, because the interesting case is the
/// instruction that is NOT pure and a column of `PURE` is noise that hides it.
fn effect(effect: crate::Effect) -> String {
    if effect.is_pure() {
        return String::new();
    }
    let mut said = Vec::new();
    for (flag, name) in [
        (crate::Effect::READS, "reads"),
        (crate::Effect::WRITES, "writes"),
        (crate::Effect::ALLOCATES, "allocates"),
        (crate::Effect::CALLS_USER, "calls"),
        (crate::Effect::THROWS, "throws"),
    ] {
        if effect.has(flag) {
            said.push(name);
        }
    }
    format!("   ; {}", said.join("|"))
}

fn value(value: &ValueId) -> String {
    format!("v{}", value.0)
}

fn value_of(held: ValueId) -> String {
    value(&held)
}

fn values(held: &[ValueId]) -> String {
    held.iter().map(value).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::FuncBuilder;
    use crate::guard::{PointId, Tier};
    use crate::{Effect, Op, Terminator};

    #[test]
    fn a_graph_prints_its_blocks_its_arguments_and_its_predecessors() {
        let mut build = FuncBuilder::new(Tier::Generic);
        let target = build.block();
        let carried = build.param(target);
        let one = build.push(Op::Const(Const::Int(1)), Effect::PURE, Default::default());
        build.end(Terminator::Jump {
            target,
            args: vec![one],
        });
        build.switch_to(target);
        build.end(Terminator::Return(Some(carried)));

        let printed = print(&build.finish(), &Indices);
        assert!(printed.contains("v1 = 1"), "{printed}");
        assert!(printed.contains("jump b1(v1)"), "{printed}");
        assert!(printed.contains("b1(v0):"), "{printed}");
        assert!(printed.contains("; from b0"), "{printed}");
        assert!(printed.contains("return v0"), "{printed}");
    }

    /// The effect is the question this form exists to answer, so an impure
    /// instruction says so and a pure one says nothing.
    #[test]
    fn an_impure_instruction_carries_its_effect_and_a_pure_one_does_not() {
        let mut build = FuncBuilder::new(Tier::Generic);
        let pure = build.push(Op::Const(Const::Int(1)), Effect::PURE, Default::default());
        build.push(
            Op::Prim {
                prim: Prim(2),
                args: vec![pure],
            },
            Effect::CALLS_USER.and(Effect::THROWS),
            Default::default(),
        );
        build.end(Terminator::Return(Some(pure)));

        let printed = print(&build.finish(), &Indices);
        assert!(
            printed.contains("v1 = prim#2(v0)   ; calls|throws"),
            "{printed}"
        );
        assert!(printed.contains("v0 = 1\n"), "{printed}");
    }

    #[test]
    fn a_guard_prints_its_point_and_the_function_lists_them() {
        let mut build = FuncBuilder::new(Tier::Specialised);
        let entry = build.current();
        let held = build.param(entry);
        build.push(
            Op::Guard {
                assertion: Assertion(1),
                on: held,
                point: PointId(5),
            },
            Effect::PURE,
            Default::default(),
        );
        build.end(Terminator::Fall(PointId(5)));

        let printed = print(&build.finish(), &Indices);
        assert!(printed.contains("deopt points: p5"), "{printed}");
        assert!(
            printed.contains("guard assert#1 of v0 else p5"),
            "{printed}"
        );
        assert!(printed.contains("fall p5"), "{printed}");
    }
}
