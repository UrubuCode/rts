pub(in crate::frame::scene3d) fn identity() -> [f32; 16] {
    [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]
}

/// view·proj ORTOGRÁFICA da luz direcional (shadow map). `dir` = direção que a luz
/// viaja; a câmera-luz é posta em `center - dir*2r` olhando na direção `dir`; ortho
/// meio-extent = radius, depth 0..1 (convenção wgpu). Column-major.
pub(in crate::frame::scene3d) fn light_view_proj(dir: [f32; 3], center: [f32; 3], radius: f32) -> [f32; 16] {
    let mut f = dir;
    let fl = (f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt().max(1e-6);
    f = [f[0] / fl, f[1] / fl, f[2] / fl];
    // up de referência; se quase paralelo a f, troca por (1,0,0)
    let up0 = if f[1].abs() > 0.99 { [1.0f32, 0.0, 0.0] } else { [0.0f32, 1.0, 0.0] };
    // right = normalize(up0 × f); up = f × right
    let mut r = cross(up0, f);
    let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt().max(1e-6);
    r = [r[0] / rl, r[1] / rl, r[2] / rl];
    let u = cross(f, r);
    let eye = [center[0] - f[0] * 2.0 * radius, center[1] - f[1] * 2.0 * radius, center[2] - f[2] * 2.0 * radius];
    let tx = -(r[0] * eye[0] + r[1] * eye[1] + r[2] * eye[2]);
    let ty = -(u[0] * eye[0] + u[1] * eye[1] + u[2] * eye[2]);
    let tz = -(f[0] * eye[0] + f[1] * eye[1] + f[2] * eye[2]);
    let v = [
        r[0], u[0], f[0], 0.0,
        r[1], u[1], f[1], 0.0,
        r[2], u[2], f[2], 0.0,
        tx, ty, tz, 1.0,
    ];
    let near = 0.05f32;
    let far = 4.0 * radius;
    let inv = 1.0 / radius;
    let dz = 1.0 / (far - near);
    let p = [
        inv, 0.0, 0.0, 0.0,
        0.0, inv, 0.0, 0.0,
        0.0, 0.0, dz, 0.0,
        0.0, 0.0, -near * dz, 1.0,
    ];
    mul(&p, &v)
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// view·proj (column-major) a partir de câmera fly (yaw/pitch) — MESMA convenção
/// do rasterizador software: right=(cyw,0,-syw), up=(-syw*spt,cpt,-cyw*spt),
/// forward=(syw*cpt,spt,cyw*cpt); proj perspectiva left-handed (z forward, depth 0..1).
/// Dados de câmera pro uniform 3D: viewProj + posição + base (right/up/fwd) +
/// tangentes do FOV (pro raio da skybox).
pub struct Cam3D {
    pub view_proj: [f32; 16],
    pub cam_pos: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub fwd: [f32; 3],
    pub tan_h: f32,
    pub tan_v: f32,
}

pub fn view_proj(
    camx: f32, camy: f32, camz: f32, yaw: f32, pitch: f32, fov_y: f32, aspect: f32,
) -> Cam3D {
    let (cyw, syw) = (yaw.cos(), yaw.sin());
    let (cpt, spt) = (pitch.cos(), pitch.sin());
    let right = [cyw, 0.0, -syw];
    let up = [-syw * spt, cpt, -cyw * spt];
    let fwd = [syw * cpt, spt, cyw * cpt];
    let tx = -(right[0] * camx + right[1] * camy + right[2] * camz);
    let ty = -(up[0] * camx + up[1] * camy + up[2] * camz);
    let tz = -(fwd[0] * camx + fwd[1] * camy + fwd[2] * camz);
    let v = [
        right[0], up[0], fwd[0], 0.0,
        right[1], up[1], fwd[1], 0.0,
        right[2], up[2], fwd[2], 0.0,
        tx, ty, tz, 1.0,
    ];
    let (p, tan_v) = perspective_lh(fov_y, aspect, 0.1, 500.0);
    Cam3D {
        view_proj: mul(&p, &v),
        cam_pos: [camx, camy, camz],
        right,
        up,
        fwd,
        tan_h: tan_v * aspect,
        tan_v,
    }
}

/// Projeção perspectiva left-handed (z forward, depth 0..1 — convenção wgpu/DX),
/// column-major. `fov_y` em radianos, `aspect` = largura/altura. Compartilhada
/// entre a câmera fly (`view_proj`) e a look-at.
fn perspective_lh(fov_y: f32, aspect: f32, near: f32, far: f32) -> ([f32; 16], f32) {
    let tan_v = (fov_y * 0.5).tan();
    let f = 1.0 / tan_v;
    let p = [
        f / aspect, 0.0, 0.0, 0.0,
        0.0, f, 0.0, 0.0,
        0.0, 0.0, far / (far - near), 1.0,
        0.0, 0.0, -(far * near) / (far - near), 0.0,
    ];
    (p, tan_v)
}

/// Câmera LOOK-AT NaN-safe (`eye` olhando pra `target`, up de referência +Y) com
/// `near`/`far` explícitos — mais robusta que yaw/pitch pro "frame selected" do
/// editor: quando a direção fica ~paralela ao up (olhar reto pra cima/baixo) usa
/// um up alternativo (+Z) em vez de gerar NaN por gimbal. Mesma base LH de
/// `view_proj` (right/up/fwd ortonormais), então skybox/shading seguem casando.
pub fn view_proj_lookat(
    eye: [f32; 3], target: [f32; 3], fov_y: f32, aspect: f32, near: f32, far: f32,
) -> Cam3D {
    let mut fwd = v_norm(v_sub(target, eye));
    if fwd == [0.0, 0.0, 0.0] {
        fwd = [0.0, 0.0, 1.0]; // eye≈target: direção default em vez de zero/NaN
    }
    // right = worldUp × fwd; se degenerado (fwd ~paralelo a +Y), usa +Z como up alt.
    let mut right = v_cross([0.0, 1.0, 0.0], fwd);
    if v_len(right) < 1e-4 {
        right = v_cross([0.0, 0.0, 1.0], fwd);
    }
    let right = v_norm(right);
    let up = v_cross(fwd, right); // já ortonormal (fwd,right unitários e ⟂)
    let tx = -(right[0] * eye[0] + right[1] * eye[1] + right[2] * eye[2]);
    let ty = -(up[0] * eye[0] + up[1] * eye[1] + up[2] * eye[2]);
    let tz = -(fwd[0] * eye[0] + fwd[1] * eye[1] + fwd[2] * eye[2]);
    let v = [
        right[0], up[0], fwd[0], 0.0,
        right[1], up[1], fwd[1], 0.0,
        right[2], up[2], fwd[2], 0.0,
        tx, ty, tz, 1.0,
    ];
    let (p, tan_v) = perspective_lh(fov_y, aspect, near, far);
    Cam3D {
        view_proj: mul(&p, &v),
        cam_pos: eye,
        right,
        up,
        fwd,
        tan_h: tan_v * aspect,
        tan_v,
    }
}

#[inline]
fn v_sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
fn v_cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
#[inline]
pub(in crate::frame::scene3d) fn v_len(a: [f32; 3]) -> f32 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}
#[inline]
fn v_norm(a: [f32; 3]) -> [f32; 3] {
    let l = v_len(a);
    if l < 1e-9 { [0.0, 0.0, 0.0] } else { [a[0] / l, a[1] / l, a[2] / l] }
}

/// model = T · Ry · Rx · S (column-major) — casa com a rotação Y-depois-X do soft.
pub fn model_matrix(
    px: f32, py: f32, pz: f32, rx: f32, ry: f32, sx: f32, sy: f32, sz: f32,
) -> [f32; 16] {
    let (cx, sxr) = (rx.cos(), rx.sin());
    let (cy, syr) = (ry.cos(), ry.sin());
    // R = Rx · Ry (Ry aplicado primeiro), 3x3
    let r00 = cy;
    let r01 = 0.0;
    let r02 = syr;
    let r10 = sxr * syr;
    let r11 = cx;
    let r12 = -sxr * cy;
    let r20 = -cx * syr;
    let r21 = sxr;
    let r22 = cx * cy;
    // model column-major: colunas escaladas por sx/sy/sz, última = translação
    [
        r00 * sx, r10 * sx, r20 * sx, 0.0,
        r01 * sy, r11 * sy, r21 * sy, 0.0,
        r02 * sz, r12 * sz, r22 * sz, 0.0,
        px, py, pz, 1.0,
    ]
}

/// a·b para matrizes 4x4 column-major.
fn mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut o = [0f32; 16];
    for c in 0..4 {
        for r in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[k * 4 + r] * b[c * 4 + k];
            }
            o[c * 4 + r] = s;
        }
    }
    o
}

