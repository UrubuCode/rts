use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::namespaces::value::RuntimeValue;
use crate::namespaces::{DispatchOutcome, arg_to_u64};

static CONST_REGISTRY: OnceLock<Arc<Mutex<HashMap<u64, u64>>>> = OnceLock::new();

fn const_registry() -> Arc<Mutex<HashMap<u64, u64>>> {
    CONST_REGISTRY
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

pub fn dispatch(callee: &str, args: &[RuntimeValue]) -> Option<DispatchOutcome> {
    match callee {
        "rts.declare_const" => {
            let name_hash = arg_to_u64(args, 0);
            let value = arg_to_u64(args, 1);
            const_registry().lock().unwrap().insert(name_hash, value);
            Some(DispatchOutcome::Value(RuntimeValue::Undefined))
        }
        "rts.get_const" => {
            let name_hash = arg_to_u64(args, 0);
            let value = const_registry()
                .lock()
                .unwrap()
                .get(&name_hash)
                .copied()
                .unwrap_or(0);
            Some(DispatchOutcome::Value(RuntimeValue::Number(value as f64)))
        }
        _ => None,
    }
}
