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

    /// Contraparte de `lookat_straight_down_no_nan`: olhar reto pra CIMA
    /// também cai no up alternativo (+Z) em vez de gerar NaN — mesmo ramo do
    /// `if v_len(right) < 1e-4` em `view_proj_lookat`, só do outro lado.
    #[test]
    fn lookat_straight_up_no_nan() {
        let cam = view_proj_lookat([0.0, -10.0, 0.0], [0.0, 0.0, 0.0], 1.0, 1.0, 0.1, 100.0);
        for c in cam.view_proj {
            assert!(c.is_finite(), "view_proj tem NaN/inf olhando reto pra cima");
        }
        assert!((v_len(cam.right) - 1.0).abs() < 1e-4);
        assert!((v_len(cam.up) - 1.0).abs() < 1e-4);
    }

    /// `cam.right`/`cam.up` são o que `vs_particle` usa pra bilheteirar
    /// (billboard) uma partícula no plano da câmera (`shader.rs`). A câmera
    /// de VOO (`view_proj_spec`, o que `setCamera` monta) NUNCA passa por
    /// `cross(forward, worldUp)` — right/up/fwd vêm direto de trigonometria
    /// em yaw/pitch, então não há gimbal pra degenerar. Este teste prova isso
    /// numericamente exatamente no ponto onde um `cross`-based basis
    /// degeneraria: pitch = ±90° (olhando reto pra cima/baixo).
    #[test]
    fn fly_camera_basis_ortonormal_em_pitch_mais_ou_menos_90_graus() {
        use std::f32::consts::FRAC_PI_2;
        for pitch in [FRAC_PI_2, -FRAC_PI_2] {
            let mut s = spec(false);
            s.pitch = pitch;
            s.yaw = 0.7; // yaw não-trivial: pega erro de sinal que um yaw=0 esconderia
            let cam = view_proj_spec(&s);
            for (nome, b) in [("right", cam.right), ("up", cam.up), ("fwd", cam.fwd)] {
                for c in b {
                    assert!(c.is_finite(), "pitch={pitch}: {nome} tem NaN/inf: {b:?}");
                }
                assert!((v_len(b) - 1.0).abs() < 1e-4, "pitch={pitch}: {nome} não é unitário: {}", v_len(b));
            }
            let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            assert!(dot(cam.right, cam.up).abs() < 1e-4, "pitch={pitch}: right·up != 0");
            assert!(dot(cam.right, cam.fwd).abs() < 1e-4, "pitch={pitch}: right·fwd != 0");
            assert!(dot(cam.up, cam.fwd).abs() < 1e-4, "pitch={pitch}: up·fwd != 0");
        }
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

    use super::lights::{attenuation, spot_factor, fog_factor, fog_params, pack_lights, SkyParams, env_floats,
        MAX_LIGHTS, LIGHT_IN, LIGHT_GPU, SKY_IN, ENV_FLOATS};
    use super::views::{ViewQueue, Fundo, FULL, MAX_VIEWS, clamp_rect, viewport_px, cam_floats};
    use super::math::{CamSpec, view_proj_spec};

    fn luz(tipo: f64, pos: [f64; 3], dir: [f64; 3], sombra: f64) -> [f64; LIGHT_IN] {
        [tipo, pos[0], pos[1], pos[2], dir[0], dir[1], dir[2], 1.0, 0.5, 0.25, 2.0, 7.0, 0.9, 0.8, sombra, 0.0]
    }
    fn spec(ortho: bool) -> CamSpec {
        CamSpec { pos: [0.0, 0.0, 0.0], yaw: 0.0, pitch: 0.0, fov_y: 1.0, aspect: 2.0, near: 0.1, far: 100.0, ortho, ortho_size: 5.0 }
    }

    #[test]
    fn atenuacao_nos_pontos_chave() {
        assert_eq!(attenuation(0.0, 10.0), 1.0);
        assert!((attenuation(5.0, 10.0) - 0.5625).abs() < 1e-6, "(1 - 0,25)^2 = 0,5625");
        assert_eq!(attenuation(10.0, 10.0), 0.0);
        assert_eq!(attenuation(12.0, 10.0), 0.0, "alem do alcance e zero, nao negativo");
        assert_eq!(attenuation(1.0, 0.0), 0.0, "alcance 0 apaga a luz");
    }

    #[test]
    fn spot_nos_cones() {
        assert_eq!(spot_factor(0.95, 0.9, 0.8), 1.0);
        assert_eq!(spot_factor(0.9, 0.9, 0.8), 1.0);
        assert_eq!(spot_factor(0.8, 0.9, 0.8), 0.0);
        assert_eq!(spot_factor(0.7, 0.9, 0.8), 0.0);
        assert!((spot_factor(0.85, 0.9, 0.8) - 0.5).abs() < 1e-6, "smoothstep no meio = 0,5");
        assert_eq!(spot_factor(0.81, 0.8, 0.8), 1.0, "cones iguais viram degrau");
        assert_eq!(spot_factor(0.79, 0.8, 0.8), 0.0);
    }

    #[test]
    fn neblina_exponencial() {
        assert_eq!(fog_factor(10.0, 0.0), 1.0, "densidade 0 desliga");
        assert!((fog_factor(10.0, 0.1) - (-1.0f32).exp()).abs() < 1e-6);
        assert_eq!(fog_params(0.5, f64::NAN, 2.0, -1.0), [0.5, 0.0, 2.0, 0.0], "NaN vira 0 e densidade negativa vira 0");
    }

    #[test]
    fn pack_lights_corta_em_8_e_acha_a_sombra() {
        let mut src = Vec::new();
        src.extend_from_slice(&luz(1.0, [1.0, 2.0, 3.0], [0.0, 0.0, 1.0], 1.0)); // pontual com sombra: ignorada
        src.extend_from_slice(&luz(0.0, [0.0; 3], [0.0, -1.0, 0.0], 0.0));      // direcional sem sombra
        src.extend_from_slice(&luz(0.0, [0.0; 3], [0.0, -2.0, 0.0], 1.0));      // direcional COM sombra
        for k in 0..7 { src.extend_from_slice(&luz(1.0, [k as f64, 0.0, 0.0], [0.0, 0.0, 1.0], 0.0)); }
        let p = pack_lights(&src, 10);
        assert_eq!(p.n, MAX_LIGHTS as u32);
        assert_eq!(p.shadow, 2, "a primeira DIRECIONAL com sombra");
        let g = &p.gpu[2 * LIGHT_GPU..3 * LIGHT_GPU];
        assert_eq!(g[3], 0.0, "tipo em a.w");
        assert_eq!([g[4], g[5], g[6]], [0.0, -1.0, 0.0], "direcao normalizada em b.xyz");
        assert_eq!(g[7], 7.0, "alcance em b.w");
        assert_eq!([g[8], g[9], g[10], g[11]], [1.0, 0.5, 0.25, 2.0], "cor em c.rgb, intensidade em c.w");
        assert_eq!([g[12], g[13], g[14]], [0.9, 0.8, 1.0], "cones e sombra em d");
        let g0 = &p.gpu[0..LIGHT_GPU];
        assert_eq!([g0[0], g0[1], g0[2], g0[3]], [1.0, 2.0, 3.0, 1.0], "posicao em a.xyz");
    }

    #[test]
    fn pack_lights_sem_sombra_buffer_curto_e_valores_ruins() {
        let mut src = Vec::new();
        let mut l = luz(2.0, [0.0; 3], [0.0, 0.0, 0.0], 0.0);
        l[10] = f64::NAN; l[12] = 0.7; l[13] = 0.95;
        src.extend_from_slice(&l);
        src.extend_from_slice(&[0.0; 4]); // sobra que nao fecha uma luz
        let p = pack_lights(&src, 3);
        assert_eq!(p.n, 1, "n preso ao que o buffer carrega");
        assert_eq!(p.shadow, -1);
        let g = &p.gpu[0..LIGHT_GPU];
        assert_eq!([g[4], g[5], g[6]], [0.0, 0.0, 1.0], "direcao nula vira +Z");
        assert_eq!(g[11], 1.0, "intensidade NaN vira 1");
        assert_eq!([g[12], g[13]], [0.95, 0.7], "cone interno sempre >= externo");
        assert_eq!(pack_lights(&[], 5).n, 0);
    }

    #[test]
    fn ceu_padrao_curto_e_fora_da_faixa() {
        assert_eq!(SkyParams::from_f64(&[]), SkyParams::padrao());
        let s = SkyParams::from_f64(&[1.0, 0.1, 0.2, 0.3]);
        assert_eq!(s.modo, 1.0);
        assert_eq!(s.topo, [0.1, 0.2, 0.3]);
        assert_eq!(s.horizonte, SkyParams::padrao().horizonte, "o que falta fica no padrao");
        assert_eq!(SkyParams::from_f64(&[7.0]).modo, 3.0, "modo preso em 0..3");
        assert_eq!(SkyParams::from_f64(&[f64::NAN]).modo, 0.0, "NaN vira o padrao");
        let mut todo = [0.0f64; SKY_IN];
        todo[0] = 3.0; todo[16] = 5.0; todo[17] = 2.0; todo[21] = 0.4;
        todo[10] = 0.0; todo[11] = 0.0; todo[12] = 0.0;
        let s2 = SkyParams::from_f64(&todo);
        assert_eq!(s2.textura, 5);
        assert_eq!(s2.amb_modo, 2.0);
        assert_eq!(s2.amb_intensidade, 0.4);
        assert!((s2.sol[1] + 0.9045).abs() < 1e-3, "sol nulo cai no sol padrao");
    }

    #[test]
    fn env_floats_layout() {
        let mut src = Vec::new();
        src.extend_from_slice(&luz(0.0, [0.0; 3], [0.0, -1.0, 0.0], 1.0));
        let p = pack_lights(&src, 1);
        let mut sky = SkyParams::padrao();
        sky.modo = 1.0; sky.topo = [0.1, 0.2, 0.3]; sky.amb_modo = 1.0; sky.amb_intensidade = 0.3;
        let e = env_floats(&p, &sky, true, [0.5, 0.6, 0.7, 0.02]);
        assert_eq!(e.len(), ENV_FLOATS);
        assert_eq!([e[0], e[1], e[2], e[3]], [1.0, 0.0, 1.0, 0.3], "info: n, sombra, modo e intensidade do ambiente");
        assert_eq!(e[8], 1.0, "sky0.x = modo");
        assert_eq!([e[12], e[13], e[14]], [0.1, 0.2, 0.3], "topo");
        assert_eq!(e[27], 1.0, "sun_dir.w = tem panorama");
        assert_eq!([e[28], e[29], e[30], e[31]], [0.5, 0.6, 0.7, 0.02], "neblina");
        assert_eq!(&e[32..48], &p.gpu[0..16], "luzes a partir do float 32");
    }

    #[test]
    fn perspectiva_respeita_near_e_far() {
        let mut s = spec(false);
        s.near = 0.5; s.far = 50.0;
        let cam = view_proj_spec(&s);
        let perto = apply(&cam.view_proj, [0.0, 0.0, 0.5]);
        let longe = apply(&cam.view_proj, [0.0, 0.0, 50.0]);
        assert!((perto[2] / perto[3]).abs() < 1e-5, "near -> z = 0");
        assert!((longe[2] / longe[3] - 1.0).abs() < 1e-5, "far -> z = 1");
        assert_eq!(cam.ortho, 0.0);
    }

    #[test]
    fn ortografica_mapeia_o_tamanho() {
        let cam = view_proj_spec(&spec(true));
        let c = apply(&cam.view_proj, [10.0, 5.0, 20.0]);
        assert!((c[3] - 1.0).abs() < 1e-6, "ortografica: w = 1");
        assert!((c[0] - 1.0).abs() < 1e-5 && (c[1] - 1.0).abs() < 1e-5, "meia altura 5, meia largura 10 = borda");
        assert!((c[2] - (20.0 - 0.1) / (100.0 - 0.1)).abs() < 1e-5, "profundidade linear");
        assert_eq!([cam.ortho, cam.half_h, cam.half_w], [1.0, 5.0, 10.0]);
    }

    #[test]
    fn view_proj_antiga_e_igual_a_nova() {
        let a = view_proj(1.0, 2.0, 3.0, 0.3, -0.2, 1.0, 1.5);
        let b = view_proj_spec(&CamSpec { pos: [1.0, 2.0, 3.0], yaw: 0.3, pitch: -0.2, fov_y: 1.0, aspect: 1.5,
            near: 0.1, far: 500.0, ortho: false, ortho_size: 5.0 });
        assert_eq!(a.view_proj, b.view_proj);
        assert_eq!(a.fwd, b.fwd);
    }

    #[test]
    fn viewport_em_pixels() {
        assert_eq!(viewport_px([0.5, 0.0, 0.5, 1.0], 1280, 720), Some([640, 0, 640, 720]));
        assert_eq!(viewport_px([0.0, 0.0, 0.0, 1.0], 1280, 720), None, "largura 0 = sem vista");
        // 1.0 - 0.9 em f32 nao bate byte-a-byte com o literal 0.1 (cancelamento);
        // o invariante e x+w==1.0 exatamente, nao o literal decimal.
        assert_eq!(clamp_rect([0.9, 0.0, 0.5, 1.0]), [0.9, 0.0, 1.0 - 0.9f32, 1.0], "x + w preso em 1");
        assert_eq!(clamp_rect([f32::NAN, -1.0, 2.0, 0.5]), [0.0, 0.0, 1.0, 0.5]);
        assert_eq!(viewport_px(clamp_rect([0.9, 0.0, 0.5, 1.0]), 1280, 720), Some([1152, 0, 128, 720]));
    }

    #[test]
    fn fila_de_vistas() {
        let a = view_proj(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0);
        let b = view_proj(2.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0);
        let mut q = ViewQueue::new(a);
        assert_eq!(q.len(), 1, "sem setViewport: uma vista cheia");
        assert_eq!(q.get(0).rect, FULL);
        q.set_viewport([0.0, 0.0, 0.5, 1.0], true); q.set_camera(a);
        q.set_viewport([0.5, 0.0, 0.5, 1.0], false); q.set_camera(b);
        assert_eq!(q.len(), 2);
        assert_eq!(q.get(0).rect, [0.0, 0.0, 0.5, 1.0]);
        assert_eq!(q.get(0).cam.cam_pos, [1.0, 0.0, 0.0]);
        assert!(!q.get(1).limpar, "limpar = 0 e fundo 'nada'");
        assert_eq!(q.get(1).cam.cam_pos, [2.0, 0.0, 0.0]);
        q.end_frame();
        assert_eq!(q.len(), 1);
        assert_eq!(q.get(0).rect, FULL, "o frame seguinte volta a vista cheia");
        assert!(q.get(0).limpar);
        assert_eq!(q.get(0).cam.cam_pos, [1.0, 0.0, 0.0], "com varias vistas, o frame seguinte comeca com a camera da 1a");
        for _ in 0..(MAX_VIEWS + 3) { q.set_viewport([0.0, 0.0, 1.0, 1.0], true); }
        assert_eq!(q.len(), MAX_VIEWS, "teto de vistas");
    }

    #[test]
    fn fundo_compativel_com_clear_color_e_skybox() {
        let mut q = ViewQueue::new(view_proj(0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0));
        assert_eq!(q.get(0).fundo, Fundo::Ceu, "padrao = ceu (o de hoje)");
        q.set_fundo(Fundo::Cor([0.1, 0.2, 0.3, 1.0]));
        q.skybox(false);
        assert_eq!(q.get(0).fundo, Fundo::Cor([0.1, 0.2, 0.3, 1.0]), "setSkybox(0) nao desfaz a cor, como hoje");
        q.set_viewport([0.0, 0.0, 0.5, 1.0], true);
        q.set_viewport([0.5, 0.0, 0.5, 1.0], true);
        assert_eq!(q.get(1).fundo, Fundo::Cor([0.1, 0.2, 0.3, 1.0]), "o fundo passa para a vista seguinte");
        q.skybox(true);
        assert_eq!(q.get(1).fundo, Fundo::Ceu);
    }

    #[test]
    fn cam_floats_layout() {
        let mut q = ViewQueue::new(view_proj_spec(&spec(true)));
        q.set_fundo(Fundo::Cor([0.1, 0.2, 0.3, 1.0]));
        let lvp = [2.0f32; 16];
        let f = cam_floats(q.get(0), [7.0, 8.0, 9.0, 0.25], &lvp, 0.4);
        assert_eq!([f[16], f[17], f[18], f[19]], [7.0, 8.0, 9.0, 0.25], "luz legada");
        assert_eq!(&f[36..52], &lvp[..], "light_vp");
        assert_eq!(f[52], 0.4, "agua");
        assert_eq!([f[56], f[57], f[58], f[59]], [0.1, 0.2, 0.3, 1.0], "fundo chapado");
        assert_eq!([f[60], f[61], f[62]], [1.0, 5.0, 10.0], "ortografica e meias extensoes");
        q.skybox(true);
        assert_eq!(cam_floats(q.get(0), [0.0; 4], &lvp, 0.0)[59], 0.0, "ceu: sem fundo chapado");
    }

    use super::shader::SHADER;
    use super::views::CAM_STRIDE;
    use super::lights::ENV_BYTES;

    fn struct_layout(m: &naga::Module, nome: &str) -> (u32, Vec<(String, u32)>) {
        for (_, t) in m.types.iter() {
            if t.name.as_deref() == Some(nome) {
                if let naga::TypeInner::Struct { members, span } = &t.inner {
                    return (*span, members.iter().map(|mm| (mm.name.clone().unwrap_or_default(), mm.offset)).collect());
                }
            }
        }
        panic!("struct {nome} ausente do shader");
    }
    fn offset(campos: &[(String, u32)], nome: &str) -> u32 {
        campos.iter().find(|c| c.0 == nome).map(|c| c.1).unwrap_or_else(|| panic!("campo {nome} ausente"))
    }

    /// O shader compila e valida no naga (o mesmo validador do wgpu): um erro de
    /// WGSL aparece aqui, sem abrir janela.
    #[test]
    fn shader_valida_no_naga() {
        let m = naga::front::wgsl::parse_str(SHADER).unwrap_or_else(|e| panic!("{}", e.emit_to_string(SHADER)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
            .validate(&m)
            .unwrap_or_else(|e| panic!("validação: {e:?}"));
    }

    /// O layout do WGSL bate com `cam_floats` e `env_floats`, float por float.
    #[test]
    fn layout_do_uniform_bate_com_o_empacotamento() {
        let m = naga::front::wgsl::parse_str(SHADER).unwrap();
        let (cam, c) = struct_layout(&m, "Cam");
        assert_eq!(cam as u64, CAM_STRIDE, "um slot de câmera = 256 bytes");
        assert_eq!(offset(&c, "light"), 16 * 4);
        assert_eq!(offset(&c, "cam_pos"), 20 * 4);
        assert_eq!(offset(&c, "cam_right"), 24 * 4);
        assert_eq!(offset(&c, "cam_fwd"), 32 * 4);
        assert_eq!(offset(&c, "light_vp"), 36 * 4);
        assert_eq!(offset(&c, "water"), 52 * 4);
        assert_eq!(offset(&c, "view_bg"), 56 * 4);
        assert_eq!(offset(&c, "proj"), 60 * 4);
        let (env, e) = struct_layout(&m, "Env");
        assert_eq!(env as u64, ENV_BYTES);
        assert_eq!(offset(&e, "sky0"), 8 * 4);
        assert_eq!(offset(&e, "sun_dir"), 24 * 4);
        assert_eq!(offset(&e, "fog"), 28 * 4);
        assert_eq!(offset(&e, "lights"), 32 * 4);
        let (luz, _) = struct_layout(&m, "LuzGpu");
        assert_eq!(luz, 64, "16 floats por luz");
    }

    /// Várias vistas: o frame seguinte começa com fundo e câmera da PRIMEIRA vista,
    /// não da última (senão a tela dividida perde o céu da vista 1 no frame 2).
    /// Uma vista só: tudo persiste como antes.
    #[test]
    fn fim_do_frame_restaura_a_primeira_vista() {
        let a = view_proj(1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0);
        let b = view_proj(2.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0);
        let mut q = ViewQueue::new(a);
        // frame 1: vista A (céu, câmera A) e vista B (cor chapada, câmera B)
        q.set_viewport(FULL, true);
        q.set_viewport([0.7, 0.0, 0.3, 0.3], true);
        q.set_fundo(Fundo::Cor([0.1, 0.1, 0.1, 1.0]));
        q.set_camera(b);
        q.end_frame();
        assert_eq!(q.len(), 1);
        assert_eq!(q.get(0).fundo, Fundo::Ceu, "fundo da vista A");
        assert_eq!(q.get(0).cam.cam_pos, [1.0, 0.0, 0.0], "camera da vista A");
        assert_eq!(q.get(0).rect, FULL);
        // vista única: fundo e câmera persistem como hoje
        q.set_fundo(Fundo::Cor([0.3, 0.3, 0.3, 1.0]));
        q.set_camera(b);
        q.end_frame();
        assert_eq!(q.get(0).fundo, Fundo::Cor([0.3, 0.3, 0.3, 1.0]));
        assert_eq!(q.get(0).cam.cam_pos, [2.0, 0.0, 0.0]);
        // setViewport chamado uma vez só também é vista única
        q.set_viewport([0.0, 0.0, 0.5, 0.5], true);
        q.set_camera(a);
        q.end_frame();
        assert_eq!(q.get(0).cam.cam_pos, [1.0, 0.0, 0.0]);
        assert_eq!(q.get(0).rect, FULL);
    }
