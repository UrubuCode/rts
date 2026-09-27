//! Linear gradients as 4-vertex meshes — split out of `pintura.rs` to keep
//! it under the 500-line ceiling; `paint_list` is the only caller.

use super::rgba_to_color32;
use rts_dom::paint::{Mat2d, Rect};

/// A mesma ideia de `paint_linear_gradient`, mas com os 4 vértices do mesh nos
/// cantos TRANSFORMADOS por `mat` — o `t` de interpolação usa a projeção do
/// canto ORIGINAL (pré-transform), porque o CSS pinta o gradiente na caixa e
/// SÓ DEPOIS aplica o `transform` (Transforms 1 §3: o alvo é a imagem já
/// composta) — a mesma ordem de `claude-raster.rs::fill_gradient_mat`.
pub(super) fn paint_linear_gradient_mat(
    painter: &egui::Painter,
    origin: egui::Pos2,
    mat: &Mat2d,
    rect: Rect,
    c0: u32,
    c1: u32,
    angle_deg: f32,
) {
    let rad = angle_deg.to_radians();
    let (dx, dy) = (rad.sin(), -rad.cos());
    let locais = [
        (rect.x, rect.y),
        (rect.x + rect.w, rect.y),
        (rect.x + rect.w, rect.y + rect.h),
        (rect.x, rect.y + rect.h),
    ];
    let proj: Vec<f32> = locais.iter().map(|(x, y)| x * dx + y * dy).collect();
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for &p in &proj {
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let span = (hi - lo).max(1e-3);
    let ca = rgba_to_color32(c0);
    let cb = rgba_to_color32(c1);
    let mut mesh = egui::epaint::Mesh::default();
    for (i, &(x, y)) in locais.iter().enumerate() {
        let t = ((proj[i] - lo) / span).clamp(0.0, 1.0);
        let color = lerp_color32(ca, cb, t);
        let (tx, ty) = mat.apply(x, y);
        mesh.colored_vertex(origin + egui::vec2(tx, ty), color);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// Pinta um GRADIENTE LINEAR de 2 cores num retângulo, como mesh de 4 vértices. A cor
/// de cada canto é a interpolação `c0`→`c1` conforme a projeção do canto no EIXO do
/// gradiente (definido por `angle_deg`, convenção CSS: 0°=de baixo p/ cima, 90°=p/ a
/// direita). Aproxima o `linear-gradient` de 2 pontos (paradas intermediárias já
/// foram descartadas no parse).
pub(super) fn paint_linear_gradient(
    painter: &egui::Painter,
    rect: egui::Rect,
    c0: u32,
    c1: u32,
    angle_deg: f32,
) {
    // Vetor de direção do gradiente. CSS: 0°=para cima (0,-1); cresce no sentido
    // horário → 90°=(1,0), 180°=(0,1). rad = angle; dir = (sin, -cos).
    let rad = angle_deg.to_radians();
    let (dx, dy) = (rad.sin(), -rad.cos());
    let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    // projeção de cada canto no eixo; normaliza para [0,1] entre min e max.
    let proj: Vec<f32> = corners.iter().map(|p| p.x * dx + p.y * dy).collect();
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for &p in &proj {
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let span = (hi - lo).max(1e-3);
    let ca = rgba_to_color32(c0);
    let cb = rgba_to_color32(c1);
    let mut mesh = egui::epaint::Mesh::default();
    for (i, corner) in corners.iter().enumerate() {
        let t = ((proj[i] - lo) / span).clamp(0.0, 1.0);
        let color = lerp_color32(ca, cb, t);
        mesh.colored_vertex(*corner, color);
    }
    // dois triângulos (0,1,2) e (0,2,3).
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// Interpola dois `Color32` no parâmetro `t` ∈ [0,1] (por canal, sem premultiply).
fn lerp_color32(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgba_unmultiplied(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
        l(a.a(), b.a()),
    )
}
