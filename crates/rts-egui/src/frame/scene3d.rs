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

/// WGSL: vertex = pos+normal (slot 0) × instância model(4×vec4)+color (slot 1).
/// Uniform (group 0): viewProj + luz (xyz=dir, w=ambiente). Shading difuso.
const SHADER: &str = r#"
struct Cam { view_proj: mat4x4<f32>, light: vec4<f32> };
@group(0) @binding(0) var<uniform> cam: Cam;

struct VOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) normal: vec3<f32>,
  @location(1) color: vec4<f32>,
};

@vertex
fn vs(
  @location(0) position: vec3<f32>,
  @location(1) normal: vec3<f32>,
  @location(2) m0: vec4<f32>,
  @location(3) m1: vec4<f32>,
  @location(4) m2: vec4<f32>,
  @location(5) m3: vec4<f32>,
  @location(6) color: vec4<f32>,
) -> VOut {
  let model = mat4x4<f32>(m0, m1, m2, m3);
  let world = model * vec4<f32>(position, 1.0);
  var o: VOut;
  o.clip = cam.view_proj * world;
  o.normal = normalize((model * vec4<f32>(normal, 0.0)).xyz);
  o.color = color;
  return o;
}

@fragment
fn fs(i: VOut) -> @location(0) vec4<f32> {
  let l = normalize(cam.light.xyz);
  let nd = max(dot(normalize(i.normal), l), 0.0);
  let lit = cam.light.w + (1.0 - cam.light.w) * nd;
  return vec4<f32>(i.color.rgb * lit, i.color.a);
}
"#;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

struct GpuMesh {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    icount: u32,
}

pub struct Scene3D {
    pipeline: wgpu::RenderPipeline,
    cam_buf: wgpu::Buffer,
    cam_bg: wgpu::BindGroup,
    depth_view: wgpu::TextureView,
    depth_w: u32,
    depth_h: u32,
    meshes: HashMap<u64, GpuMesh>,
    next_mesh: u64,
    // estado por-frame
    view_proj: [f32; 16],
    light: [f32; 4],
    draws: Vec<(u64, [f32; 16], [f32; 4])>,
    inst_buf: wgpu::Buffer,
    inst_cap: u64,
}

impl Scene3D {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Scene3D {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene3d shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        // uniform: 16 (view_proj) + 4 (light) = 20 f32 = 80 bytes
        let cam_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene3d cam"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cam_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene3d cam bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let cam_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene3d cam bg"),
            layout: &cam_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: cam_buf.as_entire_binding(),
            }],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene3d layout"),
            bind_group_layouts: &[Some(&cam_bgl)],
            immediate_size: 0,
        });

        // vertex (slot 0): pos vec3 @0, normal vec3 @12 — stride 24
        let vbl = wgpu::VertexBufferLayout {
            array_stride: 24,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 12, shader_location: 1 },
            ],
        };
        // instância (slot 1): model 4×vec4 @2..5, color vec4 @6 — stride 80
        let ibl = wgpu::VertexBufferLayout {
            array_stride: 80,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 0, shader_location: 2 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16, shader_location: 3 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 32, shader_location: 4 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 48, shader_location: 5 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 64, shader_location: 6 },
            ],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene3d pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[vbl, ibl],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multiview_mask: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
        });

        let (depth_view, _t) = make_depth(device, 1, 1);
        let inst_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene3d inst"),
            size: 80 * 64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Scene3D {
            pipeline,
            cam_buf,
            cam_bg,
            depth_view,
            depth_w: 1,
            depth_h: 1,
            meshes: HashMap::new(),
            next_mesh: 1,
            view_proj: identity(),
            light: [0.4, 0.8, 0.4, 0.25],
            draws: Vec::new(),
            inst_buf,
            inst_cap: 64,
        }
    }

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

    pub fn set_camera(&mut self, vp: [f32; 16]) {
        self.view_proj = vp;
    }
    pub fn set_light(&mut self, d: [f32; 3], ambient: f32) {
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-6);
        self.light = [d[0] / len, d[1] / len, d[2] / len, ambient];
    }
    pub fn queue_draw(&mut self, mesh: u64, model: [f32; 16], color: [f32; 4]) {
        self.draws.push((mesh, model, color));
    }

    fn ensure_depth(&mut self, device: &wgpu::Device, w: u32, h: u32) {
        if w != self.depth_w || h != self.depth_h {
            let (v, _t) = make_depth(device, w.max(1), h.max(1));
            self.depth_view = v;
            self.depth_w = w;
            self.depth_h = h;
        }
    }

    /// Roda o scene pass no `encoder` compartilhado: limpa color(bg)+depth, desenha
    /// a fila e a esvazia. Retorna `true` se limpou o color (o egui deve usar Load).
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        w: u32,
        h: u32,
    ) -> bool {
        self.ensure_depth(device, w, h);

        // uniform: view_proj (16) + light (4)
        let mut cam = [0f32; 20];
        cam[..16].copy_from_slice(&self.view_proj);
        cam[16..].copy_from_slice(&self.light);
        queue.write_buffer(&self.cam_buf, 0, f32_bytes(&cam));

        // instâncias
        let n = self.draws.len() as u64;
        if n > self.inst_cap {
            let cap = n.next_power_of_two().max(64);
            self.inst_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("scene3d inst"),
                size: 80 * cap,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.inst_cap = cap;
        }
        let mut inst: Vec<f32> = Vec::with_capacity(self.draws.len() * 20);
        for (_m, model, color) in &self.draws {
            inst.extend_from_slice(model);
            inst.extend_from_slice(color);
        }
        if !inst.is_empty() {
            queue.write_buffer(&self.inst_buf, 0, f32_bytes(&inst));
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene3d pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.02, g: 0.02, b: 0.03, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.cam_bg, &[]);
        for (i, (mesh_id, _model, _color)) in self.draws.iter().enumerate() {
            if let Some(m) = self.meshes.get(mesh_id) {
                let off = (i as u64) * 80;
                pass.set_vertex_buffer(0, m.vbuf.slice(..));
                pass.set_vertex_buffer(1, self.inst_buf.slice(off..off + 80));
                pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..m.icount, 0, 0..1);
            }
        }
        drop(pass);

        self.draws.clear();
        true
    }
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

fn identity() -> [f32; 16] {
    [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]
}

/// view·proj (column-major) a partir de câmera fly (yaw/pitch) — MESMA convenção
/// do rasterizador software: right=(cyw,0,-syw), up=(-syw*spt,cpt,-cyw*spt),
/// forward=(syw*cpt,spt,cyw*cpt); proj perspectiva left-handed (z forward, depth 0..1).
pub fn view_proj(
    camx: f32, camy: f32, camz: f32, yaw: f32, pitch: f32, fov_y: f32, aspect: f32,
) -> [f32; 16] {
    let (cyw, syw) = (yaw.cos(), yaw.sin());
    let (cpt, spt) = (pitch.cos(), pitch.sin());
    let right = [cyw, 0.0, -syw];
    let up = [-syw * spt, cpt, -cyw * spt];
    let fwd = [syw * cpt, spt, cyw * cpt];
    // view = R · translate(-cam): linhas = right/up/fwd, translação = -R·cam
    let tx = -(right[0] * camx + right[1] * camy + right[2] * camz);
    let ty = -(up[0] * camx + up[1] * camy + up[2] * camz);
    let tz = -(fwd[0] * camx + fwd[1] * camy + fwd[2] * camz);
    // view em column-major (coluna c: [right[c],up[c],fwd[c],0], última col = t)
    let v = [
        right[0], up[0], fwd[0], 0.0,
        right[1], up[1], fwd[1], 0.0,
        right[2], up[2], fwd[2], 0.0,
        tx, ty, tz, 1.0,
    ];
    let near = 0.1f32;
    let far = 500.0f32;
    let f = 1.0 / (fov_y * 0.5).tan();
    // proj perspectiva (column-major), w = z_view (forward), depth 0..1
    let p = [
        f / aspect, 0.0, 0.0, 0.0,
        0.0, f, 0.0, 0.0,
        0.0, 0.0, far / (far - near), 1.0,
        0.0, 0.0, -(far * near) / (far - near), 0.0,
    ];
    mul(&p, &v)
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
