use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::{DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string, arg_to_value};
use crate::namespaces::value::RuntimeValue;

// ── State ──────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct GlobalThisState {
    map: BTreeMap<String, RuntimeValue>,
}

static GLOBAL_THIS: OnceLock<Arc<Mutex<GlobalThisState>>> = OnceLock::new();

fn state() -> Arc<Mutex<GlobalThisState>> {
    GLOBAL_THIS
        .get_or_init(|| Arc::new(Mutex::new(GlobalThisState::default())))
        .clone()
}

pub fn define(key: impl Into<String>, value: RuntimeValue) {
    state().lock().unwrap().map.insert(key.into(), value);
}

pub fn get(key: &str) -> Option<RuntimeValue> {
    state().lock().unwrap().map.get(key).cloned()
}

pub fn has(key: &str) -> bool {
    state().lock().unwrap().map.contains_key(key)
}

pub fn delete(key: &str) -> bool {
    state().lock().unwrap().map.remove(key).is_some()
}

pub fn keys_csv() -> String {
    state()
        .lock()
        .unwrap()
        .map
        .keys()
        .cloned()
        .collect::<Vec<_>>()
        .join(",")
}

// ── Namespace ──────────────────────────────────────────────────────────────────

const MEMBERS: &[NamespaceMember] = &[
    NamespaceMember {
        name: "define",
        callee: "globalThis.define",
        doc: "Defines a global variable accessible from anywhere in the program.",
        ts_signature: "define(name: str, value: any): void",
    },
    NamespaceMember {
        name: "get",
        callee: "globalThis.get",
        doc: "Retrieves a global variable by name.",
        ts_signature: "get(name: str): any",
    },
    NamespaceMember {
        name: "has",
        callee: "globalThis.has",
        doc: "Returns true if the global variable is defined.",
        ts_signature: "has(name: str): bool",
    },
    NamespaceMember {
        name: "delete",
        callee: "globalThis.delete",
        doc: "Removes a global variable.",
        ts_signature: "delete(name: str): bool",
    },
    NamespaceMember {
        name: "keys",
        callee: "globalThis.keys",
        doc: "Returns comma-separated list of all global variable names.",
        ts_signature: "keys(): str",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "globalThis",
    doc: "Global object — stores values accessible from anywhere in the program. \
          Use globalThis.define to register globals like console, process, etc.",
    members: MEMBERS,
    ts_prelude: &[],
};

pub fn dispatch(callee: &str, args: &[RuntimeValue]) -> Option<DispatchOutcome> {
    match callee {
        "globalThis.define" if args.len() >= 2 => {
            let key = arg_to_string(args, 0);
            let value = arg_to_value(args, 1);
            define(key, value);
            Some(DispatchOutcome::Value(RuntimeValue::Undefined))
        }
        "globalThis.get" if !args.is_empty() => {
            let key = arg_to_string(args, 0);
            Some(DispatchOutcome::Value(
                get(&key).unwrap_or(RuntimeValue::Undefined),
            ))
        }
        "globalThis.has" if !args.is_empty() => Some(DispatchOutcome::Value(RuntimeValue::Bool(
            has(&arg_to_string(args, 0)),
        ))),
        "globalThis.delete" if !args.is_empty() => Some(DispatchOutcome::Value(
            RuntimeValue::Bool(delete(&arg_to_string(args, 0))),
        )),
        "globalThis.keys" => Some(DispatchOutcome::Value(RuntimeValue::String(keys_csv()))),
        _ => None,
    }
}
