use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::namespaces::value::RuntimeValue;

const UNDEFINED_HANDLE: i64 = 0;

#[derive(Debug, Clone)]
struct BindingEntry {
    handle: i64,
    mutable: bool,
}

#[derive(Debug, Default)]
struct ValueStore {
    next_handle: i64,
    values: BTreeMap<i64, RuntimeValue>,
    bindings: BTreeMap<String, BindingEntry>,
}

impl ValueStore {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn allocate_value(&mut self, value: RuntimeValue) -> i64 {
        if matches!(value, RuntimeValue::Undefined) {
            return UNDEFINED_HANDLE;
        }

        self.next_handle = self.next_handle.saturating_add(1);
        let handle = self.next_handle.max(1);
        self.values.insert(handle, value);
        handle
    }

    fn read_value(&self, handle: i64) -> RuntimeValue {
        if handle <= UNDEFINED_HANDLE {
            return RuntimeValue::Undefined;
        }
        self.values
            .get(&handle)
            .cloned()
            .unwrap_or(RuntimeValue::Undefined)
    }

    fn bind_identifier(&mut self, name: String, handle: i64, mutable: bool) -> i64 {
        if let Some(existing) = self.bindings.get(&name) {
            if !existing.mutable {
                return existing.handle;
            }
        }

        self.bindings.insert(name, BindingEntry { handle, mutable });
        handle
    }

    fn read_identifier(&self, name: &str) -> Option<RuntimeValue> {
        let handle = self.bindings.get(name).map(|entry| entry.handle)?;
        Some(self.read_value(handle))
    }
}

thread_local! {
    static VALUE_STORE: RefCell<ValueStore> = RefCell::new(ValueStore::default());
}

fn with_store_mut<R>(callback: impl FnOnce(&mut ValueStore) -> R) -> R {
    VALUE_STORE.with(|store| {
        let mut borrowed = store.borrow_mut();
        callback(&mut borrowed)
    })
}

pub fn reset_thread_state() {
    with_store_mut(ValueStore::reset);
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_reset_thread_state() {
    reset_thread_state();
}

fn push_value(value: RuntimeValue) -> i64 {
    with_store_mut(|store| store.allocate_value(value))
}

fn read_value(handle: i64) -> RuntimeValue {
    with_store_mut(|store| store.read_value(handle))
}

fn bind_identifier(name: String, handle: i64, mutable: bool) -> i64 {
    with_store_mut(|store| store.bind_identifier(name, handle, mutable))
}

fn read_identifier(name: &str) -> Option<RuntimeValue> {
    with_store_mut(|store| store.read_identifier(name))
}

pub(crate) fn push_runtime_value(value: RuntimeValue) -> i64 {
    push_value(value)
}

pub(crate) fn read_runtime_value(handle: i64) -> RuntimeValue {
    read_value(handle)
}

pub(crate) const fn undefined_handle() -> i64 {
    UNDEFINED_HANDLE
}

pub(crate) fn read_runtime_identifier_value(name: &str) -> RuntimeValue {
    read_identifier(name).unwrap_or(RuntimeValue::Undefined)
}

pub(crate) fn bind_runtime_identifier_value(name: &str, value: RuntimeValue, mutable: bool) {
    let handle = push_value(value);
    let _ = bind_identifier(name.to_string(), handle, mutable);
}

pub(crate) fn write_runtime_identifier_value(name: &str, value: RuntimeValue) {
    let handle = push_value(value);
    let _ = bind_identifier(name.to_string(), handle, true);
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
        return UNDEFINED_HANDLE;
    };

    let slots = [a0, a1, a2, a3, a4, a5];
    let count = argc.clamp(0, slots.len() as i64) as usize;
    let mut args = Vec::with_capacity(count);
    for handle in slots.into_iter().take(count) {
        args.push(read_value(handle));
    }

    let Some(outcome) = crate::namespaces::dispatch(callee.as_str(), &args) else {
        return UNDEFINED_HANDLE;
    };

    match outcome {
        crate::namespaces::DispatchOutcome::Value(value) => push_value(value),
        crate::namespaces::DispatchOutcome::Emit(message) => {
            if callee == "io.stderr_write" {
                eprint!("{message}");
            } else if callee == "io.stdout_write" {
                print!("{message}");
            } else {
                println!("{message}");
            }
            UNDEFINED_HANDLE
        }
        crate::namespaces::DispatchOutcome::Panic(message) => {
            eprintln!("RTS runtime panic: {message}");
            std::process::exit(1);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_bind_identifier(
    name_ptr: i64,
    name_len: i64,
    value_handle: i64,
    mutable_flag: i64,
) -> i64 {
    let Some(name) = read_utf8(name_ptr, name_len) else {
        return UNDEFINED_HANDLE;
    };

    bind_identifier(name, value_handle, mutable_flag != 0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_box_string(ptr: i64, len: i64) -> i64 {
    match read_utf8(ptr, len) {
        Some(s) => push_value(RuntimeValue::String(s)),
        None => UNDEFINED_HANDLE,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_box_bool(flag: i64) -> i64 {
    push_value(RuntimeValue::Bool(flag != 0))
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_eval_expr(_expr_ptr: i64, _expr_len: i64) -> i64 {
    let Some(expr) = read_utf8(_expr_ptr, _expr_len) else {
        return UNDEFINED_HANDLE;
    };
    let value = crate::namespaces::rust::eval::eval_expression_text(&expr);
    push_value(value)
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_eval_stmt(_stmt_ptr: i64, _stmt_len: i64) -> i64 {
    let Some(stmt) = read_utf8(_stmt_ptr, _stmt_len) else {
        return UNDEFINED_HANDLE;
    };
    let value = crate::namespaces::rust::eval::eval_statement_text(&stmt);
    push_value(value)
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_read_identifier(name_ptr: i64, name_len: i64) -> i64 {
    let Some(name) = read_utf8(name_ptr, name_len) else {
        return UNDEFINED_HANDLE;
    };

    match read_identifier(&name) {
        Some(value) => push_value(value),
        None => UNDEFINED_HANDLE,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_binop(op: i64, lhs_handle: i64, rhs_handle: i64) -> i64 {
    let lhs = read_value(lhs_handle);
    let rhs = read_value(rhs_handle);

    let result = match op {
        0 => {
            if lhs.is_string_like() || rhs.is_string_like() {
                RuntimeValue::String(format!(
                    "{}{}",
                    lhs.to_runtime_string(),
                    rhs.to_runtime_string()
                ))
            } else {
                RuntimeValue::Number(lhs.to_number() + rhs.to_number())
            }
        }
        1 => RuntimeValue::Number(lhs.to_number() - rhs.to_number()),
        2 => RuntimeValue::Number(lhs.to_number() * rhs.to_number()),
        3 => RuntimeValue::Number(lhs.to_number() / rhs.to_number()),
        4 => RuntimeValue::Number(lhs.to_number() % rhs.to_number()),
        5 => RuntimeValue::Bool(lhs.to_number() > rhs.to_number()),
        6 => RuntimeValue::Bool(lhs.to_number() >= rhs.to_number()),
        7 => RuntimeValue::Bool(lhs.to_number() < rhs.to_number()),
        8 => RuntimeValue::Bool(lhs.to_number() <= rhs.to_number()),
        9 => RuntimeValue::Bool(lhs == rhs),
        10 => RuntimeValue::Bool(lhs != rhs),
        11 => {
            if !lhs.truthy() {
                lhs
            } else {
                rhs
            }
        }
        12 => {
            if lhs.truthy() {
                lhs
            } else {
                rhs
            }
        }
        _ => RuntimeValue::Undefined,
    };

    push_value(result)
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_is_truthy(handle: i64) -> i64 {
    if handle == UNDEFINED_HANDLE {
        return 0;
    }
    if read_value(handle).truthy() { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_unbox_number(handle: i64) -> i64 {
    let value = read_value(handle);
    let n = value.to_number();
    i64::from_ne_bytes(n.to_ne_bytes())
}

#[unsafe(no_mangle)]
pub extern "C" fn __rts_box_number(bits: i64) -> i64 {
    let n = f64::from_ne_bytes(bits.to_ne_bytes());
    push_value(RuntimeValue::Number(n))
}
