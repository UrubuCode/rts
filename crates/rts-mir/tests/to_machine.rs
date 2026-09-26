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

use rts_cranelift::ir::{FuncRegistry, Function, NumOp, Signature, ValueId as MachineValue};
use rts_cranelift::repr::Repr;
use rts_cranelift::types::TypeRegistry;
use rts_mir::cfg::{Const, EntryId, FuncBuilder, Op, Prim, Terminator};
use rts_mir::lower::{MachineOps, Unlowerable, lower};
use rts_mir::{Assertion, Effect, Malformed, PointId, Tier, verify};

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
        Err(format!(
            "this language declares no constants, and {index} was asked for"
        ))
    }

    fn asserted_repr(&mut self, _assertion: rts_mir::Assertion) -> Option<Repr> {
        // This language has one type, so its one assertion narrows to it.
        Some(Repr::I32)
    }

    fn coerce(
        &mut self,
        _into: &mut rts_cranelift::ir::FuncBuilder,
        value: MachineValue,
        want: Repr,
    ) -> Result<MachineValue, String> {
        // ONE TYPE MEANS ONE REPRESENTATION, so nothing here ever needs converting and
        // being asked at all is a bug in the caller rather than a case to handle. A toy
        // that widened silently would make the check this method exists for untestable.
        Err(format!(
            "this language has one representation, so {value:?} into {want:?} is not a conversion it has"
        ))
    }

    fn fall(
        &mut self,
        into: &mut rts_cranelift::ir::FuncBuilder,
        _point: rts_mir::PointId,
        _live: &[MachineValue],
    ) -> Result<(), String> {
        // A TOY WITH NO SECOND TIER answers a fixed value. That is not a model of what
        // a deoptimiser does -- it is enough to prove the EDGE exists and is terminated,
        // which is what these tests are for. A real one hands the live set to the other
        // body; `rts-codegen`'s does exactly that.
        let id = into.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
            repr: Repr::I32,
            bits: rts_cranelift::ir::ScalarBits(0),
        });
        let zero = into.use_const(id);
        into.ret(&[zero]);
        Ok(())
    }

    fn prim(
        &mut self,
        into: &mut rts_cranelift::ir::FuncBuilder,
        prim: Prim,
        args: &[MachineValue],
        _inst: &rts_mir::cfg::Inst,
    ) -> Result<MachineValue, String> {
        match (prim, args) {
            (ADD, [left, right]) => into
                .arith(NumOp::Add, *left, *right)
                .map_err(|held| format!("{held:?}")),
            _ => Err(format!(
                "no lowering for {prim:?} over {} operands",
                args.len()
            )),
        }
    }

    fn call_value(
        &mut self,
        _into: &mut rts_cranelift::ir::FuncBuilder,
        _callee: MachineValue,
        _receiver: Option<MachineValue>,
        _args: &[MachineValue],
        _inst: &rts_mir::cfg::Inst,
    ) -> Result<MachineValue, String> {
        // A LANGUAGE WITH ONE TYPE AND NO FUNCTIONS calls nothing, so being asked is a bug
        // in the caller rather than a case to handle. A toy that answered something would
        // make the refusal this method exists to allow untestable.
        Err("this language has no calls, so a call through a value is not one it has".to_owned())
    }

    fn entry(
        &mut self,
        _into: &mut rts_cranelift::ir::FuncBuilder,
        entry: EntryId,
        _args: &[MachineValue],
        _inst: &rts_mir::cfg::Inst,
    ) -> Result<MachineValue, String> {
        Err(format!(
            "this language names no entry point, and {entry:?} was asked for"
        ))
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
    let params = func
        .block(func.entry)
        .expect("an entry block")
        .params
        .clone();
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
    let params = func
        .block(func.entry)
        .expect("an entry block")
        .params
        .clone();
    let start = func.entry;
    let mut into = rts_cranelift::ir::FuncBuilder::new(&mut func, &types, start);
    lower(&graph, &mut into, &mut Integers, &params).expect("the subset lowers");

    let errors = rts_cranelift::verify::verify(&func, &types, &funcs);
    assert!(errors.is_empty(), "the machine refused it: {errors:?}");
}

/// A guard AT THE ENTRY lowers, because the live set there is the parameters and a fall
/// needs nothing reconstructed. This test asserted the opposite until the side exit
/// existed, and the assertion it now makes is the structural condition rather than the
/// absence of a feature.
#[test]
fn a_guard_at_the_entry_lowers_with_its_side_exit() {
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
    let params = func
        .block(func.entry)
        .expect("an entry block")
        .params
        .clone();
    let start = func.entry;
    let mut into = rts_cranelift::ir::FuncBuilder::new(&mut func, &types, start);
    assert_eq!(lower(&graph, &mut into, &mut Integers, &params), Ok(()));
}

/// A guard behind an OBSERVABLE instruction is refused, and this is the other half of
/// the same condition.
///
/// It used to say "behind an instruction", and a pure one was enough to trip it. That
/// was the condition read too narrowly: falling means the other tier re-runs from the
/// top, and re-running something pure is sound because pure means nothing can tell. So
/// the instruction here now WRITES, which is the case that genuinely cannot be repeated.
#[test]
fn a_guard_behind_an_observable_instruction_needs_the_frame_reconstructed() {
    let mut build = FuncBuilder::new(Tier::Specialised);
    let entry = build.current();
    let x = build.param(entry);
    // ONE ORDINARY INSTRUCTION, and that is the whole difference.
    let doubled = build.push(
        Op::Prim {
            prim: ADD,
            args: vec![x, x],
        },
        // WRITES, and that is the whole of what makes this refused: a write cannot be
        // performed twice, so the other tier cannot be asked to run from the top.
        Effect::WRITES,
        Default::default(),
    );
    let proved = build.push(
        Op::Guard {
            assertion: Assertion(0),
            on: doubled,
            point: PointId(7),
        },
        Effect::PURE,
        Default::default(),
    );
    build.end(Terminator::Return(Some(proved)));
    let graph = build.finish();
    assert_eq!(verify(&graph), Ok(()));

    let (mut func, types, _funcs) = machine(1);
    let params = func
        .block(func.entry)
        .expect("an entry block")
        .params
        .clone();
    let start = func.entry;
    let mut into = rts_cranelift::ir::FuncBuilder::new(&mut func, &types, start);
    assert_eq!(
        lower(&graph, &mut into, &mut Integers, &params),
        Err(Unlowerable::NeedsSideExit(PointId(7)))
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

/// A suspension is refused by name, and it is the one refusal here that is not about
/// something the LANGUAGE has not declared: the graph is entirely well formed and the
/// machine has not been asked for `frame::resumable_form` yet.
#[test]
fn a_suspension_is_refused_because_the_frame_transform_is_not_wired() {
    let mut build = FuncBuilder::new(Tier::Generic);
    let entry = build.current();
    let x = build.param(entry);
    let back = build.push(
        Op::Suspend { value: Some(x) },
        Effect::SUSPENDS,
        Default::default(),
    );
    build.end(Terminator::Return(Some(back)));
    let graph = build.finish();
    assert_eq!(verify(&graph), Ok(()));

    let (mut func, types, _funcs) = machine(1);
    let params = func
        .block(func.entry)
        .expect("an entry block")
        .params
        .clone();
    let start = func.entry;
    let mut into = rts_cranelift::ir::FuncBuilder::new(&mut func, &types, start);
    assert_eq!(
        lower(&graph, &mut into, &mut Integers, &params),
        Err(Unlowerable::NeedsFrameTransform)
    );
}

/// The flag is DERIVED from what the body does and never passed in, so a lowering that
/// grew a parking path cannot forget to mark it — which would be a function the machine
/// compiles with an ordinary frame and then tries to leave.
#[test]
fn the_builder_derives_the_flag_from_the_effect_alone() {
    let mut build = FuncBuilder::new(Tier::Generic);
    build.end(Terminator::Return(None));
    assert!(!build.finish().may_suspend, "nothing has parked");

    let mut build = FuncBuilder::new(Tier::Generic);
    let entry = build.current();
    let x2 = build.param(entry);
    // NOT an `Op::Suspend`: a primitive of the language whose effect table says it
    // parks. The flag follows the effect, which is where the fact lives.
    build.push(
        Op::Prim {
            prim: Prim(7),
            args: vec![x2],
        },
        Effect::SUSPENDS,
        Default::default(),
    );
    build.end(Terminator::Return(None));
    assert!(build.finish().may_suspend);
}

/// A hand-assembled function that parks without saying so is malformed, and the check
/// exists for exactly that: the machine asks the flag once per function, so one that
/// lied would get an ordinary frame.
#[test]
fn a_function_that_parks_without_saying_so_is_malformed() {
    let mut build = FuncBuilder::new(Tier::Generic);
    let entry = build.current();
    let x = build.param(entry);
    build.push(
        Op::Suspend { value: Some(x) },
        Effect::SUSPENDS,
        Default::default(),
    );
    build.end(Terminator::Return(None));
    let mut graph = build.finish();
    assert_eq!(verify(&graph), Ok(()));
    graph.may_suspend = false;
    assert!(matches!(
        verify(&graph),
        Err(Malformed::SuspendsWithoutSaying(_))
    ));
}

/// **A constant's representation comes from its value**, and the version that did not was a
/// silent wrong answer: `Const::Int` holds an `i64` and every one was declared `I32` through
/// an `as i32` cast, so `5_000_000_000` became `705_032_704` with nothing said.
///
/// `I32` where it fits and not `I64` always, because the narrow form is the one the rest of
/// the machine wants — `to_f64` accepts `I32` and refuses `I64`, so a literal widened here
/// would stop being usable in the double domain every arithmetic row answers.
#[test]
fn a_constant_too_large_for_an_i32_is_not_truncated() {
    for (value, want) in [
        (0_i64, Repr::I32),
        (i64::from(i32::MAX), Repr::I32),
        (i64::from(i32::MIN), Repr::I32),
        (i64::from(i32::MAX) + 1, Repr::I64),
        (5_000_000_000, Repr::I64),
    ] {
        let mut build = FuncBuilder::new(Tier::Generic);
        let held = build.push(
            Op::Const(Const::Int(value)),
            Effect::PURE,
            Default::default(),
        );
        build.end(Terminator::Return(Some(held)));
        let graph = build.finish();

        let (mut func, types, funcs) = machine(0);
        let start = func.entry;
        let mut into = rts_cranelift::ir::FuncBuilder::new(&mut func, &types, start);
        lower(&graph, &mut into, &mut Integers, &[]).expect("a constant always lowers");
        drop(into);
        // THE VERIFIER IS THE JUDGE, and it is the right one: the signature says `I32`, so
        // a constant that landed as `I64` is a `ReturnRepr` complaint and one that landed
        // as `I32` is silence. `lower` itself does not check a return's representation --
        // asking it to was this test's first mechanism and it reported nothing.
        let found = rts_cranelift::verify(&func, &types, &funcs);
        match want {
            Repr::I32 => assert_eq!(found, Vec::new(), "{value} fits and stays an i32"),
            _ => assert!(
                !found.is_empty(),
                "{value} does not fit, so it is an i64 the i32 signature refuses: {found:?}"
            ),
        }
    }
}
