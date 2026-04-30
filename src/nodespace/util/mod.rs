//! `node:util` — fase 1 (subset RTS-extensions).
//!
//! Node `util.format(fmt, ...args)` variadic e `util.inspect(obj)`
//! ficam para fase 2 (variadic format string parsing + serializacao
//! recursiva de objeto). Esta fase expoe utilities deterministicos
//! sobre `rts::fmt` com nomes nao-conflitantes pra evitar confusao
//! com a API node oficial:
//!
//! - `formatInt(n)` / `formatFloat(n)` — to-string
//! - `formatHex/Bin/Oct(n)` — bases comuns
//! - `parseInt/parseFloat(s)` — tolerante a whitespace e prefix

use super::{NodespaceMember, NodespaceSpec};
use crate::abi::AbiType;

pub const MEMBERS: &[NodespaceMember] = &[
    // Wrappers diretos sobre rts::fmt — convertem inteiros/floats em
    // diferentes representacoes de string, retornando handle.
    NodespaceMember {
        name: "formatInt",
        symbol: "__RTS_FN_NS_FMT_FMT_I64",
        args: &[AbiType::I64],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "formatFloat",
        symbol: "__RTS_FN_NS_FMT_FMT_F64",
        args: &[AbiType::F64],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "formatHex",
        symbol: "__RTS_FN_NS_FMT_FMT_HEX",
        args: &[AbiType::I64],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "formatBin",
        symbol: "__RTS_FN_NS_FMT_FMT_BIN",
        args: &[AbiType::I64],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "formatOct",
        symbol: "__RTS_FN_NS_FMT_FMT_OCT",
        args: &[AbiType::I64],
        returns: AbiType::Handle,
    },
    NodespaceMember {
        name: "parseInt",
        symbol: "__RTS_FN_NS_FMT_PARSE_I64",
        args: &[AbiType::StrPtr],
        returns: AbiType::I64,
    },
    NodespaceMember {
        name: "parseFloat",
        symbol: "__RTS_FN_NS_FMT_PARSE_F64",
        args: &[AbiType::StrPtr],
        returns: AbiType::F64,
    },
];

pub const SPEC: NodespaceSpec = NodespaceSpec {
    node_module: "util",
    ns_prefix: "node_util",
    members: MEMBERS,
};
