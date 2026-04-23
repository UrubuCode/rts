//! Contadores de telemetria do dispatch: tempos agregados por call, per-fn_id
//! breakdown, eval/call breakdown. Renderizados por `--dump-statistics`.
//!
//! Por padrao a coleta fica desligada (hot path de ~80ns nao pode pagar 2
//! syscalls QPC + RefCell borrow por call). O CLI ativa via
//! `set_dispatch_metrics_enabled(true)`.

use std::cell::RefCell;

use super::fn_ids::FN_ID_COUNT;

#[derive(Debug, Clone)]
pub(super) struct RuntimeMetrics {
    pub(super) dispatch_calls: u64,
    pub(super) dispatch_nanos: u128,
    pub(super) eval_expr_calls: u64,
    pub(super) eval_expr_nanos: u128,
    pub(super) eval_stmt_calls: u64,
    pub(super) eval_stmt_nanos: u128,
    pub(super) call_dispatch_calls: u64,
    pub(super) call_dispatch_nanos: u128,
    /// Breakdown por `fn_id` do `__rts_dispatch`. Indexados pelas constantes
    /// `FN_*`. Usado por `--dump-statistics` para mostrar tempo gasto em
    /// cada ponto de dispatch separadamente.
    pub(super) per_fn_calls: [u64; FN_ID_COUNT],
    pub(super) per_fn_nanos: [u128; FN_ID_COUNT],
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
    pub(super) static RUNTIME_METRICS: RefCell<RuntimeMetrics> =
        RefCell::new(RuntimeMetrics::default());
}

static DISPATCH_METRICS_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

#[inline(always)]
pub(super) fn metrics_enabled() -> bool {
    DISPATCH_METRICS_ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}

pub(crate) fn dispatch_debug_enabled() -> bool {
    metrics_enabled()
}

pub(crate) fn set_dispatch_metrics_enabled(enabled: bool) {
    DISPATCH_METRICS_ENABLED.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

pub(super) fn reset_runtime_metrics() {
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
