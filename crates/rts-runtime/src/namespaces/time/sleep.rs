//! Blocking sleeps via `std::thread::sleep`.

use std::thread;
use std::time::Duration;

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TIME_SLEEP_MS(ms: i64) {
    // (#207 timer ordering) sleep eh ponto de quiescencia: drena microtasks,
    // setImmediate e setTimeout(0) pendentes ANTES de dormir. Assim
    // `setImmediate(cb); sleep_ms(20)` ve o efeito de cb — o setImmediate nao
    // spawna mais thread (rodava em paralelo), entao precisa ser drenado aqui.
    crate::namespaces::globals::text_encoding::instance::drain_microtasks();
    crate::namespaces::globals::timers::instance::drain_immediates();
    crate::namespaces::globals::timers::instance::drain_macrotasks();
    if ms > 0 {
        thread::sleep(Duration::from_millis(ms as u64));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TIME_SLEEP_NS(ns: i64) {
    if ns > 0 {
        thread::sleep(Duration::from_nanos(ns as u64));
    }
}
