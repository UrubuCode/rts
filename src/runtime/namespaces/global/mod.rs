use crate::runtime::bootstrap_lang::JsValue;
use crate::runtime::state as runtime_state;

use super::{DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string};

const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "set",
        callee: "global.set",
    },
    NamespaceMember {
        name: "get",
        callee: "global.get",
    },
    NamespaceMember {
        name: "has",
        callee: "global.has",
    },
    NamespaceMember {
        name: "delete",
        callee: "global.delete",
    },
    NamespaceMember {
        name: "keys",
        callee: "global.keys",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "global",
    members: MEMBERS,
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
