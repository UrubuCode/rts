//! Pipeline wgpu do scene pass 3D: shader WGSL (pos+cor, MVP por draw via
//! dynamic uniform offset), depth buffer `Depth32Float` e a gravação do render
//! pass da cena — que roda ANTES do pass do egui, no MESMO encoder (o egui vira
//! overlay; ver `docs/specs/gpu3d-scene-pass.md`).

use super::{DrawCmd, SceneState};
use super::math3d;

/// Stride de um vértice: pos f32×3 + cor f32×3 = 24 bytes.
pub const VERTEX_STRIDE: u64 = 24;

/// Alinhamento mínimo de dynamic offset em uniform buffers exigido pela spec
/// WebGPU (256 em todo hardware relevante). Cada draw ocupa um slot deste
/// tamanho no uniform buffer (mat4 = 64 bytes + padding).
pub const UNIFORM_SLOT: u64 = 256;

/// Máximo de draws por frame — dimensiona o uniform buffer (4096×256 = 1 MiB).
pub const MAX_DRAWS: usize = 4096;

const SHADER: &str = r#"
struct Uniforms { mvp: mat4x4<f32> };
@group(0) @binding(0) var<uniform> u: Uniforms;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(@location(0) pos: vec3<f32>, @location(1) color: vec3<f32>) -> VsOut {
    var out: VsOut;
    out.pos = u.mvp * vec4<f32>(pos, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
"#;

/// Recursos de GPU do pass 3D, criados UMA vez por janela (lazy, no 1º mesh).
pub struct GpuRes {
    pub pipeline: wgpu::RenderPipeline,
    pub bind_group: wgpu::BindGroup,
    pub uniforms: wgpu::Buffer,
    /// Depth texture + view, recriadas quando o tamanho da surface muda.
    pub depth_view: wgpu::TextureView,
    pub depth_size: (u32, u32),
}

impl GpuRes {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> GpuRes {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rts-gpu3d shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rts-gpu3d bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: std::num::NonZeroU64::new(64),
                },
                count: None,
            }],
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rts-gpu3d uniforms"),
            size: UNIFORM_SLOT * MAX_DRAWS as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rts-gpu3d bind group"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniforms,
                    offset: 0,
                    size: std::num::NonZeroU64::new(64),
                }),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rts-gpu3d layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rts-gpu3d pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: VERTEX_STRIDE,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // Sem cull no MVP: o TS ainda não tem por que garantir winding
                // consistente nos meshes; descartar triângulos "de costas"
                // agora só produziria buracos surpresa. Cull entra com a fatia
                // de iluminação (normais tornam o winding significativo).
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        // depth criada no 1º ensure_depth (precisa do tamanho real da surface).
        let depth_view = create_depth(device, 1, 1);
        GpuRes {
            pipeline,
            bind_group,
            uniforms,
            depth_view,
            depth_size: (1, 1),
        }
    }

    /// Garante que a depth texture casa com o tamanho atual da surface.
    pub fn ensure_depth(&mut self, device: &wgpu::Device, w: u32, h: u32) {
        if self.depth_size != (w, h) && w > 0 && h > 0 {
            self.depth_view = create_depth(device, w, h);
            self.depth_size = (w, h);
        }
    }
}

fn create_depth(device: &wgpu::Device, w: u32, h: u32) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("rts-gpu3d depth"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Grava o render pass da cena no `encoder`, sobre `view` (a MESMA texture da
/// surface que o pass do egui usa em seguida com `LoadOp::Load`). Limpa cor +
/// depth, desenha cada `DrawCmd` com seu MVP (uniform em dynamic offset) e
/// esvazia a fila. Chamado por `present_wgpu` quando `scene.draws` não é vazio.
pub fn record_scene_pass(
    scene: &mut SceneState,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    width: u32,
    height: u32,
) {
    let aspect = width.max(1) as f32 / height.max(1) as f32;
    let proj = math3d::perspective(scene.fovy_rad, aspect, scene.near, scene.far);
    let view_m = math3d::look_at(scene.eye, scene.target);
    let vp = proj.mul(&view_m);

    // MVP por draw nos slots do uniform buffer (um write só, contíguo).
    let draws: Vec<DrawCmd> = std::mem::take(&mut scene.draws);
    let mut ubytes = vec![0u8; draws.len() * UNIFORM_SLOT as usize];
    for (i, d) in draws.iter().enumerate() {
        let model = math3d::model_trs(d.x, d.y, d.z, d.yaw, d.pitch, d.scale);
        let mvp = vp.mul(&model);
        ubytes[i * UNIFORM_SLOT as usize..i * UNIFORM_SLOT as usize + 64]
            .copy_from_slice(&mvp.to_bytes());
    }
    let res = scene.gpu.as_mut().expect("record_scene_pass sem GpuRes");
    queue.write_buffer(&res.uniforms, 0, &ubytes);
    res.ensure_depth(device, width, height);

    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("rts-gpu3d scene pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color {
                    r: scene.clear_color[0],
                    g: scene.clear_color[1],
                    b: scene.clear_color[2],
                    a: 1.0,
                }),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: &res.depth_view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Discard,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });

    pass.set_pipeline(&res.pipeline);
    for (i, d) in draws.iter().enumerate() {
        let Some(mesh) = scene.meshes.get(&d.mesh_id) else { continue };
        pass.set_bind_group(0, &res.bind_group, &[(i as u32) * UNIFORM_SLOT as u32]);
        pass.set_vertex_buffer(0, mesh.vbuf.slice(..));
        match &mesh.ibuf {
            Some(ib) => {
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
            None => pass.draw(0..mesh.vertex_count, 0..1),
        }
    }
}
