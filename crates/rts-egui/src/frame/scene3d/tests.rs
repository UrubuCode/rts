    use super::*;
    use super::math::v_len;
    use super::math::quat_mul;

    /// Aplica a matriz column-major `m` (4×4) ao ponto homogêneo `(p,1)`.
    fn apply(m: &[f32; 16], p: [f32; 3]) -> [f32; 4] {
        let mut o = [0f32; 4];
        for r in 0..4 {
            o[r] = m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r];
        }
        o
    }

    /// Um ponto à FRENTE da câmera projeta dentro do clip volume: w>0 e z∈[0,w]
    /// (convenção wgpu, depth 0..1 após a divisão por w). Pega erro de sinal/
    /// transposição na projeção LH.
    #[test]
    fn point_in_front_projects_inside_clip() {
        let cam = view_proj_lookat([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 1.0, 1.5, 0.1, 100.0);
        let clip = apply(&cam.view_proj, [0.0, 0.0, 5.0]); // 5 à frente (+z)
        assert!(clip[3] > 0.0, "w deve ser >0 à frente, veio {}", clip[3]);
        let ndc_z = clip[2] / clip[3];
        assert!((0.0..=1.0).contains(&ndc_z), "z ndc fora de [0,1]: {ndc_z}");
    }

    /// A base look-at é ortonormal (right/up/fwd unitários e mutuamente ⟂).
    #[test]
    fn lookat_basis_orthonormal() {
        let cam = view_proj_lookat([3.0, 2.0, -4.0], [0.0, 0.0, 0.0], 1.0, 1.0, 0.1, 100.0);
        for b in [cam.right, cam.up, cam.fwd] {
            assert!((v_len(b) - 1.0).abs() < 1e-4, "base não unitária: {}", v_len(b));
        }
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        assert!(dot(cam.right, cam.up).abs() < 1e-4);
        assert!(dot(cam.right, cam.fwd).abs() < 1e-4);
        assert!(dot(cam.up, cam.fwd).abs() < 1e-4);
    }

    /// Olhar reto pra baixo (fwd ∥ up de referência) NÃO gera NaN — usa o up alt.
    #[test]
    fn lookat_straight_down_no_nan() {
        let cam = view_proj_lookat([0.0, 10.0, 0.0], [0.0, 0.0, 0.0], 1.0, 1.0, 0.1, 100.0);
        for c in cam.view_proj {
            assert!(c.is_finite(), "view_proj tem NaN/inf olhando reto pra baixo");
        }
        assert!((v_len(cam.right) - 1.0).abs() < 1e-4);
    }

    /// `model_matrix` sem rotação/escala 1 é translação pura.
    #[test]
    fn model_translation_only() {
        let m = model_matrix(2.0, -3.0, 4.0, 0.0, 0.0, 1.0, 1.0, 1.0);
        let p = apply(&m, [1.0, 1.0, 1.0]);
        assert_eq!([p[0], p[1], p[2]], [3.0, -2.0, 5.0]);
    }

    /// Mapeamento tex→flag: 0=nenhuma, 1=xadrez, e QUALQUER id real (>=2) vira 2.0.
    #[test]
    fn tex_flag_mapping() {
        assert_eq!(tex_flag(0), 0.0); // nenhuma
        assert_eq!(tex_flag(1), 1.0); // xadrez procedural
        assert_eq!(tex_flag(2), 2.0); // 1ª textura real
        assert_eq!(tex_flag(3), 2.0);
        assert_eq!(tex_flag(99), 2.0); // qualquer id real → flag 2 (bind group seleciona)
    }

    /// Contrato: `model_matrix_quat` com o quaternion de yaw puro (ry em torno de
    /// Y) bate byte-a-byte com `model_matrix(ry)` — é a identidade que fixa a
    /// convenção de sinal do quaternion contra a do renderer.
    #[test]
    fn model_matrix_quat_yaw_bate_com_model_matrix() {
        // quaternion de yaw 0.7 em torno de Y: (0, sin(0.35), 0, cos(0.35))
        let (s, c) = (0.35f32.sin(), 0.35f32.cos());
        let a = model_matrix(1.0, 2.0, 3.0, 0.0, 0.7, 1.5, 1.5, 1.5);
        let b = model_matrix_quat(1.0, 2.0, 3.0, [0.0, s, 0.0, c], 1.5, 1.5, 1.5);
        for i in 0..16 { assert!((a[i] - b[i]).abs() < 1e-5, "i={i} a={} b={}", a[i], b[i]); }
    }

    /// Composição de quaternions: yaw seguido de pitch local deve bater com a
    /// composição manual Ry · Rx que `model_matrix` usa.
    #[test]
    fn model_matrix_quat_gira_x_local_depois_de_yaw() {
        // q = yaw(pi/2) * pitch(0.5): o eixo local Z vai para (sin(pi/2)*cos .5, -sin .5?, ...)
        // conferido contra a composição manual Ry * Rx
        let qy = [0.0, (std::f32::consts::FRAC_PI_4).sin(), 0.0, (std::f32::consts::FRAC_PI_4).cos()];
        let qx = [(0.25f32).sin(), 0.0, 0.0, (0.25f32).cos()];
        let q = quat_mul(qy, qx);
        let m = model_matrix_quat(0.0, 0.0, 0.0, q, 1.0, 1.0, 1.0);
        // coluna 2 = imagem do eixo local Z
        let z = [m[8], m[9], m[10]];
        // Rx(0.5) leva Z a (0, -sin .5, cos .5); Ry(pi/2) leva (x,y,z) a (z, y, -x)
        let esperado = [0.5f32.cos(), -(0.5f32.sin()), 0.0];
        for i in 0..3 { assert!((z[i] - esperado[i]).abs() < 1e-5, "i={i} {:?}", z); }
    }
