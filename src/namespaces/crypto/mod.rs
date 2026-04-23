//! `crypto` namespace — primitivas criptograficas expostas ao runtime.
//!
//! Hash SHA-256 e helpers. Sem estado global — cada chamada opera stateless
//! sobre input do caller.

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
    hash_sha256_bytes(value.as_bytes())
}

pub fn hash_sha256_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let digest = h.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Zero-overhead extern "C" entry point — bypassa `__rts_dispatch` e o
/// boxing de RuntimeValue. Usado por codegens que ja conhecem os tipos
/// de compile-time (TS com anotacao `data: str`).
///
/// SAFETY: `ptr`/`len` devem apontar para UTF-8 valido vivo durante a
/// chamada. O handle retornado e gerenciado pelo ValueStore e deve ser
/// lido via `__rts_string_ptr`/`__rts_string_len` ou liberado via GC.
#[unsafe(no_mangle)]
pub extern "C" fn __rts_crypto_sha256_direct(ptr: *const u8, len: i64) -> i64 {
    if ptr.is_null() || len < 0 {
        return crate::namespaces::abi::undefined_handle();
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let digest = hash_sha256_bytes(bytes);
    crate::namespaces::abi::push_runtime_value(RuntimeValue::String(digest))
}
