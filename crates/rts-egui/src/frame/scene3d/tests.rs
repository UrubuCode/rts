    use super::*;
    use super::math::v_len;

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
