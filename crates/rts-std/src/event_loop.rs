//! Epílogo do event loop JS — drena microtasks + setImmediate + macrotasks +
//! timers + promises fire-and-forget pendentes após o task corrente (top-level).
//!
//! O caminho JIT (`rts-codegen::pipeline::run_jit`) chama isto host-side depois
//! de `__RTS_MAIN`. O binário AOT (`rts compile`) NÃO tinha event loop — o shim
//! `main` só chamava `__RTS_MAIN` e saía, então `await`/`.then`/`queueMicrotask`/
//! `setTimeout` nunca disparavam. Agora o shim AOT chama
//! `__RTS_FN_RT_RUN_EVENT_LOOP` (este extern) antes do return, fechando o gap.

/// Ordem JS spec: microtasks ao fim do task corrente; depois setImmediate
/// (check phase); depois macrotasks (setTimeout delay-0) que por sua vez drenam
/// suas microtasks; depois timers pendentes; depois promises fire-and-forget
/// (async fns sem await no top-level); por fim as microtasks remanescentes.
pub fn run_event_loop() {
    crate::globals::text_encoding::instance::drain_microtasks();
    crate::globals::timers::instance::drain_immediates();
    crate::globals::timers::instance::drain_macrotasks();
    crate::globals::timers::instance::drain_pending_timers();
    crate::promise::drain_pending_promises();
    crate::globals::text_encoding::instance::drain_microtasks();
    // (unhandled rejection) Após TODOS os drains — todo handler que ia anexar já
    // anexou — reporta as Promises rejeitadas que nunca tiveram handler.
    crate::promise::report_unhandled_rejections();
}

/// Símbolo `extern "C"` chamado pelo shim `main` do AOT (e disponível ao JIT por
/// link). Roda o mesmo `run_event_loop` do caminho JIT.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_RUN_EVENT_LOOP() {
    run_event_loop();
}
