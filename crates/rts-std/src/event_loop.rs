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
    // Filesystem watchers (node:fs watch/watchFile) keep the loop alive while any
    // watcher is open (Node semantics); each OS notification invokes the listener.
    drain_watch_events();
}

/// Drain `node:fs` watch events while any watcher is open, invoking each
/// listener `(eventType, filename)` on the JS thread. Two exits: `active_count`
/// reaching zero (a `watcher.close()`), or an IDLE window with no events — RTS
/// has no persistent loop and, given #195 (a listener can't capture its watcher
/// to close it), an idle watcher would otherwise pin the process forever. So the
/// loop delivers REAL FS events but returns once the watcher goes quiet for
/// `IDLE`; a watcher receiving a steady stream of changes stays alive (each event
/// resets the window). A 60 s absolute cap is a final safety net.
fn drain_watch_events() {
    use rts_engine::heap::handles::{alloc_entry, Entry};
    use rts_engine::heap::shapes::string_word;
    use rts_engine::watch_queue;
    use std::time::{Duration, Instant};

    unsafe extern "C" {
        fn __RTS_FN_RT_INVOKE_AUTO(callee: i64, this_arg: i64, args_handle: u64) -> i64;
        // watchFile change → node-side fire (builds the curr/prev Stats args the
        // listener expects; rts-std cannot build a Stats — that lives in rts-node).
        fn __RTS_FN_NODE_FS_WATCHFILE_FIRE(listener: u64, path_ptr: *const u8, path_len: i64);
    }

    const IDLE: Duration = Duration::from_millis(1500);
    if watch_queue::active_count() == 0 {
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut last_activity = Instant::now();
    while watch_queue::active_count() > 0 && Instant::now() < deadline {
        let events = watch_queue::drain();
        if !events.is_empty() {
            last_activity = Instant::now();
        } else if Instant::now() - last_activity > IDLE {
            break;
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
        // A listener may have scheduled microtasks/timers (or closed its watcher).
        crate::globals::text_encoding::instance::drain_microtasks();
        std::thread::sleep(Duration::from_millis(15));
    }
}

/// Símbolo `extern "C"` chamado pelo shim `main` do AOT (e disponível ao JIT por
/// link). Roda o mesmo `run_event_loop` do caminho JIT.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_RUN_EVENT_LOOP() {
    run_event_loop();
}
