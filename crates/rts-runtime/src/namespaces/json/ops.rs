//! Implementacao das ops do namespace `json`.
//!
//! Usa `serde_json::Value` armazenado via `Entry::Json` no HandleTable.
//! `parse` constroi a arvore inteira de uma vez; `as_*` e `*_get` extraem
//! valores conforme demanda. `array_get` / `object_get` clonam o valor
//! filho num novo handle (semantica simples — leitura nao modifica
//! parent, e free do filho nao quebra parent).

use serde_json::Value;

use super::super::gc::handles::{alloc_entry, free_handle, with_entry, Entry};

fn slice_from(ptr: u64, len: i64) -> Option<&'static [u8]> {
    if ptr == 0 || len < 0 {
        return None;
    }
    // SAFETY: o codegen entrega pares (ptr, len) provenientes de
    // gc.string_ptr ou de literais estaticos do binario, ambos validos
    // pela vida do programa do ponto de vista do callee.
    Some(unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_PARSE(ptr: u64, len: i64) -> u64 {
    let Some(bytes) = slice_from(ptr, len) else {
        return 0;
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return 0;
    };
    match serde_json::from_str::<Value>(text) {
        Ok(v) => json_value_to_handle(&v),
        Err(_) => 0,
    }
}

/// JSON5.parse — extensao do JSON com comentarios, trailing commas,
/// keys nao-quoteadas, strings com aspas simples, hex literals, NaN/
/// Infinity, etc. Usa o crate `json5` que serializa para
/// `serde_json::Value`, entao a saida segue o mesmo bridge handle-table
/// do JSON estrito (sem semantica adicional fora parse-time).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_PARSE5(ptr: u64, len: i64) -> u64 {
    let Some(bytes) = slice_from(ptr, len) else {
        return 0;
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return 0;
    };
    match json5::from_str::<Value>(text) {
        Ok(v) => json_value_to_handle(&v),
        Err(_) => 0,
    }
}

/// Converte recursivamente um `serde_json::Value` em handle nativo:
/// - Object -> Entry::Map (slots = handles ou i64 raw para scalars)
/// - Array -> Entry::Vec (idem)
/// - String -> Entry::String
/// - Number -> i64 raw (i64 direto ou f64.to_bits dentro do slot pai)
/// - Bool -> i64 raw 0/1
/// - Null -> i64 raw 0
///
/// Acesso JS-style direto (`obj.x`, `arr[0]`) le o slot raw, suficiente
/// pra `number` e `boolean`. APIs legacy (`json.array_get` etc) sintetizam
/// um wrapper Entry::Json on-demand para preservar o tipo escalar.
fn json_value_to_handle(v: &Value) -> u64 {
    match v {
        Value::Null => 0,
        Value::Bool(b) => if *b { 1 } else { 0 },
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i as u64
            } else if let Some(f) = n.as_f64() {
                f.to_bits()
            } else {
                0
            }
        }
        Value::String(s) => alloc_entry(Entry::String(s.clone().into_bytes())),
        Value::Array(arr) => {
            let slots: Vec<i64> = arr.iter().map(|v| json_value_to_handle(v) as i64).collect();
            alloc_entry(Entry::Vec(Box::new(slots)))
        }
        Value::Object(map) => {
            let mut store: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
            for (k, val) in map {
                store.insert(k.clone(), json_value_to_handle(val) as i64);
            }
            alloc_entry(Entry::Map(Box::new(store)))
        }
    }
}

/// Inspeciona um handle JSON tratando todas as representacoes possiveis:
/// Entry::Json (escalar), Entry::Map (object), Entry::Vec (array),
/// Entry::String (string). Devolve `default` quando handle invalido ou tipo
/// desconhecido.
///
/// Importante: para Map/Vec apenas reconstroi a *forma* (vazio/nao-vazio)
/// dentro do lock. APIs estruturais (`array_len`, `array_get`, `object_get`,
/// `object_has`) NAO devem usar `with_json` — usam `with_entry` direto. Isso
/// evita deadlock por reentrancia: `handle_to_json_value` precisa pegar lock
/// de shards filhos, e fazer isso enquanto o lock do pai ainda esta segurado
/// trava se filho e pai estiverem no mesmo shard.
fn with_json<R>(handle: u64, default: R, f: impl FnOnce(&Value) -> R) -> R {
    // Snapshot do tipo (sem subgrafo) sob o lock; libera; depois invoca f.
    enum Snapshot {
        Json(Value),
        StringV(String),
        ArrayLen(usize),
        ObjectLen(usize),
        Missing,
    }
    let snap = with_entry(handle, |entry| match entry {
        Some(Entry::Json(v)) => Snapshot::Json(v.as_ref().clone()),
        Some(Entry::String(b)) => Snapshot::StringV(String::from_utf8_lossy(b).into_owned()),
        Some(Entry::Vec(slots)) => Snapshot::ArrayLen(slots.len()),
        Some(Entry::Map(map)) => Snapshot::ObjectLen(map.len()),
        _ => Snapshot::Missing,
    });
    match snap {
        Snapshot::Json(v) => f(&v),
        Snapshot::StringV(s) => f(&Value::String(s)),
        Snapshot::ArrayLen(n) => {
            // Reconstruir Array completo sem lock; usamos Null para slots
            // (suficiente para type_of/as_bool/array_len que so olham forma
            // e tamanho).
            let placeholder = Value::Array(vec![Value::Null; n]);
            f(&placeholder)
        }
        Snapshot::ObjectLen(n) => {
            let mut obj = serde_json::Map::with_capacity(n);
            for i in 0..n {
                obj.insert(format!("__placeholder_{i}"), Value::Null);
            }
            f(&Value::Object(obj))
        }
        Snapshot::Missing => default,
    }
}

/// Serializa um handle de qualquer tipo conhecido (Map/Vec/String/Json) ou
/// devolve "" se nao reconhecido. Valores i64 dentro de Map/Vec sao tratados
/// como numero por padrao — nao ha como saber em runtime se i64 e' handle
/// string/map sem corromper interpretacao numerica.
fn stringify_any_inner(handle: u64) -> Option<String> {
    use std::fmt::Write;
    with_entry(handle, |e| {
        let v = e?;
        let mut out = String::new();
        match v {
            Entry::String(b) => {
                out.push('"');
                for &c in b {
                    match c {
                        b'"' => out.push_str("\\\""),
                        b'\\' => out.push_str("\\\\"),
                        b'\n' => out.push_str("\\n"),
                        b'\r' => out.push_str("\\r"),
                        b'\t' => out.push_str("\\t"),
                        0x00..=0x1f => { let _ = write!(out, "\\u{:04x}", c); }
                        _ => out.push(c as char),
                    }
                }
                out.push('"');
                Some(out)
            }
            Entry::Map(m) => {
                out.push('{');
                let mut first = true;
                for (k, val) in m.iter() {
                    if k == "__proto__" { continue; }
                    if !first { out.push(','); }
                    first = false;
                    out.push('"');
                    for c in k.bytes() {
                        match c {
                            b'"' => out.push_str("\\\""),
                            b'\\' => out.push_str("\\\\"),
                            _ => out.push(c as char),
                        }
                    }
                    out.push_str("\":");
                    out.push_str(&stringify_value_i64(*val));
                }
                out.push('}');
                Some(out)
            }
            Entry::Vec(arr) => {
                out.push('[');
                for (i, val) in arr.iter().enumerate() {
                    if i > 0 { out.push(','); }
                    out.push_str(&stringify_value_i64(*val));
                }
                out.push(']');
                Some(out)
            }
            Entry::Json(j) => serde_json::to_string(j.as_ref()).ok(),
            _ => None,
        }
    })
}

/// Tenta interpretar um i64 dentro de Map/Vec: se o handle apontar para
/// String/Map/Vec/Json, recurse; senao, formata como numero.
fn stringify_value_i64(v: i64) -> String {
    // (cross-runtime #94/#142) Sentinelas de array literal:
    //   MIN     = false, MIN+1 = true (codegen de array de bool)
    //   MIN+2   = undefined, MIN+3 = null (codegen de undefined/null literal)
    // JSON.stringify: undefined/null em array vira "null" (JS spec).
    if v == i64::MIN { return "false".to_string(); }
    if v == i64::MIN + 1 { return "true".to_string(); }
    if v == i64::MIN + 2 || v == i64::MIN + 3 || v == i64::MIN + 4 {
        return "null".to_string();
    }
    let h = v as u64;
    // Heuristica: handles RTS tem bits altos setados (gen+slot). Inteiros
    // pequenos (<= 2^31) tipicamente sao numeros, nao handles.
    if h > 0xFFFF_FFFF {
        if let Some(s) = stringify_any_inner(h) {
            return s;
        }
    }
    v.to_string()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_STRINGIFY(handle: u64) -> u64 {
    if let Some(s) = stringify_any_inner(handle) {
        return alloc_entry(Entry::String(s.into_bytes()));
    }
    // Fallback: handle 0 ou desconhecido vira "null".
    if handle == 0 {
        return alloc_entry(Entry::String(b"null".to_vec()));
    }
    alloc_entry(Entry::String(handle.to_string().into_bytes()))
}

/// `JSON.stringify` typed: o codegen passa um `kind` indicando o tipo
/// estatico do valor para preservar semantica JS (Boolean -> "true"/"false",
/// Number -> formato JS, String -> handle).
///
/// `kind`: 0=i64/handle (caminho legacy), 1=f64 bits, 2=bool, 3=null/undefined.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_STRINGIFY_TYPED(value: i64, kind: i32) -> u64 {
    let s = match kind {
        // Bool: 0/1 -> "false"/"true"
        2 => if value != 0 { "true".to_string() } else { "false".to_string() },
        // null/undefined -> "null"
        3 => "null".to_string(),
        // f64 bits: numero JS-format
        1 => {
            let f = f64::from_bits(value as u64);
            crate::namespaces::gc::string_pool::format_js_number(f)
        }
        // i64/handle: tenta como handle valido; senao formata como numero.
        _ => {
            let h = value as u64;
            if let Some(json) = stringify_any_inner(h) {
                json
            } else if h == 0 {
                "null".to_string()
            } else {
                value.to_string()
            }
        }
    };
    alloc_entry(Entry::String(s.into_bytes()))
}

/// Pretty-printer recursivo para Map/Vec/String/Json com indent.
fn stringify_pretty_inner_str(handle: u64, indent: &str, depth: usize) -> Option<String> {
    use std::fmt::Write;
    let pad_outer = indent.repeat(depth);
    let pad_inner = indent.repeat(depth + 1);
    with_entry(handle, |e| {
        let v = e?;
        let mut out = String::new();
        match v {
            Entry::String(_) => stringify_any_inner(handle),
            Entry::Map(m) => {
                let entries: Vec<(&String, &i64)> = m.iter()
                    .filter(|(k, _)| k.as_str() != "__proto__")
                    .collect();
                if entries.is_empty() {
                    out.push_str("{}");
                    return Some(out);
                }
                out.push_str("{\n");
                for (i, (k, val)) in entries.iter().enumerate() {
                    out.push_str(&pad_inner);
                    out.push('"');
                    for c in k.bytes() {
                        match c {
                            b'"' => out.push_str("\\\""),
                            b'\\' => out.push_str("\\\\"),
                            _ => out.push(c as char),
                        }
                    }
                    out.push_str("\": ");
                    out.push_str(&stringify_pretty_value_i64_str(**val, indent, depth + 1));
                    if i + 1 < entries.len() { out.push(','); }
                    out.push('\n');
                }
                out.push_str(&pad_outer);
                out.push('}');
                Some(out)
            }
            Entry::Vec(arr) => {
                if arr.is_empty() {
                    out.push_str("[]");
                    return Some(out);
                }
                out.push_str("[\n");
                for (i, val) in arr.iter().enumerate() {
                    out.push_str(&pad_inner);
                    out.push_str(&stringify_pretty_value_i64_str(*val, indent, depth + 1));
                    if i + 1 < arr.len() { out.push(','); }
                    out.push('\n');
                }
                out.push_str(&pad_outer);
                out.push(']');
                Some(out)
            }
            Entry::Json(j) => {
                let mut buf = Vec::with_capacity(64);
                let formatter = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
                let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
                serde::Serialize::serialize(j.as_ref(), &mut ser).ok()?;
                let _ = write!(out, "{}", String::from_utf8(buf).ok()?);
                Some(out)
            }
            _ => None,
        }
    })
}

fn stringify_pretty_value_i64_str(v: i64, indent: &str, depth: usize) -> String {
    let h = v as u64;
    if h > 0xFFFF_FFFF {
        if let Some(s) = stringify_pretty_inner_str(h, indent, depth) {
            return s;
        }
    }
    v.to_string()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_STRINGIFY_PRETTY(handle: u64, indent: i64) -> u64 {
    let indent = indent.max(0).min(16) as usize;
    if indent == 0 {
        return __RTS_FN_NS_JSON_STRINGIFY(handle);
    }
    let indent_str = " ".repeat(indent);
    if let Some(s) = stringify_pretty_inner_str(handle, &indent_str, 0) {
        return alloc_entry(Entry::String(s.into_bytes()));
    }
    if handle == 0 {
        return alloc_entry(Entry::String(b"null".to_vec()));
    }
    alloc_entry(Entry::String(handle.to_string().into_bytes()))
}

/// `JSON.stringify(value, null, "<str>")` — indent eh string custom (ate
/// 10 chars conforme JS spec). String vazia desativa pretty.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_STRINGIFY_PRETTY_STR(
    handle: u64,
    indent_h: u64,
) -> u64 {
    let indent_str: String = match crate::namespaces::gc::string_pool::read_string_handle(indent_h) {
        Some(s) => {
            // JS spec: trunca em 10 caracteres.
            s.chars().take(10).collect()
        }
        None => String::new(),
    };
    if indent_str.is_empty() {
        return __RTS_FN_NS_JSON_STRINGIFY(handle);
    }
    if let Some(s) = stringify_pretty_inner_str(handle, &indent_str, 0) {
        return alloc_entry(Entry::String(s.into_bytes()));
    }
    if handle == 0 {
        return alloc_entry(Entry::String(b"null".to_vec()));
    }
    alloc_entry(Entry::String(handle.to_string().into_bytes()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_FREE(handle: u64) {
    let _ = free_handle(handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_TYPE_OF(handle: u64) -> i64 {
    with_json(handle, -1, |v| match v {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Number(_) => 2,
        Value::String(_) => 3,
        Value::Array(_) => 4,
        Value::Object(_) => 5,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_AS_BOOL(handle: u64) -> i8 {
    with_json(handle, 0, |v| match v {
        Value::Bool(b) => *b as i8,
        Value::Null => 0,
        Value::Number(n) => (n.as_f64().unwrap_or(0.0) != 0.0) as i8,
        Value::String(s) => (!s.is_empty()) as i8,
        Value::Array(a) => (!a.is_empty()) as i8,
        Value::Object(o) => (!o.is_empty()) as i8,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_AS_I64(handle: u64) -> i64 {
    with_json(handle, 0, |v| match v {
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f as i64))
            .unwrap_or(0),
        Value::Bool(b) => *b as i64,
        Value::String(s) => s.parse::<i64>().unwrap_or(0),
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_AS_F64(handle: u64) -> f64 {
    with_json(handle, f64::NAN, |v| match v {
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::Bool(b) => *b as i64 as f64,
        Value::String(s) => s.parse::<f64>().unwrap_or(f64::NAN),
        _ => f64::NAN,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_AS_STRING(handle: u64) -> u64 {
    with_json(handle, 0, |v| match v {
        Value::String(s) => alloc_entry(Entry::String(s.as_bytes().to_vec())),
        Value::Bool(b) => alloc_entry(Entry::String(b.to_string().into_bytes())),
        Value::Number(n) => alloc_entry(Entry::String(n.to_string().into_bytes())),
        Value::Null => alloc_entry(Entry::String(b"null".to_vec())),
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_ARRAY_LEN(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::Vec(slots)) => slots.len() as i64,
        Some(Entry::Json(v)) => match v.as_ref() {
            Value::Array(a) => a.len() as i64,
            _ => -1,
        },
        _ => -1,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_ARRAY_GET(handle: u64, index: i64) -> u64 {
    if index < 0 {
        return 0;
    }
    let slot = with_entry(handle, |entry| match entry {
        Some(Entry::Vec(slots)) => slots.get(index as usize).copied(),
        Some(Entry::Json(v)) => match v.as_ref() {
            Value::Array(a) => a
                .get(index as usize)
                .map(|child| alloc_entry(Entry::Json(Box::new(child.clone()))) as i64),
            _ => None,
        },
        _ => None,
    });
    match slot {
        Some(s) => promote_slot_to_json_handle(s as u64),
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_OBJECT_GET(handle: u64, key_ptr: u64, key_len: i64) -> u64 {
    let Some(bytes) = slice_from(key_ptr, key_len) else {
        return 0;
    };
    let Ok(key) = std::str::from_utf8(bytes) else {
        return 0;
    };
    let slot = with_entry(handle, |entry| match entry {
        Some(Entry::Map(map)) => map.get(key).copied(),
        Some(Entry::Json(v)) => match v.as_ref() {
            Value::Object(o) => o
                .get(key)
                .map(|child| alloc_entry(Entry::Json(Box::new(child.clone()))) as i64),
            _ => None,
        },
        _ => None,
    });
    match slot {
        Some(s) => promote_slot_to_json_handle(s as u64),
        None => 0,
    }
}

/// Quando um slot escalar (i64 raw) precisa virar handle JSON dirigivel
/// pelas APIs `as_i64`/`as_f64`/`type_of`/`as_string`, encapsulamos em
/// `Entry::Json(Number)`. Slots que ja sao handle de String/Vec/Map/Json
/// sao devolvidos tais quais.
fn promote_slot_to_json_handle(slot: u64) -> u64 {
    let already_handle = with_entry(slot, |e| {
        matches!(
            e,
            Some(Entry::Json(_))
                | Some(Entry::String(_))
                | Some(Entry::Vec(_))
                | Some(Entry::Map(_))
        )
    });
    if already_handle {
        slot
    } else {
        alloc_entry(Entry::Json(Box::new(Value::Number((slot as i64).into()))))
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_OBJECT_HAS(handle: u64, key_ptr: u64, key_len: i64) -> i8 {
    let Some(bytes) = slice_from(key_ptr, key_len) else {
        return 0;
    };
    let Ok(key) = std::str::from_utf8(bytes) else {
        return 0;
    };
    with_entry(handle, |entry| match entry {
        Some(Entry::Map(map)) => map.contains_key(key) as i8,
        Some(Entry::Json(v)) => match v.as_ref() {
            Value::Object(o) => o.contains_key(key) as i8,
            _ => 0,
        },
        _ => 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alloc_str(s: &str) -> (u64, u64, i64) {
        let h = alloc_entry(Entry::String(s.as_bytes().to_vec()));
        (h, s.as_ptr() as u64, s.len() as i64)
    }

    #[test]
    fn parse_object_roundtrip() {
        let src = r#"{"x":1,"y":"two","z":[true,null]}"#;
        let h = __RTS_FN_NS_JSON_PARSE(src.as_ptr() as u64, src.len() as i64);
        assert_ne!(h, 0);
        assert_eq!(__RTS_FN_NS_JSON_TYPE_OF(h), 5); // object

        let str_h = __RTS_FN_NS_JSON_STRINGIFY(h);
        assert_ne!(str_h, 0);

        __RTS_FN_NS_JSON_FREE(h);
    }

    #[test]
    fn invalid_json_returns_zero() {
        let bad = "not json";
        let h = __RTS_FN_NS_JSON_PARSE(bad.as_ptr() as u64, bad.len() as i64);
        assert_eq!(h, 0);
    }

    #[test]
    fn object_get_extracts_field() {
        let src = r#"{"name":"alice","age":30}"#;
        let root = __RTS_FN_NS_JSON_PARSE(src.as_ptr() as u64, src.len() as i64);
        assert_ne!(root, 0);

        let key = "name";
        let name_h = __RTS_FN_NS_JSON_OBJECT_GET(root, key.as_ptr() as u64, key.len() as i64);
        assert_ne!(name_h, 0);
        assert_eq!(__RTS_FN_NS_JSON_TYPE_OF(name_h), 3); // string

        let key = "age";
        let age_h = __RTS_FN_NS_JSON_OBJECT_GET(root, key.as_ptr() as u64, key.len() as i64);
        assert_eq!(__RTS_FN_NS_JSON_AS_I64(age_h), 30);

        __RTS_FN_NS_JSON_FREE(root);
        __RTS_FN_NS_JSON_FREE(name_h);
        __RTS_FN_NS_JSON_FREE(age_h);
    }

    #[test]
    fn array_iteration() {
        let src = "[10, 20, 30]";
        let root = __RTS_FN_NS_JSON_PARSE(src.as_ptr() as u64, src.len() as i64);
        assert_eq!(__RTS_FN_NS_JSON_ARRAY_LEN(root), 3);
        assert_eq!(__RTS_FN_NS_JSON_AS_I64(__RTS_FN_NS_JSON_ARRAY_GET(root, 0)), 10);
        assert_eq!(__RTS_FN_NS_JSON_AS_I64(__RTS_FN_NS_JSON_ARRAY_GET(root, 2)), 30);
        __RTS_FN_NS_JSON_FREE(root);
    }

    // Suppress unused warning for helper kept for symmetry with other tests.
    #[allow(dead_code)]
    fn _unused(s: &str) { let _ = alloc_str(s); }
}
