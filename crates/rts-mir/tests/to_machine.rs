//! MIR through the machine's own verifier.
//!
//! The end of the pipeline this crate sits in the middle of: a graph built here,
//! lowered by `lower`, and accepted by `rts_cranelift::verify` — which is a
//! stronger statement than any assertion written in this crate could be, because
//! it is the machine layer's opinion rather than ours.
//!
//! The language here is a third one, after `toy_domain.rs`'s: it has integers and
//! nothing else, and it exists to supply [`MachineOps`] and no more. That is the
//! point of the trait — the amount a front end must write to reach the machine is
//! four methods, and the rest of the lowering is neutral.

use rts_cranelift::ir::{Function, FuncRegistry, NumOp, Signature, ValueId as MachineValue};
use rts_cranelift::repr::Repr;
use rts_cranelift::types::TypeRegistry;
use rts_mir::cfg::{Const, EntryId, FuncBuilder, Op, Prim, Terminator};
use rts_mir::lower::{MachineOps, Unlowerable, lower};
use rts_mir::{Assertion, Effect, PointId, Tier, verify};

/// The only primitive this language has.
const ADD: Prim = Prim(0);

struct Integers;

impl MachineOps for Integers {
    fn param_repr(&mut self, _value: rts_mir::ValueId) -> Repr {
        // This language has one type, so every parameter holds it. A real front
        // end asks its own lattice, and answering the generic representation is
        // what giving up looks like.
        Repr::I32
    }

    fn declared(
        &mut self,
        _into: &mut rts_cranelift::ir::FuncBuilder,
        index: u32,
    ) -> Result<MachineValue, String> {
        Err(format!("this language declares no constants, and {index} was asked for"))
    }

    fn prim(
        &mut self,
        into: &mut rts_cranelift::ir::FuncBuilder,
        prim: Prim,
        args: &[MachineValue],
    ) -> Result<MachineValue, String> {
        match (prim, args) {
            (ADD, [left, right]) => into
                .arith(NumOp::Add, *left, *right)
                .map_err(|held| format!("{held:?}")),
            _ => Err(format!("no lowering for {prim:?} over {} operands", args.len())),
        }
    }

    fn entry(
        &mut self,
        _into: &mut rts_cranelift::ir::FuncBuilder,
        entry: EntryId,
        _args: &[MachineValue],
    ) -> Result<MachineValue, String> {
        Err(format!("this language names no entry point, and {entry:?} was asked for"))
    }
}

/// A machine function with `params` of this language's one representation, and the
/// builder positioned at its entry.
fn machine(params: usize) -> (Function, TypeRegistry, FuncRegistry) {
    let signature = Signature {
        params: vec![Repr::I32; params],
        returns: vec![Repr::I32],
        ..Signature::default()
    };
    (
        Function::new(signature),
        TypeRegistry::new(),
        FuncRegistry::new(),
    )
}

#[test]
fn a_straight_line_graph_reaches_the_machine_and_verifies() {
    let mut build = FuncBuilder::new(Tier::Generic);
    let entry = build.current();
    let x = build.param(entry);
    let one = build.push(Op::Const(Const::Int(1)), Effect::PURE, Default::default());
    let sum = build.push(
        Op::Prim {
            prim: ADD,
            args: vec![x, one],
        },
        Effect::PURE,
        Default::default(),
    );
    build.end(Terminator::Return(Some(sum)));
    let graph = build.finish();
    assert_eq!(verify(&graph), Ok(()));

    let (mut func, types, funcs) = machine(1);
    let params = func.block(func.entry).expect("an entry block").params.clone();
    let start = func.entry;
    let mut into = rts_cranelift::ir::FuncBuilder::new(&mut func, &types, start);
    lower(&graph, &mut into, &mut Integers, &params).expect("the subset lowers");

    let errors = rts_cranelift::verify::verify(&func, &types, &funcs);
    assert!(errors.is_empty(), "the machine refused it: {errors:?}");
}

/// A branch and a join, which is what block parameters are for on both sides.
#[test]
fn a_branch_with_a_join_block_carries_its_parameter_through() {
    let mut build = FuncBuilder::new(Tier::Generic);
    let entry = build.current();
    let condition = build.param(entry);
    let join = build.block();
    let carried = build.param(join);
    let left = build.block();
    let right = build.block();

    let one = build.push(Op::Const(Const::Int(1)), Effect::PURE, Default::default());
    build.end(Terminator::Branch {
        condition,
        then_block: left,
        then_args: Vec::new(),
        else_block: right,
        else_args: Vec::new(),
    });

    build.switch_to(left);
    let two = build.push(Op::Const(Const::Int(2)), Effect::PURE, Default::default());
    build.end(Terminator::Jump {
        target: join,
        args: vec![two],
    });

    build.switch_to(right);
    build.end(Terminator::Jump {
        target: join,
        args: vec![one],
    });

    build.switch_to(join);
    build.end(Terminator::Return(Some(carried)));
    let graph = build.finish();
    assert_eq!(verify(&graph), Ok(()));

    // The machine refuses a branch on anything but its boolean representation,
    // which is why the condition parameter is declared as one here. A front end
    // that let a language's truth value reach a branch untested would find out
    // exactly here, which is the machine layer doing its job.
    let signature = Signature {
        params: vec![Repr::Bool],
        returns: vec![Repr::I32],
        ..Signature::default()
    };
    let mut func = Function::new(signature);
    let types = TypeRegistry::new();
    let funcs = FuncRegistry::new();
    let params = func.block(func.entry).expect("an entry block").params.clone();
    let start = func.entry;
    let mut into = rts_cranelift::ir::FuncBuilder::new(&mut func, &types, start);
    lower(&graph, &mut into, &mut Integers, &params).expect("the subset lowers");

    let errors = rts_cranelift::verify::verify(&func, &types, &funcs);
    assert!(errors.is_empty(), "the machine refused it: {errors:?}");
}

/// A guard is refused by name rather than approximated, because the side exit it
/// needs is D3 of `docs/engine/deopt-lateral.md` and does not exist.
#[test]
fn a_guard_is_refused_and_says_which_point_it_was() {
    let mut build = FuncBuilder::new(Tier::Specialised);
    let entry = build.current();
    let x = build.param(entry);
    let proved = build.push(
        Op::Guard {
            assertion: Assertion(0),
            on: x,
            point: PointId(4),
        },
        Effect::PURE,
        Default::default(),
    );
    build.end(Terminator::Return(Some(proved)));
    let graph = build.finish();
    assert_eq!(verify(&graph), Ok(()));

    let (mut func, types, _funcs) = machine(1);
    let params = func.block(func.entry).expect("an entry block").params.clone();
    let start = func.entry;
    let mut into = rts_cranelift::ir::FuncBuilder::new(&mut func, &types, start);
    assert_eq!(
        lower(&graph, &mut into, &mut Integers, &params),
        Err(Unlowerable::NeedsSideExit(PointId(4)))
    );
}

/// The language's refusal travels out as its own text, so a front end reporting a
/// missing lowering does not have to match on the machine's error type.
#[test]
fn a_primitive_the_language_cannot_lower_reports_the_languages_reason() {
    let mut build = FuncBuilder::new(Tier::Generic);
    let absent = build.push(
        Op::Prim {
            prim: Prim(99),
            args: Vec::new(),
        },
        Effect::PURE,
        Default::default(),
    );
    build.end(Terminator::Return(Some(absent)));
    let graph = build.finish();

    let (mut func, types, _funcs) = machine(0);
    let start = func.entry;
    let mut into = rts_cranelift::ir::FuncBuilder::new(&mut func, &types, start);
    match lower(&graph, &mut into, &mut Integers, &[]) {
        Err(Unlowerable::Language(said)) => assert!(said.contains("no lowering")),
        other => panic!("expected the language's own reason, got {other:?}"),
    }
}

