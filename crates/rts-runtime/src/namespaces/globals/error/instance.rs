//! Error global class family — constructor and instance method implementations.
//!
//! Each Error subtype (`TypeError`, `RangeError`, etc.) stores `Entry::ErrorObj`
//! with `name` set to the appropriate class name. All instance methods are
//! shared (same symbol `__RTS_FN_GL_ERROR_*`).

use crate::namespaces::gc::handles::{alloc_entry, with_entry, Entry};

// ── Helper ────────────────────────────────────────────────────────────────────

unsafe fn str_from_raw(ptr: i64, len: i64) -> String {
    if ptr == 0 || len <= 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    std::str::from_utf8(bytes).unwrap_or("").to_owned()
}

fn alloc_error_with_cause(name: &str, message: String, cause: u64) -> u64 {
    alloc_entry(Entry::ErrorObj {
        message,
        name: name.to_owned(),
        cause,
    })
}

fn get_field(handle: u64, field: &str) -> String {
    // Tenta primeiro Entry::ErrorObj (caso comum: new Error("msg"))
    let direct = with_entry(handle, |entry| match entry {
        Some(Entry::ErrorObj { message, name, .. }) => Some(match field {
            "message" => message.clone(),
            "name" => name.clone(),
            _ => String::new(),
        }),
        _ => None,
    });
    if let Some(s) = direct {
        return s;
    }
    // Fallback: Entry::Map com keys "message"/"name" — usado quando
    // user class extends Error e super(msg) armazena no Map.
    let slot_handle: u64 = with_entry(handle, |entry| match entry {
        Some(Entry::Map(m)) => m.get(field).copied().unwrap_or(0) as u64,
        _ => 0,
    });
    if slot_handle == 0 {
        return String::new();
    }
    with_entry(slot_handle, |entry| match entry {
        Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
        _ => String::new(),
    })
}

fn alloc_str(s: String) -> u64 {
    alloc_entry(Entry::String(s.into_bytes()))
}

// ── Constructors ──────────────────────────────────────────────────────────────

/// (cross-runtime #277) Extrai handle de `options.cause` se options for
/// um Map com chave "cause". Retorna 0 se ausente.
fn extract_cause(options: u64) -> u64 {
    if options == 0 {
        return 0;
    }
    with_entry(options, |e| match e {
        Some(Entry::Map(m)) => m.get("cause").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ERROR_NEW(ptr: i64, len: i64, options: u64) -> u64 {
    let msg = unsafe { str_from_raw(ptr, len) };
    alloc_error_with_cause("Error", msg, extract_cause(options))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_TYPE_ERROR_NEW(ptr: i64, len: i64, options: u64) -> u64 {
    let msg = unsafe { str_from_raw(ptr, len) };
    alloc_error_with_cause("TypeError", msg, extract_cause(options))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_RANGE_ERROR_NEW(ptr: i64, len: i64, options: u64) -> u64 {
    let msg = unsafe { str_from_raw(ptr, len) };
    alloc_error_with_cause("RangeError", msg, extract_cause(options))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REF_ERROR_NEW(ptr: i64, len: i64, options: u64) -> u64 {
    let msg = unsafe { str_from_raw(ptr, len) };
    alloc_error_with_cause("ReferenceError", msg, extract_cause(options))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_SYNTAX_ERROR_NEW(ptr: i64, len: i64, options: u64) -> u64 {
    let msg = unsafe { str_from_raw(ptr, len) };
    alloc_error_with_cause("SyntaxError", msg, extract_cause(options))
}

/// (cross-runtime #277) `error.cause` — retorna handle armazenado ou 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ERROR_CAUSE(handle: u64) -> u64 {
    with_entry(handle, |e| match e {
        Some(Entry::ErrorObj { cause, .. }) => *cause,
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_URI_ERROR_NEW(ptr: i64, len: i64, options: u64) -> u64 {
    let msg = unsafe { str_from_raw(ptr, len) };
    alloc_error_with_cause("URIError", msg, extract_cause(options))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EVAL_ERROR_NEW(ptr: i64, len: i64, options: u64) -> u64 {
    let msg = unsafe { str_from_raw(ptr, len) };
    alloc_error_with_cause("EvalError", msg, extract_cause(options))
}

/// (cross-runtime #748) `new AggregateError(errors, message?)` — errors
/// e' handle de Vec<i64> contendo os erros aggregados. Armazenamos o
/// handle de errors no slot `cause` da ErrorObj (single i64 disponivel
/// sem refactor de Entry); `.errors` lê esse slot.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_AGGREGATE_ERROR_NEW(errors: u64, ptr: i64, len: i64) -> u64 {
    let msg = unsafe { str_from_raw(ptr, len) };
    alloc_error_with_cause("AggregateError", msg, errors)
}

/// `agg.errors` — retorna o handle do Vec passado no construtor.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_AGGREGATE_ERROR_ERRORS(handle: u64) -> u64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::ErrorObj { cause, .. }) => *cause,
        _ => 0,
    })
}

// ── Instance methods ──────────────────────────────────────────────────────────

/// `instanceof Error` (any subtype) — handle aponta para Entry::ErrorObj
/// OU Entry::Map com slots "name"+"message" (subclass user que estende
/// Error/TypeError/etc via super()).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_IS_ERROR(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::ErrorObj { .. }) => 1,
        Some(Entry::Map(m)) => {
            // Heuristica: ambos "name" e "message" como string handles
            // indica que a instancia veio de super(msg) em user class.
            if m.contains_key("name") && m.contains_key("message") {
                1
            } else {
                0
            }
        }
        _ => 0,
    })
}

/// `instanceof TypeError` etc. — checa name field exato. Aceita tambem
/// (#663/47) Entry::Map com slot "name" string handle == want — cobre
/// `class CustomTypeError extends TypeError` onde super(msg) faz a
/// instancia user (Map) carregar o name "TypeError" via super constructor.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_IS_ERROR_NAMED(handle: u64, name_ptr: i64, name_len: i64) -> i64 {
    if name_ptr == 0 || name_len <= 0 {
        return 0;
    }
    let want = unsafe { std::slice::from_raw_parts(name_ptr as *const u8, name_len as usize) };
    let want_s = match std::str::from_utf8(want) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    // (cross-runtime #47) Snapshot da string do name DENTRO do mesmo
    // outer with_entry — evita race com GC entre lookups (se err virou
    // unreachable, o name handle pode ser reciclado antes do segundo
    // lookup). Tambem cobre deadlock potencial quando handle e name_h
    // estao no mesmo shard.
    enum Kind {
        ErrorMatch(bool),
        MapName(u64),
        MapNameStr(Vec<u8>),
        NotError,
    }
    let kind = with_entry(handle, |entry| match entry {
        Some(Entry::ErrorObj { name, .. }) => Kind::ErrorMatch(name == want_s),
        Some(Entry::Map(m)) => {
            let name_h = m.get("name").copied().unwrap_or(0) as u64;
            Kind::MapName(name_h)
        }
        _ => Kind::NotError,
    });
    let kind = match kind {
        Kind::MapName(name_h) if name_h != 0 => {
            // Snapshot a string fora do shard do `handle` — usa shard
            // diferente se aplicavel. Se name handle morreu (GC race),
            // tenta tambem cadeia __proto__ ou __rts_class.
            let snap = with_entry(name_h, |e| match e {
                Some(Entry::String(b)) => Some(b.clone()),
                _ => None,
            });
            match snap {
                Some(b) => Kind::MapNameStr(b),
                None => Kind::MapName(name_h),
            }
        }
        other => other,
    };
    match kind {
        Kind::ErrorMatch(b) => {
            if b {
                1
            } else {
                0
            }
        }
        Kind::MapNameStr(b) => {
            if b.as_slice() == want_s.as_bytes() {
                return 1;
            }
            // (cross-runtime #47) Walk class hierarchy via registry:
            // CustomTypeError extends TypeError → instanceof TypeError = true.
            let cur = match std::str::from_utf8(&b) {
                Ok(s) => s,
                Err(_) => return 0,
            };
            if crate::namespaces::gc::class_registry::is_descendant_of(cur, want_s) {
                return 1;
            }
            // Tambem checa __rts_class do Map (class name original) — pode
            // ser diferente de name (que vem do super constructor).
            let class_name = with_entry(handle, |entry| match entry {
                Some(Entry::Map(m)) => {
                    let class_h = m.get("__rts_class").copied().unwrap_or(0) as u64;
                    if class_h == 0 {
                        None
                    } else {
                        Some(class_h)
                    }
                }
                _ => None,
            });
            if let Some(class_h) = class_name {
                let class_str = with_entry(class_h, |e| match e {
                    Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
                    _ => None,
                });
                if let Some(c) = class_str {
                    if crate::namespaces::gc::class_registry::is_descendant_of(&c, want_s) {
                        return 1;
                    }
                }
            }
            0
        }
        Kind::MapName(_) | Kind::NotError => {
            // (cross-runtime #47) name field invalido (GC race ou nao
            // setado). Fallback: checa __rts_class do Map via registry.
            let class_h: u64 = with_entry(handle, |entry| match entry {
                Some(Entry::Map(m)) => m.get("__rts_class").copied().unwrap_or(0) as u64,
                _ => 0,
            });
            if class_h != 0 {
                let class_str = with_entry(class_h, |e| match e {
                    Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
                    _ => None,
                });
                if let Some(c) = class_str {
                    if crate::namespaces::gc::class_registry::is_descendant_of(&c, want_s) {
                        return 1;
                    }
                }
            }
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ERROR_MESSAGE(handle: u64) -> u64 {
    alloc_str(get_field(handle, "message"))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ERROR_NAME(handle: u64) -> u64 {
    alloc_str(get_field(handle, "name"))
}

/// (#745) `e.stack` — JS spec: string com call stack. RTS captura stack
/// no momento de ERROR_SET via trace::frame_stack — se houver stack
/// pendente, prefixa header "<name>: <msg>" + frames. Caso contrario,
/// retorna apenas o header (typeof string ainda eh true).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ERROR_STACK(handle: u64) -> u64 {
    let name = get_field(handle, "name");
    let msg = get_field(handle, "message");
    let header = if msg.is_empty() {
        name
    } else {
        format!("{name}: {msg}")
    };
    // (#745) Stack salvo pelo ERROR_CLEAR (ver gc/error.rs ERR_STACKS).
    let stack_text: String =
        crate::namespaces::gc::error::stack_for_handle(handle).unwrap_or_default();
    let s = if stack_text.is_empty() {
        header
    } else {
        format!("{header}\n{stack_text}")
    };
    alloc_str(s)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ERROR_TO_STRING(handle: u64) -> u64 {
    let name = get_field(handle, "name");
    let msg = get_field(handle, "message");
    let s = if msg.is_empty() {
        name
    } else {
        format!("{name}: {msg}")
    };
    alloc_str(s)
}

/// `Error.captureStackTrace(target, ctor?)` — no-op. V8/Node populariam
/// `target.stack`; RTS nao mantem stack traces de erro (formato diverge por
/// design). Aceita os dois handles e nao faz nada.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_ERROR_CAPTURE_STACK_TRACE(_target: u64, _ctor: u64) {}
