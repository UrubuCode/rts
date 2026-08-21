//! Pipeline 3D wgpu (scene pass) — desenha meshes com câmera/luz/depth ANTES do
//! egui no mesmo frame (`custom3d_wgpu`, sancionado em egui-ui-crate-design §1b).
//!
//! Fica no braço `Backend::Wgpu` (só-wgpu; glow não tem 3D — degrada, não panica).
//! A trait neutra `rts-render::Renderer` NÃO ganha nada disto (é backend-neutra);
//! estas capacidades vivem no namespace `egui`, tied ao handle da janela wgpu.
//!
//! Fluxo: TS chama `egui.mesh_upload` (1×, sobe vbuf/ibuf pra VRAM), depois por
//! frame `egui.set_camera` + `egui.set_light` + `egui.draw_mesh` (enfileira). No
//! `present_wgpu`, `render()` limpa color+depth, desenha a fila e a esvazia; o
//! egui pinta por cima com `LoadOp::Load`.

use std::collections::HashMap;

// cast de fatia → bytes (evita depender de bytemuck/wgpu-util).
fn f32_bytes(s: &[f32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
}
fn u32_bytes(s: &[u32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(s.as_ptr() as *const u8, std::mem::size_of_val(s)) }
}
// cria um buffer já preenchido (mapped_at_creation) — sem wgpu::util.
fn init_buffer(device: &wgpu::Device, label: &str, data: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: data.len() as u64,
        usage,
        mapped_at_creation: true,
    });
    { let mut mv = buf.slice(..).get_mapped_range_mut(); mv.copy_from_slice(data); }
    buf.unmap();
    buf
}

mod math;
mod pipeline;
mod render;
mod shader;
#[cfg(test)]
mod tests;

use math::{identity, light_view_proj};
pub use math::{Cam3D, model_matrix, view_proj, view_proj_lookat};

const SHADOW_SIZE: u32 = 2048;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

struct GpuMesh {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    icount: u32,
}

pub struct Scene3D {
    pipeline: wgpu::RenderPipeline,
    sky_pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    cam_buf: wgpu::Buffer,
    cam_bg: wgpu::BindGroup,
    shadow_view: wgpu::TextureView,
    shadow_bg: wgpu::BindGroup,
    // textura de albedo (group 2): layout + sampler + a 1×1 branca default (objeto
    // sem textura) + o mapa id→bind group das texturas de imagem subidas.
    tex_bgl: wgpu::BindGroupLayout,
    tex_sampler: wgpu::Sampler,
    default_tex_bg: wgpu::BindGroup,
    textures: HashMap<u64, wgpu::BindGroup>,
    next_tex: u64,
    depth_view: wgpu::TextureView,
    depth_w: u32,
    depth_h: u32,
    meshes: HashMap<u64, GpuMesh>,
    next_mesh: u64,
    // estado por-frame
    view_proj: [f32; 16],
    light: [f32; 4],
    light_vp: [f32; 16],   // view·proj da luz (shadow map); identidade = sem sombra
    cam_pos: [f32; 3],
    cright: [f32; 3],
    cup: [f32; 3],
    cfwd: [f32; 3],
    tan_h: f32,
    tan_v: f32,
    // (mesh, model, color, emissive, tex_flag, tex_id) — tex_flag vai pro shader
    // (0/1/2), tex_id seleciona o bind group da textura no render loop.
    draws: Vec<(u64, [f32; 16], [f32; 4], f32, f32, u64)>,
    water_pipeline: wgpu::RenderPipeline,
    /// (mesh, buffer de instâncias [vec4/partícula], count, escala) — drenada
    /// junto de `draws`. O buffer vem CLONADO do rts:gpu (mesmo device).
    water_draws: Vec<(u64, wgpu::Buffer, u32, f32)>,
    inst_buf: wgpu::Buffer,
    inst_cap: u64,
    /// Fundo do scene pass: `None` = skybox procedural (default); `Some(rgba)` =
    /// cor CHAPADA (o viewport do editor quer um fundo neutro, não o starfield).
    bg: Option<[f32; 4]>,
}

impl Scene3D {

    pub fn upload_mesh(&mut self, device: &wgpu::Device, verts: &[f32], indices: &[u32]) -> u64 {
        let vbuf = init_buffer(device, "scene3d vbuf", f32_bytes(verts), wgpu::BufferUsages::VERTEX);
        let ibuf = init_buffer(device, "scene3d ibuf", u32_bytes(indices), wgpu::BufferUsages::INDEX);
        let id = self.next_mesh;
        self.next_mesh += 1;
        self.meshes.insert(id, GpuMesh { vbuf, ibuf, icount: indices.len() as u32 });
        id
    }

    pub fn free_mesh(&mut self, id: u64) {
        self.meshes.remove(&id);
    }

    pub fn set_camera(&mut self, cd: Cam3D) {
        self.view_proj = cd.view_proj;
        self.cam_pos = cd.cam_pos;
        self.cright = cd.right;
        self.cup = cd.up;
        self.cfwd = cd.fwd;
        self.tan_h = cd.tan_h;
        self.tan_v = cd.tan_v;
    }
    pub fn set_light(&mut self, d: [f32; 3], ambient: f32) {
        // ponto de luz: guarda a POSICAO (nao normaliza)
        self.light = [d[0], d[1], d[2], ambient];
    }
    /// Configura o shadow map: direção da luz (para onde a luz VIAJA) + centro/raio
    /// da caixa ortográfica que o shadow map cobre. radius<=0 desliga a sombra.
    pub fn set_shadow(&mut self, dir: [f32; 3], center: [f32; 3], radius: f32) {
        if radius <= 0.0 {
            self.light_vp = identity();
        } else {
            self.light_vp = light_view_proj(dir, center, radius);
        }
    }
    /// `tex`: 0=nenhuma, 1=xadrez procedural, >=2 = id de textura real (imagem).
    pub fn queue_draw(&mut self, mesh: u64, model: [f32; 16], color: [f32; 4], emissive: f32, tex: u64) {
        self.draws.push((mesh, model, color, emissive, tex_flag(tex), tex));
    }

    /// ÁGUA INSTANCIADA: desenha `count` instâncias da malha `mesh`, lendo cada
    /// instância (vec4: xyz centro, w densidade assinada) de `buf` — o storage
    /// buffer da física (rts:gpu), sem readback. 1 draw call por chamada.
    pub fn queue_water(&mut self, mesh: u64, buf: wgpu::Buffer, count: u32, scale: f32) {
        self.water_draws.push((mesh, buf, count, scale));
    }

    /// Sobe uma imagem RGBA8 (`w×h`, `rgba` = w*h*4 bytes) pra VRAM e devolve um id
    /// de textura (>=2) usável em `drawMesh(..., tex=id)`. 0 se dimensões inválidas.
    pub fn upload_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        rgba: &[u8],
        w: u32,
        h: u32,
    ) -> u64 {
        if w == 0 || h == 0 || rgba.len() < (w as usize) * (h as usize) * 4 {
            return 0;
        }
        let bg = make_tex_bg(device, queue, &self.tex_bgl, &self.tex_sampler, rgba, w, h);
        let id = self.next_tex;
        self.next_tex += 1;
        self.textures.insert(id, bg);
        id
    }
    /// Fundo CHAPADO (desliga o skybox): o pass limpa o color pra `rgba` e não
    /// desenha o starfield. Ideal pro viewport do editor.
    pub fn set_clear_color(&mut self, rgba: [f32; 4]) {
        self.bg = Some(rgba);
    }
    /// Religa (on) ou mantém desligado o skybox procedural. `on` volta a `bg=None`.
    pub fn set_skybox(&mut self, on: bool) {
        if on {
            self.bg = None;
        }
    }

    fn ensure_depth(&mut self, device: &wgpu::Device, w: u32, h: u32) {
        if w != self.depth_w || h != self.depth_h {
            let (v, _t) = make_depth(device, w.max(1), h.max(1));
            self.depth_view = v;
            self.depth_w = w;
            self.depth_h = h;
        }
    }
}

/// Mapeia o `tex` da API (0=nenhuma, 1=xadrez procedural, >=2 = id de textura real)
/// pro FLAG que o shader lê no instance param: 0.0 / 1.0 / 2.0. Qualquer id real
/// (>=2) vira 2.0 — a seleção da textura em si é por bind group no render loop.
fn tex_flag(tex: u64) -> f32 {
    if tex >= 2 {
        2.0
    } else {
        tex as f32
    }
}

/// Cria uma textura RGBA8 `w×h`, preenche com `rgba` (w*h*4 bytes, sRGB) via
/// `queue.write_texture` e devolve um bind group (group 2: textura + sampler)
/// pronto pro pipeline de mesh. Usada tanto pra 1×1 branca default quanto pras
/// texturas de imagem subidas.
fn make_tex_bg(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bgl: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    rgba: &[u8],
    w: u32,
    h: u32,
) -> wgpu::BindGroup {
    let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scene3d albedo tex"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * w),
            rows_per_image: Some(h),
        },
        size,
    );
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("scene3d albedo bg"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

fn make_depth(device: &wgpu::Device, w: u32, h: u32) -> (wgpu::TextureView, wgpu::Texture) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scene3d depth"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (view, tex)
}

fn make_shadow(device: &wgpu::Device, size: u32) -> (wgpu::TextureView, wgpu::Texture) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("scene3d shadow map"),
        size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    (view, tex)
}

