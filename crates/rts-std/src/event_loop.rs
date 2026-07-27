//! Epílogo do event loop JS — drena microtasks + setImmediate + macrotasks +
//! timers + promises fire-and-forget pendentes após o task corrente (top-level).
//!
//! O caminho JIT (`rts-codegen::pipeline::run_jit`) chama isto host-side depois
//! de `__rts_startup`. O binário AOT (`rts compile`) NÃO tinha event loop — o shim
//! `main` só chamava `__rts_startup` e saía, então `await`/`.then`/`queueMicrotask`/
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
    // Native event sources keep the loop alive while any of their resources is
    // open (Node semantics): filesystem watchers (node:fs watch/watchFile) and
    // every source that registered a pump (node:dgram's UDP reader, …).
    drain_native_events();
}

/// Drain the native event sources while any of their resources is open,
/// delivering each event on the JS thread. Two families feed this:
///
///  - [`rts_engine::watch_queue`] — the fs-watch queue, whose plain-data events
///    this function turns into a `listener(eventType, filename)` invocation;
///  - [`rts_engine::loop_sources`] — the generic pump registry: each producer
///    drains its OWN typed queue when called here (node:dgram's `'message'`/
///    `'listening'`/… delivery), so this loop never names a module.
///
/// Two exits: the combined active count reaching zero (every watcher closed /
/// every socket closed or `unref`ed) AND the last pass having delivered nothing,
/// or an IDLE window with no events — RTS has no persistent loop and, given #195
/// (a listener can't capture its watcher to close it), an idle handle would
/// otherwise pin the process forever. So the loop delivers REAL events but
/// returns once every source goes quiet for `IDLE`; a steady stream keeps it
/// alive (each event resets the window). A 60 s absolute cap is a final safety
/// net.
///
/// The "delivered nothing" half of the first exit matters: closing the last
/// handle drops the active count to zero while its `'close'` event is still
/// queued, so the loop always makes one more pass and only leaves once a pass
/// comes back empty.
fn drain_native_events() {
    use rts_engine::heap::handles::{alloc_entry, Entry};
    use rts_engine::heap::shapes::string_word;
    use rts_engine::{loop_sources, watch_queue};
    use std::time::{Duration, Instant};

    unsafe extern "C" {
        fn __RTS_FN_RT_INVOKE_AUTO(callee: i64, this_arg: i64, args_handle: u64) -> i64;
        // watchFile change → node-side fire (builds the curr/prev Stats args the
        // listener expects; rts-std cannot build a Stats — that lives in rts-node).
        fn __RTS_FN_NODE_FS_WATCHFILE_FIRE(listener: u64, path_ptr: *const u8, path_len: i64);
    }

    const IDLE: Duration = Duration::from_millis(1500);
    let active = || watch_queue::active_count() + loop_sources::active_count();
    // No early return on a zero active count: a source can hold a QUEUED event
    // while holding no keep-alive (a socket whose bind failed queues its 'error'
    // and never becomes active). One pass always runs, and the exit condition
    // below leaves immediately when that pass comes back empty.
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut last_activity = Instant::now();
    loop {
        let events = watch_queue::drain();
        // Each source drains its own queue on this (the JS) thread.
        let delivered = loop_sources::pump_all();
        let busy = !events.is_empty() || delivered > 0;
        if busy {
            last_activity = Instant::now();
        }
        for ev in events {
            if ev.kind == 2 {
                // watchFile change → node builds the (curr, prev) Stats + invokes.
                unsafe {
                    __RTS_FN_NODE_FS_WATCHFILE_FIRE(ev.listener, ev.path.as_ptr(), ev.path.len() as i64);
                }
                continue;
            }
            // fs.watch: kind 0 = rename, 1 = change → listener(eventType, filename).
            let etype = if ev.kind == 0 { "rename" } else { "change" };
            let args = alloc_entry(Entry::Vec(Box::new(vec![
                string_word(etype.as_bytes()) as i64,
                string_word(ev.path.as_bytes()) as i64,
            ])));
            unsafe {
                __RTS_FN_RT_INVOKE_AUTO(ev.listener as i64, 0, args);
            }
        }
        // A listener may have scheduled microtasks (or closed its handle).
        crate::globals::text_encoding::instance::drain_microtasks();
        // Nothing left to keep the loop alive, and this pass was empty.
        if !busy && active() == 0 {
            break;
        }
        if Instant::now() >= deadline || (!busy && Instant::now() - last_activity > IDLE) {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Símbolo `extern "C"` chamado pelo shim `main` do AOT (e disponível ao JIT por
/// link). Roda o mesmo `run_event_loop` do caminho JIT.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_RUN_EVENT_LOOP() {
    run_event_loop();
}
