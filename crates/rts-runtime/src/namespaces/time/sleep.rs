//! Blocking sleeps via `std::thread::sleep`.

use std::thread;
use std::time::Duration;

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TIME_SLEEP_MS(ms: i64) {
    // (#207 timer ordering / cross-runtime #393) sleep eh ponto de quiescencia
    // do event loop: faz pump dirigido por tempo ate `target`, disparando
    // microtasks, setImmediate e setTimeout (delay 0 e >0) que vencerem dentro
    // do intervalo — em ordem (deadline, seq) deterministica. Assim
    // `setImmediate(cb); sleep_ms(20)` e `setTimeout(cb,10); sleep_ms(50)`
    // veem o efeito de cb, sem thread-per-timer (que disparava fora de ordem).
    let target = std::time::Instant::now() + Duration::from_millis(ms.max(0) as u64);
    crate::namespaces::globals::timers::instance::pump_until(target);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TIME_SLEEP_NS(ns: i64) {
    if ns > 0 {
        thread::sleep(Duration::from_nanos(ns as u64));
    }
}
