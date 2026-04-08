use crate::namespaces::state as runtime_state;
use crate::runtime::bootstrap_lang::JsValue;

use super::{DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string};

const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "set",
        callee: "global.set",
        doc: "Stores a string value in runtime global map.",
        ts_signature: "set(key: str, value: str): void",
    },
    NamespaceMember {
        name: "get",
        callee: "global.get",
        doc: "Reads a string value from runtime global map.",
        ts_signature: "get(key: str): str | undefined",
    },
    NamespaceMember {
        name: "has",
        callee: "global.has",
        doc: "Checks whether a key exists in global map.",
        ts_signature: "has(key: str): bool",
    },
    NamespaceMember {
        name: "delete",
        callee: "global.delete",
        doc: "Deletes a key from global map.",
        ts_signature: "delete(key: str): bool",
    },
    NamespaceMember {
        name: "keys",
        callee: "global.keys",
        doc: "Returns global keys joined by commas.",
        ts_signature: "keys(): str",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "global",
    doc: "Small runtime key-value storage for bootstrap state.",
    members: MEMBERS,
    ts_prelude: &[],
};

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "global.set" if args.len() >= 2 => {
            runtime_state::global_set(arg_to_string(args, 0), arg_to_string(args, 1));
            Some(DispatchOutcome::Value(JsValue::Undefined))
        }
        "global.get" if !args.is_empty() => Some(DispatchOutcome::Value(
            runtime_state::global_get(&arg_to_string(args, 0))
                .map(JsValue::String)
                .unwrap_or(JsValue::Undefined),
        )),
        "global.has" if !args.is_empty() => Some(DispatchOutcome::Value(JsValue::Bool(
            runtime_state::global_has(&arg_to_string(args, 0)),
        ))),
        "global.delete" if !args.is_empty() => Some(DispatchOutcome::Value(JsValue::Bool(
            runtime_state::global_delete(&arg_to_string(args, 0)),
        ))),
        "global.keys" if args.is_empty() => Some(DispatchOutcome::Value(JsValue::String(
            runtime_state::global_keys_csv(),
        ))),
        _ => None,
    }
}
