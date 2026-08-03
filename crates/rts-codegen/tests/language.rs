//! What the tree is required to be able to say.
//!
//! Each test names a fact about JavaScript or TypeScript and shows the tree
//! holding it. They are not tests of the structs — the structs are data and
//! testing data against itself proves nothing. They are tests that a distinction
//! the language makes survives being written down, because the failure this
//! crate is most exposed to is a tree that quietly cannot express something and
//! is only found out by a program that needed it.

use rts_codegen::names::Names;
use rts_codegen::syntax::Claim;
use rts_codegen::syntax::{ArrayPattern, Element, ObjectPattern, PatternProperty};
use rts_codegen::syntax::{
    AssignOp, AssignTarget, BinaryOp, Binding, BindingKind, Catch, Expr, ExprKind, Function,
    Literal, LogicalOp, Parameter, Pattern, Program, PropertyKey, Stmt, StmtKind, UpdateOp,
    UpdatePosition,
};
use rts_codegen::values::Singleton;
use rts_cranelift::fault::Position;

fn at() -> Position {
    Position::UNKNOWN
}

fn number(value: f64) -> Expr {
    Expr::new(ExprKind::Literal(Literal::Number(value)), at())
}

fn ident(names: &mut Names, text: &str) -> Expr {
    Expr::new(ExprKind::Ident(names.intern(text)), at())
}

#[test]
fn a_hole_in_an_array_is_not_an_undefined_in_one() {
    let hole = ExprKind::Array {
        elements: vec![Some(number(1.0)), None],
    };
    let explicit = ExprKind::Array {
        elements: vec![
            Some(number(1.0)),
            Some(Expr::new(
                ExprKind::Literal(Literal::Singleton(Singleton::Undefined)),
                at(),
            )),
        ],
    };

    assert_ne!(
        hole, explicit,
        "[1,] and [1, undefined] differ under every operation that skips holes"
    );
}

#[test]
fn compound_assignment_is_not_rewritten_to_an_assignment_of_a_sum() {
    let mut names = Names::new();
    let target = ident(&mut names, "a");

    let compound = ExprKind::Assign {
        target: AssignTarget::Place(Box::new(target.clone())),
        value: Box::new(number(1.0)),
        op: AssignOp::Compound(BinaryOp::Add),
    };
    let expanded = ExprKind::Assign {
        target: AssignTarget::Place(Box::new(target.clone())),
        value: Box::new(Expr::new(
            ExprKind::Binary {
                op: BinaryOp::Add,
                left: Box::new(target),
                right: Box::new(number(1.0)),
            },
            at(),
        )),
        op: AssignOp::Plain,
    };

    assert_ne!(
        compound, expanded,
        "a += b evaluates the target once; the rewrite evaluates it twice"
    );
}

#[test]
fn an_update_is_not_an_assignment_of_a_sum() {
    let mut names = Names::new();
    let target = ident(&mut names, "x");

    let postfix = ExprKind::Update {
        op: UpdateOp::Increment,
        position: UpdatePosition::Postfix,
        target: Box::new(target.clone()),
    };
    let prefix = ExprKind::Update {
        op: UpdateOp::Increment,
        position: UpdatePosition::Prefix,
        target: Box::new(target.clone()),
    };
    let compound = ExprKind::Assign {
        target: AssignTarget::Place(Box::new(target)),
        value: Box::new(number(1.0)),
        op: AssignOp::Compound(BinaryOp::Add),
    };

    assert_ne!(
        postfix, prefix,
        "one yields the old value and the other the new; the side is not cosmetic"
    );
    assert_ne!(
        postfix, compound,
        "x++ yields the coerced old value, x += 1 yields the new one"
    );
}

#[test]
fn a_logical_assignment_is_a_different_operator_from_a_compound_one() {
    let or_assign = AssignOp::Logical(LogicalOp::Or);
    let plus_assign = AssignOp::Compound(BinaryOp::Add);

    assert_ne!(or_assign, plus_assign);
    assert!(
        !or_assign.always_assigns(),
        "obj.x ||= f() performs no [[Set]] when obj.x is truthy, so a setter does not run"
    );
    assert!(plus_assign.always_assigns());
}

#[test]
fn only_plain_assignment_accepts_a_destructuring_target() {
    assert!(AssignOp::Plain.allows_pattern_target());
    assert!(!AssignOp::Compound(BinaryOp::Add).allows_pattern_target());
    assert!(!AssignOp::Logical(LogicalOp::Or).allows_pattern_target());
}

#[test]
fn comparing_operators_have_no_compound_spelling() {
    for op in [
        BinaryOp::Add,
        BinaryOp::Exponent,
        BinaryOp::UShr,
        BinaryOp::BitXor,
    ] {
        assert!(
            AssignOp::compound(op).is_some(),
            "{op:?} has a compound form"
        );
    }
    for op in [
        BinaryOp::StrictEqual,
        BinaryOp::LooseEqual,
        BinaryOp::Less,
        BinaryOp::In,
        BinaryOp::InstanceOf,
    ] {
        assert!(AssignOp::compound(op).is_none(), "{op:?} has none");
    }
}

#[test]
fn unsigned_right_shift_widens_where_every_other_bitwise_operator_narrows() {
    assert!(!BinaryOp::UShr.bitwise_result_fits_i32());
    assert!(BinaryOp::Shr.bitwise_result_fits_i32());
    assert!(BinaryOp::BitOr.bitwise_result_fits_i32());
}

#[test]
fn a_sequence_holds_its_operands_flat() {
    let mut names = Names::new();
    let seq = ExprKind::Sequence {
        operands: vec![ident(&mut names, "a"), ident(&mut names, "b"), number(3.0)],
    };

    match seq {
        ExprKind::Sequence { operands } => assert_eq!(operands.len(), 3),
        _ => panic!("built a Sequence and got something else"),
    }
}

#[test]
fn a_destructuring_assignment_can_write_where_a_declaration_cannot() {
    let mut names = Names::new();
    let obj = Expr::new(ExprKind::Ident(names.intern("obj")), at());
    let member = Expr::new(
        ExprKind::Member {
            object: Box::new(obj),
            property: names.intern("x"),
            optional: false,
        },
        at(),
    );

    let writes_to_a_member = Pattern::Object(ObjectPattern {
        properties: vec![PatternProperty {
            key: PropertyKey::Named(names.intern("a")),
            value: Element::new(Pattern::Target(Box::new(member))),
        }],
        rest: None,
    });

    assert!(
        !writes_to_a_member.is_valid_binding(),
        "({{ a: obj.x }} = src) is legal; `let {{ a: obj.x }}` is not"
    );
    assert!(AssignTarget::Pattern(writes_to_a_member.clone()).is_legal_under(AssignOp::Plain));
    assert!(
        !AssignTarget::Pattern(writes_to_a_member)
            .is_legal_under(AssignOp::Compound(BinaryOp::Add)),
        "[a, b] += c is a syntax error"
    );
}

#[test]
fn a_rest_element_cannot_carry_a_default_because_the_type_has_nowhere_to_put_one() {
    let mut names = Names::new();
    let pattern = ArrayPattern {
        elements: vec![Some(Element::with_default(
            Pattern::Name(names.intern("a")),
            number(1.0),
        ))],
        rest: Some(Box::new(Pattern::Name(names.intern("rest")))),
    };

    // `rest` is a bare Pattern, not an Element: `[...rest = []]` is an early
    // error and is unrepresentable here rather than rejected later.
    assert!(pattern.elements[0].as_ref().unwrap().default.is_some());
    assert!(pattern.rest.is_some());
}

#[test]
fn an_array_pattern_iterates_and_an_object_pattern_does_not() {
    let mut names = Names::new();

    let array = Pattern::Array(ArrayPattern {
        elements: vec![Some(Element::new(Pattern::Name(names.intern("a"))))],
        rest: None,
    });
    let object = Pattern::Object(ObjectPattern {
        properties: vec![PatternProperty {
            key: PropertyKey::Named(names.intern("a")),
            value: Element::new(Pattern::Name(names.intern("a"))),
        }],
        rest: None,
    });

    assert!(
        array.iterates(),
        "const [a] = new Set([1]) works, and owes an IteratorClose"
    );
    assert!(!object.iterates(), "properties are read, not stepped");
}

#[test]
fn a_parameter_list_stops_being_simple_the_moment_anything_is_added() {
    let mut names = Names::new();
    let plain = Function {
        name: None,
        parameters: vec![Parameter {
            target: Pattern::Name(names.intern("a")),
            default: None,
            claim: None,
        }],
        rest_parameter: None,
        body: vec![],
        returns: None,
        captures_this: false,
        is_async: false,
        is_generator: false,
        at: at(),
    };
    assert!(plain.has_simple_parameter_list());

    let with_default = Function {
        parameters: vec![Parameter {
            target: Pattern::Name(names.intern("a")),
            default: Some(number(1.0)),
            claim: None,
        }],
        ..plain.clone()
    };
    assert!(
        !with_default.has_simple_parameter_list(),
        "a default forbids a \"use strict\" directive in the body"
    );

    let with_rest = Function {
        rest_parameter: Some(Pattern::Name(names.intern("rest"))),
        ..plain.clone()
    };
    assert!(!with_rest.has_simple_parameter_list());

    let with_pattern = Function {
        parameters: vec![Parameter {
            target: Pattern::Object(ObjectPattern::default()),
            default: None,
            claim: None,
        }],
        ..plain
    };
    assert!(!with_pattern.has_simple_parameter_list());
}

#[test]
fn a_named_property_and_a_computed_one_are_different_nodes() {
    let mut names = Names::new();
    let object = Box::new(ident(&mut names, "o"));
    let key = names.intern("length");

    let named = ExprKind::Member {
        object: object.clone(),
        property: key,
        optional: false,
    };
    let computed = ExprKind::Index {
        object,
        index: Box::new(Expr::new(
            ExprKind::Literal(Literal::String("length".into())),
            at(),
        )),
        optional: false,
    };

    assert_ne!(
        named, computed,
        "one has a key now and the other has an expression to evaluate first"
    );
}

#[test]
fn coalesce_is_not_or_with_a_different_threshold() {
    assert_ne!(LogicalOp::Coalesce, LogicalOp::Or);
    assert!(
        !BinaryOp::StrictEqual.converts(),
        "the distinction === exists for"
    );
    assert!(BinaryOp::LooseEqual.converts());
}

#[test]
fn construction_is_not_a_call_with_a_flag() {
    let mut names = Names::new();
    let callee = Box::new(ident(&mut names, "Thing"));

    let call = ExprKind::Call {
        callee: callee.clone(),
        arguments: vec![],
        optional: false,
    };
    let construct = ExprKind::New {
        callee,
        arguments: vec![],
    };

    assert_ne!(call, construct);
}

#[test]
fn an_assertion_is_kept_rather_than_applied() {
    let mut names = Names::new();
    let value = ident(&mut names, "input");

    let asserted = Expr::new(
        ExprKind::Asserted {
            value: Box::new(value.clone()),
            claim: Claim::Number,
        },
        at(),
    );

    assert_ne!(
        asserted.kind, value.kind,
        "the program said so and nothing checked; erasing that erases the guard"
    );
    assert!(Claim::Number.is_definite());
    assert!(!Claim::Unknown.is_definite());
}

#[test]
fn a_program_can_hold_a_function_that_returns_nothing_and_one_that_returns_undefined() {
    let bare = Stmt::new(StmtKind::Return(None), at());
    let explicit = Stmt::new(
        StmtKind::Return(Some(Expr::new(
            ExprKind::Literal(Literal::Singleton(Singleton::Undefined)),
            at(),
        ))),
        at(),
    );

    assert_ne!(
        bare, explicit,
        "they produce the same value; which one was written is still a fact the lowering owns"
    );
}

#[test]
fn a_catch_without_a_binding_is_expressible() {
    let handler = Catch {
        binding: None,
        body: vec![],
    };
    let statement = Stmt::new(
        StmtKind::Try {
            body: vec![],
            catch: Some(handler),
            finally: None,
        },
        at(),
    );

    match statement.kind {
        StmtKind::Try { catch: Some(c), .. } => assert!(c.binding.is_none()),
        _ => panic!("built a Try and got something else"),
    }
}

#[test]
fn an_arrow_and_a_function_differ_in_one_recorded_fact() {
    let mut names = Names::new();
    let name = names.intern("f");

    let ordinary = Function {
        name: Some(name),
        parameters: vec![],
        rest_parameter: None,
        body: vec![],
        returns: None,
        captures_this: false,
        is_async: false,
        is_generator: false,
        at: at(),
    };
    let arrow = Function {
        captures_this: true,
        ..ordinary.clone()
    };

    assert_ne!(ordinary, arrow);
}

#[test]
fn a_whole_small_program_is_expressible() {
    let mut names = Names::new();
    let counter = names.intern("counter");
    let limit = names.intern("limit");

    let program = Program {
        body: vec![
            Stmt::new(
                StmtKind::Declare {
                    kind: BindingKind::Let,
                    bindings: vec![Binding {
                        target: Pattern::Name(counter),
                        value: Some(number(0.0)),
                        claim: Some(Claim::Number),
                    }],
                },
                at(),
            ),
            Stmt::new(
                StmtKind::While {
                    condition: Expr::new(
                        ExprKind::Binary {
                            op: BinaryOp::Less,
                            left: Box::new(Expr::new(ExprKind::Ident(counter), at())),
                            right: Box::new(Expr::new(ExprKind::Ident(limit), at())),
                        },
                        at(),
                    ),
                    body: Box::new(Stmt::new(
                        StmtKind::Expr(Expr::new(
                            ExprKind::Assign {
                                target: AssignTarget::Place(Box::new(Expr::new(
                                    ExprKind::Ident(counter),
                                    at(),
                                ))),
                                value: Box::new(number(1.0)),
                                op: AssignOp::Compound(BinaryOp::Add),
                            },
                            at(),
                        )),
                        at(),
                    )),
                },
                at(),
            ),
            Stmt::new(
                StmtKind::Function(Box::new(Function {
                    name: Some(names.intern("total")),
                    parameters: vec![Parameter {
                        target: Pattern::Name(names.intern("extra")),
                        default: Some(number(0.0)),
                        claim: Some(Claim::Number),
                    }],
                    rest_parameter: None,
                    body: vec![Stmt::new(
                        StmtKind::Return(Some(Expr::new(ExprKind::Ident(counter), at()))),
                        at(),
                    )],
                    returns: Some(Claim::Number),
                    captures_this: false,
                    is_async: false,
                    is_generator: false,
                    at: at(),
                })),
                at(),
            ),
        ],
    };

    assert_eq!(program.body.len(), 3);
    assert_eq!(names.text(counter), "counter");
}

#[test]
fn an_object_literal_keeps_the_order_it_was_written_in() {
    let mut names = Names::new();
    let first = names.intern("a");
    let second = names.intern("b");

    let literal = ExprKind::Object {
        properties: vec![
            rts_codegen::syntax::Property {
                key: PropertyKey::Named(first),
                value: number(1.0),
            },
            rts_codegen::syntax::Property {
                key: PropertyKey::Named(second),
                value: number(2.0),
            },
        ],
    };

    let reversed = ExprKind::Object {
        properties: vec![
            rts_codegen::syntax::Property {
                key: PropertyKey::Named(second),
                value: number(2.0),
            },
            rts_codegen::syntax::Property {
                key: PropertyKey::Named(first),
                value: number(1.0),
            },
        ],
    };

    assert_ne!(
        literal, reversed,
        "the order keys are added is what decides the layout, so it is not incidental"
    );
}
