//! `node:crypto` — Registry registration (member table + `register`).

mod promises;
mod symbols;

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

fn pure_func(
    name: &str,
    symbol: &str,
    sig: rts_engine::Sig,
    ts: &str,
    doc: &str,
    fp: *const u8,
) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: true,
        intrinsic: None,
    }
}

/// Registers the `node:crypto` surface into the engine Registry. No
/// `.alias(...)` — `node:crypto` resolves as its own canonical key, distinct
/// from the existing `rts:crypto` namespace (RTS-flavored names/semantics), so
/// it must not alias onto it.
pub fn register(e: &mut Engine) {
    e.ns("node:crypto")
        .doc(
            "Real crypto primitives, Node-accurate names/values (node:crypto). \
             randomBytes/randomFillSync (Buffer) and createHash/createHmac/ \
             createCipheriv/createSign (stateful objects) are deferred pending \
             Buffer + handle-backed-object support in rts-node.",
        )
        .member(pure_func(
            "randomUUID",
            "__RTS_FN_NODE_CRYPTO_RANDOM_UUID",
            sig!(=> Handle),
            "randomUUID(): string",
            "A fresh RFC 4122 v4 (random) UUID from real OS CSPRNG entropy.",
            symbols::__RTS_FN_NODE_CRYPTO_RANDOM_UUID as *const u8,
        ))
        .done();
}
