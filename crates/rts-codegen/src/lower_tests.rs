//! What the MIR lowering answers, pinned apart from it.
//!
//! A sibling file rather than an inline module, which is the convention
//!  set: the lowering itself is near the ceiling this crate holds,
//! and a test file that grows with coverage should not be what pushes it over.

use super::*;
use crate::names::Names;
use crate::names::resolve::resolve_module;
use crate::parse::parse_script;
use crate::syntax::ModuleItem;
use rts_mir::verify::verify;

/// The first function of a script, lowered.
fn only(source: &str) -> Result<Lowered, Unsupported> {
    let mut names = Names::new();
    let program = parse_script(source, &mut names).expect("the fixture parses");
    let resolution = resolve_module(&program.body);
    let function = program
        .body
        .iter()
        .find_map(|item| match item {
            ModuleItem::Stmt(Stmt {
                kind: StmtKind::Function(function),
                ..
            }) => Some(function),
            _ => None,
        })
        .expect("the fixture declares a function");
    lower(function, &resolution, Tier::Generic)
}

#[test]
fn a_straight_line_body_lowers_to_a_well_formed_graph() {
    let lowered = only("function f(x) { const two = 2; return x - two; }")
        .expect("the subset covers this");
    assert_eq!(verify(&lowered.func), Ok(()));
    assert_eq!(lowered.func.blocks.len(), 1);
}

/// The whole pipeline composing: the tree gives a graph, the graph gives
/// types, and the types are this language's answers.
#[test]
fn the_graph_infers_this_languages_types() {
    let lowered = only("function f() { const a = 2; const b = 3; return a / b; }")
        .expect("the subset covers this");
    assert_eq!(verify(&lowered.func), Ok(()));
    let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
    let last = lowered.func.insts.last().expect("three instructions");
    // Division answers a number even of two integers, which is this
    // language's rule and the toy domain's too -- for different reasons.
    assert_eq!(*types.of(last.result), Type::Double);
}

/// The effect is decided from what is known where the instruction is pushed,
/// which is what makes a proven arithmetic operation movable.
#[test]
fn an_operation_over_proven_numbers_is_pure_and_one_over_a_parameter_is_not() {
    let proven =
        only("function f() { const a = 1; const b = 2; return a + b; }").expect("covered");
    let last = proven.func.insts.last().expect("an addition");
    assert!(last.effect.is_pure());

    let unknown = only("function f(x) { const b = 2; return x + b; }").expect("covered");
    let last = unknown.func.insts.last().expect("an addition");
    assert!(last.effect.has(Effect::CALLS_USER));
    assert!(last.effect.has(Effect::THROWS));
}

#[test]
fn a_global_is_refused_by_name_and_not_treated_as_a_local() {
    let refused =
        only("function f() { return Math; }").expect_err("a global needs an entry point");
    assert!(matches!(refused, Unsupported::Global(_)));
}

/// Greater-than is NOT less-than with the operands swapped, and the refusal
/// records why rather than silently doing it.
#[test]
fn an_operator_with_no_row_is_refused_by_name() {
    let refused = only("function f(a, b) { return a > b; }").expect_err("no row for >");
    assert_eq!(refused, Unsupported::Operator(BinaryOp::Greater));
}

#[test]
fn a_parked_frame_is_refused_before_anything_is_built() {
    let refused = only("async function f() { return 1; }").expect_err("async parks");
    assert!(matches!(refused, Unsupported::Shape(_)));
    let refused = only("function* f() { return 1; }").expect_err("a generator parks");
    assert!(matches!(refused, Unsupported::Shape(_)));
}

#[test]
fn a_body_with_no_return_answers_nothing_and_is_still_well_formed() {
    let lowered = only("function f() { const a = 1; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    assert_eq!(
        lowered.func.block(lowered.func.entry()).terminator,
        Some(Terminator::Return(None))
    );
}

/// The declared-constant numbering is a coupling between two files, so it is
/// pinned rather than trusted: `lower` writes a `Singleton`'s discriminant
/// into `Const::Declared` and `domain` reads it back.
///
/// Reordering the enum would silently make `undefined` mean `null` — and both
/// are falsy, both answer `false` to every truth question, and neither is a
/// number or a string. So no assertion about behaviour would catch it, which
/// is the shape of defect this repository calls silent.
#[test]
fn the_singleton_numbering_is_the_one_the_domain_reads() {
    let domain = Js::new();
    assert_eq!(
        domain.of_const(&Const::Declared(Singleton::Undefined as u32)),
        Type::Undefined
    );
    assert_eq!(
        domain.of_const(&Const::Declared(Singleton::Null as u32)),
        Type::Null
    );
}

/// A declaration with no initialiser is `undefined`, through that same
/// numbering.
#[test]
fn a_declaration_with_no_value_holds_undefined() {
    let lowered = only("function f() { let a; return a; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
    let first = lowered.func.insts.first().expect("one constant");
    assert_eq!(*types.of(first.result), Type::Undefined);
}

/// A branch with both arms rebinding one local: the join carries exactly that
/// one binding, and the type at the join is the domain's join of the two.
#[test]
fn a_branch_merges_only_what_the_arms_disagree_about() {
    let lowered = only(
        "function f(c) {
           let a = 1;
           const kept = 9;
           if (c) { a = 2; } else { a = 3; }
           return a;
         }",
    )
    .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));

    // Four blocks: entry, the two arms, the join.
    assert_eq!(lowered.func.blocks.len(), 4);
    // And the join declares ONE parameter -- `a`, not `kept` and not `c`.
    let join = lowered
        .func
        .block_ids()
        .find(|held| lowered.func.predecessors(*held).len() == 2)
        .expect("a join block");
    assert_eq!(lowered.func.block(join).params.len(), 1);
}

/// The types the two arms leave are joined by the DOMAIN, which is the one
/// thing a lattice is for.
#[test]
fn the_join_takes_the_domains_answer_and_not_the_irs() {
    let lowered = only(
        "function f(c) {
           let a = 1;
           if (c) { a = 2; } else { a = 0.5; }
           return a;
         }",
    )
    .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
    let join = lowered
        .func
        .block_ids()
        .find(|held| lowered.func.predecessors(*held).len() == 2)
        .expect("a join block");
    let carried = lowered.func.block(join).params[0];
    // An integer from one arm and a fraction from the other. THIS language has
    // one numeric type, so the answer is a number; the toy domain's answer for
    // the same shape is its top.
    assert_eq!(*types.of(carried), Type::Double);
}

/// An arm that returned contributes nothing to the join, because control does
/// not arrive from it.
#[test]
fn an_arm_that_returned_is_not_a_second_opinion() {
    let lowered = only(
        "function f(c) {
           let a = 1;
           if (c) { return 0; } else { a = 2; }
           return a;
         }",
    )
    .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let join = lowered
        .func
        .block_ids()
        .find(|held| lowered.func.predecessors(*held).len() == 1 && held.0 != 0)
        .expect("a join reached from one arm only");
    assert!(lowered.func.block(join).params.is_empty());
}

/// Both arms leaving means nothing follows the `if`, and the function is still
/// well formed -- no empty block left behind without a terminator.
#[test]
fn both_arms_returning_ends_the_statement_run() {
    let lowered = only("function f(c) { if (c) { return 1; } else { return 2; } }")
        .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
}

/// A block is a scope walked in step, and a binding inside it is a different
/// `BindingId` -- which is why nothing here decides a shadowing rule.
#[test]
fn a_block_scope_is_walked_and_its_binding_is_its_own() {
    let lowered = only(
        "function f() {
           let i = 1;
           { let i = 2; }
           return i;
         }",
    )
    .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
    let returned = match &lowered.func.block(lowered.func.entry()).terminator {
        Some(Terminator::Return(Some(value))) => *value,
        other => panic!("expected a returned value, got {other:?}"),
    };
    // The OUTER `i` is returned. Both are `Int32`, so the type cannot tell them
    // apart -- what says it is the right one is the value: it is the constant
    // `1`, which is the first instruction.
    assert_eq!(*types.of(returned), Type::Int32);
    assert_eq!(lowered.func.insts[0].result, returned);
}

/// The compound form is refused rather than rewritten, because `a += b`
/// evaluates its target once and `a = a + b` evaluates it twice.
#[test]
fn a_compound_assignment_is_refused_rather_than_rewritten() {
    let refused =
        only("function f(a) { a += 1; return a; }").expect_err("no lowering for compound");
    assert!(matches!(refused, Unsupported::Expression(_)));
}

/// An assignment answers what was assigned, so a chain works.
#[test]
fn an_assignment_answers_the_value_it_assigned() {
    let lowered = only("function f() { let a = 0; let b = 0; a = b = 7; return a; }")
        .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
    let returned = match &lowered.func.block(lowered.func.entry()).terminator {
        Some(Terminator::Return(Some(value))) => *value,
        other => panic!("expected a returned value, got {other:?}"),
    };
    assert_eq!(*types.of(returned), Type::Int32);
}

/// The truth rule is an operation of the LANGUAGE, and the domain folds it
/// where the type decides it.
#[test]
fn a_branch_over_something_always_true_is_folded_by_the_domain() {
    let lowered = only("function f() { if (1 === 1) { return 1; } return 2; }")
        .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
    // `===` answers a boolean of unknown value, so the truth of it is unknown
    // too: nothing is folded here, and that is the honest answer.
    let tested = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if *prim == lowered.domain.prim(JsPrim::Truthy)))
        .expect("a truthiness test");
    assert_eq!(*types.of(tested.result), Type::Bool(None));
}

/// The shape a loop is: an entry that jumps to a header, a header that tests and
/// branches, a body that jumps back, and an exit.
#[test]
fn a_while_loop_carries_what_its_body_assigns_across_the_back_edge() {
    let lowered = only(
        "function f(n) {
           let total = 0;
           let at = 0;
           while (at < n) {
             total = total + at;
             at = at + 1;
           }
           return total;
         }",
    )
    .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));

    // The header is the block with two predecessors: the entry and the back edge.
    let header = lowered
        .func
        .block_ids()
        .find(|held| lowered.func.predecessors(*held).len() == 2)
        .expect("a header reached from before the loop and from the back edge");
    // It carries `total` and `at`, and not `n` -- a parameter is not assigned.
    assert_eq!(lowered.func.block(header).params.len(), 2);
}

/// A binding the body does not assign is not carried, which is what keeps a
/// header's parameter list the size of what actually changes.
#[test]
fn a_binding_the_body_only_reads_is_not_carried() {
    let lowered = only(
        "function f() {
           const limit = 10;
           let at = 0;
           while (at < limit) { at = at + 1; }
           return at;
         }",
    )
    .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let header = lowered
        .func
        .block_ids()
        .find(|held| lowered.func.predecessors(*held).len() == 2)
        .expect("a header");
    assert_eq!(lowered.func.block(header).params.len(), 1);
}

/// Inference is what answers the header's type, and this is the test that says the
/// fixed point is doing real work: the lowering recorded the domain's top there.
#[test]
fn the_header_type_comes_from_inference_and_not_from_the_lowering() {
    let lowered = only(
        "function f() {
           let at = 0;
           while (at < 3) { at = at + 1; }
           return at;
         }",
    )
    .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
    let header = lowered
        .func
        .block_ids()
        .find(|held| lowered.func.predecessors(*held).len() == 2)
        .expect("a header");
    let carried = lowered.func.block(header).params[0];
    // `0` from before the loop is an Int32; `at + 1` across the back edge is a
    // Double, because this language's addition of two int32s may not fit. So the
    // join is a number -- which is more than the lowering knew and exactly what a
    // fixed point is for.
    assert_eq!(*types.of(carried), Type::Double);
}

/// A `break` leaves through the exit block, which declares the same parameters the
/// header does -- one convention per loop, so a jump from anywhere inside needs no
/// second one.
#[test]
fn a_break_leaves_through_the_exit_with_the_carried_values() {
    let lowered = only(
        "function f(c) {
           let at = 0;
           while (at < 10) {
             if (c) { break; }
             at = at + 1;
           }
           return at;
         }",
    )
    .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    // The exit is reached from the header's false arm AND from the break.
    let exits: Vec<_> = lowered
        .func
        .block_ids()
        .filter(|held| lowered.func.predecessors(*held).len() >= 2)
        .collect();
    assert!(exits.len() >= 2, "a header and an exit, both merged into");
}

#[test]
fn a_continue_goes_back_to_the_test() {
    let lowered = only(
        "function f(c) {
           let at = 0;
           while (at < 10) {
             at = at + 1;
             if (c) { continue; }
           }
           return at;
         }",
    )
    .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
}

/// A loop that assigns a global is refused rather than carried: the value does not
/// live in a binding, so there is nothing for a block parameter to hold.
#[test]
fn a_loop_assigning_a_global_is_refused_by_name() {
    let refused = only("function f() { while (1) { undeclared = 1; } }")
        .expect_err("a global has no binding to carry");
    assert!(matches!(refused, Unsupported::Global(_)));
}

/// A labelled loop is refused at the LABEL, before the break inside it is
/// reached -- which is the right order, because a label may name a block and not
/// only a loop, so the exit a labelled break wants is not this loop's exit.
///
/// The expectation here was written as the break's own refusal and was wrong: the
/// label comes first in the tree and therefore first in the refusal. Keeping the
/// test with the real answer is the point of having written it.
#[test]
fn a_labelled_loop_is_refused_at_the_label() {
    let refused = only("function f() { outer: while (1) { break outer; } }")
        .expect_err("a label is not lowered");
    assert_eq!(refused, Unsupported::Statement("a label"));
}

/// Nested loops each carry their own set, and the inner one's back edge must not
/// reach the outer header.
#[test]
fn nested_loops_keep_their_own_headers() {
    let lowered = only(
        "function f() {
           let outer = 0;
           while (outer < 3) {
             let inner = 0;
             while (inner < 3) { inner = inner + 1; }
             outer = outer + 1;
           }
           return outer;
         }",
    )
    .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    // A header is the only kind of block reached from two places here: neither
    // loop has a `break`, so each exit has the header's false arm as its single
    // predecessor. Counting merge points therefore counts headers.
    //
    // Block ids are NOT a topological order, so "a predecessor with a higher id"
    // is not a back edge and the first version of this assertion tested nothing.
    let merged: Vec<_> = lowered
        .func
        .block_ids()
        .filter(|held| lowered.func.predecessors(*held).len() == 2)
        .collect();
    assert_eq!(merged.len(), 2, "one header per loop");
}
