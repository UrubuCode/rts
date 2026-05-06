use fltk::{
    enums::Shortcut,
    menu::{MenuBar, MenuFlag},
    prelude::{MenuExt, WidgetBase},
};

use super::store::{UiEntry, alloc_entry, call_fn_ptr, with_entry_mut};

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> &'a str {
    if ptr.is_null() || len < 0 {
        return "";
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(bytes).unwrap_or("")
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_MENUBAR_NEW(x: i64, y: i64, w: i64, h: i64) -> u64 {
    let bar = MenuBar::new(x as i32, y as i32, w as i32, h as i32, None);
    alloc_entry(UiEntry::MenuBar(bar))
}

/// Adds an item to the menu bar.
/// `path` uses '/' as separator (e.g. "File/Open\t"). `fn_ptr=0` for no callback.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_MENUBAR_ADD(
    handle: u64,
    path_ptr: *const u8,
    path_len: i64,
    fn_ptr: i64,
) {
    let path = str_from_abi(path_ptr, path_len).to_owned();
    with_entry_mut(handle, |entry| {
        if let UiEntry::MenuBar(bar) = entry {
            if fn_ptr == 0 {
                bar.add(&path, Shortcut::None, MenuFlag::Normal, |_| {});
            } else {
                let fp = fn_ptr;
                bar.add(&path, Shortcut::None, MenuFlag::Normal, move |_| unsafe {
                    call_fn_ptr(fp)
                });
            }
        }
    });
}
