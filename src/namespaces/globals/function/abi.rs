//! Function — ABI (#359).

use crate::abi::{AbiType, MemberKind, NamespaceMember};
use crate::abi::global_class::GlobalClassSpec;

pub const MEMBERS: &[NamespaceMember] = &[
    // Constructor: new Function(args..., body) — tudo strings.
    // Implementado com aridade variavel via codegen helper.
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol: "__RTS_FN_GL_FUNCTION_NEW",
        args: &[AbiType::Handle, AbiType::Handle],
        returns: AbiType::Handle,
        doc: "new Function(params, body) — compila body em runtime via runtime.eval. Aceita 1+ args; ultimo eh body, demais sao parametros separados por virgula.",
        ts_signature: "constructor(...args: string[]): Function",
        intrinsic: None,
        pure: false,
    },
    // Instance methods.
    NamespaceMember {
        name: "call",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_FUNCTION_CALL",
        args: &[AbiType::Handle, AbiType::I64, AbiType::Handle],
        returns: AbiType::I64,
        doc: "fn.call(thisArg, ...args) — invoca com this binding e args. Args como Vec handle.",
        ts_signature: "call(thisArg: any, ...args: any[]): any",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "apply",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_FUNCTION_APPLY",
        args: &[AbiType::Handle, AbiType::I64, AbiType::Handle],
        returns: AbiType::I64,
        doc: "fn.apply(thisArg, argsArray) — invoca com this binding e args como array.",
        ts_signature: "apply(thisArg: any, args: any[]): any",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "bind",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_FUNCTION_BIND",
        args: &[AbiType::Handle, AbiType::I64, AbiType::Handle],
        returns: AbiType::Handle,
        doc: "fn.bind(thisArg, ...args) — retorna nova Function com this fixo + partial args.",
        ts_signature: "bind(thisArg: any, ...args: any[]): Function",
        intrinsic: None,
        pure: false,
    },
    NamespaceMember {
        name: "toString",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_FUNCTION_TO_STRING",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "fn.toString() — source se criada via new Function, senao '[native code]'.",
        ts_signature: "toString(): string",
        intrinsic: None,
        pure: true,
    },
    // Getters.
    NamespaceMember {
        name: "name",
        kind: MemberKind::InstanceGetter,
        symbol: "__RTS_FN_GL_FUNCTION_NAME",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "fn.name — nome da fn ou 'anonymous'.",
        ts_signature: "readonly name: string",
        intrinsic: None,
        pure: true,
    },
    NamespaceMember {
        name: "length",
        kind: MemberKind::InstanceGetter,
        symbol: "__RTS_FN_GL_FUNCTION_LENGTH",
        args: &[AbiType::Handle],
        returns: AbiType::I64,
        doc: "fn.length — numero de parametros declarados (descontando bound args).",
        ts_signature: "readonly length: number",
        intrinsic: None,
        pure: true,
    },
];

pub const FUNCTION_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "Function",
    doc: "Function — JS primitive class. call/apply/bind + name/length + new Function via eval (#359).",
    members: MEMBERS,
};
