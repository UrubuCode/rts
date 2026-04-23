//! JSON namespace — stringify e parse de RuntimeValue via serde_json.
//!
//! O nome do namespace e `JSON` (maiusculo) para casar com a API canonica
//! do JavaScript: `JSON.stringify(obj)` e `JSON.parse(text)`. Todos os
//! outros namespaces do RTS sao lowercase (io, fs, net), mas JSON e uma
//! excecao deliberada porque o uso no TypeScript seria sempre pelo nome
//! maiusculo.
//!
//! Conversao RuntimeValue ↔ serde_json::Value e total: Number → Number,
//! String → String, Bool → Bool, Null/Undefined → Null, Object → Object.
//! NativeFunction nao e serializavel — vira Null.

use std::collections::BTreeMap;

use serde_json::{Map as JsonMap, Value as JsonValue};

use super::{DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string};
use crate::namespaces::value::RuntimeValue;

const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "stringify",
        callee: "JSON.stringify",
        doc: "Serializa um valor para string JSON. Retorna \"null\" para undefined ou funcoes.",
        ts_signature: "stringify(value: any): string",
    },
    NamespaceMember {
        name: "parse",
        callee: "JSON.parse",
        doc: "Desserializa uma string JSON em um valor. Retorna undefined em caso de erro.",
        ts_signature: "parse(text: string): any",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "JSON",
    doc: "JavaScript Object Notation helpers backed by serde_json.",
    members: MEMBERS,
    ts_prelude: &[],
};

pub fn dispatch(callee: &str, args: &[RuntimeValue]) -> Option<DispatchOutcome> {
    match callee {
        "JSON.stringify" => {
            let value = args.first().cloned().unwrap_or(RuntimeValue::Undefined);
            let json = runtime_to_json(&value);
            // serde_json::to_string nao falha para Value — sempre retorna Ok.
            let text = serde_json::to_string(&json).unwrap_or_else(|_| "null".to_string());
            Some(DispatchOutcome::Value(RuntimeValue::String(text)))
        }
        "JSON.parse" => {
            let text = arg_to_string(args, 0);
            match serde_json::from_str::<JsonValue>(&text) {
                Ok(json) => Some(DispatchOutcome::Value(json_to_runtime(&json))),
                Err(_) => Some(DispatchOutcome::Value(RuntimeValue::Undefined)),
            }
        }
        _ => None,
    }
}

/// Converte um `RuntimeValue` para `serde_json::Value`.
/// - `Number` com bits de NaN/Infinity vira `null` (JSON nao representa).
/// - `NativeFunction` vira `null`.
/// - `Object` recursivo vira `Object` JSON preservando a ordem de chaves
///   (BTreeMap → JsonMap baseado em Map, mas Map do serde_json preserva
///   insercao quando feature "preserve_order" ativa; por default e BTreeMap
///   interno tambem — ambos sao ordenados aqui).
pub fn runtime_to_json(value: &RuntimeValue) -> JsonValue {
    match value {
        RuntimeValue::Null | RuntimeValue::Undefined => JsonValue::Null,
        RuntimeValue::Bool(b) => JsonValue::Bool(*b),
        RuntimeValue::Number(n) => {
            if !n.is_finite() {
                // NaN/Infinity nao sao representaveis em JSON — JavaScript
                // nativo retorna "null" nesses casos, entao seguimos o padrao.
                JsonValue::Null
            } else if n.fract() == 0.0 && n.abs() < (1u64 << 53) as f64 {
                // Inteiros que cabem em i64 safe-integer sao serializados
                // sem `.0` para casar com JSON.stringify nativo do JS.
                JsonValue::Number(serde_json::Number::from(*n as i64))
            } else {
                serde_json::Number::from_f64(*n)
                    .map(JsonValue::Number)
                    .unwrap_or(JsonValue::Null)
            }
        }
        RuntimeValue::String(s) => JsonValue::String(s.clone()),
        RuntimeValue::Object(map) => {
            let mut out = JsonMap::new();
            for (k, v) in map {
                out.insert(k.clone(), runtime_to_json(v));
            }
            JsonValue::Object(out)
        }
        RuntimeValue::Array(items) => JsonValue::Array(items.iter().map(runtime_to_json).collect()),
        RuntimeValue::NativeFunction(_) => JsonValue::Null,
    }
}

/// Converte um `serde_json::Value` para `RuntimeValue`.
/// Arrays JSON sao convertidos em `Object` com chaves numericas (`"0"`,
/// `"1"`, etc.) + campo `length`. E uma representacao temporaria ate o RTS
/// ter suporte a arrays reais como variante de `RuntimeValue`.
pub fn json_to_runtime(value: &JsonValue) -> RuntimeValue {
    match value {
        JsonValue::Null => RuntimeValue::Null,
        JsonValue::Bool(b) => RuntimeValue::Bool(*b),
        JsonValue::Number(n) => {
            // serde_json::Number pode ser inteiro ou float — both fit em f64.
            RuntimeValue::Number(n.as_f64().unwrap_or(f64::NAN))
        }
        JsonValue::String(s) => RuntimeValue::String(s.clone()),
        JsonValue::Array(items) => RuntimeValue::Array(items.iter().map(json_to_runtime).collect()),
        JsonValue::Object(obj) => {
            let mut map: BTreeMap<String, RuntimeValue> = BTreeMap::new();
            for (k, v) in obj {
                map.insert(k.clone(), json_to_runtime(v));
            }
            RuntimeValue::Object(map)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stringify_primitives() {
        // Inteiros exatos saem sem `.0` (casando com JSON nativo do JS).
        assert_eq!(
            serde_json::to_string(&runtime_to_json(&RuntimeValue::Number(42.0))).unwrap(),
            "42"
        );
        // Fracionais mantem precisao completa.
        assert_eq!(
            serde_json::to_string(&runtime_to_json(&RuntimeValue::Number(3.5))).unwrap(),
            "3.5"
        );
        assert_eq!(
            serde_json::to_string(&runtime_to_json(&RuntimeValue::String("hi".into()))).unwrap(),
            "\"hi\""
        );
        assert_eq!(
            serde_json::to_string(&runtime_to_json(&RuntimeValue::Bool(true))).unwrap(),
            "true"
        );
        assert_eq!(
            serde_json::to_string(&runtime_to_json(&RuntimeValue::Null)).unwrap(),
            "null"
        );
    }

    #[test]
    fn stringify_object_preserves_fields() {
        let mut map = BTreeMap::new();
        map.insert("count".to_string(), RuntimeValue::Number(5.0));
        map.insert("name".to_string(), RuntimeValue::String("bob".into()));
        let obj = RuntimeValue::Object(map);
        let json = runtime_to_json(&obj);
        let text = serde_json::to_string(&json).unwrap();
        // BTreeMap ordena alfabeticamente: count antes de name
        assert_eq!(text, r#"{"count":5,"name":"bob"}"#);
    }

    #[test]
    fn parse_object_roundtrip() {
        let text = r#"{"count":5,"name":"bob"}"#;
        let json: JsonValue = serde_json::from_str(text).unwrap();
        let value = json_to_runtime(&json);
        if let RuntimeValue::Object(map) = value {
            assert!(matches!(map.get("count"), Some(RuntimeValue::Number(n)) if *n == 5.0));
            assert!(matches!(map.get("name"), Some(RuntimeValue::String(s)) if s == "bob"));
        } else {
            panic!("expected Object");
        }
    }

    #[test]
    fn parse_array_returns_array_variant() {
        let text = r#"[10, 20, 30]"#;
        let json: JsonValue = serde_json::from_str(text).unwrap();
        let value = json_to_runtime(&json);
        if let RuntimeValue::Array(items) = &value {
            assert_eq!(items.len(), 3);
            assert_eq!(items[0], RuntimeValue::Number(10.0));
            assert_eq!(items[1], RuntimeValue::Number(20.0));
            assert_eq!(items[2], RuntimeValue::Number(30.0));
        } else {
            panic!("expected Array, got {:?}", value);
        }
        // length via get_property
        assert_eq!(
            value.get_property("length"),
            Some(RuntimeValue::Number(3.0))
        );
    }

    #[test]
    fn parse_array_roundtrip_stringify() {
        let text = "[1,2]";
        let parsed = json_to_runtime(&serde_json::from_str::<JsonValue>(text).unwrap());
        let json = runtime_to_json(&parsed);
        assert_eq!(serde_json::to_string(&json).unwrap(), "[1,2]");
    }

    #[test]
    fn parse_empty_array() {
        let text = "[]";
        let parsed = json_to_runtime(&serde_json::from_str::<JsonValue>(text).unwrap());
        if let RuntimeValue::Array(items) = &parsed {
            assert!(items.is_empty());
        } else {
            panic!("expected Array");
        }
        assert_eq!(
            parsed.get_property("length"),
            Some(RuntimeValue::Number(0.0))
        );
    }

    #[test]
    fn parse_nested_array_in_object() {
        let text = r#"{"items":[1,2]}"#;
        let parsed = json_to_runtime(&serde_json::from_str::<JsonValue>(text).unwrap());
        if let RuntimeValue::Object(map) = &parsed {
            let items = map.get("items").unwrap();
            assert!(matches!(items, RuntimeValue::Array(v) if v.len() == 2));
        } else {
            panic!("expected Object");
        }
    }

    #[test]
    fn nan_and_infinity_serialize_as_null() {
        assert_eq!(
            serde_json::to_string(&runtime_to_json(&RuntimeValue::Number(f64::NAN))).unwrap(),
            "null"
        );
        assert_eq!(
            serde_json::to_string(&runtime_to_json(&RuntimeValue::Number(f64::INFINITY))).unwrap(),
            "null"
        );
    }
}
