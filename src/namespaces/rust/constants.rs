use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::namespaces::{DispatchOutcome, arg_to_u64};
use crate::namespaces::value::JsValue;

static CONST_REGISTRY: OnceLock<Arc<Mutex<HashMap<u64, u64>>>> = OnceLock::new();

fn const_registry() -> Arc<Mutex<HashMap<u64, u64>>> {
    CONST_REGISTRY
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "rts.declare_const" => {
            let name_hash = arg_to_u64(args, 0);
            let value = arg_to_u64(args, 1);
            const_registry().lock().unwrap().insert(name_hash, value);
            Some(DispatchOutcome::Value(JsValue::Undefined))
        }
        "rts.get_const" => {
            let name_hash = arg_to_u64(args, 0);
            let value = const_registry()
                .lock()
                .unwrap()
                .get(&name_hash)
                .copied()
                .unwrap_or(0);
            Some(DispatchOutcome::Value(JsValue::Number(value as f64)))
        }
        _ => None,
    }
}

pub const MEMBERS: &[(&str, &str, &str, &str)] = &[
    ("declare_const", "rts.declare_const", "Declara constante global imutável.", "declare_const(name_hash: u64, value: u64): void"),
    ("get_const", "rts.get_const", "Lê constante global pelo hash do nome.", "get_const(name_hash: u64): u64"),
];
