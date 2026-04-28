//! Implementacao das ops do namespace `json`.
//!
//! Usa `serde_json::Value` armazenado via `Entry::Json` no HandleTable.
//! `parse` constroi a arvore inteira de uma vez; `as_*` e `*_get` extraem
//! valores conforme demanda. `array_get` / `object_get` clonam o valor
//! filho num novo handle (semantica simples — leitura nao modifica
//! parent, e free do filho nao quebra parent).

use serde_json::Value;

use super::super::gc::handles::{alloc_entry, free_handle, shard_for_handle, Entry};

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
        Ok(v) => alloc_entry(Entry::Json(Box::new(v))),
        Err(_) => 0,
    }
}

fn with_json<R>(handle: u64, default: R, f: impl FnOnce(&Value) -> R) -> R {
    let table = shard_for_handle(handle).lock().unwrap();
    match table.get(handle) {
        Some(Entry::Json(v)) => f(v.as_ref()),
        _ => default,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_STRINGIFY(handle: u64) -> u64 {
    with_json(handle, 0, |v| {
        match serde_json::to_string(v) {
            Ok(s) => alloc_entry(Entry::String(s.into_bytes())),
            Err(_) => 0,
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_STRINGIFY_PRETTY(handle: u64, indent: i64) -> u64 {
    let indent = indent.max(0).min(16) as usize;
    with_json(handle, 0, |v| {
        let pad = " ".repeat(indent);
        let mut buf = Vec::with_capacity(64);
        let formatter = serde_json::ser::PrettyFormatter::with_indent(pad.as_bytes());
        let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
        match serde::Serialize::serialize(v, &mut ser) {
            Ok(()) => alloc_entry(Entry::String(buf)),
            Err(_) => 0,
        }
    })
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
    with_json(handle, -1, |v| match v {
        Value::Array(a) => a.len() as i64,
        _ => -1,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_ARRAY_GET(handle: u64, index: i64) -> u64 {
    with_json(handle, 0, |v| match v {
        Value::Array(a) => {
            if index < 0 {
                return 0;
            }
            match a.get(index as usize) {
                Some(child) => alloc_entry(Entry::Json(Box::new(child.clone()))),
                None => 0,
            }
        }
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_OBJECT_GET(handle: u64, key_ptr: u64, key_len: i64) -> u64 {
    let Some(bytes) = slice_from(key_ptr, key_len) else {
        return 0;
    };
    let Ok(key) = std::str::from_utf8(bytes) else {
        return 0;
    };
    with_json(handle, 0, |v| match v {
        Value::Object(o) => match o.get(key) {
            Some(child) => alloc_entry(Entry::Json(Box::new(child.clone()))),
            None => 0,
        },
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_JSON_OBJECT_HAS(handle: u64, key_ptr: u64, key_len: i64) -> i8 {
    let Some(bytes) = slice_from(key_ptr, key_len) else {
        return 0;
    };
    let Ok(key) = std::str::from_utf8(bytes) else {
        return 0;
    };
    with_json(handle, 0, |v| match v {
        Value::Object(o) => o.contains_key(key) as i8,
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
