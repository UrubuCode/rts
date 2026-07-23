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
struct Cam {
  view_proj: mat4x4<f32>,
  light: vec4<f32>,
  cam_pos: vec4<f32>,
  cam_right: vec4<f32>,  // xyz = right,   w = tanH
  cam_up: vec4<f32>,     // xyz = up,      w = tanV
  cam_fwd: vec4<f32>,    // xyz = forward
  light_vp: mat4x4<f32>, // view·proj da LUZ (shadow map)
};
@group(0) @binding(0) var<uniform> cam: Cam;
// shadow map (group 1): depth da cena vista da luz + comparison sampler
@group(1) @binding(0) var shadow_tex: texture_depth_2d;
@group(1) @binding(1) var shadow_samp: sampler_comparison;

// vertex do SHADOW PASS: projeta pela luz (só posição).
@vertex
fn shadow_vs(
  @location(0) position: vec3<f32>,
  @location(2) m0: vec4<f32>,
  @location(3) m1: vec4<f32>,
  @location(4) m2: vec4<f32>,
  @location(5) m3: vec4<f32>,
) -> @builtin(position) vec4<f32> {
  let model = mat4x4<f32>(m0, m1, m2, m3);
  return cam.light_vp * (model * vec4<f32>(position, 1.0));
}

// fator de sombra (1 = iluminado, 0 = na sombra) via PCF 3×3.
fn shadow_factor(world: vec3<f32>) -> f32 {
  let lc = cam.light_vp * vec4<f32>(world, 1.0);
  let proj = lc.xyz / lc.w;
  let uv = proj.xy * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
  if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || proj.z > 1.0) { return 1.0; }
  let d = proj.z - 0.0015;   // bias contra acne
  let texel = 1.0 / 2048.0;
  var sum = 0.0;
  for (var oy = -1; oy <= 1; oy = oy + 1) {
    for (var ox = -1; ox <= 1; ox = ox + 1) {
      let o = vec2<f32>(f32(ox), f32(oy)) * texel;
      sum = sum + textureSampleCompare(shadow_tex, shadow_samp, uv + o, d);
    }
  }
  return sum / 9.0;
}

// ── SKYBOX: triângulo fullscreen; gradiente + estrelas por DIREÇÃO de mundo
//    (giram junto com a câmera). Depth write off (fica no fundo). ──────────────
struct SkyOut { @builtin(position) clip: vec4<f32>, @location(0) ndc: vec2<f32> };
@vertex
fn sky_vs(@builtin(vertex_index) vi: u32) -> SkyOut {
  var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
  var o: SkyOut;
  o.clip = vec4<f32>(p[vi], 1.0, 1.0);
  o.ndc = p[vi];
  return o;
}
fn hash13(p3: vec3<f32>) -> f32 {
  var q = fract(p3 * 0.1031);
  q = q + dot(q, q.yzx + 33.33);
  return fract((q.x + q.y) * q.z);
}
@fragment
fn sky_fs(i: SkyOut) -> @location(0) vec4<f32> {
  let ray = normalize(cam.cam_fwd.xyz
    + cam.cam_right.xyz * (i.ndc.x * cam.cam_right.w)
    + cam.cam_up.xyz * (i.ndc.y * cam.cam_up.w));
  let t = clamp(ray.y * 0.5 + 0.5, 0.0, 1.0);
  var col = mix(vec3<f32>(0.02, 0.02, 0.035), vec3<f32>(0.01, 0.015, 0.05), t);
  // estrelas: hash da direção quantizada (esparsas e brilhantes)
  let h = hash13(floor(ray * 260.0));
  if (h > 0.9915) { let s = (h - 0.9915) * 110.0; col = col + vec3<f32>(s, s, s); }
  return vec4<f32>(col, 1.0);
}

struct VOut {
  @builtin(position) clip: vec4<f32>,
  @location(0) normal: vec3<f32>,
  @location(1) color: vec4<f32>,
  @location(2) world: vec3<f32>,
  @location(3) emissive: f32,
  @location(4) tex: f32,
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
  @location(7) iparams: vec4<f32>,
) -> VOut {
  let model = mat4x4<f32>(m0, m1, m2, m3);
  let world = model * vec4<f32>(position, 1.0);
  var o: VOut;
  o.clip = cam.view_proj * world;
  o.normal = normalize((model * vec4<f32>(normal, 0.0)).xyz);
  o.color = color;
  o.world = world.xyz;
  o.emissive = iparams.x;
  o.tex = iparams.y;
  return o;
}

@fragment
fn fs(i: VOut) -> @location(0) vec4<f32> {
  var albedo = i.color.rgb;
  // TEXTURA procedural: xadrez em espaço de mundo (iparams.y = i.tex)
  if (i.tex > 0.5) {
    let s = 1.0;
    let c = floor(i.world.x * s) + floor(i.world.z * s) + floor(i.world.y * s);
    let chk = fract(c * 0.5) * 2.0;        // 0 ou 1
    albedo = albedo * mix(0.5, 1.0, chk);
  }
  // emissivo (ex.: o Sol) — cor cheia, sem sombreamento
  if (i.emissive > 0.5) { return vec4<f32>(albedo, i.color.a); }
  let n = normalize(i.normal);
  // LUZ PONTUAL: cam.light.xyz = POSICAO da luz (ex.: o Sol)
  let l = normalize(cam.light.xyz - i.world);
  let nd = max(dot(n, l), 0.0);
  let sh = shadow_factor(i.world);          // 1 = iluminado, 0 = na sombra
  let lit = cam.light.w + (1.0 - cam.light.w) * nd * sh;
  let vdir = normalize(cam.cam_pos.xyz - i.world);
  let h = normalize(l + vdir);
  let spec = pow(max(dot(n, h), 0.0), 32.0) * 0.3 * sh;
  let rgb = albedo * lit + vec3<f32>(spec, spec, spec);
  return vec4<f32>(rgb, i.color.a);
}
"#;

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
    draws: Vec<(u64, [f32; 16], [f32; 4], f32, f32)>,
    inst_buf: wgpu::Buffer,
    inst_cap: u64,
}

impl Scene3D {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Scene3D {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene3d shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        // uniform: view_proj(16) + light(4) + cam_pos(4) + right(4) + up(4) + fwd(4)
        //          + light_vp(16) = 52 f32 = 208 bytes
        let cam_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene3d cam"),
            size: 208,
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

        // shadow map: textura depth 2048² (render target + amostrada) + comparison sampler
        let (shadow_view, _st) = make_shadow(device, SHADOW_SIZE);
        let shadow_samp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("scene3d shadow samp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let shadow_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene3d shadow bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
            ],
        });
        let shadow_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene3d shadow bg"),
            layout: &shadow_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&shadow_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&shadow_samp) },
            ],
        });

        // layout do pass principal + sky: group 0 (câmera) + group 1 (shadow map)
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene3d layout"),
            bind_group_layouts: &[Some(&cam_bgl), Some(&shadow_bgl)],
            immediate_size: 0,
        });
        // layout do shadow pass: só group 0 (light_vp vem do uniform da câmera)
        let cam_only_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene3d shadow layout"),
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
            array_stride: 96,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 0, shader_location: 2 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16, shader_location: 3 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 32, shader_location: 4 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 48, shader_location: 5 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 64, shader_location: 6 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 80, shader_location: 7 },
            ],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene3d pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[vbl.clone(), ibl.clone()],
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

        // pipeline da SKYBOX: triângulo fullscreen, sem depth write (fica no fundo).
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene3d sky pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("sky_vs"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("sky_fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multiview_mask: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
        });

        // SHADOW PIPELINE: depth-only, projeta pela luz (só group 0). Bias contra acne.
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene3d shadow pipeline"),
            layout: Some(&cam_only_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("shadow_vs"),
                buffers: &[vbl, ibl],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState { constant: 2, slope_scale: 2.0, clamp: 0.0 },
            }),
            multiview_mask: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
        });

        let (depth_view, _t) = make_depth(device, 1, 1);
        let inst_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene3d inst"),
            size: 96 * 64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Scene3D {
            pipeline,
            sky_pipeline,
            shadow_pipeline,
            cam_buf,
            cam_bg,
            shadow_view,
            shadow_bg,
            depth_view,
            depth_w: 1,
            depth_h: 1,
            meshes: HashMap::new(),
            next_mesh: 1,
            view_proj: identity(),
            light: [0.4, 0.8, 0.4, 0.25],
            light_vp: identity(),
            cam_pos: [0.0, 0.0, 0.0],
            cright: [1.0, 0.0, 0.0],
            cup: [0.0, 1.0, 0.0],
            cfwd: [0.0, 0.0, 1.0],
            tan_h: 1.0,
            tan_v: 1.0,
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
    pub fn queue_draw(&mut self, mesh: u64, model: [f32; 16], color: [f32; 4], emissive: f32, tex: f32) {
        self.draws.push((mesh, model, color, emissive, tex));
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

        // uniform: view_proj(16) + light(4) + cam_pos(4) + right(4) + up(4) + fwd(4)
        //          + light_vp(16) = 52 f32
        let mut cam = [0f32; 52];
        cam[..16].copy_from_slice(&self.view_proj);
        cam[16..20].copy_from_slice(&self.light);
        cam[20..23].copy_from_slice(&self.cam_pos);
        cam[24..27].copy_from_slice(&self.cright);
        cam[27] = self.tan_h;
        cam[28..31].copy_from_slice(&self.cup);
        cam[31] = self.tan_v;
        cam[32..35].copy_from_slice(&self.cfwd);
        cam[36..52].copy_from_slice(&self.light_vp);
        queue.write_buffer(&self.cam_buf, 0, f32_bytes(&cam));

        // instâncias
        let n = self.draws.len() as u64;
        if n > self.inst_cap {
            let cap = n.next_power_of_two().max(64);
            self.inst_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("scene3d inst"),
                size: 96 * cap,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.inst_cap = cap;
        }
        let mut inst: Vec<f32> = Vec::with_capacity(self.draws.len() * 24);
        for (_m, model, color, emissive, tex) in &self.draws {
            inst.extend_from_slice(model);
            inst.extend_from_slice(color);
            inst.push(*emissive);
            inst.push(*tex);
            inst.push(0.0);
            inst.push(0.0);
        }
        if !inst.is_empty() {
            queue.write_buffer(&self.inst_buf, 0, f32_bytes(&inst));
        }

        // ── SHADOW PASS: depth da cena vista da luz (só quando há sombra ativa) ──
        let has_shadow = self.light_vp != identity();
        if has_shadow && !self.draws.is_empty() {
            let mut sp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene3d shadow pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
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
            sp.set_pipeline(&self.shadow_pipeline);
            sp.set_bind_group(0, &self.cam_bg, &[]);
            for (i, (mesh_id, _model, _color, _emiss, _tex)) in self.draws.iter().enumerate() {
                if let Some(m) = self.meshes.get(mesh_id) {
                    let off = (i as u64) * 96;
                    sp.set_vertex_buffer(0, m.vbuf.slice(..));
                    sp.set_vertex_buffer(1, self.inst_buf.slice(off..off + 96));
                    sp.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    sp.draw_indexed(0..m.icount, 0, 0..1);
                }
            }
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

        pass.set_bind_group(0, &self.cam_bg, &[]);
        pass.set_bind_group(1, &self.shadow_bg, &[]);
        // 1. SKYBOX (fullscreen, sem depth write) — fica no fundo
        pass.set_pipeline(&self.sky_pipeline);
        pass.draw(0..3, 0..1);
        // 2. meshes (depth test/write)
        pass.set_pipeline(&self.pipeline);
        for (i, (mesh_id, _model, _color, _emiss, _tex)) in self.draws.iter().enumerate() {
            if let Some(m) = self.meshes.get(mesh_id) {
                let off = (i as u64) * 96;
                pass.set_vertex_buffer(0, m.vbuf.slice(..));
                pass.set_vertex_buffer(1, self.inst_buf.slice(off..off + 96));
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

fn identity() -> [f32; 16] {
    [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]
}

/// view·proj ORTOGRÁFICA da luz direcional (shadow map). `dir` = direção que a luz
/// viaja; a câmera-luz é posta em `center - dir*2r` olhando na direção `dir`; ortho
/// meio-extent = radius, depth 0..1 (convenção wgpu). Column-major.
fn light_view_proj(dir: [f32; 3], center: [f32; 3], radius: f32) -> [f32; 16] {
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
    let near = 0.1f32;
    let far = 500.0f32;
    let tan_v = (fov_y * 0.5).tan();
    let f = 1.0 / tan_v;
    let p = [
        f / aspect, 0.0, 0.0, 0.0,
        0.0, f, 0.0, 0.0,
        0.0, 0.0, far / (far - near), 1.0,
        0.0, 0.0, -(far * near) / (far - near), 0.0,
    ];
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
