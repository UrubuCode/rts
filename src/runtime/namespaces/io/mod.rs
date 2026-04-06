use std::collections::BTreeMap;

use crate::runtime::bootstrap_lang::JsValue;

use super::{DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_value};

const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "print",
        callee: "io.print",
    },
    NamespaceMember {
        name: "panic",
        callee: "io.panic",
    },
    NamespaceMember {
        name: "stdin_read",
        callee: "io.stdin_read",
    },
    NamespaceMember {
        name: "stdout_write",
        callee: "io.stdout_write",
    },
    NamespaceMember {
        name: "stderr_write",
        callee: "io.stderr_write",
    },
    NamespaceMember {
        name: "is_ok",
        callee: "io.is_ok",
    },
    NamespaceMember {
        name: "is_err",
        callee: "io.is_err",
    },
    NamespaceMember {
        name: "unwrap_or",
        callee: "io.unwrap_or",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "io",
    members: MEMBERS,
};

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "io.print" | "io.stdout_write" | "io.stderr_write" => {
            let value = args
                .iter()
                .map(|arg| arg.to_js_string())
                .collect::<Vec<_>>()
                .join("");
            Some(DispatchOutcome::Emit(value))
        }
        "io.panic" => {
            let message = args
                .first()
                .cloned()
                .unwrap_or(JsValue::String("runtime panic".to_string()))
                .to_js_string();
            Some(DispatchOutcome::Panic(format!("runtime panic: {message}")))
        }
        "io.stdin_read" => Some(DispatchOutcome::Value(JsValue::String(String::new()))),
        "io.is_ok" if !args.is_empty() => Some(DispatchOutcome::Value(JsValue::Bool(
            result_is_ok(args.first().unwrap_or(&JsValue::Undefined)),
        ))),
        "io.is_err" if !args.is_empty() => Some(DispatchOutcome::Value(JsValue::Bool(
            result_is_err(args.first().unwrap_or(&JsValue::Undefined)),
        ))),
        "io.unwrap_or" if !args.is_empty() => Some(DispatchOutcome::Value(result_unwrap_or(
            args.first().unwrap_or(&JsValue::Undefined),
            arg_to_value(args, 1),
        ))),
        _ => None,
    }
}

pub fn result_ok(value: JsValue) -> JsValue {
    let mut map = BTreeMap::new();
    map.insert("ok".to_string(), JsValue::Bool(true));
    map.insert("tag".to_string(), JsValue::String("ok".to_string()));
    map.insert("value".to_string(), value);
    map.insert("error".to_string(), JsValue::Undefined);
    JsValue::Object(map)
}

pub fn result_err(message: &str) -> JsValue {
    let mut error = BTreeMap::new();
    error.insert("message".to_string(), JsValue::String(message.to_string()));

    let mut map = BTreeMap::new();
    map.insert("ok".to_string(), JsValue::Bool(false));
    map.insert("tag".to_string(), JsValue::String("err".to_string()));
    map.insert("value".to_string(), JsValue::Undefined);
    map.insert("error".to_string(), JsValue::Object(error));
    JsValue::Object(map)
}

fn result_is_ok(result: &JsValue) -> bool {
    match result {
        JsValue::Object(map) => matches!(map.get("ok"), Some(JsValue::Bool(true))),
        _ => false,
    }
}

fn result_is_err(result: &JsValue) -> bool {
    match result {
        JsValue::Object(map) => matches!(map.get("ok"), Some(JsValue::Bool(false))),
        _ => false,
    }
}

fn result_unwrap_or(result: &JsValue, fallback: JsValue) -> JsValue {
    match result {
        JsValue::Object(map) if matches!(map.get("ok"), Some(JsValue::Bool(true))) => {
            map.get("value").cloned().unwrap_or(JsValue::Undefined)
        }
        _ => fallback,
    }
}
