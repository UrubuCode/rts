use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::namespaces::{DispatchOutcome, arg_to_u64};
use crate::namespaces::value::JsValue;

#[derive(Debug, Clone)]
struct FnEntry {
    arity: u64,
    body_ptr: u64,
}

static FN_REGISTRY: OnceLock<Arc<Mutex<HashMap<u64, FnEntry>>>> = OnceLock::new();

fn fn_registry() -> Arc<Mutex<HashMap<u64, FnEntry>>> {
    FN_REGISTRY
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "rts.declare_fn" => {
            let name_ptr = arg_to_u64(args, 0);
            let arity = arg_to_u64(args, 1);
            let body_ptr = arg_to_u64(args, 2);
            fn_registry()
                .lock()
                .unwrap()
                .insert(name_ptr, FnEntry { arity, body_ptr });
            Some(DispatchOutcome::Value(JsValue::Undefined))
        }
        "rts.call_fn" => {
            let name_ptr = arg_to_u64(args, 0);
            let body_ptr = fn_registry()
                .lock()
                .unwrap()
                .get(&name_ptr)
                .map(|e| e.body_ptr)
                .unwrap_or(0);
            Some(DispatchOutcome::Value(JsValue::Number(body_ptr as f64)))
        }
        "rts.return_val" => {
            let value = arg_to_u64(args, 0);
            Some(DispatchOutcome::Value(JsValue::Number(value as f64)))
        }
        _ => None,
    }
}
