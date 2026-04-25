use std::cell::Cell;

use fltk::{
    draw::{self, LineStyle},
    enums::{Color, Font},
};

thread_local! {
    static CURRENT_COLOR: Cell<Color> = Cell::new(Color::Black);
}

fn current_color() -> Color {
    CURRENT_COLOR.with(|c| c.get())
}

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> &'a str {
    if ptr.is_null() || len < 0 { return ""; }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(bytes).unwrap_or("")
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_DRAW_RECT(x: i64, y: i64, w: i64, h: i64) {
    draw::draw_rect(x as i32, y as i32, w as i32, h as i32);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_DRAW_RECT_FILL(x: i64, y: i64, w: i64, h: i64) {
    draw::draw_rect_fill(x as i32, y as i32, w as i32, h as i32, current_color());
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_DRAW_LINE(x1: i64, y1: i64, x2: i64, y2: i64) {
    draw::draw_line(x1 as i32, y1 as i32, x2 as i32, y2 as i32);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_DRAW_CIRCLE(x: i64, y: i64, r: f64) {
    draw::draw_circle(x as f64, y as f64, r);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_DRAW_ARC(x: i64, y: i64, w: i64, h: i64, a1: f64, a2: f64) {
    draw::draw_arc(x as i32, y as i32, w as i32, h as i32, a1, a2);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_DRAW_TEXT(ptr: *const u8, len: i64, x: i64, y: i64) {
    let text = str_from_abi(ptr, len);
    draw::draw_text(text, x as i32, y as i32);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_SET_DRAW_COLOR(r: i64, g: i64, b: i64) {
    let c = Color::from_rgb(r as u8, g as u8, b as u8);
    CURRENT_COLOR.with(|cell| cell.set(c));
    draw::set_draw_color(c);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_SET_FONT(font_id: i64, size: i64) {
    let font = Font::by_index(font_id as usize);
    draw::set_font(font, size as i32);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_SET_LINE_STYLE(style: i64, width: i64) {
    let ls = match style {
        1 => LineStyle::Dash,
        2 => LineStyle::Dot,
        3 => LineStyle::DashDot,
        4 => LineStyle::DashDotDot,
        _ => LineStyle::Solid,
    };
    draw::set_line_style(ls, width as i32);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_UI_MEASURE_WIDTH(ptr: *const u8, len: i64) -> i64 {
    let text = str_from_abi(ptr, len);
    draw::width(text) as i64
}
