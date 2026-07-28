//! String-producing ABI for the GC namespace — remaining `rts-std` slice.
//!
//! Most of this file's original 39 `#[no_mangle]` symbols moved down into
//! `rts-engine` (`heap::string_pool`, 2026-07-28): they only ever touched
//! `super::handles` (itself `pub use rts_engine::heap::handles`), so they sat
//! above the layer they actually needed for no reason. Re-exported below so
//! every existing consumer naming `gc::string_pool::*` keeps resolving.
//!
//! What's LEFT here are the three functions that genuinely need something
//! `rts-engine` cannot depend on:
//! - `__RTS_FN_RT_SPREAD_INTO_VEC` — Set-kind spread needs
//!   `rts_shared::collections::map` (Set/Map storage introspection), and
//!   generator-lazy spread needs the `rts-std` sibling
//!   `collector::generator::GEN_SM_DRAIN`.
//! - `__RTS_FN_RT_OBJECT_TO_STRING` / `__RTS_FN_RT_INSPECT` (+ their private
//!   `inspect_handle`/`inspect_slot` helpers) — both need
//!   `rts_shared::collections::map` (Map-vs-Set tagging) and
//!   `rts_primitives::object::is_null_proto_handle` (`Object.create(null)`
//!   tracking) to render `Map(N) {...}` / `Set(N) {...}` / `[Object: null
//!   prototype] {...}` correctly.
//!
//! `rts-engine` sits below `rts-shared`/`rts-primitives` in the crate graph
//! (`rts-engine <- rts-primitives + rts-shared <- rts-std`), so these three
//! cannot move down without an upward (cyclic) dependency.

use super::handles::{Entry, with_entry, with_entry_mut};
pub use rts_engine::heap::string_pool::*;

/// Spread universal: copia elementos de `src` para o Vec `dst`.
/// Detecta tipo de Entry e itera apropriadamente:
/// - Entry::Vec -> push de cada slot
/// - Entry::String -> push de cada char (handle de string char)
/// - Entry::Map -> push de cada value (ordem do IndexMap)
/// - outros -> no-op
///
/// Usado por `[...x]` no codegen quando `x` pode ser string/array.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_SPREAD_INTO_VEC(dst: u64, src: u64) {
    if dst == 0 || src == 0 {
        return;
    }
    enum Snap {
        Vec(Vec<i64>),
        Str(Vec<u8>),
        Map(Vec<i64>),
        Empty,
    }
    // (cross-runtime #316) Set spread (`[...set]`) itera os ELEMENTOS, que no
    // storage interno sao as KEYS do Map<keyStr,1> — nao os values (dummy 1).
    // Sem isto `[...new Set([1,2,3])]` virava `[1,1,1]`. Reusa a mesma
    // conversao key->valor de MAP_VALUES (parse int, senao handle string).
    if rts_shared::collections::map::handle_is_set_kind(src) {
        let elems = rts_shared::collections::map::__RTS_FN_NS_COLLECTIONS_MAP_VALUES(src);
        let items = with_entry(elems, |entry| match entry {
            Some(Entry::Vec(slots)) => slots.as_ref().clone(),
            _ => Vec::new(),
        });
        for v in items {
            push_vec_slot(dst, v);
        }
        return;
    }
    // (#477) Generator lazy (state-machine): drena ate done num Vec, depois
    // spread normal. Para generator infinito o spread roda pra sempre — igual JS.
    if with_entry(src, |e| matches!(e, Some(Entry::GenState(_)))) {
        let drained = crate::collector::generator::__RTS_FN_NS_GC_GEN_SM_DRAIN(src);
        let items = with_entry(drained, |entry| match entry {
            Some(Entry::Vec(slots)) => slots.as_ref().clone(),
            _ => Vec::new(),
        });
        for v in items {
            push_vec_slot(dst, v);
        }
        return;
    }
    let snap = with_entry(src, |entry| match entry {
        Some(Entry::Vec(slots)) => Snap::Vec(slots.as_ref().clone()),
        Some(Entry::String(b)) => Snap::Str(b.clone()),
        Some(Entry::Map(m)) => Snap::Map(m.values().copied().collect()),
        _ => Snap::Empty,
    });
    match snap {
        Snap::Vec(items) => {
            for v in items {
                push_vec_slot(dst, v);
            }
        }
        Snap::Str(bytes) => {
            // Iter por chars Unicode (string spread JS itera codepoints).
            let s = String::from_utf8_lossy(&bytes);
            for ch in s.chars() {
                let mut buf = [0u8; 4];
                let ch_bytes = ch.encode_utf8(&mut buf).as_bytes().to_vec();
                let h = rts_engine::heap::string_pool::__RTS_FN_NS_GC_STRING_NEW(
                    ch_bytes.as_ptr(),
                    ch_bytes.len() as i64,
                );
                push_vec_slot(dst, h as i64);
            }
        }
        Snap::Map(vals) => {
            for v in vals {
                push_vec_slot(dst, v);
            }
        }
        Snap::Empty => {}
    }
}

fn push_vec_slot(dst: u64, value: i64) {
    with_entry_mut(dst, |entry| {
        if let Some(Entry::Vec(slots)) = entry {
            slots.push(value);
        }
    });
}

/// `Object.prototype.toString.call(x)` — retorna "[object Type]".
///
/// Tag eh fornecida pelo codegen baseado no tipo estatico:
///   0 = ambiguo (inspect runtime entry); decide via Entry.
///   1 = Number, 2 = String, 3 = Boolean, 4 = Null, 5 = Undefined,
///   6 = Function
///
/// Para tag=0, inspeciona Entry: Vec->Array, Map->Object, Date->Date,
/// Regex->RegExp, Function->Function, etc.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_OBJECT_TO_STRING(value: i64, tag: i64) -> u64 {
    let kind: &str = match tag {
        1 => "Number",
        2 => "String",
        3 => "Boolean",
        4 => "Null",
        5 => "Undefined",
        6 => "Function",
        _ => {
            // tag=0: detecta via Entry
            if value == 0 {
                "Null"
            } else {
                let h = value as u64;
                with_entry(h, |e| match e {
                    Some(Entry::Vec(_)) => "Array",
                    Some(Entry::Map(_)) => {
                        if rts_shared::collections::map::handle_is_set_kind(h) {
                            "Set"
                        } else if rts_shared::collections::map::handle_is_map_kind(h) {
                            "Map"
                        } else {
                            "Object"
                        }
                    }
                    Some(Entry::Rtse { class, .. }) if *class == "Date" => "Date",
                    Some(Entry::Regex(_)) => "RegExp",
                    Some(Entry::Function(_)) => "Function",
                    Some(Entry::String(b)) => {
                        if b.as_slice() == b"undefined" {
                            "Undefined"
                        } else {
                            "String"
                        }
                    }
                    Some(_) => "Object",
                    None => "Null",
                })
            }
        }
    };
    let s = format!("[object {}]", kind);
    rts_engine::heap::handles::alloc_entry(Entry::String(s.into_bytes()))
}

/// Inspect/pretty-print no estilo Node/Bun para `console.log` — arrays
/// viram `[ 1, 2, 'a' ]`, objetos `{ k: v }`, strings TOP-LEVEL sem
/// aspas (strings DENTRO de array/object recebem aspas simples).
///
/// String top-level retorna o handle original (passthrough), igual ao
/// TPL_COERCE_AUTO, preservando `console.log("oi")` -> `oi`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_INSPECT(value: i64) -> u64 {
    use rts_engine::heap::handles::alloc_entry;
    use rts_engine::heap::string_pool::{EntrySnap, format_js_number, snapshot_entry};
    // Sentinelas: false/true/undefined/null/sparse-hole (consistente
    // com TPL_COERCE_AUTO). Sem isso console.log(opt_chain_undef) -> "null".
    if value == i64::MIN { return alloc_entry(Entry::String(b"false".to_vec())); }
    if value == i64::MIN + 1 { return alloc_entry(Entry::String(b"true".to_vec())); }
    if value == i64::MIN + 2 || value == i64::MIN + 4 {
        return alloc_entry(Entry::String(b"undefined".to_vec()));
    }
    if value == i64::MIN + 3 {
        return alloc_entry(Entry::String(b"null".to_vec()));
    }
    if value == 0 {
        return alloc_entry(Entry::String(b"null".to_vec()));
    }
    let h = value as u64;
    // (narrow-storage) float primitivo boxed → formata como número.
    if let Some(s) = with_entry(h, |e| match e {
        Some(Entry::FloatPrim(f)) => Some(format_js_number(*f)),
        _ => None,
    }) {
        return alloc_entry(Entry::String(s.into_bytes()));
    }
    let snap = snapshot_entry(h);
    match snap {
        EntrySnap::Str(_) => h,
        EntrySnap::None => alloc_entry(Entry::String(value.to_string().into_bytes())),
        _ => {
            let s = inspect_handle(h, 0);
            alloc_entry(Entry::String(s.into_bytes()))
        }
    }
}

const INSPECT_MAX_DEPTH: usize = 6;

fn inspect_handle(h: u64, depth: usize) -> String {
    use rts_engine::heap::string_pool::{entry_kind_name, format_js_number};
    if depth >= INSPECT_MAX_DEPTH {
        return "[Object]".to_string();
    }
    // (narrow-storage) float primitivo boxed → formata como número (sem aspas).
    if let Some(s) = with_entry(h, |e| match e {
        Some(Entry::FloatPrim(f)) => Some(format_js_number(*f)),
        _ => None,
    }) {
        return s;
    }
    enum R {
        Str(Vec<u8>),
        Vec(Vec<i64>),
        Map(Vec<(Vec<u8>, i64)>),
        Json(String),
        Other(&'static str),
        None,
    }
    let r = with_entry(h, |e| match e {
        Some(Entry::String(s)) => R::Str(s.clone()),
        Some(Entry::Vec(v)) => R::Vec((**v).clone()),
        Some(Entry::Map(m)) => R::Map(
            m.iter().map(|(k, v)| (k.as_bytes().to_vec(), *v)).collect(),
        ),
        Some(Entry::Json(j)) => R::Json(j.to_string()),
        Some(other) => R::Other(entry_kind_name(other)),
        None => R::None,
    });
    match r {
        // (PR #1209) Bun/Node usam aspas duplas em inspect de strings dentro
        // de arrays/objects (Node usa simples por default, Bun duplas; RTS
        // segue Bun pela maior parte das fixtures cross-runtime usarem
        // Bun como referencia).
        R::Str(b) => format!("\"{}\"", String::from_utf8_lossy(&b)),
        R::Vec(slots) => {
            if slots.is_empty() {
                return "[]".to_string();
            }
            let parts: Vec<String> =
                slots.iter().map(|x| inspect_slot(*x, depth + 1)).collect();
            format!("[ {} ]", parts.join(", "))
        }
        R::Map(entries) => {
            // (#1080) Object.create(null) — handle marcado em null_proto_set
            // (preserva mesmo se user setar __proto__ depois) ou slot
            // __proto__ existe com valor 0. Node/Bun imprimem como
            // `[Object: null prototype] {...}`.
            let is_null_proto = rts_primitives::object::is_null_proto_handle(h)
                || entries
                    .iter()
                    .any(|(k, v)| k == b"__proto__" && *v == 0);
            // (PR #1214) Map/Set instances — Bun/Node imprimem como `Map(N) { k: v, ... }`
            // e `Set(N) { v1, v2, ... }`. RTS armazena Map JS como Entry::Map
            // tagged em set_kind_set/map_kind_set (separado de obj literal Map).
            let is_map_kind = rts_shared::collections::map::handle_is_map_kind(h);
            let is_set_kind = rts_shared::collections::map::handle_is_set_kind(h);
            // Filtra slots internos das entries impressas.
            let visible: Vec<&(Vec<u8>, i64)> = entries
                .iter()
                .filter(|(k, _)| k != b"__proto__" && k != b"__rts_class")
                .collect();
            if is_set_kind {
                // Set: em RTS, item.key eh a forma stringified do valor
                // (via STRING_FROM_I64/F64) e item.value eh sentinel 1.
                // Pra inspect, mostramos as keys (raw values), nao value=1.
                // Para keys numericas, parsea de volta como number; senao
                // mostra entre aspas (string).
                if visible.is_empty() {
                    return "Set(0) {}".to_string();
                }
                let n = visible.len();
                let parts: Vec<String> = visible
                    .iter()
                    .map(|(k, _)| {
                        let s = String::from_utf8_lossy(k);
                        // Tenta number (i64 ou f64); senao mostra como string.
                        if let Ok(_) = s.parse::<i64>() {
                            s.to_string()
                        } else if let Ok(f) = s.parse::<f64>() {
                            format_js_number(f)
                        } else if s == "true" || s == "false" || s == "null" || s == "undefined" {
                            s.to_string()
                        } else {
                            format!("\"{}\"", s)
                        }
                    })
                    .collect();
                return format!("Set({}) {{ {} }}", n, parts.join(", "));
            }
            if is_map_kind {
                if visible.is_empty() {
                    return "Map(0) {}".to_string();
                }
                let n = visible.len();
                let parts: Vec<String> = visible
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "\"{}\" => {}",
                            String::from_utf8_lossy(k),
                            inspect_slot(*v, depth + 1)
                        )
                    })
                    .collect();
                return format!("Map({}) {{ {} }}", n, parts.join(", "));
            }
            let body = if visible.is_empty() {
                "{}".to_string()
            } else {
                let parts: Vec<String> = visible
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "{}: {}",
                            String::from_utf8_lossy(k),
                            inspect_slot(*v, depth + 1)
                        )
                    })
                    .collect();
                format!("{{ {} }}", parts.join(", "))
            };
            if is_null_proto {
                format!("[Object: null prototype] {body}")
            } else {
                body
            }
        }
        R::Json(s) => s,
        R::Other(name) => format!("[object {}]", name),
        R::None => String::new(),
    }
}

/// Slot cru de Vec/Map: < 2^48 = numero JS; >= 2^48 e handle valido =
/// inspect recursivo; senao numero.
fn inspect_slot(raw: i64, depth: usize) -> String {
    use rts_engine::heap::string_pool::format_js_number;
    // Sentinelas JS em slot de Vec/Map (gerados pelo codegen):
    //   MIN     = false
    //   MIN+1   = true
    //   MIN+2   = undefined
    //   MIN+3   = null
    //   MIN+4   = sparse hole (renderiza como undefined isolado)
    if raw == i64::MIN { return "false".to_string(); }
    if raw == i64::MIN + 1 { return "true".to_string(); }
    if raw == i64::MIN + 2 || raw == i64::MIN + 4 { return "undefined".to_string(); }
    if raw == i64::MIN + 3 { return "null".to_string(); }
    let h = raw as u64;
    if h < (1u64 << 48) {
        return format_js_number(raw as f64);
    }
    let exists = with_entry(h, |e| e.is_some());
    if !exists {
        return format_js_number(raw as f64);
    }
    inspect_handle(h, depth)
}
