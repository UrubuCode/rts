use std::sync::{Mutex, OnceLock};

use crate::runtime::bootstrap_lang::{JsValue, RuntimeContext, evaluate_expression};

use super::DispatchOutcome;

const MAX_ARGS: usize = 6;

#[derive(Debug, Default)]
struct ValueStore {
    values: Vec<JsValue>,
}

static VALUE_STORE: OnceLock<Mutex<ValueStore>> = OnceLock::new();

fn store() -> &'static Mutex<ValueStore> {
    VALUE_STORE.get_or_init(|| Mutex::new(ValueStore::default()))
}

fn push_value(value: JsValue) -> i64 {
    if matches!(value, JsValue::Undefined) {
        return 0;
    }

    let mut guard = lock_or_recover(store());
    guard.values.push(value);
    guard.values.len() as i64
}

fn read_value(handle: i64) -> JsValue {
    if handle <= 0 {
        return JsValue::Undefined;
    }

    let guard = lock_or_recover(store());
    guard
        .values
        .get((handle as usize).saturating_sub(1))
        .cloned()
        .unwrap_or(JsValue::Undefined)
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn read_utf8(ptr: i64, len: i64) -> Option<String> {
    if ptr <= 0 || len < 0 {
        return None;
    }

    let ptr = ptr as *const u8;
    let len = len as usize;
    let bytes = unsafe {
        // SAFETY: `ptr` and `len` are emitted by RTS codegen as static data payload references.
        std::slice::from_raw_parts(ptr, len)
    };

    std::str::from_utf8(bytes).ok().map(ToString::to_string)
}

fn decode_args(argc: i64, raw: [i64; MAX_ARGS]) -> Vec<JsValue> {
    let count = argc.clamp(0, MAX_ARGS as i64) as usize;
    (0..count).map(|index| read_value(raw[index])).collect()
}

fn dispatch_builtin(callee: &str, args: Vec<JsValue>) -> Result<JsValue, String> {
    if let Some(outcome) = super::dispatch(callee, &args) {
        return match outcome {
            DispatchOutcome::Value(value) => Ok(value),
            DispatchOutcome::Emit(message) => {
                println!("{message}");
                Ok(JsValue::Undefined)
            }
            DispatchOutcome::Panic(message) => Err(message),
        };
    }

    match callee {
        "Number" => Ok(JsValue::Number(
            args.first()
                .cloned()
                .unwrap_or(JsValue::Undefined)
                .to_number(),
        )),
        "String" => Ok(JsValue::String(
            args.first()
                .cloned()
                .unwrap_or(JsValue::Undefined)
                .to_js_string(),
        )),
        "Boolean" => Ok(JsValue::Bool(
            args.first().cloned().unwrap_or(JsValue::Undefined).truthy(),
        )),
        _ => Ok(JsValue::Undefined),
    }
}

struct AbiEvalContext;

impl RuntimeContext for AbiEvalContext {
    fn read_identifier(&self, _name: &str) -> Option<JsValue> {
        None
    }

    fn call_function(&mut self, callee: &str, args: Vec<JsValue>) -> anyhow::Result<JsValue> {
        dispatch_builtin(callee, args).map_err(anyhow::Error::msg)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_call_dispatch(
    callee_ptr: i64,
    callee_len: i64,
    argc: i64,
    a0: i64,
    a1: i64,
    a2: i64,
    a3: i64,
    a4: i64,
    a5: i64,
) -> i64 {
    let Some(callee) = read_utf8(callee_ptr, callee_len) else {
        return 0;
    };

    let args = decode_args(argc, [a0, a1, a2, a3, a4, a5]);
    match dispatch_builtin(&callee, args) {
        Ok(value) => push_value(value),
        Err(message) => {
            eprintln!("RTS runtime panic: {message}");
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_eval_expr(expr_ptr: i64, expr_len: i64) -> i64 {
    let Some(expression) = read_utf8(expr_ptr, expr_len) else {
        return 0;
    };

    let mut runtime = AbiEvalContext;
    let value =
        evaluate_expression(&expression, &mut runtime).unwrap_or_else(|_| JsValue::Undefined);
    push_value(value)
}
