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
use rts_codegen::syntax::{
    BinaryOp, Binding, BindingKind, Catch, Expr, ExprKind, Function, Literal, LogicalOp, Parameter,
    Program, PropertyKey, Stmt, StmtKind,
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
        target: Box::new(target.clone()),
        value: Box::new(number(1.0)),
        op: Some(BinaryOp::Add),
    };
    let expanded = ExprKind::Assign {
        target: Box::new(target.clone()),
        value: Box::new(Expr::new(
            ExprKind::Binary {
                op: BinaryOp::Add,
                left: Box::new(target),
                right: Box::new(number(1.0)),
            },
            at(),
        )),
        op: None,
    };

    assert_ne!(
        compound, expanded,
        "a += b evaluates the target once; the rewrite evaluates it twice"
    );
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
                        name: counter,
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
                                target: Box::new(Expr::new(ExprKind::Ident(counter), at())),
                                value: Box::new(number(1.0)),
                                op: Some(BinaryOp::Add),
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
                        name: names.intern("extra"),
                        default: Some(number(0.0)),
                        rest: false,
                        claim: Some(Claim::Number),
                    }],
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
