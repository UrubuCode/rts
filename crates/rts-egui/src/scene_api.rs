//! Superfície `extern "C"` do pipeline 3D wgpu, exposta no namespace `egui`
//! (tied ao handle da janela). NÃO mexe na trait neutra `rts-render::Renderer`.
//!
//! A TS passa PONTEIROS crus (via `buffer.ptr`) pros dados de vértice/índice, e
//! os campos de câmera/transform como f64 — a matriz view·proj e a model são
//! construídas em Rust (`scene3d::view_proj`/`model_matrix`), casando a convenção
//! do rasterizador software. Só o braço `Backend::Wgpu` age; no glow é no-op.

use crate::ctx::with_ctx;
use crate::frame::scene3d::{model_matrix, view_proj, Scene3D};
use crate::frame::Backend;

/// Garante o pipeline 3D criado na 1ª chamada e roda `f(scene, device)`.
fn with_scene<R: Copy>(win: u64, f: impl FnOnce(&mut Scene3D, &wgpu::Device) -> R, default: R) -> R {
    with_ctx(win, |c| {
        if let Backend::Wgpu(r) = &mut c.backend {
            if r.scene.is_none() {
                r.scene = Some(Scene3D::new(&r.device, r.config.format));
            }
            let dev = &r.device;
            let s = r.scene.as_mut().unwrap();
            f(s, dev)
        } else {
            default
        }
    })
    .unwrap_or(default)
}

/// Sobe uma mesh (verts interleaved pos+normal = 6 f32/vértice; índices u32) pra
/// VRAM. Retorna o id da mesh (0 se a janela não é wgpu).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_MESH_UPLOAD(
    win: u64,
    vptr: u64,
    vcount: i64,
    iptr: u64,
    icount: i64,
) -> u64 {
    if vptr == 0 || iptr == 0 || vcount <= 0 || icount <= 0 {
        return 0;
    }
    let verts = unsafe { std::slice::from_raw_parts(vptr as *const f32, (vcount * 6) as usize) };
    let inds = unsafe { std::slice::from_raw_parts(iptr as *const u32, icount as usize) };
    with_scene(win, |s, dev| s.upload_mesh(dev, verts, inds), 0)
}

/// Libera uma mesh da VRAM.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_MESH_FREE(win: u64, mesh: u64) {
    with_scene(win, |s, _d| s.free_mesh(mesh), ());
}

/// Define a câmera do frame (fly: yaw/pitch/fov/aspect). Constrói view·proj.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_SET_CAMERA(
    win: u64,
    camx: f64,
    camy: f64,
    camz: f64,
    yaw: f64,
    pitch: f64,
    fov_y: f64,
    aspect: f64,
) {
    let vp = view_proj(
        camx as f32, camy as f32, camz as f32, yaw as f32, pitch as f32, fov_y as f32, aspect as f32,
    );
    with_scene(win, |s, _d| s.set_camera(vp), ());
}

/// Define a luz direcional (dir + ambiente).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_SET_LIGHT(win: u64, dx: f64, dy: f64, dz: f64, ambient: f64) {
    with_scene(
        win,
        |s, _d| s.set_light([dx as f32, dy as f32, dz as f32], ambient as f32),
        (),
    );
}

/// Enfileira 1 draw da mesh `mesh` com transform (pos/rot/escala) + cor
/// (0xAARRGGBB). O draw acontece no scene pass, no `endFrame`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_DRAW_MESH(
    win: u64,
    mesh: u64,
    px: f64,
    py: f64,
    pz: f64,
    rx: f64,
    ry: f64,
    sx: f64,
    sy: f64,
    sz: f64,
    color: i64,
) {
    let m = model_matrix(
        px as f32, py as f32, pz as f32, rx as f32, ry as f32, sx as f32, sy as f32, sz as f32,
    );
    let c = color as u32;
    let a = (c >> 24) & 0xFF;
    let col = [
        ((c >> 16) & 0xFF) as f32 / 255.0,
        ((c >> 8) & 0xFF) as f32 / 255.0,
        (c & 0xFF) as f32 / 255.0,
        if a == 0 { 1.0 } else { a as f32 / 255.0 }, // sem byte de alpha = opaco
    ];
    with_scene(win, |s, _d| s.queue_draw(mesh, m, col), ());
}
