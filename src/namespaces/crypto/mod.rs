use crate::namespaces::state as runtime_state;
use crate::runtime::bootstrap_lang::JsValue;

use super::{DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string};

const MEMBERS: &[NamespaceMember] = &[NamespaceMember {
    name: "sha256",
    callee: "crypto.sha256",
    doc: "Computes SHA-256 digest and returns hex string.",
    ts_signature: "sha256(data: str): str",
}];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "crypto",
    doc: "Cryptographic helpers backed by Rust implementations.",
    members: MEMBERS,
    ts_prelude: &[],
};

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "crypto.sha256" if !args.is_empty() => Some(DispatchOutcome::Value(JsValue::String(
            runtime_state::hash_sha256(&arg_to_string(args, 0)),
        ))),
        _ => None,
    }
}
