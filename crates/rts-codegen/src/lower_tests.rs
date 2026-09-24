//! What the MIR lowering answers, pinned apart from it.
//!
//! A sibling file rather than an inline module, which is the convention
//! `emit/class.rs` set: the lowering itself is near the ceiling this crate holds,
//! and a test file that grows with coverage should not be what pushes it over.

use super::*;
use crate::domain::JsPrim;
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
    let lowered =
        only("function f(x) { const two = 2; return x - two; }").expect("the subset covers this");
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
    let proven = only("function f() { const a = 1; const b = 2; return a + b; }").expect("covered");
    let last = proven.func.insts.last().expect("an addition");
    assert!(last.effect.is_pure());

    let unknown = only("function f(x) { const b = 2; return x + b; }").expect("covered");
    let last = unknown.func.insts.last().expect("an addition");
    assert!(last.effect.has(Effect::CALLS_USER));
    assert!(last.effect.has(Effect::THROWS));
}

/// A name no scope declares is read through the global object, which is what the
/// language does with one — it was a refusal until the read became an operation.
#[test]
fn a_global_is_read_through_the_global_object() {
    let lowered = only("function f() { return Math; }").expect("a global read");
    assert_eq!(verify(&lowered.func), Ok(()));
    let held = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::GlobalRead)))
        .expect("a global read");
    // It may run a getter -- the global object is an ordinary object -- and throws
    // where the name is declared nowhere at all.
    assert!(held.effect.has(Effect::READS));
    assert!(held.effect.has(Effect::CALLS_USER));
    assert!(held.effect.has(Effect::THROWS));
}

/// An operator with no row is refused as an operator, and `**` is one: it is not
/// `Multiply` repeated, and nothing in the table answers it.
#[test]
fn an_operator_with_no_row_is_refused_by_name() {
    let refused = only("function f(a, b) { return a ** b; }").expect_err("no row for **");
    assert_eq!(refused, Unsupported::Operator(BinaryOp::Exponent));
}

/// Neither kind is turned away at the function any more, and this test replaced one
/// that asserted they were. What changed is not that parking became expressible by
/// approximation -- it is that the BODY of one was never the missing piece. Calling
/// one runs no body, and that is the caller's sequence, which the machine refuses by
/// name.
#[test]
fn neither_a_generator_nor_an_async_function_is_refused_at_the_function() {
    let generator = only("function* f() { return 1; }").expect("a generator body lowers");
    let asynchronous = only("async function f() { return 1; }").expect("an async body too");
    assert_eq!(verify(&generator.func), Ok(()));
    assert_eq!(verify(&asynchronous.func), Ok(()));
    // NEITHER BODY PARKS, because neither of these two writes a `yield` or an
    // `await` -- so the flag is about what the body DOES and not about how it was
    // declared, which is the whole reason it is derived.
    assert!(!generator.func.may_suspend);
    assert!(!asynchronous.func.may_suspend);
}

/// Falling off the end of a body answers `undefined`, as `return;` does: every function
/// of this language returns a value. This test asserted a bare `Return(None)` until the
/// machine's verifier, asked for the first time, refused a function that returned a value
/// on one path and fell off the end on another.
#[test]
fn a_body_with_no_return_answers_undefined_and_is_still_well_formed() {
    let lowered = only("function f() { const a = 1; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let Some(Terminator::Return(Some(answered))) =
        lowered.func.block(lowered.func.entry()).terminator
    else {
        panic!("the body answers a value");
    };
    let made = lowered
        .func
        .insts
        .iter()
        .find(|held| held.result == answered)
        .expect("defined");
    assert!(matches!(
        made.op,
        rts_mir::Op::Const(rts_mir::Const::Declared(index))
            if lowered.domain.declared(index)
                == Some(&crate::domain::JsConst::Singleton(Singleton::Undefined))
    ));
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
    let lowered =
        only("function f(c) { if (c) { return 1; } else { return 2; } }").expect("covered");
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

/// An assignment answers what was assigned, so a chain works.
#[test]
fn an_assignment_answers_the_value_it_assigned() {
    let lowered =
        only("function f() { let a = 0; let b = 0; a = b = 7; return a; }").expect("covered");
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
    let lowered = only("function f() { if (1 === 1) { return 1; } return 2; }").expect("covered");
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

/// The shape a classic `for` is: a head that runs once, a header that tests, a
/// body, an update, and a back edge.
#[test]
fn a_classic_for_carries_its_counter_across_the_back_edge() {
    let lowered = only(
        "function f(n) {
           let sum = 0;
           for (let i = 0; i < n; i++) { sum = sum + i; }
           return sum;
         }",
    )
    .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let header = lowered
        .func
        .block_ids()
        .find(|held| lowered.func.predecessors(*held).len() == 2)
        .expect("a header");
    // `sum` and `i`, and not `n`.
    assert_eq!(lowered.func.block(header).params.len(), 2);
}

/// `i++` is not `i = i + 1`: it answers the number the target HELD, coerced. Both
/// the coercion and the addition are in the graph, and the addition's answer is the
/// one the binding takes.
#[test]
fn an_increment_coerces_and_answers_the_value_from_before() {
    let lowered = only("function f(a) { let b = a++; return b; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let ops: Vec<_> = lowered
        .func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Prim { prim, .. } => lowered.domain.meaning(*prim),
            _ => None,
        })
        .collect();
    assert!(ops.contains(&JsPrim::ToNumber), "{ops:?}");
    assert!(ops.contains(&JsPrim::Add), "{ops:?}");
    // The postfix form answers the coerced old value, which is the ToNumber and
    // not the Add.
    let returned = match &lowered.func.block(lowered.func.entry()).terminator {
        Some(Terminator::Return(Some(value))) => *value,
        other => panic!("expected a returned value, got {other:?}"),
    };
    let from = lowered
        .func
        .insts
        .iter()
        .find(|held| held.result == returned)
        .expect("the returned value is an instruction");
    assert!(
        matches!(&from.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::ToNumber))
    );
}

/// And the prefix form answers the sum instead.
#[test]
fn a_prefix_increment_answers_the_sum() {
    let lowered = only("function f(a) { let b = ++a; return b; }").expect("covered");
    let returned = match &lowered.func.block(lowered.func.entry()).terminator {
        Some(Terminator::Return(Some(value))) => *value,
        other => panic!("expected a returned value, got {other:?}"),
    };
    let from = lowered
        .func
        .insts
        .iter()
        .find(|held| held.result == returned)
        .expect("an instruction");
    assert!(
        matches!(&from.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::Add))
    );
}

/// A head with no test never leaves on its own, so the header jumps straight in —
/// and the exit still exists, because a `break` needs somewhere to go.
#[test]
fn a_for_with_no_test_jumps_straight_into_its_body() {
    let lowered = only(
        "function f(c) {
           let at = 0;
           for (;;) { at = at + 1; if (c) { break; } }
           return at;
         }",
    )
    .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
}

/// The update runs after the body and before the test, so its write crosses the
/// back edge like the body's — which is why it is scanned into the carried set.
#[test]
fn an_update_that_writes_is_carried_like_the_body() {
    let lowered = only(
        "function f(n) { let seen = 0; for (let i = 0; i < n; i = i + 2) { seen = seen + 1; } return seen; }",
    )
    .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let header = lowered
        .func
        .block_ids()
        .find(|held| lowered.func.predecessors(*held).len() == 2)
        .expect("a header");
    assert_eq!(lowered.func.block(header).params.len(), 2);
}

/// A `do`-`while` runs its body before its test, which is one edge different and
/// observable as the entry going into the BODY rather than into the header.
#[test]
fn a_do_while_enters_through_its_body() {
    let lowered = only(
        "function f(c) {
           let at = 0;
           do { at = at + 1; } while (c);
           return at;
         }",
    )
    .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    // The body block is the one with two predecessors: the entry and the test.
    let body = lowered
        .func
        .block_ids()
        .find(|held| lowered.func.predecessors(*held).len() == 2)
        .expect("a body reached from the entry and from the test");
    assert!(
        lowered
            .func
            .predecessors(body)
            .contains(&lowered.func.entry()),
        "the entry jumps into the body, which is what a do-while is"
    );
}

/// A `var` in a head belongs to the function rather than to the head, so it is
/// refused by name instead of being bound in the wrong scope.
#[test]
fn a_var_in_a_loop_head_is_refused_by_name() {
    let refused = only("function f(n) { for (var i = 0; i < n; i++) {} return i; }")
        .expect_err("a var hoists out of the head");
    assert!(matches!(refused, Unsupported::Statement(_)));
}

/// A compound assignment to a plain local is the rewrite that is legal only there:
/// reading a binding has no effect to duplicate.
#[test]
fn a_compound_assignment_to_a_local_applies_its_operator() {
    let lowered = only("function f(a) { let b = 1; b += a; return b; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let ops: Vec<_> = lowered
        .func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Prim { prim, .. } => lowered.domain.meaning(*prim),
            _ => None,
        })
        .collect();
    assert_eq!(ops, vec![JsPrim::Add]);
}

/// And it answers the new value, so a chain works.
#[test]
fn a_compound_assignment_answers_the_value_it_stored() {
    let lowered = only("function f() { let a = 1; let b = (a += 2); return b; }").expect("covered");
    let returned = match &lowered.func.block(lowered.func.entry()).terminator {
        Some(Terminator::Return(Some(value))) => *value,
        other => panic!("expected a returned value, got {other:?}"),
    };
    let from = lowered
        .func
        .insts
        .iter()
        .find(|held| held.result == returned)
        .expect("an instruction");
    assert!(
        matches!(&from.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::Add))
    );
}

/// A member target is refused for the reason the tree carries the operator at all:
/// `a[i()] += 1` calls `i` once, and the rewrite would call it twice.
#[test]
fn a_compound_assignment_to_a_property_is_refused_by_that_reason() {
    let refused = only("function f(o) { o.x += 1; }").expect_err("a member target");
    assert!(matches!(refused, Unsupported::Expression(_)));
}

/// An operator with no compound row is refused as an operator, not as a shape.
#[test]
fn a_compound_form_of_an_unlowered_operator_is_refused_as_an_operator() {
    let refused =
        only("function f(a) { let b = 1; b **= a; return b; }").expect_err("no row for **");
    assert!(matches!(refused, Unsupported::Operator(_)));
}

/// An array literal is one primitive over its elements, and it allocates.
#[test]
fn an_array_literal_is_one_allocating_primitive() {
    let lowered = only("function f(a) { return [1, a]; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let built = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::NewArray)))
        .expect("a construction");
    assert!(built.effect.has(Effect::ALLOCATES));
    assert!(built.effect.may_collect());
    // Two elements, in the order written.
    match &built.op {
        rts_mir::Op::Prim { args, .. } => assert_eq!(args.len(), 2),
        other => panic!("expected a primitive, got {other:?}"),
    }
}

/// A hole is NOT `undefined`, so it is refused rather than lowered as one.
#[test]
fn a_hole_is_refused_rather_than_collapsed_into_undefined() {
    let refused = only("function f() { return [1, , 3]; }").expect_err("a hole");
    assert!(matches!(refused, Unsupported::Expression(_)));
}

/// A spread does make the length a run-time question, and this test replaced one
/// asserting the refusal that said so. The count is not what `NewArray` has to be
/// given — it needs the ELEMENTS — so a literal with a spread is built rather than
/// counted: one array, and an append per element.
#[test]
fn a_literal_with_a_spread_is_built_by_appending() {
    let lowered = only("function f(xs) { return [1, ...xs, 2]; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let entries: Vec<_> = lowered
        .func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Call {
                callee: rts_mir::cfg::Callee::Entry(entry),
                ..
            } => lowered.domain.entry_meaning(*entry),
            _ => None,
        })
        .collect();
    use crate::runtime::RuntimeOp;
    assert_eq!(
        entries,
        vec![
            RuntimeOp::ArrayAppend,
            RuntimeOp::ArrayAppendAll,
            RuntimeOp::ArrayAppend,
        ],
        "in source order, one per element"
    );
}

/// A literal with NO spread must not start paying for the feature: the elements are
/// known, `NewArray` takes them, and the array arrives full with no calls at all.
#[test]
fn a_literal_with_no_spread_still_takes_its_elements_directly() {
    let lowered = only("function f(a, b) { return [a, b]; }").expect("covered");
    let calls = lowered
        .func
        .insts
        .iter()
        .filter(|held| matches!(&held.op, rts_mir::Op::Call { .. }))
        .count();
    assert_eq!(calls, 0, "no spread, no appends");
}

/// The effect pass must not narrow an allocation away: allocating is what the
/// operation does, not something its operands decide.
#[test]
fn an_array_construction_keeps_allocating_after_the_effect_pass() {
    let mut lowered = only("function f() { return [1, 2]; }").expect("covered");
    let refined = rts_mir::passes::refine_effects(&mut lowered.func, &lowered.domain);
    assert_eq!(refined.narrowed, 0);
    let built = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::NewArray)))
        .expect("a construction");
    assert!(built.effect.may_collect());
}

/// A property read carries its key as a constant of the LANGUAGE's table, so two
/// reads of one property compare equal by index and a pass never compares names.
#[test]
fn two_reads_of_one_property_carry_one_index() {
    let lowered = only("function f(o) { return o.x + o.x; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let keys: Vec<_> = lowered
        .func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Const(rts_mir::Const::Declared(index)) => Some(*index),
            _ => None,
        })
        .collect();
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0], keys[1], "one property, one index");

    // And a DIFFERENT property is a different index.
    let other = only("function f(o) { return o.x + o.y; }").expect("covered");
    let keys: Vec<_> = other
        .func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Const(rts_mir::Const::Declared(index)) => Some(*index),
            _ => None,
        })
        .collect();
    assert_ne!(keys[0], keys[1]);
}

/// A read of an unshaped receiver goes through the runtime, where a getter may
/// run — which is what the effect says and what pins it where it is.
#[test]
fn a_read_of_an_unproven_receiver_may_run_a_getter() {
    let lowered = only("function f(o) { return o.x; }").expect("covered");
    let read = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::FieldRead)))
        .expect("a read");
    assert!(read.effect.has(Effect::READS));
    assert!(read.effect.has(Effect::CALLS_USER));
}

/// An INDEXED access is not a field read with a clever argument: the key is a
/// value, so the runtime turns it into a position even for a shaped receiver.
#[test]
fn an_indexed_access_is_its_own_operation() {
    let lowered = only("function f(a, i) { return a[i]; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let ops: Vec<_> = lowered
        .func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Prim { prim, .. } => lowered.domain.meaning(*prim),
            _ => None,
        })
        .collect();
    assert_eq!(ops, vec![JsPrim::IndexRead]);
}

/// A write to a property is a write to the HEAP and not a rebind, and it answers
/// the value written.
#[test]
fn a_property_write_answers_what_it_stored() {
    let lowered = only("function f(o) { let a = (o.x = 7); return a; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let write = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::FieldWrite)))
        .expect("a write");
    assert!(write.effect.has(Effect::WRITES));
    // The returned value is the constant 7, not the write's own result.
    let returned = match &lowered.func.block(lowered.func.entry()).terminator {
        Some(Terminator::Return(Some(value))) => *value,
        other => panic!("expected a returned value, got {other:?}"),
    };
    let from = lowered
        .func
        .insts
        .iter()
        .find(|held| held.result == returned)
        .expect("an instruction");
    assert!(matches!(
        &from.op,
        rts_mir::Op::Const(rts_mir::Const::Int(7))
    ));
}

#[test]
fn an_indexed_write_takes_the_receiver_the_key_and_the_value() {
    let lowered = only("function f(a, i, v) { a[i] = v; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let write = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::IndexWrite)))
        .expect("a write");
    match &write.op {
        rts_mir::Op::Prim { args, .. } => assert_eq!(args.len(), 3),
        other => panic!("expected a primitive, got {other:?}"),
    }
}

/// A string is a constant of this language's table, and its type is text.
#[test]
fn a_string_literal_is_a_declared_constant_of_this_language() {
    let lowered = only("function f() { return 'hello'; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
    let first = lowered.func.insts.first().expect("one constant");
    assert_eq!(*types.of(first.result), Type::Str);
    match &first.op {
        rts_mir::Op::Const(rts_mir::Const::Declared(index)) => {
            assert!(matches!(
                lowered.domain.declared(*index),
                Some(crate::domain::JsConst::Text(_))
            ));
        }
        other => panic!("expected a declared constant, got {other:?}"),
    }
}

/// A key and a string are different entries even for the same text, because one
/// names a position in a layout and the other is a value.
#[test]
fn a_key_and_a_string_of_one_spelling_are_two_constants() {
    let lowered = only("function f(o) { return o.x + 'x'; }").expect("covered");
    let declared: Vec<_> = lowered
        .func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Const(rts_mir::Const::Declared(index)) => {
                lowered.domain.declared(*index).cloned()
            }
            _ => None,
        })
        .collect();
    assert_eq!(declared.len(), 2);
    assert!(matches!(declared[0], crate::domain::JsConst::Key(_)));
    assert!(matches!(declared[1], crate::domain::JsConst::Text(_)));
}

/// An optional access short-circuits, which is a branch this stage does not build
/// — refused by name rather than lowered as an ordinary read.
#[test]
fn an_optional_access_is_refused_rather_than_read_unconditionally() {
    let refused = only("function f(o) { return o?.x; }").expect_err("an optional link");
    assert!(matches!(refused, Unsupported::Expression(_)));
}

/// An object literal is pairs of a declared key and a value, in SOURCE ORDER --
/// which is what decides the layout, so the order is a semantic and not a style.
#[test]
fn an_object_literal_is_key_value_pairs_in_the_order_written() {
    let lowered = only("function f(n) { return { x: 1, y: n }; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let built = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::NewObject)))
        .expect("a construction");
    let args = match &built.op {
        rts_mir::Op::Prim { args, .. } => args.clone(),
        other => panic!("expected a primitive, got {other:?}"),
    };
    assert_eq!(args.len(), 4, "two properties, two arguments each");
    // Argument zero is the key `x` and argument two is the key `y`, in that order.
    let key_of = |value: rts_mir::ValueId| {
        lowered
            .func
            .insts
            .iter()
            .find(|held| held.result == value)
            .and_then(|held| match &held.op {
                rts_mir::Op::Const(rts_mir::Const::Declared(index)) => {
                    lowered.domain.declared(*index).cloned()
                }
                _ => None,
            })
    };
    assert!(matches!(
        key_of(args[0]),
        Some(crate::domain::JsConst::Key(_))
    ));
    assert!(matches!(
        key_of(args[2]),
        Some(crate::domain::JsConst::Key(_))
    ));
    assert_ne!(key_of(args[0]), key_of(args[2]));
}

/// Building one allocates, and no shape is claimed: the type is the weaker one,
/// because the tree that decides layouts belongs to the runtime.
#[test]
fn an_object_literal_allocates_and_claims_no_shape() {
    let lowered = only("function f() { return { x: 1 }; }").expect("covered");
    let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
    let built = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::NewObject)))
        .expect("a construction");
    assert!(built.effect.has(Effect::ALLOCATES));
    assert_eq!(*types.of(built.result), Type::Object);
    assert!(
        !matches!(*types.of(built.result), Type::Shaped(_)),
        "a shape id here would be a number only the runtime mints"
    );
}

/// A method is not a value under a key: it is installed with a home object, which
/// is what `super.x` inside it reads from. Collapsing the two would compile and
/// would make `super` mean nothing.
#[test]
fn a_method_in_an_object_literal_is_refused_rather_than_stored_as_a_value() {
    let refused = only("function f() { return { m() { return 1; } }; }").expect_err("a method");
    assert!(matches!(refused, Unsupported::Expression(_)));
}

#[test]
fn an_accessor_in_an_object_literal_is_refused() {
    let refused = only("function f() { return { get k() { return 1; } }; }").expect_err("a getter");
    assert!(matches!(refused, Unsupported::Expression(_)));
}

/// A computed key is a value, so the layout is not the one written — refused with
/// that reason rather than lowered against a key nobody can name.
#[test]
fn a_computed_key_is_refused_because_the_layout_is_not_the_one_written() {
    let refused = only("function f(k) { return { [k]: 1 }; }").expect_err("a computed key");
    assert!(matches!(refused, Unsupported::Expression(_)));
}

/// Shorthand is the same thing written shorter, so it lowers.
#[test]
fn a_shorthand_property_lowers_like_the_long_form() {
    let lowered = only("function f(x) { return { x }; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    assert!(
        lowered.func.insts.iter().any(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::NewObject)))
    );
}

/// A method call reads its receiver ONCE, reads the callee from it, and passes the
/// receiver as a field rather than as an argument.
#[test]
fn a_method_call_reads_its_receiver_once_and_passes_it_as_itself() {
    let lowered = only("function f(o, n) { return o.scale(n); }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));

    let call = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Call { .. }))
        .expect("a call");
    let (callee, receiver, args) = match &call.op {
        rts_mir::Op::Call {
            callee,
            receiver,
            args,
        } => (*callee, *receiver, args.clone()),
        other => panic!("expected a call, got {other:?}"),
    };
    // The receiver is the function's first parameter, read once -- there is exactly
    // one field read and its object is that same value.
    let receiver = receiver.expect("a method call passes a receiver");
    assert_eq!(receiver, rts_mir::ValueId(0), "the parameter itself");
    // The arguments are what the program wrote, and the receiver is NOT among them.
    assert_eq!(args.len(), 1);
    assert_ne!(args[0], receiver);
    // The callee is the value the field read produced.
    let read = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::FieldRead)))
        .expect("a field read");
    assert_eq!(callee, rts_mir::cfg::Callee::Dynamic(read.result));
}

/// A call to a binding that holds no function of this module is a DYNAMIC call and
/// no longer a refusal — it reaches whatever the value is, and passes no receiver.
#[test]
fn a_call_through_a_parameter_is_a_dynamic_call_with_no_receiver() {
    let lowered = only("function apply(g, n) { return g(n); }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let call = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Call { .. }))
        .expect("a call");
    match &call.op {
        rts_mir::Op::Call {
            callee, receiver, ..
        } => {
            assert_eq!(*callee, rts_mir::cfg::Callee::Dynamic(rts_mir::ValueId(0)));
            assert!(receiver.is_none(), "a bare name passes no receiver");
        }
        other => panic!("expected a call, got {other:?}"),
    }
}

/// The reasons a binding holds no value here are told apart, and this test followed
/// the work: it asserted a module binding was REFUSED, then that it was read by an
/// operation naming the binding, and now that it is read out of an environment. What
/// it pins is the distinction -- a dead zone is still a dead zone.
#[test]
fn a_binding_outside_this_function_is_not_reported_as_a_dead_zone() {
    let outside = only("function f() { return outer; } let outer = 1;")
        .expect("a module binding is an outer read now");
    assert!(
        outside.func.insts.iter().any(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if outside.domain.meaning(*prim) == Some(JsPrim::EnvRead))),
        "it reads the binding out of the environment rather than being refused"
    );

    let inside = only("function f() { const a = later; const later = 1; return a; }");
    assert_eq!(
        inside.expect_err("read before its declaration"),
        Unsupported::Expression("a binding read before its declaration is in its dead zone")
    );
}

/// A call always calls user code and may throw, because what the callee does is not
/// known here.
#[test]
fn a_call_carries_the_effect_a_call_has() {
    let lowered = only("function f(o) { return o.m(); }").expect("covered");
    let call = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Call { .. }))
        .expect("a call");
    assert!(call.effect.has(Effect::CALLS_USER));
    assert!(call.effect.has(Effect::THROWS));
    assert!(!call.effect.falls_through());
}

/// A spread argument is refused: the count would stop being the count written.
#[test]
fn a_spread_argument_is_refused() {
    let refused = only("function f(o, xs) { return o.m(...xs); }").expect_err("a spread");
    assert!(matches!(refused, Unsupported::Expression(_)));
}

/// A binding outside this function is read out of the ENVIRONMENT that owns it, by
/// its spelling -- and the read reaches no user code and raises nothing, because an
/// environment is an object the program never holds and every key in it was defined
/// as an own data property when it was made.
#[test]
fn an_outer_binding_is_read_out_of_the_environment_by_its_spelling() {
    let mut names = Names::new();
    let program = parse_script(
        "let total = 0; function add(n) { return total + n; }",
        &mut names,
    )
    .expect("parses");
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
        .expect("a function");
    let lowered = lower(function, &resolution, Tier::Generic).expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));

    let read = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::EnvRead)))
        .expect("an environment read");
    assert!(read.effect.has(Effect::READS));
    assert!(!read.effect.has(Effect::THROWS));
    assert!(!read.effect.has(Effect::CALLS_USER));

    let (environment, key) = match &read.op {
        rts_mir::Op::Prim { args, .. } => (args[0], args[1]),
        other => panic!("expected a primitive, got {other:?}"),
    };
    let defined = |value| {
        lowered
            .func
            .insts
            .iter()
            .find(|inst| inst.result == value)
            .expect("defined")
    };
    // ZERO LINKS: `add` owns nothing captured, so the environment it was made in is
    // the module's, which owns `total`.
    assert!(
        matches!(&defined(environment).op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::EnclosingEnvironment))
    );
    match &defined(key).op {
        rts_mir::Op::Const(rts_mir::Const::Declared(index)) => assert_eq!(
            lowered.domain.declared(*index),
            Some(&crate::domain::JsConst::Key(names.intern("total")))
        ),
        other => panic!("expected a declared key, got {other:?}"),
    }
}

/// Two accesses to ONE outer binding carry one index, which is what a pass hoisting
/// a load out of a loop compares.
#[test]
fn two_reads_of_one_outer_binding_name_it_once() {
    let mut names = Names::new();
    let program = parse_script(
        "let a = 0; let b = 0; function f() { return a + a + b; }",
        &mut names,
    )
    .expect("parses");
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
        .expect("a function");
    let lowered = lower(function, &resolution, Tier::Generic).expect("covered");

    let indices: Vec<u32> = lowered
        .func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Const(rts_mir::Const::Declared(index)) => Some(*index),
            _ => None,
        })
        .collect();
    assert_eq!(indices.len(), 3, "three named reads");
    assert_eq!(indices[0], indices[1], "the same binding, the same index");
    assert_ne!(indices[1], indices[2], "a different binding, another index");
}

/// A write to an outer binding is a write to its environment, not a rebind: SSA
/// rebinding is for a value this function holds in a register.
#[test]
fn a_write_to_an_outer_binding_is_an_operation() {
    let mut names = Names::new();
    let program =
        parse_script("let total = 0; function add(n) { total = n; }", &mut names).expect("parses");
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
        .expect("a function");
    let lowered = lower(function, &resolution, Tier::Generic).expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));

    let write = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::EnvWrite)))
        .expect("an environment write");
    assert!(write.effect.has(Effect::WRITES));
    match &write.op {
        rts_mir::Op::Prim { args, .. } => {
            assert_eq!(args.len(), 3, "the environment, the key and the value")
        }
        other => panic!("expected a primitive, got {other:?}"),
    }
}

/// A local read before its declaration is still a dead zone, and still refused —
/// which is what keeps the two apart now that one of them lowers.
#[test]
fn a_dead_zone_read_is_still_refused_after_outer_reads_lower() {
    let refused = only("function f() { const a = later; const later = 1; return a; }")
        .expect_err("read before its declaration");
    assert_eq!(
        refused,
        Unsupported::Expression("a binding read before its declaration is in its dead zone")
    );
}

/// A bitwise operator answers an `Int32` whatever it was given, which is the reason a
/// program writes one: `x | 0` is how a number becomes provably narrow.
#[test]
fn a_bitwise_operator_answers_an_int32_and_the_next_one_is_therefore_pure() {
    let lowered = only("function f(x) { let n = x | 0; return n & 255; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);

    let bitwise: Vec<_> = lowered
        .func
        .insts
        .iter()
        .filter(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::BitwiseInt32)))
        .collect();
    assert_eq!(bitwise.len(), 2);
    // Both answer an Int32...
    assert_eq!(*types.of(bitwise[0].result), Type::Int32);
    assert_eq!(*types.of(bitwise[1].result), Type::Int32);
    // ...and the SECOND is pure, because the first proved its operand narrow. The
    // first is not: its left operand is a parameter, so it may coerce an object.
    assert!(bitwise[0].effect.has(Effect::CALLS_USER));
    assert!(bitwise[1].effect.is_pure());
}

/// `>>>` is NOT a row of that operator: it answers `ToUint32`, so `-1 >>> 0` is
/// 4294967295 — which an `Int32` cannot hold. Giving it the row would be wrong at
/// exactly the value that distinguishes it.
#[test]
fn unsigned_shift_is_refused_because_its_answer_is_not_an_int32() {
    let refused = only("function f(x) { return x >>> 0; }").expect_err("no row for >>>");
    assert_eq!(refused, Unsupported::Operator(BinaryOp::UShr));
}

/// Every expression kind names itself in a refusal. There was a fall-through arm
/// reading "an expression kind", and it was the biggest single bucket in `bench/` —
/// a refusal that does not name itself is worth the same as no refusal, because the
/// survey is the work queue and a bucket cannot be queued.
///
/// The kinds asserted here are the ones still refused: a construction, a type
/// assertion and a template literal were in this list and all three lower now, which
/// is why the list moved rather than the test being deleted.
#[test]
fn a_refused_expression_names_what_it_was() {
    assert_eq!(
        only("function f(a) { return a?.b; }").expect_err("an optional chain"),
        Unsupported::Expression("an optional chain")
    );
    assert_eq!(
        only("function f(a, b) { return (a, b); }").expect_err("a comma expression"),
        Unsupported::Expression("a comma expression")
    );
    assert_eq!(
        only("function f() { return class {}; }").expect_err("a class expression"),
        Unsupported::Expression("a class expression")
    );
}

/// `this` is an operation that reads the receiver of this activation. It is pure --
/// the value is already there -- and nothing is known about it without a proof about
/// the call site.
#[test]
fn this_is_an_operation_that_reads_the_receiver() {
    let lowered = only("function f() { return this; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let held = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::ThisValue)))
        .expect("a receiver read");
    assert!(held.effect.is_pure());
    match &held.op {
        rts_mir::Op::Prim { args, .. } => assert!(args.is_empty()),
        other => panic!("expected a primitive, got {other:?}"),
    }
    let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
    assert_eq!(*types.of(held.result), Type::Anything);
}

/// Unary plus IS `ToNumber` and gets no row of its own: `+a` and the coercion an
/// increment performs are the same operation, and two rows would let a pass fold one
/// and miss the other.
#[test]
fn unary_plus_is_the_same_operation_an_increment_coerces_with() {
    let lowered = only("function f(a) { return +a; }").expect("covered");
    let ops: Vec<_> = lowered
        .func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Prim { prim, .. } => lowered.domain.meaning(*prim),
            _ => None,
        })
        .collect();
    assert_eq!(ops, vec![JsPrim::ToNumber]);
}

/// `-a` answers a number and never an `Int32`: negating the most negative one does
/// not fit, and negative zero is not a value this lattice names apart from zero.
/// `~a` does answer an `Int32`, which is the difference worth pinning.
#[test]
fn negation_is_a_number_and_a_bitwise_not_is_an_int32() {
    let negated = only("function f() { return -1; }").expect("covered");
    let types = rts_mir::infer::infer(&negated.func, &negated.domain);
    let held = negated
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if negated.domain.meaning(*prim) == Some(JsPrim::Negate)))
        .expect("a negation");
    assert_eq!(*types.of(held.result), Type::Double);

    let flipped = only("function f() { return ~1; }").expect("covered");
    let types = rts_mir::infer::infer(&flipped.func, &flipped.domain);
    let held = flipped
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if flipped.domain.meaning(*prim) == Some(JsPrim::BitwiseNot)))
        .expect("a bitwise not");
    assert_eq!(*types.of(held.result), Type::Int32);
}

/// `void a` evaluates its operand and answers `undefined` -- both halves, because
/// dropping the operand would drop its effects.
#[test]
fn void_evaluates_its_operand_and_answers_undefined() {
    let lowered = only("function f(o) { return void o.x; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    // The property read happened...
    assert!(
        lowered.func.insts.iter().any(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::FieldRead))),
        "the operand is evaluated"
    );
    // ...and the answer is `undefined`.
    let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
    let returned = match &lowered.func.block(lowered.func.entry()).terminator {
        Some(Terminator::Return(Some(value))) => *value,
        other => panic!("expected a returned value, got {other:?}"),
    };
    assert_eq!(*types.of(returned), Type::Undefined);
}

/// `delete` removes a property, so its operand is a PLACE: lowering the operand
/// first would evaluate what is about to be deleted.
#[test]
fn delete_is_refused_because_its_operand_is_a_place() {
    let refused = only("function f(o) { return delete o.x; }").expect_err("delete");
    assert_eq!(
        refused,
        Unsupported::Expression("delete removes a property, so its operand is a place")
    );
}

/// A conditional is a branch whose arms answer a value, joined into one — and the
/// type at the join is the domain's join of the two. Not in `return` position, where it
/// is rewritten into two returns so that a call in an arm is a tail call.
#[test]
fn a_conditional_joins_the_two_arms_types() {
    let lowered = only("function f(c) { const v = c ? 1 : 0.5; return v; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
    let join = lowered
        .func
        .block_ids()
        .find(|held| lowered.func.predecessors(*held).len() == 2)
        .expect("a join");
    let held = lowered.func.block(join).params[0];
    // An integer from one arm and a fraction from the other: one numeric type here.
    assert_eq!(*types.of(held), Type::Double);
}

/// `a && b` answers `a` ITSELF when `a` is falsy, not `false`: `0 && 1` is `0`. Five
/// of the seven falsy values are not `false`, so answering a boolean would be wrong
/// for every one of them.
#[test]
fn a_short_circuit_answers_the_subject_and_not_a_boolean() {
    let lowered = only("function f(a, b) { return a && b; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    // One arm jumps with the RIGHT side, the other with the left operand itself --
    // which is parameter zero.
    let join = lowered
        .func
        .block_ids()
        .find(|held| lowered.func.predecessors(*held).len() == 2)
        .expect("a join");
    let carried: Vec<_> = lowered
        .func
        .predecessors(join)
        .iter()
        .filter_map(|from| match &lowered.func.block(*from).terminator {
            Some(Terminator::Jump { args, .. }) => args.first().copied(),
            _ => None,
        })
        .collect();
    assert_eq!(carried.len(), 2);
    assert!(
        carried.contains(&rts_mir::ValueId(0)),
        "the falsy arm answers the left operand itself, {carried:?}"
    );
}

/// `a ?? b` is NOT a truth test, which is the operator's whole point: `0 ?? 1` is `0`
/// where `0 || 1` is `1`. Its condition is a row of its own.
#[test]
fn coalescing_asks_whether_the_left_is_nullish_and_not_whether_it_is_truthy() {
    let coalesce = only("function f(a, b) { return a ?? b; }").expect("covered");
    let ops: Vec<_> = coalesce
        .func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Prim { prim, .. } => coalesce.domain.meaning(*prim),
            _ => None,
        })
        .collect();
    assert_eq!(ops, vec![JsPrim::IsNullish]);

    // Where `||` on the same operands asks about truth.
    let or = only("function f(a, b) { return a || b; }").expect("covered");
    let ops: Vec<_> = or
        .func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Prim { prim, .. } => or.domain.meaning(*prim),
            _ => None,
        })
        .collect();
    assert_eq!(ops, vec![JsPrim::Truthy]);
}

/// A choice nested inside a choice: the jumps are written after both arms are
/// lowered, because an arm that nests one moves where building is.
#[test]
fn a_nested_choice_terminates_the_block_each_arm_actually_ended_in() {
    let lowered = only("function f(a, b, c) { return a ? (b ? 1 : 2) : c; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
}

/// A bigint is still refused, and this test followed the work: the regex beside it was
/// refused for being "an object the runtime builds", which is exactly what an entry
/// point is for -- so it lowers now and the bigint keeps the assertion.
#[test]
fn a_bigint_is_refused_as_a_second_numeric_tower() {
    let bigint = only("function f() { return 1n; }").expect_err("a bigint");
    assert_eq!(
        bigint,
        Unsupported::Expression("a bigint literal is a second numeric tower")
    );
}

/// A `switch` merges at its exit, which the first version of the lowering skipped —
/// on the reasoning that it has no back edge. It has no back edge and several paths
/// into one exit, which is a different question.
#[test]
fn a_switch_merges_what_its_clauses_assign() {
    let lowered = only(
        "function f(n) {
           let out = 0;
           switch (n) { case 1: out = 10; break; default: out = 99; }
           return out;
         }",
    )
    .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));

    // The value returned is the exit's PARAMETER and not any clause's own value: a
    // clause's value is defined in a block only one path reaches.
    let returned = match &lowered.func.block(lowered.func.entry()).terminator {
        Some(_) => match lowered
            .func
            .block_ids()
            .filter_map(|block| match &lowered.func.block(block).terminator {
                Some(Terminator::Return(Some(value))) => Some((block, *value)),
                _ => None,
            })
            .next()
        {
            Some(held) => held,
            None => panic!("a return"),
        },
        None => panic!("a terminator"),
    };
    let (block, value) = returned;
    assert!(
        lowered.func.block(block).params.contains(&value),
        "the exit answers its own parameter, not a clause's value"
    );
    assert!(lowered.func.predecessors(block).len() >= 2);
}

/// Fall-through is the whole of what makes a switch not an if-chain: a clause that
/// does not `break` continues into the NEXT clause's statements.
#[test]
fn a_clause_without_a_break_falls_into_the_next_one() {
    let lowered = only(
        "function f(n) {
           let out = 0;
           switch (n) { case 1: out = 1; case 2: out = out + 1; break; }
           return out;
         }",
    )
    .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    // The second clause's body is reached from TWO places: its own test and the first
    // clause falling through.
    let merged = lowered
        .func
        .block_ids()
        .filter(|held| lowered.func.predecessors(*held).len() >= 2)
        .count();
    assert!(merged >= 2, "a body merge and an exit merge");
}

/// `default` is matched LAST and executed where it sits, so a switch whose default is
/// written first still tries every case before it.
#[test]
fn a_default_written_first_is_still_matched_last() {
    let lowered = only(
        "function f(n) {
           let out = 0;
           switch (n) { default: out = 9; break; case 1: out = 1; break; }
           return out;
         }",
    )
    .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    // Exactly one comparison, for the one case -- the default is not a test.
    let tests = lowered
        .func
        .insts
        .iter()
        .filter(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::StrictEquals)))
        .count();
    assert_eq!(tests, 1);
}

/// A `continue` inside a switch inside a loop reaches the LOOP. Without the frame
/// kind the two stacks are one and it would leave the loop instead — a wrong answer
/// that compiles, and a graph that looks perfectly well formed.
#[test]
fn a_continue_inside_a_switch_reaches_the_loop_and_not_the_switch() {
    let lowered = only(
        "function f(n) {
           let at = 0;
           while (at < n) {
             at = at + 1;
             switch (at) { case 1: continue; default: break; }
           }
           return at;
         }",
    )
    .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    // The loop header is reached from more than one place: the entry and the back
    // edge through the `continue`.
    let header = lowered
        .func
        .block_ids()
        .find(|held| lowered.func.predecessors(*held).len() >= 2)
        .expect("a header");
    assert!(!lowered.func.block(header).params.is_empty());
}

/// `!==` is a negation of `===`, and this is the case where rewriting is LEGAL —
/// unlike `a > b`, which could not become `b < a` because the two coerce in opposite
/// orders.
#[test]
fn strict_inequality_is_a_negation_and_not_a_row() {
    let lowered = only("function f(a, b) { return a !== b; }").expect("covered");
    let ops: Vec<_> = lowered
        .func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Prim { prim, .. } => lowered.domain.meaning(*prim),
            _ => None,
        })
        .collect();
    assert_eq!(ops, vec![JsPrim::StrictEquals, JsPrim::Not]);
}

/// Loose equality is a DIFFERENT operation from strict, not a laxer spelling: it may
/// call `valueOf` where strict calls nothing, so the two have different effects over
/// the same operands.
#[test]
fn loose_equality_may_call_user_code_where_strict_cannot() {
    let loose = only("function f(a, b) { return a == b; }").expect("covered");
    let held = loose.func.insts.last().expect("a comparison");
    assert!(held.effect.has(Effect::CALLS_USER));

    let strict = only("function f(a, b) { return a === b; }").expect("covered");
    let held = strict.func.insts.last().expect("a comparison");
    assert!(held.effect.is_pure());
}

/// `instanceof` and `in` both read the heap and both may reach user code — one
/// through `Symbol.hasInstance`, the other through a proxy's `has` trap.
#[test]
fn instanceof_and_in_may_both_reach_user_code() {
    for source in [
        "function f(a, b) { return a instanceof b; }",
        "function f(a, b) { return a in b; }",
    ] {
        let lowered = only(source).expect("covered");
        let held = lowered.func.insts.last().expect("a comparison");
        assert!(held.effect.has(Effect::READS), "{source}");
        assert!(held.effect.has(Effect::CALLS_USER), "{source}");
    }
}

/// An object pattern is the property reads it is.
#[test]
fn an_object_pattern_reads_each_property_it_names() {
    let lowered = only("function f(o) { const { a, b } = o; return a + b; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let reads = lowered
        .func
        .insts
        .iter()
        .filter(|held| matches!(&held.op, rts_mir::Op::Prim { prim, .. } if lowered.domain.meaning(*prim) == Some(JsPrim::FieldRead)))
        .count();
    assert_eq!(reads, 2);
}

/// A default is a BRANCH, because it runs only when the value read was `undefined` —
/// so `{ a = f() }` over an object that has `a` must not call `f`.
#[test]
fn a_pattern_default_is_a_branch_and_not_a_coalesce() {
    let lowered = only("function f(o) { const { a = 7 } = o; return a; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let ops: Vec<_> = lowered
        .func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Prim { prim, .. } => lowered.domain.meaning(*prim),
            _ => None,
        })
        .collect();
    // A comparison against `undefined` specifically -- not `IsNullish`, because
    // `{ a = 1 }` over `{ a: null }` binds `null`: it is a value that was there.
    assert!(ops.contains(&JsPrim::StrictEquals), "{ops:?}");
    assert!(!ops.contains(&JsPrim::IsNullish), "{ops:?}");
    // And it is a branch: there is a join.
    assert!(
        lowered
            .func
            .block_ids()
            .any(|held| lowered.func.predecessors(held).len() == 2)
    );
}

/// `const [a] = xs` is NOT `a = xs[0]`, and this test replaced one asserting the
/// refusal that said so. It steps the iterator, which is the same machinery `for`-`of`
/// steps -- so it works on a `Set` and fails on an object with numeric keys and no
/// iterator, and a lowering that indexed would be wrong in both directions at once.
#[test]
fn an_array_pattern_steps_the_iterator_rather_than_indexing() {
    let lowered = only("function f(xs) { const [a] = xs; return a; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let ops: Vec<_> = lowered
        .func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Prim { prim, .. } => lowered.domain.meaning(*prim),
            _ => None,
        })
        .collect();
    // NOTHING IS INDEXED. An `IndexRead` here would be the defect this refusal used
    // to prevent, arriving under a different name.
    assert!(!ops.contains(&crate::domain::JsPrim::IndexRead));
    assert!(
        ops.contains(&crate::domain::JsPrim::Truthy),
        "done is tested"
    );
}

/// A slot past the end binds `undefined` and NOT the step's own `value`. `{ done:
/// true, value: 42 }` is a legal answer from a hand-written iterator, and reading
/// `value` unconditionally is correct for every well-behaved one and silently wrong for
/// that one — so each slot is a join whose two arms are the value and the singleton.
#[test]
fn a_slot_past_the_end_binds_undefined_rather_than_the_steps_value() {
    let lowered = only("function f(xs) { const [a, b] = xs; return b; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    // One join per bound slot, each taking exactly one parameter.
    let joins = lowered
        .func
        .block_ids()
        .filter(|held| {
            lowered.func.predecessors(*held).len() == 2
                && lowered.func.block(*held).params.len() == 1
        })
        .count();
    assert!(joins >= 2, "two slots, two joins: {joins}");
}

/// A HOLE still takes a step and binds nothing: `[, b] = xs` reads two elements. The
/// tree keeps the hole rather than omitting it for exactly this reason — dropping one
/// would shift every element after it onto the wrong value.
#[test]
fn a_hole_takes_a_step_and_binds_nothing() {
    let with_hole = only("function f(xs) { const [, b] = xs; return b; }").expect("covered");
    let without = only("function f(xs) { const [b] = xs; return b; }").expect("covered");
    let steps = |held: &Lowered| {
        held.func
            .insts
            .iter()
            .filter(|inst| matches!(&inst.op, rts_mir::Op::Call { .. }))
            .count()
    };
    assert!(
        steps(&with_hole) > steps(&without),
        "the hole costs a step: {} against {}",
        steps(&with_hole),
        steps(&without)
    );
}

/// The close is owed only when the PATTERN stopped first, which is what the last
/// slot's `done` test decides: `const [a] = endless()` takes one element and closes.
#[test]
fn the_close_is_under_the_last_slots_done_test() {
    let lowered = only("function f(xs) { const [a] = xs; return a; }").expect("covered");
    let ops: Vec<_> = lowered
        .func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Prim { prim, .. } => lowered.domain.meaning(*prim),
            _ => None,
        })
        .collect();
    // The close is guarded by a nullish test on `return`, the same as the loop's.
    assert!(ops.contains(&crate::domain::JsPrim::IsNullish));
}

/// An array rest target gathers what the iterator has LEFT, which is a LOOP and not a
/// drain — the two differ by however many slots came before it. This test replaced one
/// asserting the refusal that drew exactly that distinction and could not act on it.
#[test]
fn an_array_rest_target_gathers_by_stepping_and_appending() {
    let lowered = only("function f(xs) { const [a, ...r] = xs; return r; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    use crate::runtime::RuntimeOp;
    let appends = lowered
        .func
        .insts
        .iter()
        .filter(|held| match &held.op {
            rts_mir::Op::Call {
                callee: rts_mir::cfg::Callee::Entry(entry),
                ..
            } => lowered.domain.entry_meaning(*entry) == Some(RuntimeOp::ArrayAppend),
            _ => false,
        })
        .count();
    // ONE append, inside a loop -- not one per element, which is the whole difference
    // between gathering and a literal's fixed list.
    assert_eq!(appends, 1);
    // And it is a loop: some block is its own successor's predecessor twice over,
    // which is what a back edge looks like from here.
    assert!(
        lowered
            .func
            .block_ids()
            .any(|held| lowered.func.predecessors(held).len() == 2),
        "a back edge"
    );
}

/// Gathering cannot close, and that is not an omission: it runs until the iterator
/// reports `done`, which is the one way out that owes nothing. So a pattern WITH a rest
/// target has no close where the same pattern without one does.
#[test]
fn a_rest_target_owes_no_close_because_the_iterator_ended_itself() {
    let gathering = only("function f(xs) { const [a, ...r] = xs; return r; }").expect("covered");
    let stopping = only("function f(xs) { const [a] = xs; return a; }").expect("covered");
    let nullish = |held: &Lowered| {
        held.func
            .insts
            .iter()
            .filter_map(|inst| match &inst.op {
                rts_mir::Op::Prim { prim, .. } => held.domain.meaning(*prim),
                _ => None,
            })
            .filter(|held| *held == crate::domain::JsPrim::IsNullish)
            .count()
    };
    assert_eq!(nullish(&gathering), 0, "nothing is closed");
    assert_eq!(nullish(&stopping), 1, "the pattern stopped, so it closes");
}

/// An object rest collects the own enumerable properties NOT already named, which
/// needs the key set at run time.
#[test]
fn an_object_rest_is_refused_because_it_needs_the_keys() {
    let refused =
        only("function f(o) { const { a, ...rest } = o; return rest; }").expect_err("a rest");
    assert_eq!(
        refused,
        Unsupported::Expression("an object rest target needs the own keys at run time")
    );
}

/// A nested pattern and a computed key each keep their own refusal, so a survey can
/// count them apart.
#[test]
fn a_nested_pattern_and_a_computed_key_are_refused_apart() {
    let nested =
        only("function f(o) { const { a: { b } } = o; return b; }").expect_err("a nested pattern");
    assert!(matches!(nested, Unsupported::Expression(_)));
    let computed =
        only("function f(o, k) { const { [k]: a } = o; return a; }").expect_err("a computed key");
    assert!(matches!(computed, Unsupported::Expression(_)));
}

/// A `try`/`catch` is a protected region with one handler, and the handler receives
/// the raised value as its first block parameter.
#[test]
fn a_try_protects_its_body_and_the_handler_receives_the_value() {
    let lowered = only("function f(o) { try { o.risky(); } catch (e) { return e; } return 0; }")
        .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));

    let protected = lowered
        .func
        .block_ids()
        .find(|held| lowered.func.region_of(*held).is_some())
        .expect("a protected block");
    let region = lowered
        .func
        .region_of(protected)
        .expect("the block says which");
    let handler = lowered
        .func
        .region(region)
        .handler
        .expect("a catch is a handler");
    // The handler takes exactly one parameter: exactly one thing arrives with the
    // exception, and nothing jumps to it to carry anything else.
    assert_eq!(lowered.func.block(handler).params.len(), 1);
    // And nothing jumps to it -- an exception edge is not a jump.
    assert!(lowered.func.predecessors(handler).is_empty());
}

/// `catch (e)` binds the handler's parameter, which means entering the clause's own
/// scope: without that the name is not found at all and reads as a global, which is
/// what the first run of this reported.
#[test]
fn the_caught_binding_is_the_handlers_parameter() {
    let lowered = only("function f(o) { try { o.risky(); } catch (e) { return e; } return 0; }")
        .expect("covered");
    let protected = lowered
        .func
        .block_ids()
        .find(|held| lowered.func.region_of(*held).is_some())
        .expect("a protected block");
    let handler = lowered
        .func
        .region(lowered.func.region_of(protected).unwrap())
        .handler
        .unwrap();
    let param = lowered.func.block(handler).params[0];
    assert_eq!(
        lowered.func.block(handler).terminator,
        Some(Terminator::Return(Some(param))),
        "the clause returns what it caught"
    );
}

/// `catch {}` with no binding still receives the value, because a handler that did not
/// would have to find it somewhere else.
#[test]
fn a_catch_with_no_binding_still_receives_the_value() {
    let lowered = only("function f(o) { try { o.risky(); } catch { return 1; } return 0; }")
        .expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let protected = lowered
        .func
        .block_ids()
        .find(|held| lowered.func.region_of(*held).is_some())
        .expect("a protected block");
    let handler = lowered
        .func
        .region(lowered.func.region_of(protected).unwrap())
        .handler
        .unwrap();
    assert_eq!(lowered.func.block(handler).params.len(), 1);
}

/// An assignment in a protected body cannot reach the handler as an SSA value: nothing
/// jumps to a handler, so there is no edge to carry an argument and no single value to
/// carry. The first draft passed one as a block parameter, which compiles and is wrong.
#[test]
fn an_assignment_in_a_protected_body_is_refused_with_its_reason() {
    let refused = only("function f(o) { let x = 1; try { x = 2; } catch (e) { } return x; }")
        .expect_err("an assignment in the body");
    assert_eq!(
        refused,
        Unsupported::Statement(
            "an assignment in a protected body is not visible to the handler without a cell"
        )
    );
}

/// A `finally` is a CLEANUP PIECE, and this test replaced one asserting it was refused
/// for running on every way out. It does run on every way out, and routing those paths
/// was never this lowering's job: `unwind::plan_unwind` computes which need it and the
/// cleanup is COPIED into each. What is built here is one piece with one exit.
#[test]
fn a_finally_is_a_cleanup_piece_on_its_region() {
    let lowered =
        only("function f(o) { try { o.m(); } catch (e) { } finally { o.n(); } }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    // TWO REGIONS, the cleanup's around the handler's: the `catch` runs before the
    // `finally`, and a throw from the `catch` still owes it. One region holding both
    // ran the `finally` first, because the machine runs a region's cleanup before its
    // handler.
    let outer = lowered
        .func
        .regions
        .iter()
        .position(|held| held.cleanup.is_some())
        .expect("a region carries the cleanup");
    let inner = lowered
        .func
        .regions
        .iter()
        .find(|held| held.handler.is_some())
        .expect("a region carries the handler");
    assert_eq!(inner.parent, Some(rts_mir::region::RegionId(outer as u32)));
    let region = &lowered.func.regions[outer];
    let entry = region.cleanup.expect("the region carries a cleanup");
    // ONE ENTRY AND NO PARAMETER: nothing jumps to a cleanup, so no edge could carry
    // one -- the same argument the handler's single parameter rests on, reaching the
    // opposite answer because a cleanup is not handed a value.
    assert!(lowered.func.block(entry).params.is_empty());
    assert!(lowered.func.predecessors(entry).is_empty());
    assert_eq!(
        lowered.func.block(entry).terminator,
        Some(Terminator::CleanupDone)
    );
}

/// A `try` with no `catch` is now ordinary, and the refusal that stood here called it
/// "only a finally". `Region::handler` is an Option precisely so that a region can
/// protect nothing and still clean up.
#[test]
fn a_try_with_only_a_finally_is_a_region_with_no_handler() {
    let lowered = only("function f(o) { try { o.m(); } finally { o.n(); } }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let protected = lowered
        .func
        .block_ids()
        .find(|held| lowered.func.region_of(*held).is_some())
        .expect("a protected block");
    let region = lowered
        .func
        .region(lowered.func.region_of(protected).unwrap());
    assert_eq!(region.handler, None);
    assert!(region.cleanup.is_some());
}

/// The cleanup block is created BEFORE the region opens, which is what puts it outside
/// the region it cleans up after. Inside its own region, a `finally` that threw would
/// re-enter its own `catch`.
#[test]
fn a_cleanup_is_outside_the_region_it_cleans_up_after() {
    let lowered =
        only("function f(o) { try { o.m(); } catch (e) { } finally { o.n(); } }").expect("covered");
    let of = lowered
        .func
        .regions
        .iter()
        .position(|held| held.cleanup.is_some())
        .expect("a region carries the cleanup");
    let of = rts_mir::region::RegionId(of as u32);
    let entry = lowered.func.region(of).cleanup.unwrap();
    // Outside the region AND every region inside it.
    let mut at = lowered.func.region_of(entry);
    while let Some(here) = at {
        assert_ne!(
            here, of,
            "the cleanup sits inside the region it cleans up after"
        );
        at = lowered.func.region(here).parent;
    }
}

/// A `finally` that can complete ABRUPTLY is a different shape, not a missing feature:
/// `try { return "t" } finally { return "f" }` answers "f", and a return inside a
/// copied cleanup is a terminator with no successor -- a copy left through a path the
/// unwind knows nothing about, which the machine's verifier names.
#[test]
fn a_finally_that_can_complete_abruptly_is_refused_as_the_wrong_shape() {
    let refused = only("function f(o) { try { return 1; } finally { return 2; } }")
        .expect_err("an abrupt finally");
    assert_eq!(
        refused,
        Unsupported::Statement(
            "a finally that can complete abruptly is a handler rather than a cleanup"
        )
    );
}

/// A cleanup BESIDE a handler that assigns is refused for a reason neither half has
/// alone: the cleanup is copied into the body's exit and the handler's, and those two
/// disagree about what the binding holds.
#[test]
fn a_cleanup_beside_an_assigning_handler_is_refused_because_the_copies_disagree() {
    let refused =
        only("function f(o) { let x = 1; try { o.m(); } catch (e) { x = 2; } finally { o.n(); } return x; }")
            .expect_err("the copies disagree");
    assert_eq!(
        refused,
        Unsupported::Statement(
            "a cleanup beside a handler that assigns needs a cell, because the copies disagree"
        )
    );
    // Each half ALONE is fine, which is what makes this a combination and not either.
    only("function f(o) { let x = 1; try { o.m(); } catch (e) { x = 2; } return x; }")
        .expect("a handler that assigns, with no cleanup");
    only("function f(o) { try { o.m(); } catch (e) { } finally { o.n(); } }")
        .expect("a cleanup, with a handler that assigns nothing");
}

/// A `throw` is a TERMINATOR, and this test replaced one asserting it was refused for
/// being an entry point. That reason was true of the runtime and false of the graph:
/// recording the value is an entry point, and where control goes next is the region
/// tree, which is control flow. Turning the statement away needed the first half to
/// stand for the whole, which is a machine answer given in a language file.
#[test]
fn a_throw_ends_its_block_by_raising() {
    let lowered = only("function f() { throw 1; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let entry = lowered.func.entry();
    let raised = match lowered.func.block(entry).terminator {
        Some(Terminator::Raise(held)) => held,
        ref other => panic!("expected a raise, got {other:?}"),
    };
    // IT READS THE VALUE, which is what makes the operand live up to this point --
    // a terminator that did not report reading it would let a pass drop the
    // instruction that computed what is being thrown.
    assert_eq!(
        lowered
            .func
            .block(entry)
            .terminator
            .as_ref()
            .unwrap()
            .reads(),
        vec![raised]
    );
}

/// And it names no successor. An exception edge is not a jump, which is the same thing
/// the handler side already says: nothing jumps to one, so nothing carries arguments.
#[test]
fn a_raise_names_no_successor_because_an_exception_edge_is_not_a_jump() {
    let lowered =
        only("function f(o) { try { throw 1; } catch (e) { return e; } }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let raising = lowered
        .func
        .block_ids()
        .find(|held| {
            matches!(
                lowered.func.block(*held).terminator,
                Some(Terminator::Raise(_))
            )
        })
        .expect("a raising block");
    assert!(
        lowered
            .func
            .block(raising)
            .terminator
            .as_ref()
            .unwrap()
            .successors()
            .is_empty()
    );
    // And it sits INSIDE the region, which is what says where it lands.
    assert!(lowered.func.region_of(raising).is_some());
}

/// A regular expression is an ENTRY POINT, and the first one this lowering names:
/// compiling a pattern and installing the object's state are things the runtime does.
#[test]
fn a_regex_literal_calls_an_entry_point() {
    let lowered = only("function f() { return /ab+c/gi; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let call = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(&held.op, rts_mir::Op::Call { .. }))
        .expect("a call");
    match &call.op {
        rts_mir::Op::Call { callee, args, .. } => {
            let entry = lowered
                .domain
                .entry_point(crate::runtime::RuntimeOp::RegexNew);
            assert_eq!(callee, &rts_mir::cfg::Callee::Entry(entry));
            // The pattern and the flags, both text.
            assert_eq!(args.len(), 2);
        }
        other => panic!("expected a call, got {other:?}"),
    }
    // And the domain knows what it answers, which is what an entry table is for.
    let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
    assert_eq!(*types.of(call.result), Type::Object);
}

/// A generator's body is an ordinary graph, and the suspension is one instruction in it.
/// Both used to be refused at the function for "parking a frame"; parking is now
/// something the graph SAYS rather than something it cannot express.
#[test]
fn a_generator_body_lowers_and_says_it_may_park() {
    let lowered = only("function* g(a) { yield a; return 1; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    assert!(
        lowered.func.may_suspend,
        "the flag is derived from the body, never passed in"
    );
    let parked = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(held.op, rts_mir::Op::Suspend { .. }))
        .expect("one suspension");
    assert!(parked.effect.may_suspend());
    // And control does not certainly reach what follows: `gen.throw(e)` resumes the
    // frame by raising exactly here.
    assert!(!parked.effect.falls_through());
}

/// An `await` is the same instruction as a `yield`, which is the finding rather than a
/// shortcut: `rts_cranelift::frame` owns one capability for both.
#[test]
fn an_await_is_the_same_suspension_as_a_yield() {
    let awaiting = only("async function f(p) { return await p; }").expect("covered");
    let yielding = only("function* g(p) { yield p; }").expect("covered");
    let op = |held: &Lowered| {
        held.func
            .insts
            .iter()
            .filter(|inst| matches!(inst.op, rts_mir::Op::Suspend { .. }))
            .map(|inst| inst.effect)
            .collect::<Vec<_>>()
    };
    assert_eq!(op(&awaiting), op(&yielding));
    assert!(awaiting.func.may_suspend && yielding.func.may_suspend);
}

/// What comes back is not what went out. Narrowing the result from the operand is the
/// natural mistake — the operand is right there — and it would be an unsound type a
/// later pass trusts with nothing checking it.
#[test]
fn what_a_suspension_answers_is_unknown_however_well_known_its_operand_is() {
    let lowered = only("function* g() { yield 1; }").expect("covered");
    let types = rts_mir::infer::infer(&lowered.func, &lowered.domain);
    let parked = lowered
        .func
        .insts
        .iter()
        .find(|held| matches!(held.op, rts_mir::Op::Suspend { .. }))
        .expect("one suspension");
    assert_eq!(*types.of(parked.result), Type::Anything);
}

/// A bare `yield` hands out nothing, which is not the same as handing out `undefined`:
/// no operand in the graph means nothing here decides which singleton stands for
/// absence, and the resumer's side already says.
#[test]
fn a_bare_yield_carries_no_operand() {
    let lowered = only("function* g() { yield; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let parked = lowered
        .func
        .insts
        .iter()
        .find_map(|held| match &held.op {
            rts_mir::Op::Suspend { value } => Some(*value),
            _ => None,
        })
        .expect("one suspension");
    assert_eq!(parked, None);
}

/// `yield*` is not a suspension, it is a loop around one — three methods forwarded to an
/// inner iterator — so it shares the piece the array pattern and `for`-`of` wait on.
#[test]
fn a_delegating_yield_is_refused_because_it_is_a_loop() {
    let refused = only("function* g(i) { yield* i; }").expect_err("a delegating yield");
    assert_eq!(
        refused,
        Unsupported::Expression(
            "yield* forwards next, throw and return to an inner iterator, which is a loop"
        )
    );
}

/// Nothing may be moved across a suspension — in either direction and whatever the other
/// operation is. Between the two halves of one, anything at all may run.
#[test]
fn nothing_commutes_with_a_suspension() {
    use rts_mir::Effect;
    for other in [
        Effect::PURE,
        Effect::READS,
        Effect::WRITES,
        Effect::ALLOCATES,
        Effect::SUSPENDS,
    ] {
        assert!(!Effect::SUSPENDS.commutes_with(other));
        assert!(!other.commutes_with(Effect::SUSPENDS));
    }
}

/// A `for`-`of` STEPS the protocol. Draining it through the entry point that already
/// exists would have been one call and no loop, and it changes three answers: a `break`
/// never closes, a mutated `Map` is walked as it was, and an endless source never ends.
#[test]
fn a_for_of_steps_the_protocol_rather_than_draining_it() {
    let lowered = only("function f(xs, o) { for (const x of xs) { o.m(x); } }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    // Four calls with a receiver: the iterator method, `next`, and the two closes --
    // one in the cleanup and one on the breaking path. No call to an entry point,
    // which is what says it did not drain.
    let entries = lowered
        .func
        .insts
        .iter()
        .filter(|held| {
            matches!(
                &held.op,
                rts_mir::Op::Call {
                    callee: rts_mir::cfg::Callee::Entry(_),
                    ..
                }
            )
        })
        .count();
    assert_eq!(entries, 0, "nothing is drained");
    let dynamic = lowered
        .func
        .insts
        .iter()
        .filter(|held| {
            matches!(
                &held.op,
                rts_mir::Op::Call {
                    callee: rts_mir::cfg::Callee::Dynamic(_),
                    receiver: Some(_),
                    ..
                }
            )
        })
        .count();
    assert!(dynamic >= 3, "the iterator, next, and the close: {dynamic}");
}

/// The keys it reads are ones the PROGRAM NEVER WROTE, so there is no spelling for the
/// interner to have a `Name` for. They are well-known keys rather than operations: `done`
/// off a step result is an ordinary property read.
#[test]
fn the_protocol_keys_are_well_known_rather_than_interned() {
    let lowered = only("function f(xs, o) { for (const x of xs) { o.m(x); } }").expect("covered");
    let mut seen = Vec::new();
    for held in &lowered.func.insts {
        if let rts_mir::Op::Const(rts_mir::Const::Declared(index)) = &held.op
            && let Some(crate::domain::JsConst::WellKnown(which)) = lowered.domain.declared(*index)
        {
            seen.push(*which);
        }
    }
    use crate::domain::WellKnown;
    for wanted in [
        WellKnown::IteratorSymbol,
        WellKnown::Next,
        WellKnown::Done,
        WellKnown::Element,
        WellKnown::Return,
    ] {
        assert!(seen.contains(&wanted), "{wanted:?} is read, saw {seen:?}");
    }
}

/// `done` is read with ToBoolean and not compared against `true`: an iterator answering
/// `done: 1` ends the loop, which is what the specification says.
#[test]
fn done_is_a_truth_test_and_not_a_comparison() {
    let lowered = only("function f(xs, o) { for (const x of xs) { o.m(x); } }").expect("covered");
    let ops: Vec<_> = lowered
        .func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Prim { prim, .. } => lowered.domain.meaning(*prim),
            _ => None,
        })
        .collect();
    assert!(ops.contains(&crate::domain::JsPrim::Truthy));
    assert!(
        !ops.contains(&crate::domain::JsPrim::StrictEquals),
        "nothing is compared to true"
    );
}

/// The body runs inside a region whose CLEANUP closes the iterator, which is how a
/// `return` out of the body and a raise from it reach the close: neither is a jump out of
/// the loop, so no block this lowering writes could be on their path.
#[test]
fn the_body_is_in_a_region_whose_cleanup_closes_the_iterator() {
    let lowered = only("function f(xs, o) { for (const x of xs) { o.m(x); } }").expect("covered");
    let inside = lowered
        .func
        .block_ids()
        .find(|held| lowered.func.region_of(*held).is_some())
        .expect("the body is protected");
    let region = lowered.func.region(lowered.func.region_of(inside).unwrap());
    // NO HANDLER. Nothing is caught here -- the obligation is a close on the way out,
    // not a chance to handle what went wrong.
    assert_eq!(region.handler, None);
    assert!(region.cleanup.is_some());
}

/// The `done` path must NOT close. A cleanup runs on every way out of a region, and the
/// ordinary end of a sequence owes nothing — `it.return()` after `done` is an observable
/// extra call on a user's iterator. So the exit is reached from the header, past the
/// region and past the closing block.
#[test]
fn the_done_path_leaves_without_closing() {
    let lowered = only("function f(xs, o) { for (const x of xs) { o.m(x); } }").expect("covered");
    let header = lowered
        .func
        .block_ids()
        .find(|held| {
            matches!(
                lowered.func.block(*held).terminator,
                Some(Terminator::Branch { .. })
            ) && lowered.func.region_of(*held).is_none()
        })
        .expect("the header tests done outside the region");
    // Whatever the true arm reaches, it is not in the region and not the cleanup.
    if let Some(Terminator::Branch { then_block, .. }) = lowered.func.block(header).terminator {
        assert_eq!(lowered.func.region_of(then_block), None);
    }
}

/// A `break` closes, and it is the one of the three ways out that IS a jump — so it gets
/// a block between the loop and the exit rather than riding the cleanup.
#[test]
fn a_break_leaves_through_a_closing_block() {
    let lowered = only("function f(xs) { for (const x of xs) { break; } }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    // The breaking block ends in a jump, and what it reaches is not the exit directly:
    // the close sits between, which is what the extra block is.
    let closes = lowered
        .func
        .insts
        .iter()
        .filter(|held| matches!(&held.op, rts_mir::Op::Call { .. }))
        .count();
    assert!(closes >= 3, "the iterator, next, and two closes: {closes}");
}

/// `return` on an iterator is OPTIONAL, so the close asks whether the key holds anything
/// first. Calling `undefined` would raise where the specification says do nothing, and
/// the piece that branches to avoid it is exactly what `CleanupDone` allows.
#[test]
fn the_close_asks_whether_return_exists_before_calling_it() {
    let lowered = only("function f(xs, o) { for (const x of xs) { o.m(x); } }").expect("covered");
    let ops: Vec<_> = lowered
        .func
        .insts
        .iter()
        .filter_map(|held| match &held.op {
            rts_mir::Op::Prim { prim, .. } => lowered.domain.meaning(*prim),
            _ => None,
        })
        .collect();
    assert!(
        ops.contains(&crate::domain::JsPrim::IsNullish),
        "the close is guarded by a nullish test"
    );
}

/// `for`-`in` is not this protocol and shares nothing with it: it walks enumerable string
/// keys INCLUDING inherited ones, which is a walk of the prototype chain.
#[test]
fn for_in_is_refused_because_it_is_not_the_protocol() {
    let refused = only("function f(o) { for (const k in o) { o.m(k); } }").expect_err("for-in");
    assert_eq!(
        refused,
        Unsupported::Statement(
            "for-in walks the prototype chain, which is not the iteration protocol"
        )
    );
}

/// `for await` suspends INSIDE the region that owes the close, and the machine's frame
/// transform has a measured bug class exactly there — a `Return` left inside a region ran
/// its `finally` once per yield.
#[test]
fn for_await_is_refused_because_it_suspends_inside_the_region() {
    let refused = only("async function f(xs, o) { for await (const x of xs) { o.m(x); } }")
        .expect_err("for await");
    assert_eq!(
        refused,
        Unsupported::Statement("for await suspends inside the region that owes the close")
    );
}

/// A `try` with a `finally` that completes NORMALLY runs the `finally` on the way out.
/// The machine runs a cleanup where a region is left by a return or a throw, and falling
/// off the end of the body is neither -- so a copy runs on that path, outside every
/// region. Without it, `try { print("c") } finally { print("f") }` printed only `c`.
#[test]
fn a_finally_runs_on_the_ordinary_way_out_too() {
    let lowered =
        only("function f(o) { try { o.m(); } finally { o.n(); } return 1; }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    // `o.n()` is written once and lowered twice: the cleanup piece and the ordinary copy.
    let calls = lowered
        .func
        .insts
        .iter()
        .filter(|held| matches!(held.op, rts_mir::Op::Call { .. }))
        .count();
    assert_eq!(
        calls, 3,
        "o.m(), and o.n() in the cleanup and on the ordinary path"
    );
}

/// A `break` out of a `try` with a `finally` is a jump out of the region, where the
/// machine runs no cleanup -- so it is refused rather than skipping the `finally`.
#[test]
fn a_break_out_of_a_try_with_a_finally_is_refused() {
    let refused = only("function f(o) { while (o) { try { break; } finally { o.n(); } } }")
        .expect_err("the finally would be skipped");
    assert_eq!(
        refused,
        Unsupported::Statement("a break or continue inside a try with a finally skips the cleanup")
    );
}

/// `return c ? a : b` is two returns, which is what makes a call in either arm a TAIL
/// call -- lowered through a join, `sumAcc(n - 1, acc + n)` answered into a block
/// parameter first, and a recursion a million deep overflowed the stack.
#[test]
fn a_conditional_return_is_two_returns() {
    let lowered = only("function f(c, g) { return c ? g(1) : g(2); }").expect("covered");
    assert_eq!(verify(&lowered.func), Ok(()));
    let tails = crate::machine::tail_positions(&lowered.func);
    assert_eq!(tails.len(), 2, "each arm's call is in tail position");
}
