//! Várias câmeras por frame. Cada vista é (câmera, retângulo em fração da
//! janela com y a partir do topo, fundo, limpar). `setViewport` COMEÇA uma
//! vista: a primeira chamada do frame só define o retângulo da vista corrente;
//! as seguintes guardam a anterior. Sem chamada nenhuma há uma vista cheia —
//! o comportamento de antes. Câmera e fundo persistem entre frames, como antes.

use super::math::Cam3D;

pub const MAX_VIEWS: usize = 8;
/// Bytes de um slot de câmera no uniform (alinhamento de offset dinâmico).
pub const CAM_STRIDE: u64 = 256;
pub const CAM_FLOATS: usize = 64;
pub const FULL: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fundo {
    Ceu,
    Cor([f32; 4]),
}

#[derive(Clone, Copy, Debug)]
pub struct View {
    pub cam: Cam3D,
    pub rect: [f32; 4],
    pub fundo: Fundo,
    /// false = fundo "nada": não pinta o fundo, só limpa a profundidade.
    pub limpar: bool,
}

pub struct ViewQueue {
    done: Vec<View>,
    cur: View,
    aberta: bool,
}

impl ViewQueue {
    pub fn new(cam: Cam3D) -> ViewQueue {
        ViewQueue { done: Vec::with_capacity(MAX_VIEWS), cur: View { cam, rect: FULL, fundo: Fundo::Ceu, limpar: true }, aberta: false }
    }
    pub fn set_camera(&mut self, cam: Cam3D) { self.cur.cam = cam; }
    pub fn set_fundo(&mut self, f: Fundo) { self.cur.fundo = f; }
    /// `setSkybox(on)`: liga o céu; desligar não desfaz uma cor (como antes).
    pub fn skybox(&mut self, on: bool) { if on { self.cur.fundo = Fundo::Ceu; } }
    pub fn set_viewport(&mut self, rect: [f32; 4], limpar: bool) {
        if self.aberta && self.done.len() < MAX_VIEWS - 1 { self.done.push(self.cur); }
        self.aberta = true;
        self.cur.rect = clamp_rect(rect);
        self.cur.limpar = limpar;
    }
    pub fn len(&self) -> usize { self.done.len() + 1 }
    pub fn get(&self, i: usize) -> &View { if i < self.done.len() { &self.done[i] } else { &self.cur } }
    /// Fecha o frame. Com várias vistas, o próximo frame começa com o fundo e a
    /// câmera da PRIMEIRA (não da última, que é quase sempre um detalhe como um
    /// minimapa); com uma vista só, tudo persiste como antes.
    pub fn end_frame(&mut self) {
        if let Some(primeira) = self.done.first() {
            self.cur.fundo = primeira.fundo;
            self.cur.cam = primeira.cam;
        }
        self.done.clear();
        self.aberta = false;
        self.cur.rect = FULL;
        self.cur.limpar = true;
    }
}

fn fin(v: f32) -> f32 { if v.is_finite() { v } else { 0.0 } }

/// Retângulo dentro da janela: x, y em [0, 1]; w, h cortados na borda.
pub fn clamp_rect(r: [f32; 4]) -> [f32; 4] {
    let x = fin(r[0]).clamp(0.0, 1.0);
    let y = fin(r[1]).clamp(0.0, 1.0);
    [x, y, fin(r[2]).clamp(0.0, 1.0 - x), fin(r[3]).clamp(0.0, 1.0 - y)]
}

/// Retângulo em pixels do alvo; `None` quando a vista não tem área.
pub fn viewport_px(rect: [f32; 4], w: u32, h: u32) -> Option<[u32; 4]> {
    let (wf, hf) = (w as f32, h as f32);
    let x0 = (rect[0] * wf).round() as u32;
    let y0 = (rect[1] * hf).round() as u32;
    let x1 = (((rect[0] + rect[2]) * wf).round() as u32).min(w);
    let y1 = (((rect[1] + rect[3]) * hf).round() as u32).min(h);
    if x1 <= x0 || y1 <= y0 { None } else { Some([x0, y0, x1 - x0, y1 - y0]) }
}

/// Um slot do uniform `Cam` (ver `layout_do_uniform_bate_com_o_empacotamento`).
pub fn cam_floats(v: &View, light: [f32; 4], light_vp: &[f32; 16], water: f32) -> [f32; CAM_FLOATS] {
    let c = &v.cam;
    let mut f = [0f32; CAM_FLOATS];
    f[0..16].copy_from_slice(&c.view_proj);
    f[16..20].copy_from_slice(&light);
    f[20..23].copy_from_slice(&c.cam_pos);
    f[24..27].copy_from_slice(&c.right); f[27] = c.tan_h;
    f[28..31].copy_from_slice(&c.up); f[31] = c.tan_v;
    f[32..35].copy_from_slice(&c.fwd);
    f[36..52].copy_from_slice(light_vp);
    f[52] = water;
    if let Fundo::Cor(cor) = v.fundo { f[56] = cor[0]; f[57] = cor[1]; f[58] = cor[2]; f[59] = 1.0; }
    f[60] = c.ortho; f[61] = c.half_h; f[62] = c.half_w;
    f
}
