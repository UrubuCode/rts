//! `backtrace` runtime ops.

use super::super::gc::handles::{Entry, table};
use std::backtrace::{Backtrace, BacktraceStatus};

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BACKTRACE_CAPTURE() -> u64 {
    let bt = Backtrace::force_capture();
    table().lock().unwrap().alloc(Entry::Backtrace(Box::new(bt)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BACKTRACE_CAPTURE_IF_ENABLED() -> u64 {
    let bt = Backtrace::capture();
    if matches!(bt.status(), BacktraceStatus::Captured) {
        table().lock().unwrap().alloc(Entry::Backtrace(Box::new(bt)))
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BACKTRACE_IS_ENABLED() -> i64 {
    // Backtrace::capture() retorna Captured ou Disabled/Unsupported.
    // Forcar capture e checar status é o jeito portavel.
    match Backtrace::capture().status() {
        BacktraceStatus::Captured => 1,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BACKTRACE_TO_STRING(handle: u64) -> u64 {
    let s = {
        let guard = table().lock().unwrap();
        match guard.get(handle) {
            Some(Entry::Backtrace(bt)) => format!("{}", bt),
            _ => return 0,
        }
    };
    table().lock().unwrap().alloc(Entry::String(s.into_bytes()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_BACKTRACE_FREE(handle: u64) {
    let _ = table().lock().unwrap().free(handle);
}
