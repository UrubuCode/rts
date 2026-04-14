use crate::codegen::mir_parse::parse_call_statement;
use crate::codegen::object_builder::{build_namespace_dispatch_object, lower_to_native_object};
use crate::mir::cfg::{BasicBlock, Terminator};
use crate::mir::{MirFunction, MirModule, MirStatement};

#[test]
fn emits_non_empty_native_object() {
    let module = MirModule {
        functions: vec![MirFunction {
            name: "main".to_string(),
            blocks: vec![BasicBlock {
                label: "entry".to_string(),
                statements: vec![MirStatement {
                    text: "ret 3".to_string(),
                }],
                terminator: Terminator::Return,
            }],
        }],
    };

    let bytes = lower_to_native_object(&module).expect("AOT object must compile");
    assert!(!bytes.is_empty());
}

#[test]
fn parses_direct_call_with_arguments() {
    let parsed = parse_call_statement(r#"io.print("hello", 123)"#).expect("call parse");
    assert_eq!(parsed.callee, "io.print");
    assert_eq!(parsed.args, vec![r#""hello""#, "123"]);
}

#[test]
fn lowers_imported_namespace_call_without_panicking() {
    let module = MirModule {
        functions: vec![MirFunction {
            name: "main".to_string(),
            blocks: vec![BasicBlock {
                label: "entry".to_string(),
                statements: vec![MirStatement {
                    text: r#"io.print("hello")"#.to_string(),
                }],
                terminator: Terminator::Return,
            }],
        }],
    };

    let bytes = lower_to_native_object(&module).expect("AOT object should compile");
    assert!(!bytes.is_empty());
}

#[test]
fn builds_namespace_wrapper_object() {
    let bytes =
        build_namespace_dispatch_object(&[String::from("io.print"), String::from("process.arch")], false)
            .expect("namespace wrapper object should compile");
    assert!(!bytes.is_empty());
}

#[test]
fn lowers_typed_variable_declaration_without_panicking() {
    let module = MirModule {
        functions: vec![MirFunction {
            name: "main".to_string(),
            blocks: vec![BasicBlock {
                label: "entry".to_string(),
                statements: vec![MirStatement {
                    text: "const valor: i32 = 2 * 60 * 60 * 1000;".to_string(),
                }],
                terminator: Terminator::Return,
            }],
        }],
    };

    let bytes = lower_to_native_object(&module).expect("declaration should lower to AOT");
    assert!(!bytes.is_empty());
}

#[test]
fn lowers_if_else_statement_via_runtime_evaluator() {
    let module = MirModule {
        functions: vec![MirFunction {
            name: "main".to_string(),
            blocks: vec![BasicBlock {
                label: "entry".to_string(),
                statements: vec![MirStatement {
                    text: "if (true) { io.print(1); } else { io.print(2); }".to_string(),
                }],
                terminator: Terminator::Return,
            }],
        }],
    };

    let bytes = lower_to_native_object(&module).expect("if/else should lower to AOT");
    assert!(!bytes.is_empty());
}
