use fltk::dialog;

use crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW;

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> &'a str {
    if ptr.is_null() || len < 0 { return ""; }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(bytes).unwrap_or("")
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_ALERT(ptr: *const u8, len: i64) {
    let msg = str_from_abi(ptr, len);
    dialog::alert_default(msg);
}

/// Returns 1 if Yes, 0 if No.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_DIALOG_ASK(ptr: *const u8, len: i64) -> i8 {
    let msg = str_from_abi(ptr, len);
    // choice2_default returns Some(0)=No, Some(1)=Yes, None=dismiss
    if matches!(dialog::choice2_default(msg, "No", "Yes", ""), Some(1)) { 1 } else { 0 }
}

/// Returns GC string handle with the typed text, or 0 if cancelled.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_DIALOG_INPUT(
    label_ptr: *const u8, label_len: i64,
    default_ptr: *const u8, default_len: i64,
) -> u64 {
    let label = str_from_abi(label_ptr, label_len);
    let default = str_from_abi(default_ptr, default_len);
    match dialog::input_default(label, default) {
        Some(s) => __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64),
        None => 0,
    }
}
