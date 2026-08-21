use super::*;

impl Scene3D {

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
        let mut cam = [0f32; 56];
        cam[..16].copy_from_slice(&self.view_proj);
        cam[16..20].copy_from_slice(&self.light);
        cam[20..23].copy_from_slice(&self.cam_pos);
        cam[24..27].copy_from_slice(&self.cright);
        cam[27] = self.tan_h;
        cam[28..31].copy_from_slice(&self.cup);
        cam[31] = self.tan_v;
        cam[32..35].copy_from_slice(&self.cfwd);
        cam[36..52].copy_from_slice(&self.light_vp);
        cam[52] = self.water_draws.first().map(|w| w.3).unwrap_or(0.0);
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
            let (_m, model, color, emissive, tex_flag, _tid) = &self.draws[i];
            inst.extend_from_slice(model);
            inst.extend_from_slice(color);
            inst.push(*emissive);
            inst.push(*tex_flag);
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

        // Cor de clear: fundo chapado (`bg`) OU o escuro padrão sob o skybox.
        let clear = match self.bg {
            Some(c) => wgpu::Color { r: c[0] as f64, g: c[1] as f64, b: c[2] as f64, a: c[3] as f64 },
            None => wgpu::Color { r: 0.02, g: 0.02, b: 0.03, a: 1.0 },
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene3d pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
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
        // 1. SKYBOX (fullscreen, sem depth write) — fica no fundo. Pulado quando há
        // fundo chapado (`bg`), que o clear acima já pintou.
        if self.bg.is_none() {
            pass.set_pipeline(&self.sky_pipeline);
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
        drop(pass);

        self.draws.clear();
        self.water_draws.clear();
        true
    }
}
