//! `test` namespace — asserts e hooks de teste expostos ao runtime TS.
//!
//! Usado por packages de testing (ex: `packages/test`) para escrever assertions
//! nativas sem depender de framework externo.

use super::{DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string};
use crate::namespaces::value::RuntimeValue;

const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "assert",
        callee: "test.assert",
        doc: "Panics if condition is false. Optional message is shown on failure.",
        ts_signature: "assert(condition: bool, message?: str): void",
    },
    NamespaceMember {
        name: "assert_eq",
        callee: "test.assert_eq",
        doc: "Panics if a and b are not equal (string comparison). Optional message shown on failure.",
        ts_signature: "assert_eq(a: str, b: str, message?: str): void",
    },
    NamespaceMember {
        name: "assert_ne",
        callee: "test.assert_ne",
        doc: "Panics if a and b are equal (string comparison). Optional message shown on failure.",
        ts_signature: "assert_ne(a: str, b: str, message?: str): void",
    },
    NamespaceMember {
        name: "pass",
        callee: "test.pass",
        doc: "Emits a passing test message to stdout.",
        ts_signature: "pass(message?: str): void",
    },
    NamespaceMember {
        name: "fail",
        callee: "test.fail",
        doc: "Unconditionally panics with an optional message.",
        ts_signature: "fail(message?: str): never",
    },
    NamespaceMember {
        name: "describe",
        callee: "test.describe",
        doc: "Emits a test suite header to stdout.",
        ts_signature: "describe(name: str): void",
    },
    NamespaceMember {
        name: "it",
        callee: "test.it",
        doc: "Emits a test case header to stdout.",
        ts_signature: "it(name: str): void",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "test",
    doc: "Assertion helpers and test output utilities for rts:test.",
    members: MEMBERS,
    ts_prelude: &[],
};

pub fn dispatch(callee: &str, args: &[RuntimeValue]) -> Option<DispatchOutcome> {
    match callee {
        "test.assert" => {
            let condition = args.first().cloned().unwrap_or(RuntimeValue::Undefined);
            if !condition.truthy() {
                let message = if args.len() > 1 {
                    arg_to_string(args, 1)
                } else {
                    "assertion failed".to_string()
                };
                return Some(DispatchOutcome::Panic(format!("test.assert: {message}")));
            }
            Some(DispatchOutcome::Value(RuntimeValue::Undefined))
        }
        "test.assert_eq" => {
            let a = arg_to_string(args, 0);
            let b = arg_to_string(args, 1);
            if a != b {
                let message = if args.len() > 2 {
                    format!("{}: left={a:?} right={b:?}", arg_to_string(args, 2))
                } else {
                    format!("assert_eq failed: left={a:?} right={b:?}")
                };
                return Some(DispatchOutcome::Panic(format!("test.assert_eq: {message}")));
            }
            Some(DispatchOutcome::Value(RuntimeValue::Undefined))
        }
        "test.assert_ne" => {
            let a = arg_to_string(args, 0);
            let b = arg_to_string(args, 1);
            if a == b {
                let message = if args.len() > 2 {
                    format!("{}: values are equal: {a:?}", arg_to_string(args, 2))
                } else {
                    format!("assert_ne failed: both equal to {a:?}")
                };
                return Some(DispatchOutcome::Panic(format!("test.assert_ne: {message}")));
            }
            Some(DispatchOutcome::Value(RuntimeValue::Undefined))
        }
        "test.pass" => {
            let message = if args.is_empty() {
                "ok".to_string()
            } else {
                format!("ok — {}", arg_to_string(args, 0))
            };
            Some(DispatchOutcome::Emit(format!("  PASS {message}")))
        }
        "test.fail" => {
            let message = if args.is_empty() {
                "test failed".to_string()
            } else {
                arg_to_string(args, 0)
            };
            Some(DispatchOutcome::Panic(format!("test.fail: {message}")))
        }
        "test.describe" => {
            let name = arg_to_string(args, 0);
            println!("\n{name}");
            if let Some(RuntimeValue::NativeFunction(fn_name)) = args.get(1) {
                crate::namespaces::abi::call_jit_fn_by_name(fn_name);
            }
            Some(DispatchOutcome::Value(RuntimeValue::Undefined))
        }
        "test.it" => {
            let name = arg_to_string(args, 0);
            println!("  · {name}");
            if let Some(RuntimeValue::NativeFunction(fn_name)) = args.get(1) {
                crate::namespaces::abi::call_jit_fn_by_name(fn_name);
            }
            Some(DispatchOutcome::Value(RuntimeValue::Undefined))
        }
        _ => None,
    }
}
