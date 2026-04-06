use crate::runtime::bootstrap_lang::JsValue;
use crate::runtime::state as runtime_state;

use super::{DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string};

const MEMBERS: &[NamespaceMember] = &[NamespaceMember {
    name: "sha256",
    callee: "crypto.sha256",
}];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "crypto",
    members: MEMBERS,
};

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "crypto.sha256" if !args.is_empty() => Some(DispatchOutcome::Value(JsValue::String(
            runtime_state::hash_sha256(&arg_to_string(args, 0)),
        ))),
        _ => None,
    }
}
