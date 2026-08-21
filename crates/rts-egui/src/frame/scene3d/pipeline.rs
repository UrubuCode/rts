use super::shader::SHADER;
use super::*;

impl Scene3D {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, color_format: wgpu::TextureFormat) -> Scene3D {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene3d shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        // uniform: view_proj(16) + light(4) + cam_pos(4) + right(4) + up(4) + fwd(4)
        //          + light_vp(16) + water(4) = 56 f32 = 224 bytes
        let cam_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene3d cam"),
            size: 224,
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

        // textura de albedo (group 2): layout texture_2d + sampler linear/repeat.
        let tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene3d tex bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let tex_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("scene3d tex samp"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest, // mip_level_count=1 → irrelevante
            ..Default::default()
        });
        // 1×1 branca default: bindada em objetos SEM textura (o shader só a usa se
        // tex_flag>=2, mas o pipeline exige group 2 sempre bindado).
        let default_tex_bg = make_tex_bg(device, queue, &tex_bgl, &tex_sampler, &[255, 255, 255, 255], 1, 1);

        // layout do pass principal: group 0 (câmera) + group 1 (shadow) + group 2 (albedo).
        let mesh_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene3d mesh layout"),
            bind_group_layouts: &[Some(&cam_bgl), Some(&shadow_bgl), Some(&tex_bgl)],
            immediate_size: 0,
        });
        // layout do sky: group 0 (câmera) + group 1 (shadow) — sem textura de albedo.
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
            array_stride: 32,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 12, shader_location: 1 },
                wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 24, shader_location: 8 },
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
            layout: Some(&mesh_layout),
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
                // sem backface culling: lajes finas / beiral de telhado / escala
                // não-uniforme não "vazam" ao serem vistos por trás (editor).
                cull_mode: None,
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
                buffers: &[vbl.clone(), ibl],
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

        // PIPELINE DA ÁGUA INSTANCIADA: mesmo fs/bind groups; instância é UM
        // vec4 (stride 16) vindo DIRETO do storage buffer da física (rts:gpu).
        let water_vbl = wgpu::VertexBufferLayout {
            array_stride: 16,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x4,
                offset: 0,
                shader_location: 2,
            }],
        };
        let water_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene3d water pipeline"),
            layout: Some(&mesh_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_water"),
                buffers: &[vbl.clone(), water_vbl],
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
                cull_mode: None,
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
            tex_bgl,
            tex_sampler,
            default_tex_bg,
            textures: HashMap::new(),
            next_tex: 2, // 0=nenhuma, 1=xadrez procedural reservados
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
            water_pipeline,
            water_draws: Vec::new(),
            inst_buf,
            inst_cap: 64,
            bg: None,
        }
    }
}
