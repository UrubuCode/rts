//! Thread-local runtime error slot for `throw` / `try-catch`.
//!
//! When a value is thrown, the current TS call frame stack is captured
//! (from `trace::frame_stack`) and stored as a GC string handle so uncaught
//! throws can be reported with a formatted stack trace.

use std::cell::RefCell;

use super::handles::{Entry, table};
use super::string_pool::read_string_handle;

#[derive(Clone, Copy, Debug, Default)]
struct ErrorSlot {
    message: u64,
    stack: u64,
}

#[derive(Debug, Clone)]
pub struct RuntimeErrorReport {
    pub message: String,
    pub stack: Option<String>,
}

thread_local! {
    static ERROR_SLOT: RefCell<ErrorSlot> = const { RefCell::new(ErrorSlot { message: 0, stack: 0 }) };
}

fn capture_stack_handle() -> u64 {
    let text = crate::namespaces::trace::frame_stack::capture_string();
    if text.is_empty() {
        return 0;
    }
    table()
        .lock()
        .expect("handle table poisoned")
        .alloc(Entry::String(text.into_bytes()))
}

fn free_handle_if_any(handle: u64) {
    if handle != 0 {
        let _ = table().lock().expect("handle table poisoned").free(handle);
    }
}

/// Sets the pending runtime error (thrown value handle) and captures stack.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_ERROR_SET(handle: u64) {
    ERROR_SLOT.with(|slot| {
        let mut slot = slot.borrow_mut();
        free_handle_if_any(slot.stack);
        slot.message = handle;
        slot.stack = capture_stack_handle();
    });
}

/// Reads pending thrown value handle. `0` means no pending error.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_ERROR_GET() -> u64 {
    ERROR_SLOT.with(|slot| slot.borrow().message)
}

/// Reads pending stack handle associated with the error, `0` when none.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_ERROR_GET_STACK() -> u64 {
    ERROR_SLOT.with(|slot| slot.borrow().stack)
}

/// Clears pending runtime error and releases captured stack handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_ERROR_CLEAR() {
    ERROR_SLOT.with(|slot| {
        let mut slot = slot.borrow_mut();
        free_handle_if_any(slot.stack);
        slot.message = 0;
        slot.stack = 0;
    });
}

/// Takes the current runtime error report and clears the slot.
pub fn take_runtime_error_report() -> Option<RuntimeErrorReport> {
    let (message_h, stack_h) = ERROR_SLOT.with(|slot| {
        let mut slot = slot.borrow_mut();
        let handles = (slot.message, slot.stack);
        slot.message = 0;
        slot.stack = 0;
        handles
    });

    if message_h == 0 {
        return None;
    }

    let message =
        read_string_handle(message_h).unwrap_or_else(|| format!("<non-string error handle:{message_h}>"));
    let stack = if stack_h != 0 {
        let s = read_string_handle(stack_h);
        free_handle_if_any(stack_h);
        s
    } else {
        None
    };

    Some(RuntimeErrorReport { message, stack })
}
