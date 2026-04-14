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
pub(crate) const FN_BOX_NATIVE_FN: i64 = 21; // (ptr, len) -> handle to NativeFunction
pub(crate) const FN_CALL_BY_HANDLE: i64 = 22; // (fn_handle, argc, a0..a5) -> i64
pub(crate) const FN_NEW_INSTANCE: i64 = 23; // (class_ptr, class_len) -> object_handle (fields vazios)
pub(crate) const FN_LOAD_FIELD: i64 = 24; // (obj_handle, field_ptr, field_len) -> value_handle
pub(crate) const FN_STORE_FIELD: i64 = 25; // (obj_handle, field_ptr, field_len, value_handle) -> 1/0
pub(crate) const FN_PIN_HANDLE: i64 = 26; // (handle) -> handle
pub(crate) const FN_UNPIN_HANDLE: i64 = 27; // (handle) -> handle
pub(crate) const FN_COMPACT_EXCLUDING: i64 = 28; // (handle) -> freed count

/// Numero total de FN_* distintos. Usado como tamanho dos arrays de metricas
/// por-fn_id em `RuntimeMetrics`.
pub(crate) const FN_ID_COUNT: usize = 29;

/// Mapeia `fn_id` para nome legivel. Usado pela renderizacao de
/// `--dump-statistics` para mostrar tempo gasto em cada ponto de dispatch
/// separadamente. Indices fora do range retornam `"unknown"`.
pub fn fn_id_label(fn_id: i64) -> &'static str {
    match fn_id {
        0 => "reset_thread_state",
        1 => "bind_identifier",
        2 => "box_string",
        3 => "box_bool",
        4 => "eval_expr",
        5 => "eval_stmt",
        6 => "read_identifier",
        7 => "binop",
        8 => "is_truthy",
        9 => "unbox_number",
        10 => "box_number",
        11 => "io.print",
        12 => "io.stdout_write",
        13 => "io.stderr_write",
        14 => "io.panic",
        15 => "crypto.sha256",
        16 => "process.exit",
        17 => "global.set",
        18 => "global.get",
        19 => "global.has",
        20 => "global.remove",
        21 => "box_native_fn",
        22 => "call_by_handle",
        23 => "new_instance",
        24 => "load_field",
        25 => "store_field",
        26 => "pin_handle",
        27 => "unpin_handle",
        _ => "unknown",
    }
}

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
    /// Vec de slots opcionais. Slots marcados como `None` foram liberados
    /// por compactacao mas ficam reservados no Vec para preservar os
    /// indices (o handle e o `index + 1`, entao re-indexar quebraria
    /// ponteiros em uso).
    values: Vec<Option<RuntimeValue>>,
    bindings: FxHashMap<String, BindingEntry>,
    /// Free list of indices into `values` where slot is None.
    /// Allows O(1) reuse of compacted slots instead of growing the Vec.
    free_slots: Vec<usize>,
    /// Contador de compactacoes executadas. Reportado em `--dump-statistics`.
    compactions: u64,
    /// Total de slots liberados atraves de todas as compactacoes.
    /// Monotonicamente crescente.
    slots_freed: u64,
    /// Handles temporariamente protegidos contra compactacao.
    /// Refcount permite pinagem repetida no mesmo handle.
    pinned_handles: FxHashMap<i64, u32>,
}

impl ValueStore {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn allocate_value(&mut self, value: RuntimeValue) -> i64 {
        if matches!(value, RuntimeValue::Undefined) {
            return UNDEFINED_HANDLE;
        }

        // O(1) reuse of compacted slots via free list.
        if let Some(idx) = self.free_slots.pop() {
            self.values[idx] = Some(value);
            crate::namespaces::gc::notify_alloc();
            return (idx + 1) as i64;
        }

        self.values.push(Some(value));
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
            .and_then(|slot| slot.clone())
            .unwrap_or(RuntimeValue::Undefined)
    }

    /// Compactacao leve: percorre os slots, libera (`= None`) os que nao
    /// sao referenciados por nenhum binding ativo. Nao re-indexa — os
    /// handles permanecem validos, apenas os valores sao soltados.
    ///
    /// Critico: so pode ser chamado em um ponto de quiescencia top-level
    /// (scope_depth == 0) porque handles vivos num registrador do JIT nao
    /// sao visiveis aqui. No caminho atual, e disparado por `exit_scope()`
    /// quando a ultima funcao TS volta ao top-level.
    fn compact_excluding(&mut self, exclude: i64) -> (usize, usize) {
        use std::collections::HashSet;
        let live: HashSet<i64> = self.bindings.values().map(|b| b.handle).collect();

        let mut freed = 0usize;
        for (idx, slot) in self.values.iter_mut().enumerate() {
            let handle = (idx + 1) as i64;
            let is_pinned = self.pinned_handles.get(&handle).copied().unwrap_or(0) > 0;
            if slot.is_some() && handle != exclude && !is_pinned && !live.contains(&handle) {
                *slot = None;
                self.free_slots.push(idx);
                freed += 1;
            }
        }

        if freed > 0 {
            self.compactions += 1;
            self.slots_freed += freed as u64;
        }

        // Retorna tambem o tamanho atual para logging.
        (freed, self.values.len())
    }

    fn compact(&mut self) -> (usize, usize) {
        self.compact_excluding(UNDEFINED_HANDLE)
    }

    fn pin_handle(&mut self, handle: i64) -> i64 {
        if handle <= UNDEFINED_HANDLE {
            return handle;
        }
        let entry = self.pinned_handles.entry(handle).or_insert(0);
        *entry = entry.saturating_add(1);
        handle
    }

    fn unpin_handle(&mut self, handle: i64) -> i64 {
        if handle <= UNDEFINED_HANDLE {
            return handle;
        }
        if let Some(entry) = self.pinned_handles.get_mut(&handle) {
            if *entry > 1 {
                *entry -= 1;
            } else {
                self.pinned_handles.remove(&handle);
            }
        }
        handle
    }

    fn bind_identifier(&mut self, name: &str, handle: i64, mutable: bool) -> i64 {
        // Fast path: re-binding de uma global já conhecida — atualiza o slot
        // sem nenhuma alocação de String nem clone do valor.
        if let Some(existing) = self.bindings.get_mut(name) {
            if !existing.mutable {
                return existing.handle;
            }
            existing.handle = handle;
            existing.mutable = mutable;
            return handle;
        }

        self.bindings
            .insert(name.to_string(), BindingEntry { handle, mutable });
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
                    *slot = Some(value);
                    return existing.handle;
                }
            }
        }

        let handle = self.allocate_value(value);
        self.bindings
            .insert(name.to_string(), BindingEntry { handle, mutable });
        handle
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
                    *slot = Some(value);
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
            *slot = Some(value);
            return true;
        }
        false
    }
}

#[derive(Debug, Clone)]
struct RuntimeMetrics {
    dispatch_calls: u64,
    dispatch_nanos: u128,
    eval_expr_calls: u64,
    eval_expr_nanos: u128,
    eval_stmt_calls: u64,
    eval_stmt_nanos: u128,
    call_dispatch_calls: u64,
    call_dispatch_nanos: u128,
    /// Breakdown por `fn_id` do `__rts_dispatch`. Indexados pelas constantes
    /// `FN_*`. Usado por `--dump-statistics` para mostrar tempo gasto em
    /// cada ponto de dispatch separadamente.
    per_fn_calls: [u64; FN_ID_COUNT],
    per_fn_nanos: [u128; FN_ID_COUNT],
}

impl Default for RuntimeMetrics {
    fn default() -> Self {
        Self {
            dispatch_calls: 0,
            dispatch_nanos: 0,
            eval_expr_calls: 0,
            eval_expr_nanos: 0,
            eval_stmt_calls: 0,
            eval_stmt_nanos: 0,
            call_dispatch_calls: 0,
            call_dispatch_nanos: 0,
            per_fn_calls: [0; FN_ID_COUNT],
            per_fn_nanos: [0; FN_ID_COUNT],
        }
    }
}

#[derive(Debug, Clone)]
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
    /// Tempo/chamadas por `fn_id`. Ordem igual aos indices das constantes
    /// `FN_*`. Renderizado linha a linha em `--dump-statistics` com o nome
    /// devolvido por `fn_id_label()`.
    pub per_fn_calls: [u64; FN_ID_COUNT],
    pub per_fn_nanos: [u128; FN_ID_COUNT],
}

impl Default for RuntimeMetricsSnapshot {
    fn default() -> Self {
        Self {
            dispatch_calls: 0,
            dispatch_nanos: 0,
            eval_expr_calls: 0,
            eval_expr_nanos: 0,
            eval_stmt_calls: 0,
            eval_stmt_nanos: 0,
            eval_parse_calls: 0,
            eval_parse_nanos: 0,
            eval_identifier_reads: 0,
            eval_identifier_writes: 0,
            eval_call_dispatches: 0,
            eval_binding_cache_hits: 0,
            eval_binding_cache_misses: 0,
            call_dispatch_calls: 0,
            call_dispatch_nanos: 0,
            per_fn_calls: [0; FN_ID_COUNT],
            per_fn_nanos: [0; FN_ID_COUNT],
        }
    }
}

thread_local! {
    static VALUE_STORE: RefCell<ValueStore> = RefCell::new(ValueStore::default());
    static RUNTIME_METRICS: RefCell<RuntimeMetrics> = RefCell::new(RuntimeMetrics::default());
    static JIT_FN_TABLE: RefCell<rustc_hash::FxHashMap<String, usize>> =
        RefCell::new(rustc_hash::FxHashMap::default());
}

/// Called by the JIT after finalization to register all function pointers.
pub fn register_jit_fn_table(table: rustc_hash::FxHashMap<String, usize>) {
    JIT_FN_TABLE.with(|t| *t.borrow_mut() = table);
}

/// Call a user-defined JIT function by name with zero arguments.
pub(crate) fn call_jit_fn_by_name(name: &str) -> i64 {
    JIT_FN_TABLE.with(|table| {
        let table = table.borrow();
        if let Some(&ptr) = table.get(name) {
            let f = unsafe {
                std::mem::transmute::<
                    usize,
                    extern "C" fn(i64, i64, i64, i64, i64, i64, i64) -> i64,
                >(ptr)
            };
            f(0, 0, 0, 0, 0, 0, 0)
        } else {
            UNDEFINED_HANDLE
        }
    })
}

static DISPATCH_METRICS_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[inline(always)]
fn metrics_enabled() -> bool {
    DISPATCH_METRICS_ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}

pub(crate) fn dispatch_debug_enabled() -> bool {
    metrics_enabled()
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

/// Snapshot leve das metricas do ValueStore thread-local.
/// Usado por `--dump-statistics` para reportar uso do store sem vazar
/// detalhes internos de layout.
#[derive(Debug, Clone, Copy, Default)]
pub struct ValueStoreStats {
    /// Numero de slots no Vec (handles alocados, incluindo os liberados
    /// por compactacao — slots ficam reservados mesmo quando mortos para
    /// preservar indices).
    pub values_len: usize,
    /// Slots que tem valor efetivamente (nao `None`). `values_len - live_slots`
    /// = slots liberados pela compactacao aguardando reuso/trunc.
    pub live_slots: usize,
    /// Numero de bindings nomeados registrados (cada binding aponta para
    /// um handle).
    pub bindings_len: usize,
    /// Quantas compactacoes foram executadas nesta thread.
    pub compactions: u64,
    /// Total de slots liberados atraves de todas as compactacoes.
    pub slots_freed: u64,
}

/// Retorna as metricas atuais do ValueStore da thread corrente.
pub fn value_store_stats() -> ValueStoreStats {
    with_store(|store| ValueStoreStats {
        values_len: store.values.len(),
        live_slots: store.values.iter().filter(|s| s.is_some()).count(),
        bindings_len: store.bindings.len(),
        compactions: store.compactions,
        slots_freed: store.slots_freed,
    })
}

/// Dispara compactacao do ValueStore thread-local: libera slots nao
/// referenciados por nenhum binding. Seguro apenas em quiescencia top-level
/// (scope_depth == 0) — chamado automaticamente pelo `exit_scope` do GC.
pub fn compact_value_store() -> (usize, usize) {
    with_store_mut(ValueStore::compact)
}

pub fn compact_value_store_excluding(exclude: i64) -> (usize, usize) {
    with_store_mut(|store| store.compact_excluding(exclude))
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
            per_fn_calls: metrics.per_fn_calls,
            per_fn_nanos: metrics.per_fn_nanos,
        }
    })
}

fn push_value(value: RuntimeValue) -> i64 {
    with_store_mut(|store| store.allocate_value(value))
}

fn read_value(handle: i64) -> RuntimeValue {
    with_store(|store| store.read_value(handle))
}

fn bind_identifier(name: &str, handle: i64, mutable: bool) -> i64 {
    with_store_mut(|store| store.bind_identifier(name, handle, mutable))
}

fn pin_value_handle(handle: i64) -> i64 {
    with_store_mut(|store| store.pin_handle(handle))
}

fn unpin_value_handle(handle: i64) -> i64 {
    with_store_mut(|store| store.unpin_handle(handle))
}

/// Versão rápida de FN_READ_IDENTIFIER usada no hot path: devolve o
/// handle existente diretamente, sem clonar o `RuntimeValue` nem alocar
/// um novo slot no `values` vec. Handles são opacos para o chamador e o
/// backing storage permanece estável enquanto o binding não for
/// sobrescrito via WriteBind (que cria um handle novo, não muta in-place).
fn read_identifier_handle(name: &str) -> Option<i64> {
    with_store(|store| store.bindings.get(name).map(|entry| entry.handle))
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
    read_utf8_static(ptr, len).map(ToString::to_string)
}

/// Devolve um `&'static str` apontando para o data segment do módulo
/// emitido pelo codegen. Evita a alocação de `String` no hot path de
/// bind/read de identificadores.
#[inline]
fn read_utf8_static(ptr: i64, len: i64) -> Option<&'static str> {
    if ptr <= 0 || len < 0 {
        return None;
    }

    let ptr = ptr as *const u8;
    let len = len as usize;
    let bytes = unsafe {
        // SAFETY: `ptr` e `len` são emitidos pelo codegen RTS como referências
        // a dados estáticos no `.rdata`. Vivem enquanto o módulo estiver
        // carregado, o que é superset do tempo de execução das dispatches.
        std::slice::from_raw_parts(ptr, len)
    };

    std::str::from_utf8(bytes).ok()
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
            let Some(name) = read_utf8_static(a0, a1) else {
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
            let Some(name) = read_utf8_static(a0, a1) else {
                return UNDEFINED_HANDLE;
            };
            // Fast path: devolve o handle existente do binding diretamente,
            // sem clonar o RuntimeValue nem alocar um slot novo no `values`
            // vec. Handles são opacos para o chamador e o backing storage
            // permanece estável enquanto o binding não for sobrescrito.
            read_identifier_handle(name).unwrap_or(UNDEFINED_HANDLE)
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
        FN_BOX_NATIVE_FN => match read_utf8(a0, a1) {
            Some(name) => push_value(RuntimeValue::NativeFunction(name)),
            None => UNDEFINED_HANDLE,
        },
        FN_CALL_BY_HANDLE => {
            // a0 = fn_handle, a1 = argc, a2..a7 = arg handles
            let fn_value = read_value(a0);
            match fn_value {
                RuntimeValue::NativeFunction(fn_name) => {
                    call_jit_fn_by_name(&fn_name)
                }
                _ => UNDEFINED_HANDLE,
            }
        }
        FN_NEW_INSTANCE => {
            // a0 = class_name_ptr, a1 = class_name_len
            // Por enquanto ignoramos o class_name — criamos um Object vazio
            // e deixamos o constructor (chamado separadamente pelo codegen)
            // popular os campos. O nome da classe servirá no futuro para
            // lookup de metadata / checagem de tipo em runtime.
            let _ = read_utf8(a0, a1);
            push_value(RuntimeValue::Object(std::collections::BTreeMap::new()))
        }
        FN_LOAD_FIELD => {
            // a0 = obj_handle, a1 = field_ptr, a2 = field_len
            let Some(field) = read_utf8(a1, a2) else {
                return UNDEFINED_HANDLE;
            };
            let value = read_value(a0)
                .get_property(&field)
                .unwrap_or(RuntimeValue::Undefined);
            push_value(value)
        }
        FN_STORE_FIELD => {
            // a0 = obj_handle, a1 = field_ptr, a2 = field_len, a3 = value_handle
            let Some(field) = read_utf8(a1, a2) else {
                return 0;
            };
            let new_value = read_value(a3);
            with_store_mut(|store| {
                // Handle = index+1; handle 0 reservado para Undefined.
                if a0 <= 0 {
                    return 0i64;
                }
                let index = (a0 - 1) as usize;
                let Some(slot) = store.values.get_mut(index) else {
                    return 0i64;
                };
                if let Some(RuntimeValue::Object(map)) = slot {
                    map.insert(field, new_value);
                    1
                } else {
                    0
                }
            })
        }
        FN_PIN_HANDLE => pin_value_handle(a0),
        FN_UNPIN_HANDLE => unpin_value_handle(a0),
        FN_COMPACT_EXCLUDING => {
            let (freed, _) = compact_value_store_excluding(a0);
            freed as i64
        }
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
            // Breakdown por fn_id: incrementa apenas se o id cai na faixa
            // conhecida, senao o total agregado em `dispatch_*` ja cobre.
            if fn_id >= 0 && (fn_id as usize) < FN_ID_COUNT {
                let idx = fn_id as usize;
                metrics.per_fn_calls[idx] = metrics.per_fn_calls[idx].saturating_add(1);
                metrics.per_fn_nanos[idx] = metrics.per_fn_nanos[idx].saturating_add(elapsed);
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
        RuntimeValue, FN_BOX_NUMBER, FN_BOX_STRING, FN_PIN_HANDLE, FN_UNPIN_HANDLE, bind_runtime_identifier_value,
        compact_value_store, read_runtime_value, read_utf8_static, reset_thread_state,
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

    #[test]
    fn call_dispatch_keeps_pinned_transient_handles() {
        reset_thread_state();

        let transient = super::__rts_dispatch(
            FN_BOX_NUMBER,
            i64::from_ne_bytes(42.0f64.to_ne_bytes()),
            0,
            0,
            0,
            0,
            0,
        );
        super::__rts_dispatch(FN_PIN_HANDLE, transient, 0, 0, 0, 0, 0);

        let callee = b"process.arch";
        let _ = super::__rts_call_dispatch(
            callee.as_ptr() as i64,
            callee.len() as i64,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        );

        assert_eq!(read_runtime_value(transient), RuntimeValue::Number(42.0));
        super::__rts_dispatch(FN_UNPIN_HANDLE, transient, 0, 0, 0, 0, 0);
    }

    #[test]
    fn compact_respects_pinned_handles_until_unpinned() {
        reset_thread_state();
        let handle = super::__rts_dispatch(
            FN_BOX_NUMBER,
            i64::from_ne_bytes(7.0f64.to_ne_bytes()),
            0,
            0,
            0,
            0,
            0,
        );
        super::__rts_dispatch(FN_PIN_HANDLE, handle, 0, 0, 0, 0, 0);
        let _ = compact_value_store();
        assert_eq!(read_runtime_value(handle), RuntimeValue::Number(7.0));

        super::__rts_dispatch(FN_UNPIN_HANDLE, handle, 0, 0, 0, 0, 0);
        let _ = compact_value_store();
        assert_eq!(read_runtime_value(handle), RuntimeValue::Undefined);
    }

    #[test]
    fn unpinned_transient_survives_call_dispatch_when_excluded() {
        // Regression test for issue #5: an unpinned transient handle (simulating
        // a temporary from a prior dynamic call in a multi-call expression)
        // must survive a subsequent __rts_call_dispatch if it is pinned before
        // the call. This test verifies the pin/unpin mechanism works for
        // temporaries that the codegen now pins via vreg_map scanning.
        reset_thread_state();

        // Simulate first dynamic call result (e.g., process.arch())
        let first_result = super::__rts_dispatch(
            FN_BOX_STRING,
            b"x86_64".as_ptr() as i64,
            6,
            0,
            0,
            0,
            0,
        );
        // Pin it (as the codegen now does for vreg_map temporaries)
        super::__rts_dispatch(FN_PIN_HANDLE, first_result, 0, 0, 0, 0, 0);

        // Simulate second dynamic call (e.g., another process.arch())
        let callee = b"process.arch";
        let _second_result = super::__rts_call_dispatch(
            callee.as_ptr() as i64,
            callee.len() as i64,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        );

        // First result must still be alive
        assert_eq!(
            read_runtime_value(first_result),
            RuntimeValue::String("x86_64".to_string())
        );
        super::__rts_dispatch(FN_UNPIN_HANDLE, first_result, 0, 0, 0, 0, 0);
    }

    #[test]
    fn read_utf8_static_rejects_negative_ptr() {
        assert!(read_utf8_static(-1, 5).is_none());
        assert!(read_utf8_static(-100, 0).is_none());
        assert!(read_utf8_static(i64::MIN, 10).is_none());
    }

    #[test]
    fn read_utf8_static_rejects_zero_ptr() {
        assert!(read_utf8_static(0, 5).is_none());
    }

    #[test]
    fn read_utf8_static_rejects_negative_len() {
        let data = b"hello";
        let ptr = data.as_ptr() as i64;
        assert!(read_utf8_static(ptr, -1).is_none());
        assert!(read_utf8_static(ptr, i64::MIN).is_none());
    }
}
