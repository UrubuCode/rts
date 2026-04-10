use std::cell::RefCell;
use std::collections::HashMap;

use crate::namespaces::{DispatchOutcome, arg_to_u64};
use crate::namespaces::value::JsValue;

thread_local! {
    static SCOPE_STACK: RefCell<Vec<HashMap<u64, u64>>> = RefCell::new(vec![]);
}

pub fn dispatch(callee: &str, args: &[JsValue]) -> Option<DispatchOutcome> {
    match callee {
        "rts.scope_push" => {
            SCOPE_STACK.with(|stack| stack.borrow_mut().push(HashMap::new()));
            Some(DispatchOutcome::Value(JsValue::Undefined))
        }
        "rts.scope_pop" => {
            SCOPE_STACK.with(|stack| stack.borrow_mut().pop());
            Some(DispatchOutcome::Value(JsValue::Undefined))
        }
        "rts.set_var" => {
            let name_hash = arg_to_u64(args, 0);
            let value = arg_to_u64(args, 1);
            SCOPE_STACK.with(|stack| {
                let mut stack = stack.borrow_mut();
                if let Some(frame) = stack.last_mut() {
                    frame.insert(name_hash, value);
                }
            });
            Some(DispatchOutcome::Value(JsValue::Undefined))
        }
        "rts.get_var" => {
            let name_hash = arg_to_u64(args, 0);
            let result = SCOPE_STACK.with(|stack| {
                let stack = stack.borrow();
                for frame in stack.iter().rev() {
                    if let Some(&val) = frame.get(&name_hash) {
                        return val;
                    }
                }
                0u64
            });
            Some(DispatchOutcome::Value(JsValue::Number(result as f64)))
        }
        _ => None,
    }
}
