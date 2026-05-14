//! Thread-local runtime error slot for `throw` / `try-catch`.
//!
//! When a value is thrown, the current TS call frame stack is captured
//! (from `trace::frame_stack`) and stored as a GC string handle so uncaught
//! throws can be reported with a formatted stack trace.

use std::cell::RefCell;

use super::handles::{Entry, alloc_entry, free_handle, with_entry};
use super::string_pool::read_string_handle;
use rts_abi::JsErrorKind;

#[derive(Clone, Copy, Debug, Default)]
struct ErrorSlot {
    message: u64,
    stack: u64,
}

/// Structured stack frame captured at throw time.
#[derive(Debug, Clone)]
pub struct StackFrame {
    pub file: String,
    pub fn_name: String,
    pub line: u32,
    pub col: u32,
}

/// Typed runtime error report — replaces the old unstructured string pair.
#[derive(Debug, Clone)]
pub struct RuntimeErrorReport {
    pub kind: JsErrorKind,
    pub message: String,
    pub frames: Vec<StackFrame>,
    /// Chained cause, if the thrown Error had `{ cause }`.
    pub cause: Option<Box<RuntimeErrorReport>>,
}

thread_local! {
    static ERROR_SLOT: RefCell<ErrorSlot> = const { RefCell::new(ErrorSlot { message: 0, stack: 0 }) };
}

fn capture_stack_handle() -> u64 {
    let text = crate::namespaces::trace::frame_stack::capture_string();
    if text.is_empty() {
        return 0;
    }
    alloc_entry(Entry::String(text.into_bytes()))
}


fn free_handle_if_any(handle: u64) {
    if handle != 0 {
        let _ = free_handle(handle);
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

// (#745) Mapa thread-local handle->stack_text. Antes de CLEAR liberar
// o stack handle pendente, copiamos o conteudo aqui para que
// `e.stack` possa ler dentro do catch body.
thread_local! {
    static ERR_STACKS: std::cell::RefCell<std::collections::HashMap<u64, String>>
        = std::cell::RefCell::new(std::collections::HashMap::new());
}

pub fn stack_for_handle(handle: u64) -> Option<String> {
    ERR_STACKS.with(|m| m.borrow().get(&handle).cloned())
}

/// Clears pending runtime error and releases captured stack handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_ERROR_CLEAR() {
    ERROR_SLOT.with(|slot| {
        let mut slot = slot.borrow_mut();
        // (#745) Salva stack para ler dentro do catch body — RTS_FN_GL_ERROR_STACK
        // consulta ERR_STACKS por handle do thrown value.
        let msg = slot.message;
        let stk = slot.stack;
        if msg != 0 && stk != 0 {
            let text = with_entry(stk, |e| match e {
                Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
                _ => String::new(),
            });
            if !text.is_empty() {
                ERR_STACKS.with(|m| m.borrow_mut().insert(msg, text));
            }
        }
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

    // Try to extract structured ErrorObj data first (most common case: `throw new TypeError(...)`).
    let structured = with_entry(message_h, |entry| match entry {
        Some(Entry::ErrorObj { name, message, cause }) => Some((
            JsErrorKind::from_name(name),
            message.clone(),
            *cause,
        )),
        _ => None,
    });

    let (kind, message, cause_h) = if let Some((k, m, c)) = structured {
        (k, m, c)
    } else {
        // Thrown value is a plain string or unknown handle.
        let msg = read_string_handle(message_h)
            .unwrap_or_else(|| format!("<non-string throw, handle={message_h}>"));
        (JsErrorKind::Error, msg, 0u64)
    };

    // Structured frames captured at throw time (stored in stack handle as text,
    // but we also re-read from the live frame stack if still available).
    let frames = if stack_h != 0 {
        // Re-parse the pre-captured formatted stack into structured frames.
        // The canonical source is the stack handle string — parse "      at fn (file:line:col)"
        let text = read_string_handle(stack_h).unwrap_or_default();
        free_handle_if_any(stack_h);
        parse_stack_frames(&text)
    } else {
        Vec::new()
    };

    // Recursive cause.
    let cause = if cause_h != 0 {
        extract_error_report_from_handle(cause_h)
    } else {
        None
    };

    Some(RuntimeErrorReport { kind, message, frames, cause })
}

fn extract_error_report_from_handle(handle: u64) -> Option<Box<RuntimeErrorReport>> {
    with_entry(handle, |entry| match entry {
        Some(Entry::ErrorObj { name, message, cause }) => Some(Box::new(RuntimeErrorReport {
            kind: JsErrorKind::from_name(name),
            message: message.clone(),
            frames: Vec::new(),
            cause: if *cause != 0 { extract_error_report_from_handle(*cause) } else { None },
        })),
        _ => None,
    })
}

/// Parse "      at fn (file:line:col)\n" lines into StackFrame vec.
fn parse_stack_frames(text: &str) -> Vec<StackFrame> {
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let rest = trimmed.strip_prefix("at ")?;
            // "fnName (file:line:col)" or "fnName (file)"
            if let Some(paren_open) = rest.rfind('(') {
                let fn_name = rest[..paren_open].trim().to_owned();
                let inner = rest[paren_open + 1..].trim_end_matches(')');
                // Try "file:line:col"
                let mut parts = inner.rsplitn(3, ':');
                let col = parts.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
                let line_n = parts.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
                let file = parts.next().unwrap_or(inner).to_owned();
                Some(StackFrame { file, fn_name, line: line_n, col })
            } else {
                // No parens — just "at file"
                Some(StackFrame { file: rest.to_owned(), fn_name: String::new(), line: 0, col: 0 })
            }
        })
        .collect()
}
