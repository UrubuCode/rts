use crate::codegen::jit::execute;
use crate::mir::cfg::{BasicBlock, Terminator};
use crate::mir::{MirFunction, MirModule, MirStatement};

#[test]
fn jit_executes_main_and_returns_literal() {
    let module = MirModule {
        functions: vec![MirFunction {
            name: "main".to_string(),
            blocks: vec![BasicBlock {
                label: "entry".to_string(),
                statements: vec![MirStatement {
                    text: "ret 7".to_string(),
                }],
                terminator: Terminator::Return,
            }],
        }],
    };

    let report = execute(&module, "main").expect("jit should compile");
    assert_eq!(report.entry_function, "main");
    assert!(report.compiled_functions >= 1);
    assert_eq!(report.entry_return_value, 7);
    assert!(report.executed);
}

#[test]
fn jit_entry_can_call_another_function() {
    let module = MirModule {
        functions: vec![
            MirFunction {
                name: "helper".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![MirStatement {
                        text: "ret 42".to_string(),
                    }],
                    terminator: Terminator::Return,
                }],
            },
            MirFunction {
                name: "main".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![MirStatement {
                        text: "call helper".to_string(),
                    }],
                    terminator: Terminator::Return,
                }],
            },
        ],
    };

    let report = execute(&module, "main").expect("jit should compile");
    assert_eq!(report.entry_return_value, 42);
    assert!(report.executed);
}

#[test]
fn jit_skips_execution_when_entry_is_missing() {
    let module = MirModule {
        functions: vec![MirFunction {
            name: "helper".to_string(),
            blocks: vec![BasicBlock {
                label: "entry".to_string(),
                statements: vec![MirStatement {
                    text: "ret 42".to_string(),
                }],
                terminator: Terminator::Return,
            }],
        }],
    };

    let report = execute(&module, "main").expect("jit should compile");
    assert_eq!(report.entry_function, "main");
    assert_eq!(report.entry_return_value, 0);
    assert!(!report.executed);
}

#[test]
fn jit_unknown_namespace_call_falls_back_to_runtime_dispatch() {
    let module = MirModule {
        functions: vec![MirFunction {
            name: "main".to_string(),
            blocks: vec![BasicBlock {
                label: "entry".to_string(),
                statements: vec![MirStatement {
                    text: r#"io.print("hello from jit")"#.to_string(),
                }],
                terminator: Terminator::Return,
            }],
        }],
    };

    let report = execute(&module, "main").expect("jit should compile");
    assert_eq!(report.entry_return_value, 0);
    assert!(report.executed);
}

#[test]
fn jit_accepts_typed_variable_declaration_statement() {
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

    let report = execute(&module, "main").expect("jit should compile declarations");
    assert_eq!(report.entry_return_value, 0);
    assert!(report.executed);
}

#[test]
fn jit_executes_if_else_statement_via_runtime_evaluator() {
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

    let report = execute(&module, "main").expect("jit should evaluate if/else statement");
    assert_eq!(report.entry_return_value, 0);
    assert!(report.executed);
}

#[test]
fn jit_supports_nested_user_function_call_arguments() {
    let module = MirModule {
        functions: vec![
            MirFunction {
                name: "helper".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![MirStatement {
                        text: "return 4;".to_string(),
                    }],
                    terminator: Terminator::Return,
                }],
            },
            MirFunction {
                name: "id".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![
                        MirStatement {
                            text: "enter id(n)".to_string(),
                        },
                        MirStatement {
                            text: "return n;".to_string(),
                        },
                    ],
                    terminator: Terminator::Return,
                }],
            },
            MirFunction {
                name: "main".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![MirStatement {
                        text: "id(helper())".to_string(),
                    }],
                    terminator: Terminator::Return,
                }],
            },
        ],
    };

    let report = execute(&module, "main").expect("jit should support nested call arguments");
    assert_ne!(report.entry_return_value, 0);
    assert!(report.executed);
}
