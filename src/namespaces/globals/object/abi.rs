//! Object — ABI registration as GlobalClassSpec.

use crate::abi::{AbiType, GlobalClassSpec, MemberKind, NamespaceMember};

/// Membros estáticos do Object (métodos de classe).
pub const STATIC_MEMBERS: &[NamespaceMember] = &[
    // Object.create(proto) — cria novo objeto com proto especificado
    NamespaceMember {
        name: "create",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_GL_OBJECT_CREATE",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Object.create(proto) — cria novo objeto com __proto__ = proto.",
        ts_signature: "create(proto: any): any",
        intrinsic: None,
        pure: false,
    },
    // Object.keys(obj) — retorna array de chaves
    NamespaceMember {
        name: "keys",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_COLLECTIONS_MAP_KEYS",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Object.keys(obj) — retorna array com as chaves do objeto.",
        ts_signature: "keys(o: object): string[]",
        intrinsic: None,
        pure: true,
    },
    // Object.values(obj) — retorna array de valores
    NamespaceMember {
        name: "values",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_COLLECTIONS_MAP_VALUES",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Object.values(obj) — retorna array com os valores do objeto.",
        ts_signature: "values(o: object): any[]",
        intrinsic: None,
        pure: true,
    },
    // Object.entries(obj) — retorna array de [key, value]
    NamespaceMember {
        name: "entries",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_COLLECTIONS_MAP_ENTRIES",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Object.entries(obj) — retorna array de pares [chave, valor].",
        ts_signature: "entries(o: object): [string, any][]",
        intrinsic: None,
        pure: true,
    },
    // Object.assign(target, ...sources) — copia propriedades
    NamespaceMember {
        name: "assign",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_COLLECTIONS_MAP_ASSIGN",
        args: &[AbiType::Handle, AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Object.assign(target, source) — copia props de source para target.",
        ts_signature: "assign<T extends object>(target: T, source: any): T",
        intrinsic: None,
        pure: false,
    },
    // Object.freeze(obj) — congela objeto (v0 no-op)
    NamespaceMember {
        name: "freeze",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_COLLECTIONS_MAP_FREEZE",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Object.freeze(obj) — congela objeto (v0 retorna o proprio handle).",
        ts_signature: "freeze<T extends object>(o: T): T",
        intrinsic: None,
        pure: false,
    },
    // Object.fromEntries(arr) — cria objeto a partir de array de pares
    NamespaceMember {
        name: "fromEntries",
        kind: MemberKind::Function,
        symbol: "__RTS_FN_NS_COLLECTIONS_MAP_FROM_ENTRIES",
        args: &[AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Object.fromEntries(arr) — cria objeto a partir de array de [key, value].",
        ts_signature: "fromEntries(entries: Iterable<[string, any]>): object",
        intrinsic: None,
        pure: false,
    },
    // hasOwnProperty(key) — verifica se propriedade é própria (também disponível como método de instância)
    NamespaceMember {
        name: "hasOwnProperty",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_OBJECT_HAS_OWN_PROPERTY",
        args: &[AbiType::Handle, AbiType::StrPtr],
        returns: AbiType::Bool,
        doc: "obj.hasOwnProperty(key) — true se key é propriedade própria.",
        ts_signature: "hasOwnProperty(key: string): boolean",
        intrinsic: None,
        pure: true,
    },
];

pub const OBJECT_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "Object",
    doc: "Object — JS primitive class. Métodos estáticos e de instância para manipulação de objetos.",
    members: STATIC_MEMBERS,
};
