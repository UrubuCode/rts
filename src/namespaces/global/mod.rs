use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::{DispatchOutcome, NamespaceMember, NamespaceSpec, arg_to_string};
use crate::namespaces::value::RuntimeValue;

// ── Estado de globals ──────────────────────────────────────────────────────────

#[derive(Default)]
struct GlobalState {
    map: BTreeMap<String, String>,
}

static GLOBAL_STATE: OnceLock<Arc<Mutex<GlobalState>>> = OnceLock::new();

fn state() -> Arc<Mutex<GlobalState>> {
    GLOBAL_STATE
        .get_or_init(|| Arc::new(Mutex::new(GlobalState::default())))
        .clone()
}

pub fn set(key: impl Into<String>, value: impl Into<String>) {
    state().lock().unwrap().map.insert(key.into(), value.into());
}

pub fn get(key: &str) -> Option<String> {
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
        name: "set",
        callee: "global.set",
        doc: "Stores a string value in runtime global map.",
        ts_signature: "set(key: str, value: str): void",
    },
    NamespaceMember {
        name: "get",
        callee: "global.get",
        doc: "Reads a string value from runtime global map.",
        ts_signature: "get(key: str): str | undefined",
    },
    NamespaceMember {
        name: "has",
        callee: "global.has",
        doc: "Checks whether a key exists in global map.",
        ts_signature: "has(key: str): bool",
    },
    NamespaceMember {
        name: "delete",
        callee: "global.delete",
        doc: "Deletes a key from global map.",
        ts_signature: "delete(key: str): bool",
    },
    NamespaceMember {
        name: "keys",
        callee: "global.keys",
        doc: "Returns global keys joined by commas.",
        ts_signature: "keys(): str",
    },
];

pub const SPEC: NamespaceSpec = NamespaceSpec {
    name: "global",
    doc: "Small runtime key-value storage for bootstrap state.",
    members: MEMBERS,
    ts_prelude: &[],
};

pub fn dispatch(callee: &str, args: &[RuntimeValue]) -> Option<DispatchOutcome> {
    match callee {
        "global.set" if args.len() >= 2 => {
            set(arg_to_string(args, 0), arg_to_string(args, 1));
            Some(DispatchOutcome::Value(RuntimeValue::Undefined))
        }
        "global.get" if !args.is_empty() => Some(DispatchOutcome::Value(
            get(&arg_to_string(args, 0))
                .map(RuntimeValue::String)
                .unwrap_or(RuntimeValue::Undefined),
        )),
        "global.has" if !args.is_empty() => Some(DispatchOutcome::Value(RuntimeValue::Bool(has(
            &arg_to_string(args, 0),
        )))),
        "global.delete" if !args.is_empty() => Some(DispatchOutcome::Value(RuntimeValue::Bool(
            delete(&arg_to_string(args, 0)),
        ))),
        "global.keys" if args.is_empty() => {
            Some(DispatchOutcome::Value(RuntimeValue::String(keys_csv())))
        }
        _ => None,
    }
}
