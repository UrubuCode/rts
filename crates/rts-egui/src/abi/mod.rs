//! Os símbolos `extern "C"` do motor ANTIGO, e só eles.
//!
//! # Por que este arquivo existe separado
//!
//! No motor antigo um nativo é um SÍMBOLO DE LINKER: o `rts-symbol-baker` rende
//! `__RTS_FN_NS_EGUI_*` numa tabela e o JIT resolve por nome. No motor novo um
//! nativo é um ponteiro de função ao lado de uma célula, sem nome nenhum — ver
//! `docs/engine/authoring-natives.md`.
//!
//! Enquanto os dois convivem, a lógica não pode pertencer a nenhum dos dois. Ela
//! está nos módulos vizinhos, em Rust comum (`&str`, `f64`, `u64`), e este
//! arquivo é a CASCA que a apresenta ao motor antigo: converte a ABI de
//! ponteiro+comprimento e chama. Está atrás da feature `old-engine` porque é a
//! única parte do crate que alcança `rts-engine` — e portanto `rts-abi`, a
//! interface que `rts_cranelift::abi` substituiu.
//!
//! Uma casca não decide nada. Se algo aqui precisar de um `if`, é semântica que
//! escorregou para fora do módulo dono e vai voltar para lá.

use rts_engine::abi::str_abi;

mod register;
pub use register::register;

// `rts:gpu` é um namespace próprio no motor antigo, com registro próprio — a
// facade o alcança como `ns::gpu::register`.
pub mod gpu;

/// O texto que um par ponteiro+comprimento nomeia, vazio quando não nomeia um.
///
/// # Segurança
///
/// O par vem do motor, que o produziu do seu pool de strings — a mesma condição
/// que todo nativo do motor antigo já assume. Vazio em vez de pânico: um
/// `extern "C"` não desenrola, então um pânico aqui abortaria o processo por uma
/// string malformada.
fn text(ptr: *const u8, len: i64) -> String {
    unsafe { str_abi::from_abi(ptr, len) }.unwrap_or("").to_string()
}


#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_OPEN_WINDOW(title_ptr: *const u8, title_len: i64, w: i64, h: i64, config: i64) -> u64 {
    crate::app::open_window(&text(title_ptr, title_len), w, h, config)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_PUMP(h: u64) -> i64 {
    crate::app::pump(h)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_IS_OPEN(h: u64) -> i64 {
    crate::app::is_open(h)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_CLOSE(h: u64) {
    crate::app::close(h)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_MOVE_WINDOW(h: u64, x: i64, y: i64) {
    crate::app::move_window(h, x, y)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_MOUSE_LOCK(h: u64, on: i64) {
    crate::app::mouse_lock(h, on)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_SET_NEXT_POS(x: i64, y: i64) {
    crate::app::set_next_pos(x, y)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_BEGIN_FRAME(h: u64) {
    crate::frame::begin_frame(h)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_END_FRAME(h: u64) {
    crate::frame::end_frame(h)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_SET_VSYNC(h: u64, on: i64) {
    crate::frame::set_vsync(h, on)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_SNAPSHOT(h: u64, path_ptr: *const u8, path_len: i64) {
    crate::frame::snapshot(h, &text(path_ptr, path_len))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_LABEL(h: u64, text_ptr: *const u8, text_len: i64) {
    crate::widgets::label(h, &text(text_ptr, text_len))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_BUTTON(h: u64, label_ptr: *const u8, label_len: i64) -> i64 {
    crate::widgets::button(h, &text(label_ptr, label_len))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_SLIDER(h: u64, value: f64, min: f64, max: f64) -> f64 {
    crate::widgets::slider(h, value, min, max)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_HORIZONTAL_BEGIN(h: u64) {
    crate::widgets::horizontal_begin(h)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_HORIZONTAL_END(h: u64) {
    crate::widgets::horizontal_end(h)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_HTML(h: u64, ptr: *const u8, len: i64) {
    crate::widgets::html(h, &text(ptr, len))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_RENDER(h: u64, dom_handle: u64) {
    crate::widgets::render(h, dom_handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_DEFINE_BLOCK(tag_ptr: *const u8, tag_len: i64, display: i64, indent: f64, prefix: i64, flags: i64) {
    crate::widgets::define_block(&text(tag_ptr, tag_len), display, indent, prefix, flags)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_DEFINE_STYLE(tag_ptr: *const u8, tag_len: i64, slot: i64, val: i64) {
    crate::widgets::define_style(&text(tag_ptr, tag_len), slot, val)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_DEFINE_INLINE(tag_ptr: *const u8, tag_len: i64, flags: i64) {
    crate::widgets::define_inline(&text(tag_ptr, tag_len), flags)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_QUERY_SELECTOR(h: u64, sel_ptr: *const u8, sel_len: i64) -> i64 {
    crate::widgets::query_selector(h, &text(sel_ptr, sel_len))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_SET_TEXT(h: u64, id: i64, ptr: *const u8, len: i64) {
    crate::widgets::set_text(h, id, &text(ptr, len))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_SET_ATTR(h: u64, id: i64, name_ptr: *const u8, name_len: i64, val_ptr: *const u8, val_len: i64) {
    crate::widgets::set_attr(h, id, &text(name_ptr, name_len), &text(val_ptr, val_len))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_CREATE_ELEMENT(h: u64, tag_ptr: *const u8, tag_len: i64) -> i64 {
    crate::widgets::create_element(h, &text(tag_ptr, tag_len))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_APPEND_CHILD(h: u64, parent: i64, child: i64) {
    crate::widgets::append_child(h, parent, child)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_REMOVE_NODE(h: u64, id: i64) {
    crate::widgets::remove_node(h, id)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_DOM_DUMP(h: u64) {
    crate::widgets::dom_dump(h)
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn __RTS_FN_NS_EGUI_DRAW_RECT(h: u64, x: f64, y: f64, w: f64, h_: f64, fill: i64, stroke_w: f64, stroke: i64, radius: f64) {
    crate::canvas::draw_rect(h, x, y, w, h_, fill, stroke_w, stroke, radius)
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn __RTS_FN_NS_EGUI_DRAW_TEXT(h: u64, x: f64, y: f64, text_ptr: *const u8, text_len: i64, color: i64, size: f64, flags: i64) {
    crate::canvas::draw_text(h, x, y, &text(text_ptr, text_len), color, size, flags)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_DRAW_LINE(h: u64, x1: f64, y1: f64, x2: f64, y2: f64, w: f64, color: i64) {
    crate::canvas::draw_line(h, x1, y1, x2, y2, w, color)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_MEASURE_TEXT(h: u64, text_ptr: *const u8, text_len: i64, size: f64, bold: i64) -> f64 {
    crate::canvas::measure_text(h, &text(text_ptr, text_len), size, bold)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_MESH_UPLOAD(win: u64, vptr: u64, vcount: i64, iptr: u64, icount: i64) -> u64 {
    unsafe { crate::scene_api::mesh_upload(win, vptr, vcount, iptr, icount) }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_MESH_FREE(win: u64, mesh: u64) {
    crate::scene_api::mesh_free(win, mesh)
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn __RTS_FN_NS_EGUI_SET_CAMERA(win: u64, camx: f64, camy: f64, camz: f64, yaw: f64, pitch: f64, fov_y: f64, aspect: f64) {
    crate::scene_api::set_camera(win, camx, camy, camz, yaw, pitch, fov_y, aspect)
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn __RTS_FN_NS_EGUI_SET_CAMERA_LOOKAT(win: u64, ex: f64, ey: f64, ez: f64, tx: f64, ty: f64, tz: f64, fov_y: f64, aspect: f64, near: f64, far: f64) {
    crate::scene_api::set_camera_lookat(win, ex, ey, ez, tx, ty, tz, fov_y, aspect, near, far)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_SET_CLEAR_COLOR(win: u64, r: f64, g: f64, b: f64) {
    crate::scene_api::set_clear_color(win, r, g, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_SET_SKYBOX(win: u64, on: i64) {
    crate::scene_api::set_skybox(win, on)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_SET_LIGHT(win: u64, dx: f64, dy: f64, dz: f64, ambient: f64) {
    crate::scene_api::set_light(win, dx, dy, dz, ambient)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_WIN_WIDTH(win: u64) -> f64 {
    crate::scene_api::win_width(win)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_WIN_HEIGHT(win: u64) -> f64 {
    crate::scene_api::win_height(win)
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn __RTS_FN_NS_EGUI_SET_SHADOW(win: u64, dx: f64, dy: f64, dz: f64, cx: f64, cy: f64, cz: f64, radius: f64) {
    crate::scene_api::set_shadow(win, dx, dy, dz, cx, cy, cz, radius)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_TEXTURE_UPLOAD(win: u64, ptr: u64, w: i64, h: i64) -> u64 {
    unsafe { crate::scene_api::texture_upload(win, ptr, w, h) }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn __RTS_FN_NS_EGUI_DRAW_MESH(win: u64, mesh: u64, px: f64, py: f64, pz: f64, rx: f64, ry: f64, sx: f64, sy: f64, sz: f64, color: i64, emissive: i64, tex: i64) {
    crate::scene_api::draw_mesh(win, mesh, px, py, pz, rx, ry, sx, sy, sz, color, emissive, tex)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_DRAW_WATER(win: u64, mesh: u64, gbuf: u64, count: i64, scale: f64) -> i64 {
    crate::scene_api::draw_water(win, mesh, gbuf, count, scale)
}
