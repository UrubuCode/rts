mod constants;
pub mod debug;
pub(crate) mod eval;
mod functions;
pub mod hotops;
mod memory;
pub mod natives;
mod scope;

use crate::namespaces::value::RuntimeValue;
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

pub const DEBUG_MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "load_metadata",
        callee: "rts.debug.load_metadata",
        doc: "Carrega arquivo .ometa, retorna handle numérico.",
        ts_signature: "load_metadata(path_ptr: u64): u64",
    },
    NamespaceMember {
        name: "resolve_location",
        callee: "rts.debug.resolve_location",
        doc: "Resolve offset de PC para localização no arquivo fonte.",
        ts_signature: "resolve_location(handle: u64, pc_offset: u64): str",
    },
    NamespaceMember {
        name: "format_error",
        callee: "rts.debug.format_error",
        doc: "Formata mensagem de erro com localização fonte (modo dev).",
        ts_signature: "format_error(message_ptr: u64, pc_offset: u64): str",
    },
];

pub const DEBUG_SPEC: NamespaceSpec = NamespaceSpec {
    name: "rts.debug",
    doc: "Debug info em runtime: carrega .ometa, resolve PC → source location, formata erros.",
    members: DEBUG_MEMBERS,
    ts_prelude: &[],
};

pub const HOTOPS_MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "i64_sub",
        callee: "rts.hotops.i64_sub",
        doc: "Subtração i64.",
        ts_signature: "i64_sub(a: i64, b: i64): i64",
    },
    NamespaceMember {
        name: "i64_div",
        callee: "rts.hotops.i64_div",
        doc: "Divisão i64.",
        ts_signature: "i64_div(a: i64, b: i64): i64",
    },
    NamespaceMember {
        name: "i64_mod",
        callee: "rts.hotops.i64_mod",
        doc: "Módulo i64.",
        ts_signature: "i64_mod(a: i64, b: i64): i64",
    },
    NamespaceMember {
        name: "i64_eq",
        callee: "rts.hotops.i64_eq",
        doc: "Igualdade i64.",
        ts_signature: "i64_eq(a: i64, b: i64): bool",
    },
    NamespaceMember {
        name: "i64_lt",
        callee: "rts.hotops.i64_lt",
        doc: "Menor que i64.",
        ts_signature: "i64_lt(a: i64, b: i64): bool",
    },
    NamespaceMember {
        name: "i64_le",
        callee: "rts.hotops.i64_le",
        doc: "Menor ou igual i64.",
        ts_signature: "i64_le(a: i64, b: i64): bool",
    },
    NamespaceMember {
        name: "f64_add",
        callee: "rts.hotops.f64_add",
        doc: "Adição f64.",
        ts_signature: "f64_add(a: f64, b: f64): f64",
    },
    NamespaceMember {
        name: "f64_sub",
        callee: "rts.hotops.f64_sub",
        doc: "Subtração f64.",
        ts_signature: "f64_sub(a: f64, b: f64): f64",
    },
    NamespaceMember {
        name: "f64_div",
        callee: "rts.hotops.f64_div",
        doc: "Divisão f64.",
        ts_signature: "f64_div(a: f64, b: f64): f64",
    },
    NamespaceMember {
        name: "f64_eq",
        callee: "rts.hotops.f64_eq",
        doc: "Igualdade f64.",
        ts_signature: "f64_eq(a: f64, b: f64): bool",
    },
    NamespaceMember {
        name: "f64_lt",
        callee: "rts.hotops.f64_lt",
        doc: "Menor que f64.",
        ts_signature: "f64_lt(a: f64, b: f64): bool",
    },
    NamespaceMember {
        name: "i64_to_string",
        callee: "rts.hotops.i64_to_string",
        doc: "i64 para string (tabela pré-computada para 0..=255).",
        ts_signature: "i64_to_string(n: i64): u64",
    },
    NamespaceMember {
        name: "f64_to_string",
        callee: "rts.hotops.f64_to_string",
        doc: "f64 para string.",
        ts_signature: "f64_to_string(n: f64): u64",
    },
];

pub const HOTOPS_SPEC: NamespaceSpec = NamespaceSpec {
    name: "rts.hotops",
    doc: "Operações inline com tipos já conhecidos pelo MIR. \
          Sem overhead de coerção — tipos são garantidos pelo compilador.",
    members: HOTOPS_MEMBERS,
    ts_prelude: &[],
};

pub const NATIVES_SPEC: NamespaceSpec = NamespaceSpec {
    name: "rts.natives",
    doc: "Extensões C nativas para coerção de tipos mistos. \
          Injetadas pelo HIR quando operandos têm tipos incompatíveis.",
    members: NATIVES_MEMBERS,
    ts_prelude: &[],
};

pub(crate) fn eval_runtime_expression(expression: &str) -> RuntimeValue {
    eval::eval_expression_text(expression)
}

pub(crate) fn dispatch_runtime_call(
    callee: &str,
    args: &[RuntimeValue],
) -> Option<DispatchOutcome> {
    if callee.starts_with("rts.") {
        return dispatch(callee, args);
    }
    crate::namespaces::dispatch(callee, args)
}

pub fn dispatch(callee: &str, args: &[RuntimeValue]) -> Option<DispatchOutcome> {
    functions::dispatch(callee, args)
        .or_else(|| scope::dispatch(callee, args))
        .or_else(|| constants::dispatch(callee, args))
        .or_else(|| memory::dispatch(callee, args))
        .or_else(|| natives::dispatch(callee, args))
        .or_else(|| hotops::dispatch(callee, args))
        .or_else(|| debug::dispatch(callee, args))
}

// Implementações internas — chamadas via __rts_dispatch no launcher, sem exportação C.

pub(crate) fn rts_io_print(handle: i64) -> i64 {
    if crate::namespaces::abi::dispatch_debug_enabled() {
        eprintln!("[dbg io.print] handle={handle}");
    }
    let message = crate::namespaces::abi::read_runtime_value(handle).to_runtime_string();
    println!("{message}");
    crate::namespaces::abi::undefined_handle()
}

pub(crate) fn rts_io_stdout_write(handle: i64) -> i64 {
    let message = crate::namespaces::abi::read_runtime_value(handle).to_runtime_string();
    print!("{message}");
    crate::namespaces::abi::undefined_handle()
}

pub(crate) fn rts_io_stderr_write(handle: i64) -> i64 {
    let message = crate::namespaces::abi::read_runtime_value(handle).to_runtime_string();
    eprint!("{message}");
    crate::namespaces::abi::undefined_handle()
}

pub(crate) fn rts_io_panic(handle: i64) -> i64 {
    let message = if handle == crate::namespaces::abi::undefined_handle() {
        "runtime panic".to_string()
    } else {
        crate::namespaces::abi::read_runtime_value(handle).to_runtime_string()
    };
    eprintln!("RTS runtime panic: {message}");
    std::process::exit(1);
}

pub(crate) fn rts_crypto_sha256(handle: i64) -> i64 {
    let input = crate::namespaces::abi::read_runtime_value(handle).to_runtime_string();
    let digest = crate::namespaces::crypto::hash_sha256(&input);
    crate::namespaces::abi::push_runtime_value(RuntimeValue::String(digest))
}

pub(crate) fn rts_process_exit(code: i64) -> i64 {
    std::process::exit(code as i32);
}

pub(crate) fn rts_global_set(key_handle: i64, value_handle: i64) -> i64 {
    let key = crate::namespaces::abi::read_runtime_value(key_handle).to_runtime_string();
    let value = crate::namespaces::abi::read_runtime_value(value_handle);
    crate::namespaces::globals::set(&key, value);
    crate::namespaces::abi::undefined_handle()
}

pub(crate) fn rts_global_get(key_handle: i64) -> i64 {
    let key = crate::namespaces::abi::read_runtime_value(key_handle).to_runtime_string();
    match crate::namespaces::globals::get(&key) {
        Some(value) => crate::namespaces::abi::push_runtime_value(value),
        None => crate::namespaces::abi::undefined_handle(),
    }
}

pub(crate) fn rts_global_has(key_handle: i64) -> i64 {
    let key = crate::namespaces::abi::read_runtime_value(key_handle).to_runtime_string();
    let exists = crate::namespaces::globals::has(&key);
    crate::namespaces::abi::push_runtime_value(RuntimeValue::Bool(exists))
}

pub(crate) fn rts_global_delete(key_handle: i64) -> i64 {
    let key = crate::namespaces::abi::read_runtime_value(key_handle).to_runtime_string();
    let removed = crate::namespaces::globals::delete(&key);
    crate::namespaces::abi::push_runtime_value(RuntimeValue::Bool(removed))
}

#[cfg(test)]
mod tests {
    use super::dispatch_runtime_call;
    use crate::namespaces::DispatchOutcome;
    use crate::namespaces::value::RuntimeValue;

    #[test]
    fn dispatch_runtime_call_prefers_rust_machine_namespace() {
        let args = vec![RuntimeValue::Number(2.0), RuntimeValue::Number(3.0)];
        let outcome = dispatch_runtime_call("rts.i64_add", &args).expect("must dispatch rts call");
        match outcome {
            DispatchOutcome::Value(RuntimeValue::Number(value)) => assert_eq!(value, 5.0),
            other => panic!("unexpected dispatch outcome: {other:?}"),
        }
    }

    #[test]
    fn dispatch_runtime_call_keeps_non_rts_namespaces() {
        let args = vec![RuntimeValue::String("ok".to_string())];
        let outcome = dispatch_runtime_call("io.print", &args).expect("must dispatch io call");
        match outcome {
            DispatchOutcome::Emit(message) => assert_eq!(message, "ok"),
            other => panic!("unexpected dispatch outcome: {other:?}"),
        }
    }
}
