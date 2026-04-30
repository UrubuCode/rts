//! `node:crypto` — fase 1 (mapeamento RTS-extension).
//!
//! Node `createHash().update().digest()` builder + `randomUUID` etc
//! requerem state-streaming/UUID que `rts::crypto` ainda nao tem.
//! Esta fase 1 expoe os one-shot hashes/encodings nao-conflitantes:
//!
//! - `sha256(s) -> hex string` (one-shot, sem update())
//! - `randomBytesBuffer(n) -> Buffer handle`
//! - `hexEncode/Decode(buf) -> string/buffer`
//! - `base64Encode/Decode(buf) -> string/buffer`

use super::{NodespaceMember, NodespaceSpec};
use crate::abi::AbiType;

pub const MEMBERS: &[NodespaceMember] = &[
    NodespaceMember {
        name: "sha256",
        symbol: "__RTS_FN_NS_CRYPTO_SHA256_STR",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "randomBytesBuffer",
        symbol: "__RTS_FN_NS_CRYPTO_RANDOM_BUFFER",
        args: &[AbiType::I64],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "hexEncode",
        symbol: "__RTS_FN_NS_CRYPTO_HEX_ENCODE",
        args: &[AbiType::I64, AbiType::I64],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "hexDecode",
        symbol: "__RTS_FN_NS_CRYPTO_HEX_DECODE",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "base64Encode",
        symbol: "__RTS_FN_NS_CRYPTO_BASE64_ENCODE",
        args: &[AbiType::I64, AbiType::I64],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "base64Decode",
        symbol: "__RTS_FN_NS_CRYPTO_BASE64_DECODE",
        args: &[AbiType::StrPtr],
        returns: AbiType::Handle,
    },
];

pub const SPEC: NodespaceSpec = NodespaceSpec {
    node_module: "crypto",
    ns_prefix: "node_crypto",
    members: MEMBERS,
};
