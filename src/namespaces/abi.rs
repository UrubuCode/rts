use std::cell::RefCell;
use std::time::Instant;

use rustc_hash::FxHashMap;

use crate::namespaces::value::RuntimeValue;

const UNDEFINED_HANDLE: i64 = 0;

// --- fn_id constants para __rts_dispatch ---
// Slot layout: __rts_dispatch(fn_id, a0, a1, a2, a3, a4, a5) -> i64
pub(crate) const FN_RESET_THREAD_STATE: i64 = 0;
pub(crate) const FN_BIND_IDENTIFIER: i64 = 1; // (ptr, len, handle, mutable)
pub(crate) const FN_BOX_STRING: i64 = 2; // (ptr, len)
pub(crate) const FN_BOX_BOOL: i64 = 3; // (flag)
pub(crate) const FN_EVAL_EXPR: i64 = 4; // (ptr, len)
pub(crate) const FN_EVAL_STMT: i64 = 5; // (ptr, len)
pub(crate) const FN_READ_IDENTIFIER: i64 = 6; // (ptr, len)
pub(crate) const FN_BINOP: i64 = 7; // (op, lhs, rhs)
pub(crate) const FN_IS_TRUTHY: i64 = 8; // (handle)
pub(crate) const FN_UNBOX_NUMBER: i64 = 9; // (handle)
pub(crate) const FN_BOX_NUMBER: i64 = 10; // (bits as i64)
pub(crate) const FN_IO_PRINT: i64 = 11;
pub(crate) const FN_IO_STDOUT_WRITE: i64 = 12;
pub(crate) const FN_IO_STDERR_WRITE: i64 = 13;
pub(crate) const FN_IO_PANIC: i64 = 14;
pub(crate) const FN_CRYPTO_SHA256: i64 = 15;
pub(crate) const FN_PROCESS_EXIT: i64 = 16;
pub(crate) const FN_GLOBAL_SET: i64 = 17; // (key, value)
pub(crate) const FN_GLOBAL_GET: i64 = 18;
pub(crate) const FN_GLOBAL_HAS: i64 = 19;
pub(crate) const FN_GLOBAL_DELETE: i64 = 20;

#[derive(Debug, Clone)]
struct BindingEntry {
    handle: i64,
    mutable: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RuntimeBinding {
    pub handle: i64,
    pub mutable: bool,
}

#[derive(Debug, Default)]
struct ValueStore {
    values: Vec<RuntimeValue>,
    bindings: FxHashMap<String, BindingEntry>,
}

impl ValueStore {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn allocate_value(&mut self, value: RuntimeValue) -> i64 {
        if matches!(value, RuntimeValue::Undefined) {
            return UNDEFINED_HANDLE;
        }

        self.values.push(value);
        crate::namespaces::gc::notify_alloc();
        self.values.len() as i64
    }

    fn read_value(&self, handle: i64) -> RuntimeValue {
        if handle <= UNDEFINED_HANDLE {
            return RuntimeValue::Undefined;
        }
        let index = (handle - 1) as usize;
        self.values
            .get(index)
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

    fn bind_identifier_value(&mut self, name: &str, value: RuntimeValue, mutable: bool) -> i64 {
        if let Some(existing) = self.bindings.get(name).cloned() {
            if !existing.mutable {
                return existing.handle;
            }

            if mutable && existing.handle > UNDEFINED_HANDLE {
                let index = (existing.handle - 1) as usize;
                if let Some(slot) = self.values.get_mut(index) {
                    *slot = value;
                    return existing.handle;
                }
            }
        }

        let handle = self.allocate_value(value);
        self.bindings
            .insert(name.to_string(), BindingEntry { handle, mutable });
        handle
    }

    fn read_identifier(&self, name: &str) -> Option<RuntimeValue> {
        let handle = self.bindings.get(name).map(|entry| entry.handle)?;
        Some(self.read_value(handle))
    }

    fn resolve_binding(&self, name: &str) -> Option<BindingEntry> {
        self.bindings.get(name).cloned()
    }

    fn write_identifier_value(&mut self, name: &str, value: RuntimeValue) -> i64 {
        if let Some(existing) = self.bindings.get(name).cloned() {
            if !existing.mutable {
                return existing.handle;
            }

            if existing.handle > UNDEFINED_HANDLE {
                let index = (existing.handle - 1) as usize;
                if let Some(slot) = self.values.get_mut(index) {
                    *slot = value;
                    return existing.handle;
                }
            }
        }

        let handle = self.allocate_value(value);
        self.bindings.insert(
            name.to_string(),
            BindingEntry {
                handle,
                mutable: true,
            },
        );
        handle
    }

    fn write_value_handle(&mut self, handle: i64, value: RuntimeValue) -> bool {
        if handle <= UNDEFINED_HANDLE {
            return false;
        }
        let index = (handle - 1) as usize;
        if let Some(slot) = self.values.get_mut(index) {
            *slot = value;
            return true;
        }
        false
    }
}

#[derive(Debug, Default, Clone)]
struct RuntimeMetrics {
    dispatch_calls: u64,
    dispatch_nanos: u128,
    eval_expr_calls: u64,
    eval_expr_nanos: u128,
    eval_stmt_calls: u64,
    eval_stmt_nanos: u128,
    call_dispatch_calls: u64,
    call_dispatch_nanos: u128,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct RuntimeMetricsSnapshot {
    pub dispatch_calls: u64,
    pub dispatch_nanos: u128,
    pub eval_expr_calls: u64,
    pub eval_expr_nanos: u128,
    pub eval_stmt_calls: u64,
    pub eval_stmt_nanos: u128,
    pub eval_parse_calls: u64,
    pub eval_parse_nanos: u128,
    pub eval_identifier_reads: u64,
    pub eval_identifier_writes: u64,
    pub eval_call_dispatches: u64,
    pub eval_binding_cache_hits: u64,
    pub eval_binding_cache_misses: u64,
    pub call_dispatch_calls: u64,
    pub call_dispatch_nanos: u128,
}

thread_local! {
    static VALUE_STORE: RefCell<ValueStore> = RefCell::new(ValueStore::default());
    static RUNTIME_METRICS: RefCell<RuntimeMetrics> = RefCell::new(RuntimeMetrics::default());
}

static DISPATCH_METRICS_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[inline(always)]
fn metrics_enabled() -> bool {
    DISPATCH_METRICS_ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}

pub(crate) fn set_dispatch_metrics_enabled(enabled: bool) {
    DISPATCH_METRICS_ENABLED.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

fn with_store_mut<R>(callback: impl FnOnce(&mut ValueStore) -> R) -> R {
    VALUE_STORE.with(|store| {
        let mut borrowed = store.borrow_mut();
        callback(&mut borrowed)
    })
}

fn with_store<R>(callback: impl FnOnce(&ValueStore) -> R) -> R {
    VALUE_STORE.with(|store| {
        let borrowed = store.borrow();
        callback(&borrowed)
    })
}

pub fn reset_thread_state() {
    with_store_mut(ValueStore::reset);
    crate::namespaces::gc::safe_collect();
    crate::namespaces::rust::eval::reset_metrics();
    reset_runtime_metrics();
}

fn reset_runtime_metrics() {
    RUNTIME_METRICS.with(|metrics| {
        *metrics.borrow_mut() = RuntimeMetrics::default();
    });
}

pub(crate) fn runtime_metrics_snapshot() -> RuntimeMetricsSnapshot {
    let eval = crate::namespaces::rust::eval::metrics_snapshot();
    RUNTIME_METRICS.with(|metrics| {
        let metrics = metrics.borrow();
        RuntimeMetricsSnapshot {
            dispatch_calls: metrics.dispatch_calls,
            dispatch_nanos: metrics.dispatch_nanos,
            eval_expr_calls: metrics.eval_expr_calls,
            eval_expr_nanos: metrics.eval_expr_nanos,
            eval_stmt_calls: metrics.eval_stmt_calls,
            eval_stmt_nanos: metrics.eval_stmt_nanos,
            eval_parse_calls: eval.parse_calls,
            eval_parse_nanos: eval.parse_nanos,
            eval_identifier_reads: eval.identifier_reads,
            eval_identifier_writes: eval.identifier_writes,
            eval_call_dispatches: eval.call_dispatches,
            eval_binding_cache_hits: eval.binding_cache_hits,
            eval_binding_cache_misses: eval.binding_cache_misses,
            call_dispatch_calls: metrics.call_dispatch_calls,
            call_dispatch_nanos: metrics.call_dispatch_nanos,
        }
    })
}

fn push_value(value: RuntimeValue) -> i64 {
    with_store_mut(|store| store.allocate_value(value))
}

fn read_value(handle: i64) -> RuntimeValue {
    with_store(|store| store.read_value(handle))
}

fn bind_identifier(name: String, handle: i64, mutable: bool) -> i64 {
    with_store_mut(|store| store.bind_identifier(name, handle, mutable))
}

fn read_identifier(name: &str) -> Option<RuntimeValue> {
    with_store(|store| store.read_identifier(name))
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

pub(crate) fn resolve_runtime_identifier_binding(name: &str) -> Option<RuntimeBinding> {
    with_store(|store| {
        let binding = store.resolve_binding(name)?;
        Some(RuntimeBinding {
            handle: binding.handle,
            mutable: binding.mutable,
        })
    })
}

pub(crate) fn bind_runtime_identifier_value(name: &str, value: RuntimeValue, mutable: bool) -> i64 {
    with_store_mut(|store| store.bind_identifier_value(name, value, mutable))
}

pub(crate) fn write_runtime_identifier_value(name: &str, value: RuntimeValue) {
    let _ = with_store_mut(|store| store.write_identifier_value(name, value));
}

pub(crate) fn write_runtime_value_handle(handle: i64, value: RuntimeValue) -> bool {
    with_store_mut(|store| store.write_value_handle(handle, value))
}

fn read_utf8(ptr: i64, len: i64) -> Option<String> {
    if ptr <= 0 || len < 0 {
        return None;
    }

    let ptr = ptr as *const u8;
    let len = len as usize;
    let bytes = unsafe {
        // SAFETY: `ptr` and `len` são emitidos pelo codegen RTS como referências a dados estáticos.
        std::slice::from_raw_parts(ptr, len)
    };

    std::str::from_utf8(bytes).ok().map(ToString::to_string)
}

fn binop_dispatch(op: i64, lhs_handle: i64, rhs_handle: i64) -> i64 {
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

/// Ponto de entrada único do launcher para todas as chamadas de runtime de assinatura fixa.
/// O código Cranelift compilado chama apenas este símbolo (e __rts_call_dispatch para dispatch dinâmico).
#[unsafe(no_mangle)]
pub extern "C" fn __rts_dispatch(
    fn_id: i64,
    a0: i64,
    a1: i64,
    a2: i64,
    a3: i64,
    _a4: i64,
    _a5: i64,
) -> i64 {
    // Instrumentação de tempo por call é cara (2 syscalls QPC + RefCell borrow
    // em um hot path de ~80ns); ligamos apenas em modo debug via
    // metrics_enabled(), que usa AtomicBool relaxed.
    let metrics_on = metrics_enabled();
    let started = if metrics_on {
        Some(Instant::now())
    } else {
        None
    };
    let result = match fn_id {
        FN_RESET_THREAD_STATE => {
            reset_thread_state();
            UNDEFINED_HANDLE
        }
        FN_BIND_IDENTIFIER => {
            let Some(name) = read_utf8(a0, a1) else {
                return UNDEFINED_HANDLE;
            };
            bind_identifier(name, a2, a3 != 0)
        }
        FN_BOX_STRING => match read_utf8(a0, a1) {
            Some(s) => push_value(RuntimeValue::String(s)),
            None => UNDEFINED_HANDLE,
        },
        FN_BOX_BOOL => push_value(RuntimeValue::Bool(a0 != 0)),
        FN_EVAL_EXPR => {
            let Some(expr) = read_utf8(a0, a1) else {
                return UNDEFINED_HANDLE;
            };
            crate::namespaces::gc::enter_scope();
            let value = crate::namespaces::rust::eval_runtime_expression(&expr);
            crate::namespaces::gc::exit_scope();
            push_value(value)
        }
        FN_EVAL_STMT => {
            let Some(stmt) = read_utf8(a0, a1) else {
                return UNDEFINED_HANDLE;
            };
            crate::namespaces::gc::enter_scope();
            let value = crate::namespaces::rust::eval::eval_statement_text(&stmt);
            crate::namespaces::gc::exit_scope();
            push_value(value)
        }
        FN_READ_IDENTIFIER => {
            let Some(name) = read_utf8(a0, a1) else {
                return UNDEFINED_HANDLE;
            };
            match read_identifier(&name) {
                Some(value) => push_value(value),
                None => UNDEFINED_HANDLE,
            }
        }
        FN_BINOP => binop_dispatch(a0, a1, a2),
        FN_IS_TRUTHY => {
            if a0 == UNDEFINED_HANDLE {
                return 0;
            }
            if read_value(a0).truthy() { 1 } else { 0 }
        }
        FN_UNBOX_NUMBER => {
            let n = read_value(a0).to_number();
            i64::from_ne_bytes(n.to_ne_bytes())
        }
        FN_BOX_NUMBER => {
            let n = f64::from_ne_bytes(a0.to_ne_bytes());
            push_value(RuntimeValue::Number(n))
        }
        FN_IO_PRINT => crate::namespaces::rust::rts_io_print(a0),
        FN_IO_STDOUT_WRITE => crate::namespaces::rust::rts_io_stdout_write(a0),
        FN_IO_STDERR_WRITE => crate::namespaces::rust::rts_io_stderr_write(a0),
        FN_IO_PANIC => crate::namespaces::rust::rts_io_panic(a0),
        FN_CRYPTO_SHA256 => crate::namespaces::rust::rts_crypto_sha256(a0),
        FN_PROCESS_EXIT => crate::namespaces::rust::rts_process_exit(a0),
        FN_GLOBAL_SET => crate::namespaces::rust::rts_global_set(a0, a1),
        FN_GLOBAL_GET => crate::namespaces::rust::rts_global_get(a0),
        FN_GLOBAL_HAS => crate::namespaces::rust::rts_global_has(a0),
        FN_GLOBAL_DELETE => crate::namespaces::rust::rts_global_delete(a0),
        _ => UNDEFINED_HANDLE,
    };

    if let Some(started) = started {
        let elapsed = started.elapsed().as_nanos();
        RUNTIME_METRICS.with(|metrics| {
            let mut metrics = metrics.borrow_mut();
            metrics.dispatch_calls = metrics.dispatch_calls.saturating_add(1);
            metrics.dispatch_nanos = metrics.dispatch_nanos.saturating_add(elapsed);
            if fn_id == FN_EVAL_EXPR {
                metrics.eval_expr_calls = metrics.eval_expr_calls.saturating_add(1);
                metrics.eval_expr_nanos = metrics.eval_expr_nanos.saturating_add(elapsed);
            }
            if fn_id == FN_EVAL_STMT {
                metrics.eval_stmt_calls = metrics.eval_stmt_calls.saturating_add(1);
                metrics.eval_stmt_nanos = metrics.eval_stmt_nanos.saturating_add(elapsed);
            }
        });
    }

    result
}

/// Dispatch dinâmico por string para callees não resolvidos em tempo de compilação.
/// Assinatura diferente de __rts_dispatch: recebe callee como (ptr, len) + argc + 6 slots.
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
    let started = Instant::now();
    let Some(callee) = read_utf8(callee_ptr, callee_len) else {
        return UNDEFINED_HANDLE;
    };

    let slots = [a0, a1, a2, a3, a4, a5];
    let count = argc.clamp(0, slots.len() as i64) as usize;
    let mut args = Vec::with_capacity(count);
    for handle in slots.into_iter().take(count) {
        args.push(read_value(handle));
    }

    crate::namespaces::gc::enter_scope();
    let Some(outcome) = crate::namespaces::rust::dispatch_runtime_call(callee.as_str(), &args)
    else {
        crate::namespaces::gc::exit_scope();
        return UNDEFINED_HANDLE;
    };

    let result = match outcome {
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
    };

    let elapsed = started.elapsed().as_nanos();
    RUNTIME_METRICS.with(|metrics| {
        let mut metrics = metrics.borrow_mut();
        metrics.call_dispatch_calls = metrics.call_dispatch_calls.saturating_add(1);
        metrics.call_dispatch_nanos = metrics.call_dispatch_nanos.saturating_add(elapsed);
    });
    crate::namespaces::gc::exit_scope();

    result
}

#[cfg(test)]
mod tests {
    use super::{
        RuntimeValue, bind_runtime_identifier_value, read_runtime_value, reset_thread_state,
        resolve_runtime_identifier_binding, with_store, write_runtime_identifier_value,
    };

    #[test]
    fn mutable_write_updates_existing_slot() {
        reset_thread_state();
        bind_runtime_identifier_value("counter", RuntimeValue::Number(1.0), true);

        let before = with_store(|store| store.values.len());
        write_runtime_identifier_value("counter", RuntimeValue::Number(2.0));
        let after = with_store(|store| store.values.len());
        let handle = resolve_runtime_identifier_binding("counter")
            .map(|binding| binding.handle)
            .unwrap_or(0);

        assert_eq!(before, after);
        assert_eq!(read_runtime_value(handle), RuntimeValue::Number(2.0));
    }

    #[test]
    fn const_write_keeps_original_value() {
        reset_thread_state();
        bind_runtime_identifier_value("base", RuntimeValue::Number(7.0), false);

        write_runtime_identifier_value("base", RuntimeValue::Number(99.0));
        let handle = resolve_runtime_identifier_binding("base")
            .map(|binding| binding.handle)
            .unwrap_or(0);

        assert_eq!(read_runtime_value(handle), RuntimeValue::Number(7.0));
    }
}
