use super::frame_stack;
use super::super::gc::handles::{Entry, table};

unsafe fn str_from_raw(ptr: *const u8, len: usize) -> String {
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    String::from_utf8_lossy(slice).into_owned()
}

/// Push a TS call frame onto the thread-local trace stack.
/// Called manually from TS or (future) by codegen instrumentation.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TRACE_PUSH_FRAME(
    file_ptr: *const u8,
    file_len: i64,
    fn_ptr: *const u8,
    fn_len: i64,
    line: i64,
    col: i64,
) {
    let file = unsafe { str_from_raw(file_ptr, file_len as usize) };
    let fn_name = unsafe { str_from_raw(fn_ptr, fn_len as usize) };
    frame_stack::push(file, fn_name, line as u32, col as u32);
}

/// Pop the top TS call frame from the trace stack.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TRACE_POP_FRAME() {
    frame_stack::pop();
}

/// Capture current trace as a GC string handle.
/// Returns 0 if no frames are present.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TRACE_CAPTURE() -> u64 {
    let s = frame_stack::capture_string();
    if s.is_empty() {
        return 0;
    }
    table().lock().unwrap().alloc(Entry::String(s.into_bytes()))
}

/// Print current trace stack to stderr.
/// Prints "<no trace frames>" if the stack is empty.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TRACE_PRINT() {
    let s = frame_stack::format_stack();
    if s.is_empty() {
        eprintln!("<no trace frames>");
    } else {
        eprint!("{s}");
    }
}

/// Returns current trace stack depth (number of frames).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TRACE_DEPTH() -> i64 {
    frame_stack::depth() as i64
}

/// Free a GC handle returned by `trace.capture()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_TRACE_FREE(handle: u64) {
    let _ = table().lock().unwrap().free(handle);
}
