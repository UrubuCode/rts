//! Mat4 mínima para o scene pass 3D — perspectiva, look-at e composição
//! T·Ry·Rx·S. Column-major no layout de memória (o que o WGSL `mat4x4<f32>`
//! espera ao ler o uniform), mas as operações abaixo pensam em colunas como
//! vetores-base, convenção OpenGL/wgpu (clip-space Z em [0,1] — ver `perspective`).
//!
//! Feito à mão de propósito: são ~6 funções; puxar `glam` para isto adicionaria
//! uma dependência ao crate por nada.

/// Matriz 4×4 column-major: `m[c]` é a coluna `c` (4 floats).
#[derive(Clone, Copy)]
pub struct Mat4(pub [[f32; 4]; 4]);

impl Mat4 {
    /// `self * rhs` (aplica `rhs` primeiro, depois `self`).
    pub fn mul(&self, rhs: &Mat4) -> Mat4 {
        let a = &self.0;
        let b = &rhs.0;
        let mut out = [[0.0f32; 4]; 4];
        for c in 0..4 {
            for r in 0..4 {
                out[c][r] = a[0][r] * b[c][0]
                    + a[1][r] * b[c][1]
                    + a[2][r] * b[c][2]
                    + a[3][r] * b[c][3];
            }
        }
        Mat4(out)
    }

    /// Bytes do uniform (64 bytes, column-major — layout direto do `mat4x4<f32>`).
    pub fn to_bytes(&self) -> [u8; 64] {
        let mut out = [0u8; 64];
        for (i, col) in self.0.iter().enumerate() {
            for (j, v) in col.iter().enumerate() {
                out[i * 16 + j * 4..i * 16 + j * 4 + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
        out
    }
}

/// Perspectiva com clip-space Z ∈ [0,1] (convenção wgpu/DX — NÃO a [-1,1] do GL).
/// `fovy` em radianos; `aspect` = largura/altura.
pub fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fovy / 2.0).tan();
    let r = far / (near - far);
    Mat4([
        [f / aspect, 0.0, 0.0, 0.0],
        [0.0, f, 0.0, 0.0],
        [0.0, 0.0, r, -1.0],
        [0.0, 0.0, r * near, 0.0],
    ])
}

/// Câmera look-at (right-handed, up de referência +Y). Se `eye`≈`target` ou a
/// direção é paralela ao up, degrada com um up alternativo em vez de NaN.
pub fn look_at(eye: [f32; 3], target: [f32; 3]) -> Mat4 {
    let fwd = norm(sub(target, eye));
    // up de referência: +Y; se a câmera olha quase reto pra cima/baixo, usa +Z.
    let up_ref = if fwd[1].abs() > 0.999 { [0.0, 0.0, 1.0] } else { [0.0, 1.0, 0.0] };
    let right = norm(cross(fwd, up_ref));
    let up = cross(right, fwd);
    Mat4([
        [right[0], up[0], -fwd[0], 0.0],
        [right[1], up[1], -fwd[1], 0.0],
        [right[2], up[2], -fwd[2], 0.0],
        [-dot(right, eye), -dot(up, eye), dot(fwd, eye), 1.0],
    ])
}

/// Modelo = Translate(x,y,z) · RotY(yaw) · RotX(pitch) · Scale(s).
pub fn model_trs(x: f32, y: f32, z: f32, yaw: f32, pitch: f32, s: f32) -> Mat4 {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    // RotY·RotX·S composta direto (colunas = eixos rotacionados × escala).
    Mat4([
        [cy * s, 0.0, -sy * s, 0.0],
        [sy * sp * s, cp * s, cy * sp * s, 0.0],
        [sy * cp * s, -sp * s, cy * cp * s, 0.0],
        [x, y, z, 1.0],
    ])
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(v: [f32; 3]) -> [f32; 3] {
    let len = dot(v, v).sqrt();
    if len < 1e-9 {
        return [0.0, 0.0, -1.0];
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Um ponto na frente da câmera projeta dentro do clip volume com w>0 e
    /// z∈[0,1] (convenção wgpu) — pega erro de sinal/transposição na pipeline.
    #[test]
    fn point_in_front_projects_inside_clip() {
        let view = look_at([0.0, 0.0, 5.0], [0.0, 0.0, 0.0]);
        let proj = perspective(60f32.to_radians(), 16.0 / 9.0, 0.1, 100.0);
        let vp = proj.mul(&view);
        // ponto na origem (5 unidades à frente da câmera)
        let m = &vp.0;
        let p = [0.0f32, 0.0, 0.0, 1.0];
        let mut clip = [0.0f32; 4];
        for r in 0..4 {
            clip[r] = m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r] * p[3];
        }
        assert!(clip[3] > 0.0, "w deve ser positivo à frente da câmera (w={})", clip[3]);
        let ndc_z = clip[2] / clip[3];
        assert!(
            (0.0..=1.0).contains(&ndc_z),
            "z NDC deve cair em [0,1] (wgpu), veio {ndc_z}"
        );
    }

    /// model_trs sem rotação/escala é uma translação pura.
    #[test]
    fn model_trs_translation_only() {
        let m = model_trs(1.0, 2.0, 3.0, 0.0, 0.0, 1.0);
        assert_eq!(m.0[3][0], 1.0);
        assert_eq!(m.0[3][1], 2.0);
        assert_eq!(m.0[3][2], 3.0);
        assert_eq!(m.0[0][0], 1.0);
        assert_eq!(m.0[1][1], 1.0);
        assert_eq!(m.0[2][2], 1.0);
    }
}
