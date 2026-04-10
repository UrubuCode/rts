mod constants;
mod functions;
mod memory;
pub mod natives;
mod scope;

use crate::namespaces::lang::JsValue;
use crate::namespaces::{DispatchOutcome, NamespaceMember, NamespaceSpec};

const MEMBERS: &[NamespaceMember] = &[
    // functions
    NamespaceMember {
        name: "declare_fn",
        callee: "rts.declare_fn",
        doc: "Declara uma função no registry de runtime.",
        ts_signature: "declare_fn(name_ptr: u64, arity: u64, body_ptr: u64): void",
    },
    NamespaceMember {
        name: "call_fn",
        callee: "rts.call_fn",
        doc: "Invoca função pelo ponteiro de nome, retorna ponteiro do corpo.",
        ts_signature: "call_fn(name_ptr: u64, args_ptr: u64, args_len: u64): u64",
    },
    NamespaceMember {
        name: "return_val",
        callee: "rts.return_val",
        doc: "Retorna um valor do escopo atual.",
        ts_signature: "return_val(value: u64): u64",
    },
    // scope
    NamespaceMember {
        name: "scope_push",
        callee: "rts.scope_push",
        doc: "Empilha novo escopo de variáveis.",
        ts_signature: "scope_push(): void",
    },
    NamespaceMember {
        name: "scope_pop",
        callee: "rts.scope_pop",
        doc: "Desempilha o escopo atual.",
        ts_signature: "scope_pop(): void",
    },
    NamespaceMember {
        name: "set_var",
        callee: "rts.set_var",
        doc: "Define variável no escopo atual pelo hash do nome.",
        ts_signature: "set_var(name_hash: u64, value: u64): void",
    },
    NamespaceMember {
        name: "get_var",
        callee: "rts.get_var",
        doc: "Lê variável percorrendo o stack de escopos.",
        ts_signature: "get_var(name_hash: u64): u64",
    },
    // constants
    NamespaceMember {
        name: "declare_const",
        callee: "rts.declare_const",
        doc: "Declara constante global imutável.",
        ts_signature: "declare_const(name_hash: u64, value: u64): void",
    },
    NamespaceMember {
        name: "get_const",
        callee: "rts.get_const",
        doc: "Lê constante global pelo hash do nome.",
        ts_signature: "get_const(name_hash: u64): u64",
    },
    // memory
    NamespaceMember {
        name: "alloc",
        callee: "rts.alloc",
        doc: "Aloca `size` bytes zerados, retorna ponteiro.",
        ts_signature: "alloc(size: u64): u64",
    },
    NamespaceMember {
        name: "free",
        callee: "rts.free",
        doc: "Libera bloco de memória.",
        ts_signature: "free(ptr: u64, size: u64): void",
    },
    NamespaceMember {
        name: "mem_copy",
        callee: "rts.mem_copy",
        doc: "Copia `len` bytes de src para dst sem overlap.",
        ts_signature: "mem_copy(dst: u64, src: u64, len: u64): void",
    },
    NamespaceMember {
        name: "i64_add",
        callee: "rts.i64_add",
        doc: "Soma dois inteiros i64 sem overhead JS.",
        ts_signature: "i64_add(a: i64, b: i64): i64",
    },
    NamespaceMember {
        name: "f64_mul",
        callee: "rts.f64_mul",
        doc: "Multiplica dois floats f64.",
        ts_signature: "f64_mul(a: f64, b: f64): f64",
    },
    NamespaceMember {
        name: "str_new",
        callee: "rts.str_new",
        doc: "Cria handle de string a partir de ponteiro e comprimento.",
        ts_signature: "str_new(ptr: u64, len: u64): u64",
    },
];

pub const NATIVES_MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "to_string",
        callee: "rts.natives.to_string",
        doc: "Converte qualquer valor para string (semântica JS).",
        ts_signature: "to_string(value: u64): u64",
    },
    NamespaceMember {
        name: "to_number",
        callee: "rts.natives.to_number",
        doc: "Converte qualquer valor para número (semântica JS).",
        ts_signature: "to_number(value: u64): f64",
    },
    NamespaceMember {
        name: "to_bool",
        callee: "rts.natives.to_bool",
        doc: "Converte qualquer valor para bool (truthy/falsy JS).",
        ts_signature: "to_bool(value: u64): bool",
    },
    NamespaceMember {
        name: "merge",
        callee: "rts.natives.merge",
        doc: "Merge genérico de dois valores com coerção (string ou número).",
        ts_signature: "merge(a: u64, b: u64): u64",
    },
    NamespaceMember {
        name: "add_mixed",
        callee: "rts.natives.add_mixed",
        doc: "Operador `+` com coerção: string+qualquer=concat, número+número=soma.",
        ts_signature: "add_mixed(a: u64, b: u64): u64",
    },
    NamespaceMember {
        name: "eq_loose",
        callee: "rts.natives.eq_loose",
        doc: "Igualdade fraca `==` com coerção de tipos JS.",
        ts_signature: "eq_loose(a: u64, b: u64): bool",
    },
    NamespaceMember {
        name: "compare",
        callee: "rts.natives.compare",
        doc: "Comparação com coerção JS, retorna -1, 0 ou 1.",
        ts_signature: "compare(a: u64, b: u64): i64",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "rts",
    doc: "Primitivas brutas de máquina: memória, escopo, funções e constantes. \
          Rust expõe apenas tipos de máquina (i64, f64, u64, bool) — sem semântica JS.",
    members: MEMBERS,
    ts_prelude: &[],
};

pub const NATIVES_SPEC: NamespaceSpec = NamespaceSpec {
    name: "rts.natives",
    doc: "Extensões C nativas para coerção de tipos mistos. \
          Injetadas pelo HIR quando operandos têm tipos incompatíveis.",
    members: NATIVES_MEMBERS,
    ts_prelude: &[],
};

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    functions::dispatch(callee, args)
        .or_else(|| scope::dispatch(callee, args))
        .or_else(|| constants::dispatch(callee, args))
        .or_else(|| memory::dispatch(callee, args))
        .or_else(|| natives::dispatch(callee, args))
}
