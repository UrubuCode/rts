use crate::namespaces::value::JsValue;
use crate::namespaces::task::executor as runtime_state;

use super::{DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string, arg_to_u64};

const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "resolve",
        callee: "promise.resolve",
        doc: "Creates a fulfilled promise handle.",
        ts_signature: "resolve(value: str): Handle",
    },
    NamespaceMember {
        name: "reject",
        callee: "promise.reject",
        doc: "Creates a rejected promise handle.",
        ts_signature: "reject(reason: str): Handle",
    },
    NamespaceMember {
        name: "status",
        callee: "promise.status",
        doc: "Returns current state of a promise handle.",
        ts_signature: "status(handle: Handle): State | undefined",
    },
    NamespaceMember {
        name: "is_settled",
        callee: "promise.is_settled",
        doc: "Checks whether promise is fulfilled or rejected.",
        ts_signature: "is_settled(handle: Handle): bool",
    },
    NamespaceMember {
        name: "await",
        callee: "promise.await",
        doc: "Waits for promise completion and returns its payload.",
        ts_signature: "await(handle: Handle): str | undefined",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "promise",
    doc: "Promise handles and synchronous await bridge.",
    members: MEMBERS,
    ts_prelude: &[
        "export type Handle = usize;",
        "export type State = \"pending\" | \"fulfilled\" | \"rejected\";",
    ],
};

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "promise.resolve" if !args.is_empty() => Some(DispatchOutcome::Value(JsValue::Number(
            runtime_state::promise_resolve(arg_to_string(args, 0)) as f64,
        ))),
        "promise.reject" if !args.is_empty() => Some(DispatchOutcome::Value(JsValue::Number(
            runtime_state::promise_reject(arg_to_string(args, 0)) as f64,
        ))),
        "promise.status" if !args.is_empty() => Some(DispatchOutcome::Value(
            runtime_state::promise_status(arg_to_u64(args, 0))
                .map(|status| JsValue::String(status.as_str().to_string()))
                .unwrap_or(JsValue::Undefined),
        )),
        "promise.is_settled" if !args.is_empty() => Some(DispatchOutcome::Value(JsValue::Bool(
            runtime_state::promise_is_settled(arg_to_u64(args, 0)),
        ))),
        "promise.await" if !args.is_empty() => Some(DispatchOutcome::Value(
            match runtime_state::promise_await(arg_to_u64(args, 0)) {
                Some(Ok(value)) => JsValue::String(value),
                Some(Err(reason)) => JsValue::String(format!("rejected:{reason}")),
                None => JsValue::Undefined,
            },
        )),
        _ => None,
    }
}
