use super::*;
use super::lights::{env_floats, ENV_FLOATS};
use super::views::{cam_floats, viewport_px, Fundo, CAM_STRIDE, FULL};

impl Scene3D {

    /// Roda o scene pass no `encoder` compartilhado: um pass por vista (câmera,
    /// retângulo e fundo próprios), desenha a fila e a esvazia. Retorna `true`
    /// (o color foi limpo; o egui deve usar Load).
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

        // um slot `Cam` por vista (offset dinâmico) + o `Env` do frame
        let water = self.water_draws.first().map(|d| d.3).unwrap_or(0.0);
        let nviews = self.vq.len();
        for i in 0..nviews {
            let floats = cam_floats(self.vq.get(i), self.light, &self.light_vp, water);
            queue.write_buffer(&self.cam_buf, i as u64 * CAM_STRIDE, f32_bytes(&floats));
        }
        let has_pano = self.sky.modo > 2.5 && self.textures.contains_key(&self.sky.textura);
        let env: [f32; ENV_FLOATS] = env_floats(&self.lights, &self.sky, has_pano, self.fog);
        queue.write_buffer(&self.env_buf, 0, f32_bytes(&env));

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
        // ── AGRUPAMENTO POR (malha, textura) — o que torna o draw instanciado ──
        //
        // Antes, cada objeto era um draw call com `0..1` instância: 350 objetos
        // custavam 350 draws no pass principal e outros 350 no de sombra. Medido
        // no `castelo_gpu_demo`: desligar a sombra (que só remove os 350 draws
        // do depth, com um shader que nem calcula iluminação) levava de 81 para
        // 115 fps — ou seja ~10 µs de CPU POR DRAW CALL, que é overhead puro.
        //
        // As instâncias já estavam todas num buffer; o que faltava era ordená-lo
        // por grupo e pedir `0..n` em vez de `0..1`. Um castelo de um tipo de
        // bloco vira UM draw.
        //
        // O agrupamento é por (malha, textura) porque a textura é um bind group
        // por draw — dois objetos com texturas diferentes não podem entrar na
        // mesma chamada. Na prática quase tudo usa a textura default.
        let mut ordem: Vec<usize> = (0..self.draws.len()).collect();
        ordem.sort_by_key(|&i| (self.draws[i].0, self.draws[i].5));
        // Faixas contíguas de mesma (malha, textura): cada uma vira um draw.
        let mut grupos: Vec<(u64, u64, u32, u32)> = Vec::new(); // (malha, tex, inicio, n)
        for (posicao, &i) in ordem.iter().enumerate() {
            let chave = (self.draws[i].0, self.draws[i].5);
            match grupos.last_mut() {
                Some(g) if (g.0, g.1) == chave => g.3 += 1,
                _ => grupos.push((chave.0, chave.1, posicao as u32, 1)),
            }
        }
        let mut inst: Vec<f32> = Vec::with_capacity(self.draws.len() * 24);
        for &i in &ordem {
            let (_m, model, color, emissive, tex_flag, _tid, tile) = &self.draws[i];
            inst.extend_from_slice(model);
            inst.extend_from_slice(color);
            inst.push(*emissive);
            inst.push(*tex_flag);
            inst.push(*tile);
            inst.push(0.0);
        }
        if !inst.is_empty() {
            queue.write_buffer(&self.inst_buf, 0, f32_bytes(&inst));
        }

        // ── PARTÍCULAS: um lote por `queue_particles` (billboard instanciado,
        // sem malha própria). Preparado FORA do laço por-vista, como as
        // malhas acima: um `write_buffer` por frame, não um por vista.
        let part_total: usize =
            self.particle_draws.iter().map(|(f, _, _)| f.len() / particles::PART_FLOATS).sum();
        if part_total as u64 > self.particle_inst_cap {
            let cap = (part_total as u64).next_power_of_two().max(64);
            self.particle_inst_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("scene3d particle inst"),
                size: (particles::PART_FLOATS * 4) as u64 * cap,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.particle_inst_cap = cap;
        }
        let mut part_inst: Vec<f32> = Vec::with_capacity(part_total * particles::PART_FLOATS);
        // (início em INSTÂNCIAS, n, aditivo, textura opcional)
        let mut part_batches: Vec<(u32, u32, bool, Option<u64>)> = Vec::new();
        for (floats, aditivo, tex) in &self.particle_draws {
            let n = (floats.len() / particles::PART_FLOATS) as u32;
            if n == 0 {
                continue;
            }
            let inicio = (part_inst.len() / particles::PART_FLOATS) as u32;
            part_inst.extend_from_slice(floats);
            part_batches.push((inicio, n, *aditivo, *tex));
        }
        if !part_inst.is_empty() {
            queue.write_buffer(&self.particle_inst_buf, 0, f32_bytes(&part_inst));
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
            sp.set_bind_group(0, &self.cam_bg, &[0]); // light_vp é o mesmo em todos os slots
            // O pass de sombra não lê textura, então poderia agrupar só por
            // malha — mas reusa os MESMOS grupos de propósito: um segundo
            // critério de agrupamento seria uma segunda ordenação do buffer de
            // instâncias, e as duas teriam de concordar sobre qual instância
            // está em qual posição.
            for &(mesh_id, _tid, inicio, n) in &grupos {
                if let Some(m) = self.meshes.get(&mesh_id) {
                    let off = (inicio as u64) * 96;
                    let bytes = (n as u64) * 96;
                    sp.set_vertex_buffer(0, m.vbuf.slice(..));
                    sp.set_vertex_buffer(1, self.inst_buf.slice(off..off + bytes));
                    sp.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    sp.draw_indexed(0..m.icount, 0, 0..n);
                }
            }
        }

        // Clear da janela inteira: com UMA vista cheia é o de antes (a cor do
        // fundo chapado ou o escuro sob o céu); com várias, o escuro (faixas).
        // É o PRIMEIRO pass com área que limpa a janela inteira, mesmo que a
        // vista dele tenha `limpar = false` — "nada" só vale dentro do frame.
        let v0 = *self.vq.get(0);
        let base = match v0.fundo {
            Fundo::Cor(c) if nviews == 1 && v0.rect == FULL =>
                wgpu::Color { r: c[0] as f64, g: c[1] as f64, b: c[2] as f64, a: c[3] as f64 },
            _ => wgpu::Color { r: 0.02, g: 0.02, b: 0.03, a: 1.0 },
        };
        let sky_bg = if has_pano { &self.textures[&self.sky.textura] } else { &self.default_tex_bg };
        let mut limpou = false;
        for i in 0..nviews {
            let v = *self.vq.get(i);
            let Some(px) = viewport_px(v.rect, w, h) else { continue };
            let load = if limpou { wgpu::LoadOp::Load } else { wgpu::LoadOp::Clear(base) };
            limpou = true;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene3d pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
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
            pass.set_viewport(px[0] as f32, px[1] as f32, px[2] as f32, px[3] as f32, 0.0, 1.0);
            pass.set_scissor_rect(px[0], px[1], px[2], px[3]);
            pass.set_bind_group(0, &self.cam_bg, &[(i as u64 * CAM_STRIDE) as u32]);
            pass.set_bind_group(1, &self.shadow_bg, &[]);
            // 1. fundo da vista: céu, ou cor chapada (view_bg.w = 1) só dentro
            // do retângulo. `limpar = false` não pinta nada (só a profundidade).
            if v.limpar {
                pass.set_pipeline(&self.sky_pipeline);
                pass.set_bind_group(2, sky_bg, &[]);
                pass.draw(0..3, 0..1);
            }
            // 2. meshes (depth test/write). Group 2 = textura de albedo: por-draw,
            // a textura do objeto (tex_id>=2) ou a 1×1 branca default.
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(2, &self.default_tex_bg, &[]);
            for &(mesh_id, tid, inicio, n) in &grupos {
                if let Some(m) = self.meshes.get(&mesh_id) {
                    let tex_bg = self.textures.get(&tid).unwrap_or(&self.default_tex_bg);
                    pass.set_bind_group(2, tex_bg, &[]);
                    let off = (inicio as u64) * 96;
                    let bytes = (n as u64) * 96;
                    pass.set_vertex_buffer(0, m.vbuf.slice(..));
                    pass.set_vertex_buffer(1, self.inst_buf.slice(off..off + bytes));
                    pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..m.icount, 0, 0..n);
                }
            }
            // 2.5 PARTÍCULAS: billboard instanciado, DEPOIS das malhas opacas
            // (depth já escrito) e ANTES da água. Depth TEST ligado (ficam
            // atrás de paredes), WRITE desligado (translúcidas entre si, não
            // se ocultam na ordem de chegada). Um dos 4 pipelines por lote —
            // `aditivo` (blend) × tem-textura (disco procedural ou amostra
            // `albedo_tex`), decidido por `particles::escolher_pipeline`
            // (pura, testada sem GPU) e só mapeado pro `RenderPipeline` aqui.
            for &(inicio, n, aditivo, tex) in &part_batches {
                let pipeline = match particles::escolher_pipeline(aditivo, tex.is_some()) {
                    particles::PipelineParticula::Alfa => &self.particle_pipeline_alfa,
                    particles::PipelineParticula::Aditivo => &self.particle_pipeline_aditivo,
                    particles::PipelineParticula::AlfaTex => &self.particle_pipeline_tex_alfa,
                    particles::PipelineParticula::AditivoTex => &self.particle_pipeline_tex_aditivo,
                };
                pass.set_pipeline(pipeline);
                // A textura do lote (se houver) ou a 1×1 branca default — mas
                // só importa VISUALMENTE quando o pipeline escolhido é uma
                // variante `*Tex`; o disco procedural nunca a amostra.
                let tex_bg = tex.and_then(|t| self.textures.get(&t)).unwrap_or(&self.default_tex_bg);
                pass.set_bind_group(2, tex_bg, &[]);
                let stride = (particles::PART_FLOATS * 4) as u64;
                let off = inicio as u64 * stride;
                let bytes = n as u64 * stride;
                pass.set_vertex_buffer(0, self.particle_inst_buf.slice(off..off + bytes));
                pass.draw(0..4, 0..n);
            }

            // 3. ÁGUA INSTANCIADA: 1 draw call por fila; instâncias direto do
            // storage buffer da física. Sem sombra própria (v1): a água recebe a
            // sombra do mundo pelo shadow_factor, mas não a projeta.
            if !self.water_draws.is_empty() {
                pass.set_pipeline(&self.water_pipeline);
                pass.set_bind_group(2, &self.default_tex_bg, &[]);
                for (mesh_id, buf, count, _scale) in &self.water_draws {
                    if let Some(m) = self.meshes.get(mesh_id) {
                        pass.set_vertex_buffer(0, m.vbuf.slice(..));
                        pass.set_vertex_buffer(1, buf.slice(..));
                        pass.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..m.icount, 0, 0..*count);
                    }
                }
            }
        }
        if !limpou {
            // nenhuma vista com área: ainda assim o frame precisa ser limpo
            let _ = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene3d clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(base), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }

        self.draws.clear();
        self.water_draws.clear();
        self.particle_draws.clear();
        self.vq.end_frame();
        true
    }
}
