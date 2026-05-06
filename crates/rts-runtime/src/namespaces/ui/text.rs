use fltk::{
    prelude::{DisplayExt, WidgetBase, WidgetExt},
    text::{TextBuffer, TextDisplay, TextEditor},
};

use crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW;

use super::store::{UiEntry, alloc_entry, clone_textbuf, with_entry, with_entry_mut};

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> &'a str {
    if ptr.is_null() || len < 0 {
        return "";
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(bytes).unwrap_or("")
}

// ── TextBuffer ────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_TEXTBUF_NEW() -> u64 {
    alloc_entry(UiEntry::TextBuffer(TextBuffer::default()))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_TEXTBUF_SET_TEXT(handle: u64, ptr: *const u8, len: i64) {
    let text = str_from_abi(ptr, len).to_owned();
    with_entry_mut(handle, |e| {
        if let UiEntry::TextBuffer(b) = e {
            b.set_text(&text);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_TEXTBUF_TEXT(handle: u64) -> u64 {
    let text = with_entry(handle, |e| {
        if let UiEntry::TextBuffer(b) = e {
            b.text()
        } else {
            String::new()
        }
    })
    .unwrap_or_default();
    __RTS_FN_NS_GC_STRING_NEW(text.as_ptr(), text.len() as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_TEXTBUF_APPEND(handle: u64, ptr: *const u8, len: i64) {
    let text = str_from_abi(ptr, len).to_owned();
    with_entry_mut(handle, |e| {
        if let UiEntry::TextBuffer(b) = e {
            b.append(&text);
        }
    });
}

// ── TextDisplay ───────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_TEXTDISPLAY_NEW(
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    label_ptr: *const u8,
    label_len: i64,
) -> u64 {
    let label = str_from_abi(label_ptr, label_len).to_owned();
    let mut d = TextDisplay::new(x as i32, y as i32, w as i32, h as i32, None);
    d.set_label(&label);
    alloc_entry(UiEntry::TextDisplay(d))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_TEXTDISPLAY_SET_BUFFER(display: u64, buf: u64) {
    let Some(buf_clone) = clone_textbuf(buf) else {
        return;
    };
    with_entry_mut(display, |e| {
        if let UiEntry::TextDisplay(d) = e {
            d.set_buffer(buf_clone);
        }
    });
}

// ── TextEditor ────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_TEXTEDITOR_NEW(
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    label_ptr: *const u8,
    label_len: i64,
) -> u64 {
    let label = str_from_abi(label_ptr, label_len).to_owned();
    let mut e = TextEditor::new(x as i32, y as i32, w as i32, h as i32, None);
    e.set_label(&label);
    alloc_entry(UiEntry::TextEditor(e))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_TEXTEDITOR_SET_BUFFER(editor: u64, buf: u64) {
    let Some(buf_clone) = clone_textbuf(buf) else {
        return;
    };
    with_entry_mut(editor, |e| {
        if let UiEntry::TextEditor(ed) = e {
            ed.set_buffer(buf_clone);
        }
    });
}
