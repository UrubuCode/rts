//! Superfície `extern "C"` do pipeline 3D wgpu, exposta no namespace `egui`
//! (tied ao handle da janela). NÃO mexe na trait neutra `rts-render::Renderer`.
//!
//! A TS passa PONTEIROS crus (via `buffer.ptr`) pros dados de vértice/índice, e
//! os campos de câmera/transform como f64 — a matriz view·proj e a model são
//! construídas em Rust (`scene3d::view_proj`/`model_matrix`), casando a convenção
//! do rasterizador software. Só o braço `Backend::Wgpu` age; no glow é no-op.

use crate::ctx::with_ctx;
use crate::frame::scene3d::{model_matrix, view_proj, view_proj_lookat, Scene3D};
use crate::frame::Backend;

/// Garante o pipeline 3D criado na 1ª chamada e roda `f(scene, device)`.
fn with_scene<R: Copy>(win: u64, f: impl FnOnce(&mut Scene3D, &wgpu::Device) -> R, default: R) -> R {
    with_ctx(win, |c| {
        if let Backend::Wgpu(r) = &mut c.backend {
            if r.scene.is_none() {
                r.scene = Some(Scene3D::new(&r.device, &r.queue, r.config.format));
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

/// Sobe uma mesh (verts interleaved pos+normal+uv = 8 f32/vértice; índices u32)
/// pra VRAM. Retorna o id da mesh (0 se a janela não é wgpu).
/// # Segurança
///
/// `vptr`/`iptr` são endereços crus que o chamador promete apontarem para
/// `vcount*8` `f32` e `icount` `u32` vivos. É a forma que a ABI do motor antigo
/// tem; um chamador que possa passar fatias deve preferir [`upload_mesh`], que
/// não promete nada.
pub unsafe fn mesh_upload(win: u64, vptr: u64, vcount: i64, iptr: u64, icount: i64) -> u64 {
    if vptr == 0 || iptr == 0 || vcount <= 0 || icount <= 0 {
        return 0;
    }
    let verts = unsafe { std::slice::from_raw_parts(vptr as *const f32, (vcount * 8) as usize) };
    let inds = unsafe { std::slice::from_raw_parts(iptr as *const u32, icount as usize) };
    upload_mesh(win, verts, inds)
}

/// Sobe uma mesh a partir de fatias — a forma sem promessa nenhuma.
///
/// Existe porque o motor NOVO entrega os dados como uma view tipada, cujos bytes
/// são copiados na fronteira: não há endereço para o chamador acertar, e o
/// coletor pode mover a célula sem que isto veja. Ver `rts-ui::value::bytes`.
pub fn upload_mesh(win: u64, verts: &[f32], indices: &[u32]) -> u64 {
    if verts.is_empty() || indices.is_empty() {
        return 0;
    }
    with_scene(win, |s, dev| s.upload_mesh(dev, verts, indices), 0)
}

/// Libera uma mesh da VRAM.
pub fn mesh_free(win: u64, mesh: u64) {
    with_scene(win, |s, _d| s.free_mesh(mesh), ());
}

/// Define a câmera do frame (fly: yaw/pitch/fov/aspect). Constrói view·proj.
#[allow(clippy::too_many_arguments)]
pub fn set_camera(
    win: u64,
    camx: f64,
    camy: f64,
    camz: f64,
    yaw: f64,
    pitch: f64,
    fov_y: f64,
    aspect: f64,
) {
    let cd = view_proj(
        camx as f32, camy as f32, camz as f32, yaw as f32, pitch as f32, fov_y as f32, aspect as f32,
    );
    with_scene(win, |s, _d| s.set_camera(cd), ());
}

/// Câmera LOOK-AT (olho→alvo) com `near`/`far` explícitos — NaN-safe (não trava
/// no gimbal ao olhar reto pra cima/baixo). Mais conveniente que yaw/pitch pro
/// "frame selected" do editor. Enxertado do gpu3d (origin/main), adaptado à base LH.
#[allow(clippy::too_many_arguments)]
pub fn set_camera_lookat(
    win: u64,
    ex: f64,
    ey: f64,
    ez: f64,
    tx: f64,
    ty: f64,
    tz: f64,
    fov_y: f64,
    aspect: f64,
    near: f64,
    far: f64,
) {
    let cd = view_proj_lookat(
        [ex as f32, ey as f32, ez as f32],
        [tx as f32, ty as f32, tz as f32],
        fov_y as f32,
        aspect as f32,
        near as f32,
        far as f32,
    );
    with_scene(win, |s, _d| s.set_camera(cd), ());
}

/// Fundo CHAPADO do scene pass (r,g,b em 0..1) — desliga o skybox procedural.
/// Ideal pro viewport do editor, que quer um fundo neutro em vez do starfield.
pub fn set_clear_color(win: u64, r: f64, g: f64, b: f64) {
    with_scene(win, |s, _d| s.set_clear_color([r as f32, g as f32, b as f32, 1.0]), ());
}

/// Religa o skybox procedural (`on!=0`) desfazendo um `setClearColor`.
pub fn set_skybox(win: u64, on: i64) {
    with_scene(win, |s, _d| s.set_skybox(on != 0), ());
}

/// Define a luz direcional (dir + ambiente).
pub fn set_light(win: u64, dx: f64, dy: f64, dz: f64, ambient: f64) {
    with_scene(
        win,
        |s, _d| s.set_light([dx as f32, dy as f32, dz as f32], ambient as f32),
        (),
    );
}

/// Largura LÓGICA (points) atual da janela — segue o resize. 0 se não existe.
pub fn win_width(win: u64) -> f64 {
    crate::ctx::with_ctx(win, |c| {
        let s = c.window.inner_size();
        let sf = c.window.scale_factor();
        ((s.width as f64) / sf).round()
    })
    .unwrap_or(0.0)
}

/// Altura LÓGICA (points) atual da janela — segue o resize. 0 se não existe.
pub fn win_height(win: u64) -> f64 {
    crate::ctx::with_ctx(win, |c| {
        let s = c.window.inner_size();
        let sf = c.window.scale_factor();
        ((s.height as f64) / sf).round()
    })
    .unwrap_or(0.0)
}

/// Configura o shadow map: direção da luz (dx,dy,dz = para onde a luz viaja) +
/// centro (cx,cy,cz) e raio da caixa que a sombra cobre. radius<=0 desliga.
#[allow(clippy::too_many_arguments)]
pub fn set_shadow(
    win: u64,
    dx: f64,
    dy: f64,
    dz: f64,
    cx: f64,
    cy: f64,
    cz: f64,
    radius: f64,
) {
    with_scene(
        win,
        |s, _d| s.set_shadow([dx as f32, dy as f32, dz as f32], [cx as f32, cy as f32, cz as f32], radius as f32),
        (),
    );
}

/// A conversão de UM registro de desenho no que o scene pass enfileira.
///
/// Existe extraída porque [`draw_mesh`] e [`draw_mesh_batch`] precisam dela e
/// duas grafias da mesma decisão é como a cor de um lote passaria a divergir da
/// cor de um desenho solto sem que nada acusasse. A regra do alpha em especial
/// é uma decisão, não uma fórmula: cor sem byte de alpha é OPACA.
#[allow(clippy::too_many_arguments)]
fn draw_record(
    px: f32,
    py: f32,
    pz: f32,
    rx: f32,
    ry: f32,
    sx: f32,
    sy: f32,
    sz: f32,
    color: u32,
    emissive: bool,
) -> ([f32; 16], [f32; 4], f32) {
    let m = model_matrix(px, py, pz, rx, ry, sx, sy, sz);
    let a = (color >> 24) & 0xFF;
    let col = [
        ((color >> 16) & 0xFF) as f32 / 255.0,
        ((color >> 8) & 0xFF) as f32 / 255.0,
        (color & 0xFF) as f32 / 255.0,
        if a == 0 { 1.0 } else { a as f32 / 255.0 }, // sem byte de alpha = opaco
    ];
    (m, col, if emissive { 1.0 } else { 0.0 })
}

/// Enfileira 1 draw da mesh `mesh` com transform (pos/rot/escala) + cor
/// (0xAARRGGBB) + emissivo (0/1) + textura procedural (0=nenhuma, 1=xadrez).
/// O draw acontece no scene pass, no `endFrame`.
#[allow(clippy::too_many_arguments)]
pub fn draw_mesh(
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
    emissive: i64,
    tex: i64,
) {
    let (m, col, em) = draw_record(
        px as f32, py as f32, pz as f32, rx as f32, ry as f32, sx as f32, sy as f32, sz as f32,
        color as u32,
        emissive != 0,
    );
    // tex: 0=nenhuma, 1=xadrez procedural, >=2 = id de textura real (textureUpload).
    with_scene(win, |s, _d| s.queue_draw(mesh, m, col, em, tex.max(0) as u64), ());
}

/// Enfileira N draws de uma vez. Responde quantos entraram.
///
/// # Por que isto existe, já que o pass JÁ instancia
///
/// `Scene3D::flush` ordena os draws por `(mesh, tex)` e monta um instance buffer
/// — um draw call por grupo, e isso vale desde antes desta função. O que ela
/// remove não é draw call nenhum: é a TRAVESSIA. Uma cena de 500 objetos fazia
/// 500 idas TS→nativo por frame, cada uma materializando um objeto de 12 campos
/// que o outro lado relia campo a campo, para no fim empurrar 500 tuplas na
/// mesma `Vec`. Aqui é uma ida e um laço em Rust.
///
/// # A forma dos dados, e por que são DUAS views
///
/// `floats` traz 8 f32 por instância (x, y, z, rx, ry, sx, sy, sz) e `codes` 4
/// u32 (mesh, color, emissive, tex). Separar custa um argumento e evita um bug
/// silencioso: uma cor `0xAARRGGBB` com alpha passa de 2^24 e NÃO é exata em
/// f32 — `0xFF203040` voltaria com o canal errado por arredondamento. Números
/// que são identidade ou padrão de bits não viajam em ponto flutuante.
///
/// Views tipadas e não ponteiros, pela razão que `crate::upload_mesh` e o doc de
/// `rts-ui` já dão: o coletor move células.
pub fn draw_mesh_batch(win: u64, floats: &[f32], codes: &[u32]) -> i64 {
    let count = (floats.len() / 8).min(codes.len() / 4);
    if count == 0 {
        return 0;
    }
    with_scene(
        win,
        |s, _d| {
            for i in 0..count {
                let f = &floats[i * 8..i * 8 + 8];
                let c = &codes[i * 4..i * 4 + 4];
                let (m, col, em) =
                    draw_record(f[0], f[1], f[2], f[3], f[4], f[5], f[6], f[7], c[1], c[2] != 0);
                s.queue_draw(c[0] as u64, m, col, em, c[3] as u64);
            }
            count as i64
        },
        0,
    )
}

/// ÁGUA INSTANCIADA: desenha `count` instâncias da malha `mesh` lendo cada
/// instância (vec4 f32: xyz centro, w densidade assinada) DIRETO do buffer
/// `gbuf` do rts:gpu — zero readback, zero FFI por partícula, 1 draw call.
/// `scale` = raio de desenho. 1 ok, 0 = buffer/janela inválidos.
///
pub fn draw_water(win: u64, mesh: u64, gbuf: u64, count: i64, scale: f64) -> i64 {
    if count <= 0 {
        return 0;
    }
    let Some(buf) = crate::compute::buffer_handle(gbuf) else {
        return 0;
    };
    with_scene(
        win,
        |s, _| {
            s.queue_water(mesh, buf.clone(), count as u32, scale as f32);
            1
        },
        0,
    )
}

/// Sobe uma imagem RGBA8 (`ptr` → w*h*4 bytes, sRGB) pra VRAM e devolve um id de
/// textura (>=2) usável em `drawMesh(..., tex=id)`. Fluxo típico: `fs` lê o arquivo,
/// `imgdec.decode` devolve RGBA + w/h, e isto sobe pra GPU. 0 se inválido/não-wgpu.
/// # Segurança
///
/// `ptr` aponta para `w*h*4` bytes vivos — a promessa que a ABI antiga exige.
/// [`upload_texture`] é a forma que não exige nenhuma.
pub unsafe fn texture_upload(win: u64, ptr: u64, w: i64, h: i64) -> u64 {
    if ptr == 0 || w <= 0 || h <= 0 {
        return 0;
    }
    let rgba = unsafe {
        std::slice::from_raw_parts(ptr as *const u8, (w as usize) * (h as usize) * 4)
    };
    upload_texture(win, rgba, w, h)
}

/// Sobe uma imagem RGBA8 a partir de uma fatia. Ver [`upload_mesh`].
pub fn upload_texture(win: u64, rgba: &[u8], w: i64, h: i64) -> u64 {
    if w <= 0 || h <= 0 {
        return 0;
    }
    let (w, h) = (w as u32, h as u32);
    if rgba.len() < (w as usize) * (h as usize) * 4 {
        return 0;
    }
    with_ctx(win, |c| {
        if let Backend::Wgpu(r) = &mut c.backend {
            if r.scene.is_none() {
                r.scene = Some(Scene3D::new(&r.device, &r.queue, r.config.format));
            }
            let (dev, q) = (&r.device, &r.queue);
            let s = r.scene.as_mut().unwrap();
            s.upload_texture(dev, q, rgba, w, h)
        } else {
            0
        }
    })
    .unwrap_or(0)
}
