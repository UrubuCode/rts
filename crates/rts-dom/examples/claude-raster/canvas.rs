//! The pixel buffer of `claude-raster` and its rectangle, quad, gradient and
//! matrix fills — moved out of `claude-raster.rs` unchanged when painting
//! text (`text.rs`) would have pushed that file past the 500-line ceiling.

use rts_dom::paint::{Mat2d, Rect};

pub const W: usize = 1280;
pub const H: usize = 800;

/// Um pixel RGBA reto (sem alpha pré-multiplicado) num buffer W×H.
pub struct Canvas {
    pub px: Vec<u8>,
}

impl Canvas {
    pub fn new(bg: u32) -> Canvas {
        let mut px = vec![0u8; W * H * 4];
        let (r, g, b, a) = argb_bytes(bg);
        for i in 0..(W * H) {
            px[i * 4] = r;
            px[i * 4 + 1] = g;
            px[i * 4 + 2] = b;
            px[i * 4 + 3] = a;
        }
        Canvas { px }
    }

    /// Alpha-blend de UM pixel. `clip` é a interseção corrente das
    /// `BeginClip` abertas — `None` fora de qualquer rect pintável.
    pub fn blend(&mut self, x: i32, y: i32, color: u32, clip: Option<Rect>) {
        if x < 0 || y < 0 || x as usize >= W || y as usize >= H {
            return;
        }
        if let Some(c) = clip {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;
            if fx < c.x || fy < c.y || fx > c.x + c.w || fy > c.y + c.h {
                return;
            }
        }
        let (r, g, b, a) = argb_bytes(color);
        if a == 0 {
            return;
        }
        let i = (y as usize * W + x as usize) * 4;
        if a == 255 {
            self.px[i] = r;
            self.px[i + 1] = g;
            self.px[i + 2] = b;
            self.px[i + 3] = 255;
            return;
        }
        let af = a as f32 / 255.0;
        for (k, s) in [r, g, b].into_iter().enumerate() {
            let d = self.px[i + k] as f32;
            self.px[i + k] = (s as f32 * af + d * (1.0 - af)).round() as u8;
        }
        self.px[i + 3] = 255;
    }

    pub fn fill_rect(&mut self, r: Rect, color: u32, clip: Option<Rect>) {
        let x0 = r.x.floor() as i32;
        let y0 = r.y.floor() as i32;
        let x1 = (r.x + r.w).ceil() as i32;
        let y1 = (r.y + r.h).ceil() as i32;
        for y in y0..y1 {
            for x in x0..x1 {
                self.blend(x, y, color, clip);
            }
        }
    }

    /// Um bitmap RGBA8 esticado a `r` por vizinho mais próximo — o suficiente
    /// para uma régua de 1280×800 sobre ícones e fixtures; um filtro bilinear
    /// mudaria pixels de borda que a tolerância por canal já absorve.
    pub fn fill_pixels(&mut self, r: Rect, data: &[u8], w: u32, h: u32, clip: Option<Rect>) {
        let x0 = r.x.floor() as i32;
        let y0 = r.y.floor() as i32;
        let x1 = (r.x + r.w).ceil() as i32;
        let y1 = (r.y + r.h).ceil() as i32;
        for y in y0..y1 {
            let sy = (((y as f32 + 0.5 - r.y) / r.h) * h as f32).floor().clamp(0.0, (h - 1) as f32) as usize;
            for x in x0..x1 {
                let sx = (((x as f32 + 0.5 - r.x) / r.w) * w as f32).floor().clamp(0.0, (w - 1) as f32) as usize;
                let i = (sy * w as usize + sx) * 4;
                if i + 3 >= data.len() {
                    continue;
                }
                let c = (u32::from(data[i]) << 24) | (u32::from(data[i + 1]) << 16) | (u32::from(data[i + 2]) << 8) | u32::from(data[i + 3]);
                self.blend(x, y, c, clip);
            }
        }
    }

    /// Um quadrilátero CONVEXO já em coordenadas de tela, por varrimento: em
    /// cada linha de pixels, o vão entre a menor e a maior intersecção das
    /// quatro arestas com o centro da linha. É o que pinta um lado de borda
    /// com junção diagonal (`DisplayItem::Quad`) — o `fill_rect_mat` não
    /// serve porque um trapézio não é a imagem de um retângulo por uma matriz.
    pub fn fill_quad(&mut self, pts: [(f32, f32); 4], color: u32, clip: Option<Rect>) {
        let y0 = pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min).floor() as i32;
        let y1 = pts.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max).ceil() as i32;
        for y in y0..y1 {
            let fy = y as f32 + 0.5;
            let (mut xa, mut xb) = (f32::INFINITY, f32::NEG_INFINITY);
            for i in 0..4 {
                let (p, q) = (pts[i], pts[(i + 1) % 4]);
                if (p.1 <= fy) != (q.1 <= fy) {
                    let x = p.0 + (fy - p.1) * (q.0 - p.0) / (q.1 - p.1);
                    xa = xa.min(x);
                    xb = xb.max(x);
                }
            }
            if xa > xb {
                continue;
            }
            for x in (xa - 0.5).ceil() as i32..(xb - 0.5).ceil() as i32 {
                self.blend(x, y, color, clip);
            }
        }
    }

    /// Borda como quatro tiras — não um retângulo vazado, para não assumir
    /// que `width` é igual nos quatro lados (a `DisplayList` já colapsou para
    /// um valor só neste item; ver o comentário em `paint/item.rs`).
    pub fn stroke_rect(&mut self, r: Rect, width: f32, color: u32, clip: Option<Rect>) {
        let w = width.max(1.0);
        self.fill_rect(Rect::new(r.x, r.y, r.w, w), color, clip); // topo
        self.fill_rect(Rect::new(r.x, r.y + r.h - w, r.w, w), color, clip); // fundo
        self.fill_rect(Rect::new(r.x, r.y, w, r.h), color, clip); // esquerda
        self.fill_rect(Rect::new(r.x + r.w - w, r.y, w, r.h), color, clip); // direita
    }

    /// `r` sob `mat` (rotação/skew/matrix, não a translação/escala pura que
    /// `fill_rect` já cobre exatamente): preenche o QUADRILÁTERO real, não a
    /// bounding box axis-aligned. Percorre a bbox dos 4 cantos transformados
    /// e, por pixel, volta ao referencial ORIGINAL pela INVERSA — a mesma
    /// técnica de um rasterizador de textura (ponto-no-polígono por
    /// coordenadas locais em vez de um teste geométrico de aresta, porque o
    /// polígono é sempre um paralelogramo e a inversa já responde "dentro?"
    /// em uma multiplicação).
    pub fn fill_rect_mat(&mut self, r: Rect, mat: &Mat2d, color: u32, clip: Option<Rect>) {
        let Some(inv) = mat_invert(mat) else { return };
        let (bx0, by0, bx1, by1) = transformed_bbox(r, mat);
        for y in by0..by1 {
            for x in bx0..bx1 {
                let fx = x as f32 + 0.5;
                let fy = y as f32 + 0.5;
                let (ox, oy) = inv.apply(fx, fy);
                if ox >= r.x && ox <= r.x + r.w && oy >= r.y && oy <= r.y + r.h {
                    self.blend(x, y, color, clip);
                }
            }
        }
    }

    /// A mesma ideia de `fill_rect_mat`, mas com o gradiente calculado no
    /// referencial LOCAL (`ox`,`oy`, já sem a matriz) — o CSS pinta o
    /// gradiente na caixa e SÓ DEPOIS aplica o `transform` (Transforms 1 §3:
    /// o alvo é a imagem já composta), então o `t` de interpolação usa a
    /// mesma fórmula de `fill_gradient`, só que alimentada pelo ponto
    /// devolvido pela inversa em vez do ponto de tela cru.
    pub fn fill_gradient_mat(&mut self, r: Rect, mat: &Mat2d, c0: u32, c1: u32, angle_deg: f32, clip: Option<Rect>) {
        let Some(inv) = mat_invert(mat) else { return };
        let rad = angle_deg.to_radians();
        let (dx, dy) = (rad.sin(), -rad.cos());
        let corners = [(r.x, r.y), (r.x + r.w, r.y), (r.x, r.y + r.h), (r.x + r.w, r.y + r.h)];
        let ts: Vec<f32> = corners.iter().map(|(cx, cy)| cx * dx + cy * dy).collect();
        let tmin = ts.iter().cloned().fold(f32::INFINITY, f32::min);
        let tmax = ts.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let span = (tmax - tmin).max(0.0001);
        let (bx0, by0, bx1, by1) = transformed_bbox(r, mat);
        for y in by0..by1 {
            for x in bx0..bx1 {
                let fx = x as f32 + 0.5;
                let fy = y as f32 + 0.5;
                let (ox, oy) = inv.apply(fx, fy);
                if ox < r.x || ox > r.x + r.w || oy < r.y || oy > r.y + r.h {
                    continue;
                }
                let t = ((ox * dx + oy * dy) - tmin) / span;
                let t = t.clamp(0.0, 1.0);
                self.blend(x, y, lerp_color(c0, c1, t), clip);
            }
        }
    }

    /// Gradiente linear por pixel. `angle_deg` na convenção do CSS (0 = para
    /// cima, 90 = para a direita) — a mesma leitura que `rts-egui`'s `pintura.rs` faz no
    /// backend egui, para que o mesh e este pixel-a-pixel concordem.
    pub fn fill_gradient(&mut self, r: Rect, c0: u32, c1: u32, angle_deg: f32, clip: Option<Rect>) {
        let rad = angle_deg.to_radians();
        let (dx, dy) = (rad.sin(), -rad.cos()); // direção do gradiente
        let x0 = r.x.floor() as i32;
        let y0 = r.y.floor() as i32;
        let x1 = (r.x + r.w).ceil() as i32;
        let y1 = (r.y + r.h).ceil() as i32;
        // Projeta os 4 cantos na direção do gradiente para achar [tmin, tmax] —
        // o intervalo real que o rect ocupa, não [0,1] cru.
        let corners = [(r.x, r.y), (r.x + r.w, r.y), (r.x, r.y + r.h), (r.x + r.w, r.y + r.h)];
        let ts: Vec<f32> = corners.iter().map(|(cx, cy)| cx * dx + cy * dy).collect();
        let tmin = ts.iter().cloned().fold(f32::INFINITY, f32::min);
        let tmax = ts.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let span = (tmax - tmin).max(0.0001);
        for y in y0..y1 {
            for x in x0..x1 {
                let fx = x as f32 + 0.5;
                let fy = y as f32 + 0.5;
                let t = ((fx * dx + fy * dy) - tmin) / span;
                let t = t.clamp(0.0, 1.0);
                self.blend(x, y, lerp_color(c0, c1, t), clip);
            }
        }
    }

    /// `stroke_rect` sob uma matriz: as 4 tiras, cada uma pelo seu próprio
    /// `fill_rect_mat` — um paralelogramo por lado é exato para uma
    /// transformação afim (a aresta reta de um retângulo continua reta sob
    /// `matrix()`), o mesmo argumento que já vale para `fill_rect_mat`.
    pub fn stroke_rect_mat(&mut self, r: Rect, mat: &Mat2d, width: f32, color: u32, clip: Option<Rect>) {
        let w = width.max(1.0);
        self.fill_rect_mat(Rect::new(r.x, r.y, r.w, w), mat, color, clip);
        self.fill_rect_mat(Rect::new(r.x, r.y + r.h - w, r.w, w), mat, color, clip);
        self.fill_rect_mat(Rect::new(r.x, r.y, w, r.h), mat, color, clip);
        self.fill_rect_mat(Rect::new(r.x + r.w - w, r.y, w, r.h), mat, color, clip);
    }
}

/// A inversa de uma matriz afim 2D (`[[a,c,e],[b,d,f],[0,0,1]]`), ou `None`
/// se o determinante for ~0 (`scale(0)`, degenerada — nada pintável de todo
/// modo). Fórmula fechada de uma 2×2 mais a translação recomposta.
pub fn mat_invert(m: &Mat2d) -> Option<Mat2d> {
    let det = m.a * m.d - m.b * m.c;
    if det.abs() < 1.0e-9 {
        return None;
    }
    let inv_det = 1.0 / det;
    let a = m.d * inv_det;
    let b = -m.b * inv_det;
    let c = -m.c * inv_det;
    let d = m.a * inv_det;
    Some(Mat2d {
        a,
        b,
        c,
        d,
        e: -(a * m.e + c * m.f),
        f: -(b * m.e + d * m.f),
    })
}

/// A caixa de pixels inteiros que cobre os 4 cantos de `r` sob `mat` —
/// mesma conta que `Mat2d::transform_rect_bbox`, mas já em `i32` de canvas
/// (floor/ceil, como `fill_rect`) para os dois rasterizadores por matriz
/// iterarem sobre ela.
pub fn transformed_bbox(r: Rect, mat: &Mat2d) -> (i32, i32, i32, i32) {
    let pts = [
        mat.apply(r.x, r.y),
        mat.apply(r.x + r.w, r.y),
        mat.apply(r.x, r.y + r.h),
        mat.apply(r.x + r.w, r.y + r.h),
    ];
    let min_x = pts.iter().fold(f32::INFINITY, |m, p| m.min(p.0));
    let max_x = pts.iter().fold(f32::NEG_INFINITY, |m, p| m.max(p.0));
    let min_y = pts.iter().fold(f32::INFINITY, |m, p| m.min(p.1));
    let max_y = pts.iter().fold(f32::NEG_INFINITY, |m, p| m.max(p.1));
    (
        min_x.floor().max(0.0) as i32,
        min_y.floor().max(0.0) as i32,
        (max_x.ceil() as i32).min(W as i32),
        (max_y.ceil() as i32).min(H as i32),
    )
}

pub fn argb_bytes(c: u32) -> (u8, u8, u8, u8) {
    // A `DisplayList` guarda RGBA em u32 (ver comentário em `paint/item.rs`:
    // "cor é u32 RGBA"). Byte mais significativo = R.
    (
        ((c >> 24) & 0xff) as u8,
        ((c >> 16) & 0xff) as u8,
        ((c >> 8) & 0xff) as u8,
        (c & 0xff) as u8,
    )
}

pub fn lerp_color(c0: u32, c1: u32, t: f32) -> u32 {
    let (r0, g0, b0, a0) = argb_bytes(c0);
    let (r1, g1, b1, a1) = argb_bytes(c1);
    let l = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u32;
    (l(r0, r1) << 24) | (l(g0, g1) << 16) | (l(b0, b1) << 8) | l(a0, a1)
}

pub fn rect_intersect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.w).min(b.x + b.w);
    let y1 = (a.y + a.h).min(b.y + b.h);
    Rect::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}
