//! O pass 3D: malhas, câmera, luz, sombra e textura.
//!
//! Desenhado ANTES da UI, no mesmo encoder, com profundidade própria — a UI do
//! egui fica por cima da cena. Só o backend wgpu age; no glow é no-op.
//!
//! # O que mudou de verdade em relação à superfície antiga
//!
//! Os dados de geometria chegam como **view tipada**, não como endereço.
//! `meshUpload(win, new Float32Array(…), new Uint32Array(…))` em vez de
//! `meshUpload(win, verts.ptr(), n, idx.ptr(), m)`. Um endereço cru é uma
//! leitura arbitrária de memória a partir de um número que o programa calcula, e
//! neste motor é pior que isso: o coletor move células, então o endereço lido
//! pode não ser mais o buffer quando o Rust o usa. Ver `crate::value::bytes`.

use rts_core::entry::Provided;

use crate::value::{self, bytes, handle, integer, number};
use crate::window::options;

/// Os membros do pass 3D de `rts:egui`.
pub const MEMBERS: &[(&str, Provided)] = &[
    ("meshUpload", mesh_upload),
    ("meshFree", mesh_free),
    ("textureUpload", texture_upload),
    ("setCamera", set_camera),
    ("setCameraLookAt", set_camera_look_at),
    ("setClearColor", set_clear_color),
    ("setSkybox", set_skybox),
    ("setLight", set_light),
    ("setShadow", set_shadow),
    ("drawMesh", draw_mesh),
];

/// `meshUpload(win, vertices, indices)` — o id da malha, `0` em falha.
///
/// `vertices` é um `Float32Array` de 8 floats por vértice (posição, normal, uv);
/// `indices` um `Uint32Array`. Os comprimentos vêm das próprias views, então não
/// há como o programa declarar uma contagem que discorde dos dados — o que a
/// superfície antiga permitia, e cujo resultado era ler além do buffer.
extern "C" fn mesh_upload(_e: u64, _t: u64, win: u64, vertices: u64, indices: u64, _c: u64) -> u64 {
    let (Some(vertices), Some(indices)) = (bytes(vertices), bytes(indices)) else {
        return value::from_number(0.0);
    };
    let vertices: Vec<f32> = vertices
        .chunks_exact(4)
        .map(|word| f32::from_ne_bytes([word[0], word[1], word[2], word[3]]))
        .collect();
    let indices: Vec<u32> = indices
        .chunks_exact(4)
        .map(|word| u32::from_ne_bytes([word[0], word[1], word[2], word[3]]))
        .collect();
    let made = rts_egui::upload_mesh(handle(win), &vertices, &indices);
    value::from_number(made as f64)
}

/// `meshFree(win, mesh)`.
extern "C" fn mesh_free(_e: u64, _t: u64, win: u64, mesh: u64, _b: u64, _c: u64) -> u64 {
    rts_egui::mesh_free(handle(win), handle(mesh));
    value::nothing()
}

/// `textureUpload(win, pixels, width, height)` — o id da textura (≥2), `0` em
/// falha. `pixels` é um `Uint8Array` RGBA8 em sRGB.
extern "C" fn texture_upload(_e: u64, _t: u64, win: u64, pixels: u64, w: u64, h: u64) -> u64 {
    let Some(pixels) = bytes(pixels) else {
        return value::from_number(0.0);
    };
    let made = rts_egui::upload_texture(handle(win), &pixels, integer(w, 0), integer(h, 0));
    value::from_number(made as f64)
}

/// `setCamera(win, { x, y, z, yaw, pitch, fov, aspect })` — câmera de voo.
/// Ângulos em RADIANOS; base canhota, `yaw` 0 olha para +Z.
extern "C" fn set_camera(_e: u64, _t: u64, win: u64, spec: u64, _b: u64, _c: u64) -> u64 {
    let read = options(
        spec,
        &["x", "y", "z", "yaw", "pitch", "fov", "aspect"],
        &[0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0],
    );
    rts_egui::set_camera(
        handle(win),
        read[0], read[1], read[2], read[3], read[4], read[5], read[6],
    );
    value::nothing()
}

/// `setCameraLookAt(win, { ex, ey, ez, tx, ty, tz, fov, aspect, near, far })`.
///
/// NaN-safe: olhar reto para cima não trava em gimbal, degrada. É a forma
/// conveniente para enquadrar um objeto.
extern "C" fn set_camera_look_at(_e: u64, _t: u64, win: u64, spec: u64, _b: u64, _c: u64) -> u64 {
    let read = options(
        spec,
        &["ex", "ey", "ez", "tx", "ty", "tz", "fov", "aspect", "near", "far"],
        &[0.0, 0.0, -5.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.1, 1000.0],
    );
    rts_egui::set_camera_lookat(
        handle(win),
        read[0], read[1], read[2], read[3], read[4], read[5], read[6], read[7], read[8], read[9],
    );
    value::nothing()
}

/// `setClearColor(win, r, g, b)` — fundo chapado (0..1), desligando o skybox.
extern "C" fn set_clear_color(_e: u64, _t: u64, win: u64, r: u64, g: u64, b: u64) -> u64 {
    rts_egui::set_clear_color(
        handle(win),
        number(r, 0.0),
        number(g, 0.0),
        number(b, 0.0),
    );
    value::nothing()
}

/// `setSkybox(win, on)` — religa o skybox procedural.
extern "C" fn set_skybox(_e: u64, _t: u64, win: u64, on: u64, _b: u64, _c: u64) -> u64 {
    rts_egui::set_skybox(handle(win), integer(on, 1));
    value::nothing()
}

/// `setLight(win, { x, y, z, ambient })` — luz PONTUAL na posição dada.
extern "C" fn set_light(_e: u64, _t: u64, win: u64, spec: u64, _b: u64, _c: u64) -> u64 {
    let read = options(spec, &["x", "y", "z", "ambient"], &[0.0, 10.0, 0.0, 0.2]);
    rts_egui::set_light(handle(win), read[0], read[1], read[2], read[3]);
    value::nothing()
}

/// `setShadow(win, { dx, dy, dz, cx, cy, cz, radius })` — a caixa ortográfica
/// que a sombra cobre. `radius <= 0` desliga.
extern "C" fn set_shadow(_e: u64, _t: u64, win: u64, spec: u64, _b: u64, _c: u64) -> u64 {
    let read = options(
        spec,
        &["dx", "dy", "dz", "cx", "cy", "cz", "radius"],
        &[0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    );
    rts_egui::set_shadow(
        handle(win),
        read[0], read[1], read[2], read[3], read[4], read[5], read[6],
    );
    value::nothing()
}

/// `drawMesh(win, { mesh, x, y, z, rx, ry, sx, sy, sz, color, emissive, tex })`.
///
/// Cor `0xAARRGGBB`; `tex` 0=nenhuma, 1=xadrez procedural, ≥2 = id de
/// `textureUpload`. A escala vale 1 por default, porque uma escala 0 é uma malha
/// invisível e é o que um objeto de opções incompleto produziria.
extern "C" fn draw_mesh(_e: u64, _t: u64, win: u64, spec: u64, _b: u64, _c: u64) -> u64 {
    let read = options(
        spec,
        &["mesh", "x", "y", "z", "rx", "ry", "sx", "sy", "sz", "color", "emissive", "tex"],
        &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0xFFFF_FFFFu32 as f64, 0.0, 0.0],
    );
    rts_egui::draw_mesh(
        handle(win),
        read[0] as u64,
        read[1], read[2], read[3], read[4], read[5], read[6], read[7], read[8],
        read[9] as i64,
        read[10] as i64,
        read[11] as i64,
    );
    value::nothing()
}
