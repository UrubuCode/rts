//! Source text in, tree out.
//!
//! These are the first tests in this crate that read a *program* rather than
//! building a tree by hand. That is the point: the hand-built tests in
//! `language.rs` prove the tree can express a distinction, and these prove the
//! bridge actually produces the side of it the program was written on. A tree
//! that can say two things apart is worth nothing if everything arrives as the
//! same one.

use rts_codegen::names::Names;
use rts_codegen::parse::{ParseError, parse_module, parse_script};
use rts_codegen::syntax::{
    AssignOp, AssignTarget, BinaryOp, BindingKind, ClassElement, ClassKey, Expr, ExprKind,
    ForEachSource, ForEachTarget, FunctionBody, Goal, Literal, LogicalOp, ModuleItem, Pattern,
    Program, Property, PropertyKey, Stmt, StmtKind, UpdatePosition,
};

fn module(source: &str) -> (Program, Names) {
    let mut names = Names::new();
    let program = parse_module(source, &mut names)
        .unwrap_or_else(|error| panic!("{source:?} did not parse: {error}"));
    (program, names)
}

/// The statements of a program, for the many tests that want one.
fn statements(program: &Program) -> Vec<&Stmt> {
    program
        .body
        .iter()
        .filter_map(|item| match item {
            ModuleItem::Stmt(statement) => Some(statement),
            _ => None,
        })
        .collect()
}

fn only_statement(source: &str) -> (StmtKind, Names) {
    let (program, names) = module(source);
    let list = statements(&program);
    assert_eq!(
        list.len(),
        1,
        "{source:?} produced {} statements",
        list.len()
    );
    (list[0].kind.clone(), names)
}

fn only_expr(source: &str) -> (ExprKind, Names) {
    let (kind, names) = only_statement(source);
    match kind {
        StmtKind::Expr(expression) => (expression.kind, names),
        other => panic!("expected an expression statement, got {other:?}"),
    }
}

#[test]
fn every_operator_survives_the_trip() {
    for (source, expected) in [
        ("a ** b;", BinaryOp::Exponent),
        ("a << b;", BinaryOp::Shl),
        ("a >> b;", BinaryOp::Shr),
        ("a >>> b;", BinaryOp::UShr),
        ("a & b;", BinaryOp::BitAnd),
        ("a | b;", BinaryOp::BitOr),
        ("a ^ b;", BinaryOp::BitXor),
        ("a in b;", BinaryOp::In),
        ("a instanceof b;", BinaryOp::InstanceOf),
        ("a === b;", BinaryOp::StrictEqual),
        ("a <= b;", BinaryOp::LessEqual),
    ] {
        let (kind, _) = only_expr(source);
        match kind {
            ExprKind::Binary { op, .. } => assert_eq!(op, expected, "{source}"),
            other => panic!("{source} became {other:?}"),
        }
    }
}

#[test]
fn short_circuiting_operators_do_not_arrive_as_binary_ones() {
    for (source, expected) in [
        ("a && b;", LogicalOp::And),
        ("a || b;", LogicalOp::Or),
        ("a ?? b;", LogicalOp::Coalesce),
    ] {
        let (kind, _) = only_expr(source);
        match kind {
            ExprKind::Logical { op, .. } => assert_eq!(op, expected, "{source}"),
            other => panic!("{source} became {other:?} — it evaluates one side, not two"),
        }
    }
}

#[test]
fn a_logical_assignment_arrives_as_one() {
    let (kind, _) = only_expr("a ||= b;");
    match kind {
        ExprKind::Assign { op, .. } => {
            assert_eq!(op, AssignOp::Logical(LogicalOp::Or));
            assert!(!op.always_assigns());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_compound_assignment_is_not_expanded_on_the_way_in() {
    let (kind, _) = only_expr("a += 1;");
    match kind {
        ExprKind::Assign { op, value, .. } => {
            assert_eq!(op, AssignOp::Compound(BinaryOp::Add));
            assert!(
                matches!(value.kind, ExprKind::Literal(_)),
                "the right side is `1`, not `a + 1` — the target is read once"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn both_sides_of_an_update_arrive_distinguishable() {
    let (prefix, _) = only_expr("++x;");
    let (postfix, _) = only_expr("x++;");

    match (prefix, postfix) {
        (
            ExprKind::Update {
                position: first, ..
            },
            ExprKind::Update {
                position: second, ..
            },
        ) => {
            assert_eq!(first, UpdatePosition::Prefix);
            assert_eq!(second, UpdatePosition::Postfix);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_hole_arrives_as_a_hole_and_a_spread_as_a_spread() {
    let (kind, _) = only_expr("[1, , ...rest];");
    match kind {
        ExprKind::Array { elements } => {
            assert_eq!(elements.len(), 3);
            assert!(elements[0].is_some());
            assert!(elements[1].is_none(), "the hole is not an undefined");
            assert!(
                !elements[2].as_ref().unwrap().count_is_static(),
                "and the spread makes the length a runtime value"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn proto_arrives_as_a_prototype_only_in_the_one_spelling_that_sets_it() {
    let (sets, _) = only_expr("({ __proto__: p });");
    match sets {
        ExprKind::Object { properties } => assert!(
            matches!(properties[0], Property::Prototype(_)),
            "`__proto__: v` sets the prototype"
        ),
        other => panic!("{other:?}"),
    }

    let (ordinary, _) = only_expr("({ [\"__proto__\"]: p });");
    match ordinary {
        ExprKind::Object { properties } => assert!(
            matches!(properties[0], Property::Value { .. }),
            "the computed spelling is a property named __proto__"
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_method_arrives_as_a_method_and_an_accessor_as_an_accessor() {
    let (kind, _) = only_expr("({ m() {}, get g() { return 1; }, set s(v) {} });");
    match kind {
        ExprKind::Object { properties } => {
            assert!(matches!(properties[0], Property::Method { .. }));
            assert!(matches!(properties[1], Property::Getter { .. }));
            assert!(matches!(properties[2], Property::Setter { .. }));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_destructuring_declaration_arrives_as_a_pattern_that_can_be_a_binding() {
    let (kind, _) = only_statement("const [a, , b = 1, ...rest] = xs;");
    match kind {
        StmtKind::Declare { kind, bindings } => {
            assert_eq!(kind, BindingKind::Const);
            let pattern = &bindings[0].target;
            assert!(pattern.is_valid_binding());
            assert!(pattern.iterates(), "an array pattern owes an IteratorClose");

            let mut bound = Vec::new();
            pattern.bound_names(&mut bound);
            assert_eq!(bound.len(), 3, "a, b and rest — the hole binds nothing");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_destructuring_assignment_into_a_member_arrives_as_a_place() {
    let (kind, _) = only_expr("({ a: obj.x } = src);");
    match kind {
        ExprKind::Assign { target, op, .. } => {
            assert_eq!(op, AssignOp::Plain);
            match target {
                AssignTarget::Pattern(pattern) => assert!(
                    !pattern.is_valid_binding(),
                    "it writes into obj.x, which no declaration may do"
                ),
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_rest_parameter_arrives_in_its_own_field() {
    let (kind, _) = only_statement("function f(a, b = 1, ...rest) {}");
    match kind {
        StmtKind::Function(function) => {
            assert_eq!(function.parameters.len(), 2);
            assert!(function.rest_parameter.is_some());
            assert!(
                !function.has_simple_parameter_list(),
                "a default and a rest both make it non-simple"
            );
            assert_eq!(function.parameter_names().len(), 3);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_arrow_records_where_this_comes_from_and_a_function_does_not() {
    let (arrow, _) = only_statement("const f = x => x;");
    match arrow {
        StmtKind::Declare { bindings, .. } => match &bindings[0].value.as_ref().unwrap().kind {
            ExprKind::Function(function) => {
                assert!(function.captures_this);
                assert!(
                    function.body.returns_implicitly(),
                    "a concise body returns without a return being written"
                );
            }
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }

    let (ordinary, _) = only_statement("function f(x) { return x; }");
    match ordinary {
        StmtKind::Function(function) => {
            assert!(!function.captures_this);
            assert!(matches!(function.body, FunctionBody::Block(_)));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_three_for_each_loops_arrive_as_three_sources() {
    for (source, expected) in [
        ("for (const k in o) {}", ForEachSource::In),
        ("for (const v of xs) {}", ForEachSource::Of),
        (
            "async function f() { for await (const v of xs) {} }",
            ForEachSource::AwaitOf,
        ),
    ] {
        let (program, _) = module(source);
        let found = find_for_each(&program).unwrap_or_else(|| panic!("{source} had no loop"));
        assert_eq!(found, expected, "{source}");
    }
}

fn find_for_each(program: &Program) -> Option<ForEachSource> {
    fn in_stmt(statement: &Stmt) -> Option<ForEachSource> {
        match &statement.kind {
            StmtKind::ForEach { source, .. } => Some(*source),
            StmtKind::Function(function) => match &function.body {
                FunctionBody::Block(body) => body.iter().find_map(in_stmt),
                FunctionBody::Expression(_) => None,
            },
            StmtKind::Block(body) => body.iter().find_map(in_stmt),
            _ => None,
        }
    }
    program.body.iter().find_map(|item| match item {
        ModuleItem::Stmt(statement) => in_stmt(statement),
        _ => None,
    })
}

#[test]
fn a_switch_keeps_default_where_it_was_written() {
    let (kind, _) = only_statement("switch (x) { case 1: break; default: break; case 2: break; }");
    match kind {
        StmtKind::Switch { clauses, .. } => {
            assert_eq!(clauses.len(), 3);
            assert!(clauses[0].test.is_some());
            assert!(
                clauses[1].test.is_none(),
                "default sits where it was written"
            );
            assert!(clauses[2].test.is_some());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_labelled_break_keeps_its_label() {
    let (kind, names) = only_statement("outer: for (;;) { break outer; }");
    match kind {
        StmtKind::Labelled { label, body } => {
            assert_eq!(names.text(label), "outer");
            match &body.kind {
                StmtKind::For { body, .. } => match &body.kind {
                    StmtKind::Block(inner) => {
                        assert!(matches!(inner[0].kind, StmtKind::Break(Some(_))));
                    }
                    other => panic!("{other:?}"),
                },
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_class_arrives_with_its_members_in_order_and_privacy_intact() {
    let (kind, names) = only_statement(
        "class C extends Base {
             #secret = 1;
             static shared = 2;
             static { }
             constructor() { super(); }
             get value() { return this.#secret; }
         }",
    );

    match kind {
        StmtKind::Class(class) => {
            assert!(class.is_derived());
            assert_eq!(class.body.len(), 5);

            assert!(
                matches!(&class.body[0], ClassElement::Field(f) if f.key.is_private()),
                "#secret is a private name, not a property called \"#secret\""
            );
            assert_eq!(
                class.static_elements().count(),
                2,
                "the static field and the static block"
            );
            assert_eq!(class.instance_elements().count(), 1);

            let constructor = class.constructor(&names).expect("declared one");
            assert!(!constructor.is_static);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn super_arrives_as_super_and_not_as_a_call_of_something_named_super() {
    let (kind, _) = only_statement("class C extends B { constructor() { super(1); } }");
    match kind {
        StmtKind::Class(class) => {
            let element = &class.body[0];
            let ClassElement::Method(method) = element else {
                panic!("{element:?}")
            };
            let FunctionBody::Block(body) = &method.function.body else {
                panic!("expected a block")
            };
            match &body[0].kind {
                StmtKind::Expr(expression) => assert!(
                    matches!(expression.kind, ExprKind::SuperCall { .. }),
                    "super() binds `this`; it is not a call"
                ),
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_optional_chain_arrives_wrapped_in_its_boundary() {
    let (kind, _) = only_expr("a?.b.c;");
    match kind {
        ExprKind::Chain(inner) => match &inner.kind {
            // The outer link is `.c`, which carries no flag of its own and is
            // still inside the boundary.
            ExprKind::Member { optional, .. } => assert!(!optional),
            other => panic!("{other:?}"),
        },
        other => {
            panic!("{other:?} — without the boundary there is nowhere to say how far it reaches")
        }
    }
}

#[test]
fn a_template_keeps_raw_and_cooked() {
    let (kind, _) = only_expr("`a${x}b`;");
    match kind {
        ExprKind::Template { parts, expressions } => {
            assert_eq!(parts.len(), 2);
            assert_eq!(expressions.len(), 1);
            assert_eq!(parts[0].raw, "a");
            // `Text` guarda UNIDADES UTF-16, não uma `String`: um template
            // pode conter metade de um par surrogado, e uma `String` de Rust não
            // representa isso. `as_rust` é a conversão que falha nesse caso — é
            // por isso que devolve `Option`, e por isso que o teste passa por
            // ela em vez de tratar o `Text` como texto de Rust.
            assert_eq!(parts[0].cooked.as_ref().and_then(|t| t.as_rust()).as_deref(), Some("a"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn every_import_shape_arrives() {
    let (program, names) = module(
        "import d from \"m\";
         import * as ns from \"m\";
         import { a, b as c } from \"m\";
         import \"side-effect\";
         import data from \"d.json\" with { type: \"json\" };",
    );

    let imports: Vec<_> = program.imports().collect();
    assert_eq!(imports.len(), 5);

    assert!(imports[3].is_side_effect_only());
    assert_eq!(
        imports[4].attributes[0].value, "json",
        "attributes are part of the request"
    );
    assert_eq!(imports[2].bindings.len(), 2);

    // `b as c` binds `c` here and names `b` over there.
    match &imports[2].bindings[1] {
        rts_codegen::syntax::ImportBinding::Named { exported, local } => {
            assert_eq!(exported, "b");
            assert_eq!(names.text(*local), "c");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_re_export_carries_its_source_and_a_local_export_does_not() {
    let (program, _) = module("export { x } from \"m\"; const y = 1; export { y };");
    let exports: Vec<_> = program.exports().collect();
    assert_eq!(exports.len(), 2);

    match (&exports[0].kind, &exports[1].kind) {
        (
            rts_codegen::syntax::ExportKind::Named { source: from, .. },
            rts_codegen::syntax::ExportKind::Named { source: none, .. },
        ) => {
            assert_eq!(from.as_deref(), Some("m"));
            assert!(
                none.is_none(),
                "without a source the name is one this module holds"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_directive_is_read_from_raw_text() {
    let mut names = Names::new();
    let strict = parse_script("\"use strict\"; x;", &mut names).unwrap();
    assert!(strict.directives[0].is_use_strict());

    let escaped = parse_script("\"use\\u0020strict\"; x;", &mut names).unwrap();
    assert!(
        !escaped.directives.is_empty(),
        "it is still a string statement in the prologue position"
    );
    assert!(
        !escaped.directives[0].is_use_strict(),
        "but the escaped spelling is not the directive"
    );
}

#[test]
fn the_goal_is_what_the_caller_asked_for() {
    let mut names = Names::new();
    let as_script = parse_script("x;", &mut names).unwrap();
    let as_module = parse_module("x;", &mut names).unwrap();

    assert_eq!(as_script.goal, Goal::Script);
    assert_eq!(as_module.goal, Goal::Module);
    assert!(as_module.goal.allows_top_level_await());
    assert!(
        !as_module.requires_module_goal(),
        "nothing in it needs the goal, and it is a module anyway"
    );
}

#[test]
fn a_typescript_annotation_arrives_as_a_claim() {
    let (kind, _) = only_statement("const x: number = f();");
    match kind {
        StmtKind::Declare { bindings, .. } => {
            let claim = bindings[0].claim.as_ref().expect("annotated");
            assert!(claim.is_definite());
        }
        other => panic!("{other:?}"),
    }

    let (unknown, _) = only_statement("const x: unknown = f();");
    match unknown {
        StmtKind::Declare { bindings, .. } => {
            let claim = bindings[0].claim.as_ref().expect("annotated");
            assert!(
                !claim.is_definite(),
                "a claim that proves nothing must not read as one that does"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_cast_is_kept_rather_than_applied() {
    let (kind, _) = only_expr("(x as number);");
    assert!(
        matches!(kind, ExprKind::Asserted { .. }),
        "the program said so and nothing checked; erasing it erases the guard"
    );
}

#[test]
fn an_interface_is_erased_and_an_enum_becomes_the_object_it_means() {
    let mut names = Names::new();

    let erased = parse_module("interface I { a: number } const x = 1;", &mut names).unwrap();
    assert_eq!(
        statements(&erased).len(),
        2,
        "the interface is present as an empty statement — erased, not dropped silently"
    );

    // An enum emits code, so it was refused rather than approximated. It is not
    // approximated now either: every rule about it — the auto-increment, which
    // members get a reverse mapping — is settled before anything runs, so it is
    // the object TypeScript's own emitter makes, built here.
    let lowered = parse_module("enum E { A }", &mut names).expect("an enum is a declaration");
    assert_eq!(statements(&lowered).len(), 1);

    // A NAMESPACE also emits code now: an object built by an IIFE, matching
    // TypeScript's own emit for it. `var N;` plus the call is two statements,
    // held under one `Block` so the parser hands back exactly one `Stmt` per
    // declaration, the same shape every other declaration arrives in.
    let lowered = parse_module("namespace N { export const a = 1; }", &mut names)
        .expect("a namespace lowers to an IIFE building an object");
    assert_eq!(statements(&lowered).len(), 1);
}

#[test]
fn an_unsupported_construct_is_named_rather_than_dropped() {
    // This test has now outlived two constructs. It named a regular expression
    // until the tree gained somewhere to put one, then `using` in a for-head
    // until the same thing happened again, then an `enum` until that became the
    // object it means. Each time the refusal was a missing capability wearing
    // the clothes of a language limit, and each time the fix moved the gap to
    // where it actually is.
    //
    // It follows the refusal rather than being deleted with whatever construct
    // it happened to name, because what it pins is the *shape* of a refusal:
    // named, with a position, and distinguishable from a syntax error.
    //
    // Now pinned to a dotted namespace name (`namespace A.B { … }`): a plain
    // `namespace N { … }` lowers (see the test above), but a dotted one is a
    // namespace nested inside another with nothing this bridge builds for it
    // yet, and refusing it is still the honest answer.
    let mut names = Names::new();
    match parse_module("namespace A.B { export const a = 1; }", &mut names) {
        Err(ParseError::Unsupported { construct, .. }) => {
            assert!(construct.contains("namespace"), "{construct}");
        }
        other => panic!("expected a named refusal, got {other:?}"),
    }
}

#[test]
fn using_in_a_for_head_arrives_as_the_disposal_it_is() {
    // Its own case rather than a `BindingKind`, because a binding kind answers
    // "what scope, and can it be assigned again" — and `using` changes neither.
    // What it adds is an obligation on the way out, which no binding kind can
    // carry and every place that reads one would have to learn to ignore.
    let (kind, _) = only_statement("for (using x of xs) {}");
    let StmtKind::ForEach { target, .. } = kind else {
        panic!("expected a for-each");
    };
    let ForEachTarget::Dispose { is_async, .. } = target else {
        panic!("expected a disposal target: {target:?}");
    };
    assert!(!is_async);
}

#[test]
fn a_regular_expression_arrives_as_its_pattern_and_its_flags() {
    // Held as the text that was written, because the grammar of a regular
    // expression is its own and a literal and a `new RegExp(s)` should reach
    // the engine by one path.
    let (kind, _) = only_statement("const r = /ab+/gi;");
    let StmtKind::Declare { bindings, .. } = kind else {
        panic!("expected a declaration");
    };
    let Some(ExprKind::Literal(Literal::Regex { pattern, flags })) =
        bindings[0].value.as_ref().map(|value| &value.kind)
    else {
        panic!("expected a regular expression literal");
    };
    assert_eq!(pattern, "ab+");
    assert_eq!(flags, "gi");
}

#[test]
fn a_bigint_arrives_as_its_digits() {
    // Not as a number: a `BigInt` is exactly what a double cannot hold, so
    // parsing it into one here would lose the literal in the act of recording
    // it.
    let (kind, _) = only_statement("const n = 9007199254740993n;");
    let StmtKind::Declare { bindings, .. } = kind else {
        panic!("expected a declaration");
    };
    let Some(ExprKind::Literal(Literal::BigInt(digits))) =
        bindings[0].value.as_ref().map(|value| &value.kind)
    else {
        panic!("expected a BigInt literal");
    };
    assert_eq!(digits, "9007199254740993");
}

#[test]
fn asi_is_honoured_because_swc_honours_it() {
    // A newline after `return` ends the statement. Not formatting: meaning.
    let (kind, _) = only_statement("function f() { return\n 1; }");
    match kind {
        StmtKind::Function(function) => {
            let FunctionBody::Block(body) = &function.body else {
                panic!("expected a block")
            };
            assert_eq!(body.len(), 2, "the `1` is its own statement");
            assert!(
                matches!(body[0].kind, StmtKind::Return(None)),
                "and the return produced nothing"
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_whole_small_program_arrives_intact() {
    let (program, names) = module(
        "export class Counter {
             #count = 0;
             increment(by = 1) { this.#count += by; return this; }
             get value() { return this.#count; }
         }

         export function sum(...numbers: number[]): number {
             let total = 0;
             for (const n of numbers) { total += n; }
             return total;
         }",
    );

    assert_eq!(program.exports().count(), 2);
    assert!(names.len() > 5);

    // Both exports declare, so both carry a statement that also binds locally.
    for export in program.exports() {
        match &export.kind {
            rts_codegen::syntax::ExportKind::Declaration(statement) => assert!(matches!(
                statement.kind,
                StmtKind::Class(_) | StmtKind::Function(_)
            )),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn a_pattern_reached_through_a_catch_clause_is_a_pattern() {
    let (kind, _) = only_statement("try { f(); } catch ({ message }) { g(message); }");
    match kind {
        StmtKind::Try { catch, .. } => {
            let binding = catch.expect("has a handler").binding.expect("names it");
            assert!(matches!(binding, Pattern::Object(_)));
        }
        other => panic!("{other:?}"),
    }

    let (bare, _) = only_statement("try { f(); } catch { g(); }");
    match bare {
        StmtKind::Try { catch, .. } => {
            assert!(catch.expect("has a handler").binding.is_none());
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_for_head_with_no_declaration_may_write_to_a_place() {
    // `for ([a, obj.b] of xs)` is `[a, obj.b] = xs` once per pass, so the head
    // is read in the assignment role. It was read in the binding role, which
    // refused every member target in a for-head: the same shape, read on the
    // wrong side of the one distinction `parse::pat` exists to draw.
    let (kind, _) = only_statement("for ([a, obj.b] of xs) {}");
    let StmtKind::ForEach { target, .. } = kind else {
        panic!("expected a for-each");
    };
    let ForEachTarget::Assign(Pattern::Array(array)) = target else {
        panic!("a head with no declaration assigns, it does not bind: {target:?}");
    };
    assert!(matches!(
        array.elements[1].as_ref().unwrap().pattern,
        Pattern::Target(_)
    ));
}

#[test]
fn super_is_a_place_and_not_only_a_read() {
    // SWC hands `super.x` out as an `Expr` and `super.x = v` as a
    // `SimpleAssignTarget`. Only the first was bridged, so a method could read
    // through `super` and not assign through it.
    // Written inside a method, because that is the only place the assignment is
    // legal: `check/deep` refuses `super` anywhere else, and `early_errors.rs`
    // asserts that refusal. The fixture said `super.x = 1;` at the top level of
    // a module until the rule existed, and then said something the language
    // does not allow.
    let (kind, _) = only_statement("class C extends B { m() { super.x = 1; } }");
    let StmtKind::Class(class) = kind else {
        panic!("expected a class");
    };
    let ClassElement::Method(method) = &class.body[0] else {
        panic!("expected a method");
    };
    let FunctionBody::Block(body) = &method.function.body else {
        panic!("expected a block");
    };
    let StmtKind::Expr(expression) = &body[0].kind else {
        panic!("expected an expression statement");
    };
    let ExprKind::Assign { target, .. } = &expression.kind else {
        panic!("expected an assignment");
    };
    let AssignTarget::Place(place) = target else {
        panic!("`super.x` is a place: {target:?}");
    };
    assert!(matches!(place.kind, ExprKind::SuperMember { .. }));
}

#[test]
fn a_bigint_property_key_is_its_digits() {
    // The one place a BigInt needs no arithmetic: ToPropertyKey runs it through
    // ToString, and a BigInt's ToString is the digits already in hand. So the
    // key is decidable even though the value is not.
    let (kind, names) = only_expr("({ 1n: x });");
    let ExprKind::Object { properties } = kind else {
        panic!("expected an object");
    };
    let Property::Value { key, .. } = &properties[0] else {
        panic!("expected a value property");
    };
    let PropertyKey::Named(name) = key else {
        panic!("a BigInt key is static: {key:?}");
    };
    assert_eq!(names.text(*name), "1");
}

#[test]
fn a_private_member_is_not_the_property_of_the_same_letters() {
    // SWC hands `#x` over as `x`, so interning it plainly made `this.#x` and
    // `this.x` the *same* `Member` node — one tree for two things that are not
    // alike at all: a property anyone can read, and a name reachable only from
    // inside the class body.
    //
    // The separator is `@@#`, not `#`: `@@` is the space the runtime already
    // excludes from every enumeration, and `#` alone is a prefix a program can
    // write — `o["#main"]` is an ordinary property that would have vanished from
    // `Object.keys`.
    let (kind, names) = only_expr("(class { #x; m() { return this.#x; } });");
    let ExprKind::Class(class) = kind else {
        panic!("expected a class");
    };
    let ClassElement::Field(field) = &class.body[0] else {
        panic!("expected a field");
    };
    let ClassKey::Private(declared) = &field.key else {
        panic!("expected a private key");
    };
    // The `1` is the CLASS's number, not part of the spelling: a private name is
    // resolved lexically, one scope per class body, and `0` is reserved for
    // "outside any class". Interning by text alone made `class Box { #x = 7 }`
    // and `class Sub extends Box { #x = 20 }` one field on one object — `20:20`
    // where every other engine answers `7:20`. See `parse/mod.rs::private_name`.
    assert_eq!(names.text(*declared), "@@#1#x");
}

#[test]
fn two_namespace_blocks_with_one_name_merge_into_one_object() {
    // TypeScript merges `namespace N { … }` written twice under one name into
    // one object — a program may extend a namespace from a second block the
    // same way it extends a class from nowhere else. The hoisted `var N;` is
    // what makes that possible: only the FIRST block emits it, and the second
    // block's `N || (N = {})` finds the object the first one already built.
    let mut names = Names::new();
    let program = parse_module(
        "namespace N { export const a = 1; } namespace N { export const b = 2; }",
        &mut names,
    )
    .expect("two blocks with one name merge rather than refuse");
    let body = statements(&program);
    assert_eq!(body.len(), 2, "one statement per block, not one per name");

    let StmtKind::Block(first) = &body[0].kind else {
        panic!("the first block hoists its `var N;` under it: {:?}", body[0].kind);
    };
    assert!(
        matches!(first[0].kind, StmtKind::Declare { kind: BindingKind::Var, .. }),
        "the first block's own `var N;`: {:?}",
        first[0].kind
    );

    // The second block merges: no `var` of its own, just the call.
    assert!(
        matches!(body[1].kind, StmtKind::Expr(_)),
        "the second block does not re-hoist: {:?}",
        body[1].kind
    );
}

#[test]
fn class_decorators_apply_bottom_up() {
    // `@first @second class C {}` runs `second` before `first` — the
    // decorator nearest the class runs first, which is the one point the
    // legacy `experimentalDecorators` design and the ES2022 standard design
    // agree on. Pinned on the SHAPE emitted (an assignment per decorator, in
    // reverse source order) rather than by running the program, because this
    // is `rts-codegen`'s tree, not `rts-host`'s runtime — the suite files
    // `decorator_multiple.test.ts` pin the same rule end to end.
    let mut names = Names::new();
    let program = parse_module("@first @second class C {}", &mut names)
        .expect("a decorated class declaration lowers rather than being refused");
    let body = statements(&program);
    assert_eq!(body.len(), 1);
    let StmtKind::Block(stmts) = &body[0].kind else {
        panic!("expected the var/assign/apply block: {:?}", body[0].kind);
    };
    // `var C;`, `C = class C {};`, then one application per decorator.
    assert_eq!(stmts.len(), 4, "{stmts:?}");

    let applications: Vec<&str> = stmts[2..]
        .iter()
        .map(|stmt| {
            let StmtKind::Expr(Expr {
                kind: ExprKind::Assign { value, .. },
                ..
            }) = &stmt.kind
            else {
                panic!("expected an assignment applying a decorator: {stmt:?}");
            };
            let ExprKind::Call { callee, .. } = &value.kind else {
                panic!("expected a call: {value:?}");
            };
            let ExprKind::Ident(callee_name) = &callee.kind else {
                panic!("expected the decorator called by name: {callee:?}");
            };
            names.text(*callee_name)
        })
        .collect();
    assert_eq!(
        applications,
        vec!["second", "first"],
        "the decorator nearest the class runs first"
    );
}
