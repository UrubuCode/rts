//! `@operator("op") m() {}` is sugar for `[operators.<name>] m() {}` and nothing else.
//!
//! The claim is that the parser hands the emitter the SAME tree the symbol spelling does, so the
//! runtime overload protocol and `emit::operator_direct` see no difference. These read the method
//! keys out of the parsed class, and pin the refusals.

use rts_codegen::names::Names;
use rts_codegen::parse::{ParseError, parse_module, parse_script};
use rts_codegen::syntax::{ClassElement, ClassKey, ExprKind, ModuleItem, Program, PropertyKey, StmtKind};

/// The `base.member` pairs of every computed method key in the first class, as text.
fn computed_keys(program: &Program, names: &Names) -> Vec<(String, String)> {
    let mut keys = Vec::new();
    for item in &program.body {
        let ModuleItem::Stmt(statement) = item else { continue };
        let StmtKind::Class(class) = &statement.kind else { continue };
        for element in &class.body {
            let ClassElement::Method(method) = element else { continue };
            if let ClassKey::Public(PropertyKey::Computed(key)) = &method.key
                && let ExprKind::Member { object, property, .. } = &key.kind
                && let ExprKind::Ident(base) = &object.kind
            {
                keys.push((names.text(*base).to_owned(), names.text(*property).to_owned()));
            }
        }
    }
    keys
}

fn imports(program: &Program) -> usize {
    program.body.iter().filter(|item| matches!(item, ModuleItem::Import(_))).count()
}

fn parsed(source: &str) -> (Program, Names) {
    let mut names = Names::new();
    let program = parse_module(source, &mut names).unwrap_or_else(|e| panic!("{source:?}: {e}"));
    (program, names)
}

fn refused(source: &str) -> String {
    let mut names = Names::new();
    match parse_module(source, &mut names) {
        Ok(_) => panic!("{source:?} parsed"),
        Err(error) => error.to_string(),
    }
}

const ALL: &str = r#"class V {
  @operator("+") a(o, r) {}  @operator("-") b(o, r) {}  @operator("*") c(o, r) {}
  @operator("/") d(o, r) {}  @operator("%") e(o, r) {}  @operator("**") f(o, r) {}
  @operator("<") g(o, r) {}  @operator("<=") h(o, r) {}  @operator(">") i(o, r) {}
  @operator(">=") j(o, r) {} @operator("==") k(o, r) {}  @operator("unary-") l() {}
}"#;

#[test]
fn every_spelling_keys_the_method_by_its_symbol() {
    let (program, names) = parsed(ALL);
    let members: Vec<String> = computed_keys(&program, &names).into_iter().map(|(_, m)| m).collect();
    assert_eq!(members, ["add", "sub", "mul", "div", "mod", "pow", "lt", "le", "gt", "ge", "eq", "neg"]);
}

#[test]
fn it_matches_the_symbol_spelling_with_the_users_own_import() {
    let sugar = "import { operators as ops } from \"rts\";\nclass V { @operator(\"+\") add(o, r) {} }";
    let plain = "import { operators as ops } from \"rts\";\nclass V { [ops.add](o, r) {} }";
    let (a, an) = parsed(sugar);
    let (b, bn) = parsed(plain);
    assert_eq!(computed_keys(&a, &an), computed_keys(&b, &bn));
    // The user's binding is reused, so the module keeps exactly one `operators` import.
    assert_eq!(imports(&a), 1);
}

#[test]
fn a_module_without_the_import_gets_a_hidden_one() {
    let (program, names) = parsed("class V { @operator(\"+\") add(o, r) {} }");
    assert_eq!(imports(&program), 1);
    assert_eq!(computed_keys(&program, &names), [("__rts_operators".to_owned(), "add".to_owned())]);
}

#[test]
fn a_class_with_no_decorator_gets_no_import() {
    let (program, _) = parsed("class V { add(o, r) {} }");
    assert_eq!(imports(&program), 0);
}

#[test]
fn a_malformed_operator_is_a_parse_error() {
    for bad in [
        "@operator(\"blah\")",
        "@operator(123)",
        "@operator()",
        "@operator(\"+\", \"-\")",
        "@operator",
        "@operator(\"+\") @operator(\"-\")",
        "@operator(...x)",
    ] {
        let message = refused(&format!("class V {{ {bad} m(o, r) {{}} }}"));
        assert!(message.contains("operator"), "{bad}: {message}");
    }
}

#[test]
fn another_decorator_beside_it_is_refused() {
    let message = refused("class V { @log @operator(\"+\") m(o, r) {} }");
    assert!(message.contains("another decorator"), "{message}");
}

#[test]
fn a_script_cannot_take_the_hidden_import() {
    let mut names = Names::new();
    let error = parse_script("class V { @operator(\"+\") m(o, r) {} }", &mut names)
        .expect_err("a script has no import to inject");
    assert!(matches!(error, ParseError::Unsupported { .. }), "{error}");
}
