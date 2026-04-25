use fltk::{
    button, enums::Color, frame, input, misc, output, prelude::*, valuator,
};

use crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW;

use super::store::{
    UiEntry, alloc_entry, apply_set_callback, apply_set_draw, apply_widget_op, free_entry,
    with_entry, with_entry_mut,
};

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> &'a str {
    if ptr.is_null() || len < 0 { return ""; }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(bytes).unwrap_or("")
}

fn rgb_color(r: i64, g: i64, b: i64) -> Color {
    Color::from_rgb(r as u8, g as u8, b as u8)
}

// ── Generic widget ops ────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_WIDGET_SET_LABEL(handle: u64, ptr: *const u8, len: i64) {
    let text = str_from_abi(ptr, len).to_owned();
    apply_widget_op(handle, |w| w.set_label(&text));
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_WIDGET_LABEL(handle: u64) -> u64 {
    let label = with_entry(handle, |entry| match entry {
        UiEntry::Button(w) => w.label(),
        UiEntry::Frame(w) => w.label(),
        UiEntry::Window(w) => w.label(),
        UiEntry::CheckButton(w) => w.label(),
        UiEntry::RadioButton(w) => w.label(),
        UiEntry::Input(w) => w.label(),
        UiEntry::Output(w) => w.label(),
        UiEntry::Slider(w) => w.label(),
        UiEntry::Progress(w) => w.label(),
        UiEntry::Spinner(w) => w.label(),
        UiEntry::MenuBar(w) => w.label(),
        UiEntry::TextDisplay(w) => w.label(),
        UiEntry::TextEditor(w) => w.label(),
        _ => String::new(),
    })
    .unwrap_or_default();
    __RTS_FN_NS_GC_STRING_NEW(label.as_ptr(), label.len() as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_WIDGET_SET_CALLBACK(handle: u64, fn_ptr: i64) {
    apply_set_callback(handle, fn_ptr);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_WIDGET_SET_COLOR(handle: u64, r: i64, g: i64, b: i64) {
    let c = rgb_color(r, g, b);
    apply_widget_op(handle, |w| w.set_color(c));
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_WIDGET_SET_LABEL_COLOR(handle: u64, r: i64, g: i64, b: i64) {
    let c = rgb_color(r, g, b);
    apply_widget_op(handle, |w| w.set_label_color(c));
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_WIDGET_RESIZE(handle: u64, x: i64, y: i64, w: i64, h: i64) {
    apply_widget_op(handle, |wid| wid.resize(x as i32, y as i32, w as i32, h as i32));
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_WIDGET_REDRAW(handle: u64) {
    apply_widget_op(handle, |w| w.redraw());
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_WIDGET_HIDE(handle: u64) {
    apply_widget_op(handle, |w| w.hide());
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_WIDGET_SHOW(handle: u64) {
    apply_widget_op(handle, |w| w.show());
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_WIDGET_SET_DRAW(handle: u64, fn_ptr: i64) {
    apply_set_draw(handle, fn_ptr);
}

// ── Button ────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_BUTTON_NEW(
    x: i64, y: i64, w: i64, h: i64,
    label_ptr: *const u8, label_len: i64,
) -> u64 {
    let label = str_from_abi(label_ptr, label_len).to_owned();
    let mut btn = button::Button::new(x as i32, y as i32, w as i32, h as i32, None);
    btn.set_label(&label);
    alloc_entry(UiEntry::Button(btn))
}

// ── Frame ─────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_FRAME_NEW(
    x: i64, y: i64, w: i64, h: i64,
    text_ptr: *const u8, text_len: i64,
) -> u64 {
    let text = str_from_abi(text_ptr, text_len).to_owned();
    let mut frm = frame::Frame::new(x as i32, y as i32, w as i32, h as i32, None);
    frm.set_label(&text);
    alloc_entry(UiEntry::Frame(frm))
}

// ── CheckButton ───────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_CHECK_NEW(
    x: i64, y: i64, w: i64, h: i64,
    label_ptr: *const u8, label_len: i64,
) -> u64 {
    let label = str_from_abi(label_ptr, label_len).to_owned();
    let mut chk = button::CheckButton::new(x as i32, y as i32, w as i32, h as i32, None);
    chk.set_label(&label);
    alloc_entry(UiEntry::CheckButton(chk))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_CHECK_VALUE(handle: u64) -> i8 {
    with_entry(handle, |e| {
        if let UiEntry::CheckButton(b) = e { if b.value() { 1 } else { 0 } } else { 0 }
    })
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_CHECK_SET_VALUE(handle: u64, val: i8) {
    with_entry_mut(handle, |e| {
        if let UiEntry::CheckButton(b) = e { b.set_value(val != 0); }
    });
}

// ── RadioButton ───────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_RADIO_NEW(
    x: i64, y: i64, w: i64, h: i64,
    label_ptr: *const u8, label_len: i64,
) -> u64 {
    let label = str_from_abi(label_ptr, label_len).to_owned();
    let mut rb = button::RadioButton::new(x as i32, y as i32, w as i32, h as i32, None);
    rb.set_label(&label);
    alloc_entry(UiEntry::RadioButton(rb))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_RADIO_VALUE(handle: u64) -> i8 {
    with_entry(handle, |e| {
        if let UiEntry::RadioButton(b) = e { if b.value() { 1 } else { 0 } } else { 0 }
    })
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_RADIO_SET_VALUE(handle: u64, val: i8) {
    with_entry_mut(handle, |e| {
        if let UiEntry::RadioButton(b) = e { b.set_value(val != 0); }
    });
}

// ── Input ─────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_INPUT_NEW(
    x: i64, y: i64, w: i64, h: i64,
    label_ptr: *const u8, label_len: i64,
) -> u64 {
    let label = str_from_abi(label_ptr, label_len).to_owned();
    let mut inp = input::Input::new(x as i32, y as i32, w as i32, h as i32, None);
    inp.set_label(&label);
    alloc_entry(UiEntry::Input(inp))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_INPUT_VALUE(handle: u64) -> u64 {
    let val = with_entry(handle, |e| {
        if let UiEntry::Input(i) = e { i.value() } else { String::new() }
    })
    .unwrap_or_default();
    __RTS_FN_NS_GC_STRING_NEW(val.as_ptr(), val.len() as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_INPUT_SET_VALUE(handle: u64, ptr: *const u8, len: i64) {
    let text = str_from_abi(ptr, len).to_owned();
    with_entry_mut(handle, |e| {
        if let UiEntry::Input(i) = e { let _ = i.set_value(&text); }
    });
}

// ── Output ────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_OUTPUT_NEW(
    x: i64, y: i64, w: i64, h: i64,
    label_ptr: *const u8, label_len: i64,
) -> u64 {
    let label = str_from_abi(label_ptr, label_len).to_owned();
    let mut out = output::Output::new(x as i32, y as i32, w as i32, h as i32, None);
    out.set_label(&label);
    alloc_entry(UiEntry::Output(out))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_OUTPUT_SET_VALUE(handle: u64, ptr: *const u8, len: i64) {
    let text = str_from_abi(ptr, len).to_owned();
    with_entry_mut(handle, |e| {
        if let UiEntry::Output(o) = e { let _ = o.set_value(&text); }
    });
}

// ── Slider ────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_SLIDER_NEW(
    x: i64, y: i64, w: i64, h: i64,
    label_ptr: *const u8, label_len: i64,
) -> u64 {
    let label = str_from_abi(label_ptr, label_len).to_owned();
    let mut s = valuator::HorSlider::new(x as i32, y as i32, w as i32, h as i32, None);
    s.set_label(&label);
    s.set_bounds(0.0, 100.0);
    alloc_entry(UiEntry::Slider(s))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_SLIDER_VALUE(handle: u64) -> f64 {
    with_entry(handle, |e| {
        if let UiEntry::Slider(s) = e { s.value() } else { 0.0 }
    })
    .unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_SLIDER_SET_VALUE(handle: u64, val: f64) {
    with_entry_mut(handle, |e| {
        if let UiEntry::Slider(s) = e { s.set_value(val); }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_SLIDER_SET_BOUNDS(handle: u64, min: f64, max: f64) {
    with_entry_mut(handle, |e| {
        if let UiEntry::Slider(s) = e { s.set_bounds(min, max); }
    });
}

// ── Progress ──────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_PROGRESS_NEW(
    x: i64, y: i64, w: i64, h: i64,
    label_ptr: *const u8, label_len: i64,
) -> u64 {
    let label = str_from_abi(label_ptr, label_len).to_owned();
    let mut p = misc::Progress::new(x as i32, y as i32, w as i32, h as i32, None);
    p.set_label(&label);
    p.set_minimum(0.0);
    p.set_maximum(100.0);
    alloc_entry(UiEntry::Progress(p))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_PROGRESS_VALUE(handle: u64) -> f64 {
    with_entry(handle, |e| {
        if let UiEntry::Progress(p) = e { p.value() } else { 0.0 }
    })
    .unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_PROGRESS_SET_VALUE(handle: u64, val: f64) {
    with_entry_mut(handle, |e| {
        if let UiEntry::Progress(p) = e { p.set_value(val); }
    });
}

// ── Spinner ───────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_SPINNER_NEW(
    x: i64, y: i64, w: i64, h: i64,
    label_ptr: *const u8, label_len: i64,
) -> u64 {
    let label = str_from_abi(label_ptr, label_len).to_owned();
    let mut sp = misc::Spinner::new(x as i32, y as i32, w as i32, h as i32, None);
    sp.set_label(&label);
    alloc_entry(UiEntry::Spinner(sp))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_SPINNER_VALUE(handle: u64) -> f64 {
    with_entry(handle, |e| {
        if let UiEntry::Spinner(s) = e { s.value() } else { 0.0 }
    })
    .unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_SPINNER_SET_VALUE(handle: u64, val: f64) {
    with_entry_mut(handle, |e| {
        if let UiEntry::Spinner(s) = e { s.set_value(val); }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_SPINNER_SET_BOUNDS(handle: u64, min: f64, max: f64) {
    with_entry_mut(handle, |e| {
        if let UiEntry::Spinner(s) = e { s.set_range(min, max); }
    });
}

// ── Free helpers ──────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_MENUBAR_FREE(handle: u64) {
    free_entry(handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_TEXTBUF_FREE(handle: u64) {
    free_entry(handle);
}
