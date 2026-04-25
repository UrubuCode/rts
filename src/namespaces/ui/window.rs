use fltk::{
    enums::Color,
    prelude::{GroupExt, WidgetBase, WidgetExt},
    window,
};

use super::store::{UiEntry, alloc_entry, apply_set_callback, free_entry, with_entry_mut};

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> &'a str {
    if ptr.is_null() || len < 0 { return ""; }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(bytes).unwrap_or("")
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_WINDOW_NEW(w: i64, h: i64, title_ptr: *const u8, title_len: i64) -> u64 {
    let title = str_from_abi(title_ptr, title_len).to_owned();
    let mut win = window::Window::new(0, 0, w as i32, h as i32, None);
    win.set_label(&title);
    alloc_entry(UiEntry::Window(win))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_WINDOW_SHOW(handle: u64) {
    with_entry_mut(handle, |entry| {
        if let UiEntry::Window(w) = entry { w.show(); }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_WINDOW_END(handle: u64) {
    with_entry_mut(handle, |entry| {
        if let UiEntry::Window(w) = entry { w.end(); }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_WINDOW_FREE(handle: u64) {
    free_entry(handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_WINDOW_SET_CALLBACK(handle: u64, fn_ptr: i64) {
    apply_set_callback(handle, fn_ptr);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_WINDOW_SET_COLOR(handle: u64, r: i64, g: i64, b: i64) {
    let c = Color::from_rgb(r as u8, g as u8, b as u8);
    with_entry_mut(handle, |entry| {
        if let UiEntry::Window(w) = entry { w.set_color(c); }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_WINDOW_RESIZE(handle: u64, x: i64, y: i64, w: i64, h: i64) {
    with_entry_mut(handle, |entry| {
        if let UiEntry::Window(win) = entry {
            win.resize(x as i32, y as i32, w as i32, h as i32);
        }
    });
}
