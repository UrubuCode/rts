use sha2::{Digest, Sha256};

use super::{DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string};
use crate::namespaces::value::RuntimeValue;

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

pub fn dispatch(callee: &str, args: &[RuntimeValue]) -> Option<DispatchOutcome> {
    match callee {
        "crypto.sha256" if !args.is_empty() => Some(DispatchOutcome::Value(RuntimeValue::String(
            hash_sha256(&arg_to_string(args, 0)),
        ))),
        _ => None,
    }
}

pub fn hash_sha256(value: &str) -> String {
    let mut h = Sha256::new();
    h.update(value.as_bytes());
    let digest = h.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
