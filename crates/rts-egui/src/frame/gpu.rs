//! Backend de GPU (wgpu) de uma janela: `GpuConfig`/`WindowChrome` (decode do
//! bitfield de `openWindow`), a GPU compartilhada do processo, `RenderState`
//! (surface + device/queue + `egui_wgpu::Renderer`), `Backend` (wgpu/glow) e o
//! `present_wgpu` (render + present de um frame já tesselado).

use std::cell::RefCell;
use std::sync::Arc;

use winit::window::Window;

/// Present mode da surface de TODA janela. **Fifo (vsync)** é OBRIGATÓRIO, não
/// uma preferência: o loop de render é dirigido pelo TS sem throttle próprio, e
/// sem vsync ele gira a milhares de fps — queima CPU e, sob essa cadência, o
/// swapchain entra em estado ruim (a janela parava de avançar após alguns
/// milhares de frames — bug real corrigido). Fifo é garantido em todo backend e
/// limita à taxa do monitor. NÃO troque para `Immediate`/`Mailbox` sem reintroduzir
/// um throttle no loop; o teste `vsync_kill_gate` falha o build se isto mudar.
const UI_PRESENT_MODE: wgpu::PresentMode = wgpu::PresentMode::Fifo;

/// GPU compartilhada do processo (thread do TS): Instance + Adapter + Device +
/// Queue são por-GPU, NÃO por-janela. wgpu os expõe como handles `Clone`
/// (Arc-backed). Criados LAZY na 1ª janela e REUSADOS por todas.
///
/// CAUSA RAIZ do vazamento de RAM: a versão anterior criava um Device NOVO (~190
/// MB) por `openWindow`. Abrir janelas em loop (hot-reload / re-render) sem fechar
/// — ou só churn de abrir/fechar — acumulava/recriava Devices gigantes (medido:
/// 30 janelas sem close = 5,6 GB). Compartilhando o Device, cada janela nova custa
/// só uma Surface + Renderer (poucos MB), e a RAM fica limitada.
/// GPU device knobs, decoded from the `openWindow` config bitfield. Defaults are
/// the RAM-minimizing choices; the user opts INTO the heavier ones per case.
/// NOTE: the device is created ONCE (shared); these apply on the FIRST window —
/// later windows reuse the same device. Per-window knobs live in the surface.
#[derive(Clone, Copy)]
pub struct GpuConfig {
    /// `true` → `PowerPreference::HighPerformance` (discrete GPU). Default `false`
    /// (`LowPower`, integrated when present — lighter driver).
    pub high_perf: bool,
    /// `true` → `MemoryHints::Performance` (pre-allocates big blocks). Default
    /// `false` (`MemoryHints::MemoryUsage` — small blocks, less RAM).
    pub mem_performance: bool,
    /// `true` → high device limits (`Limits::default`). Default `false`
    /// (`downlevel_defaults` — modest limits, less heap reserved).
    pub high_limits: bool,
    /// `true` → backend OpenGL (glow) em vez do wgpu/DX12. Bem mais leve em RAM
    /// (dezenas de MB vs ~224 MB), à custa de menos throughput. Os outros bits de
    /// device (perf/mem/limits) só se aplicam ao wgpu — no glow são ignorados.
    pub use_glow: bool,
}

impl GpuConfig {
    /// Decode the `openWindow` config bitfield: bit0 = high_perf, bit1 =
    /// mem_performance, bit2 = high_limits, bit3 = use_glow. `0` (the common case)
    /// = wgpu, all-optimized.
    pub fn from_bits(bits: i64) -> Self {
        GpuConfig {
            high_perf: bits & 0b0001 != 0,
            mem_performance: bits & 0b0010 != 0,
            high_limits: bits & 0b0100 != 0,
            use_glow: bits & 0b1000 != 0,
        }
    }
}

/// Chrome da janela (winit), do mesmo config bitfield: bit4 = transparent,
/// bit5 = SEM decorations (frameless). Vale p/ ambos os backends. `transparent`
/// também troca o fundo do painel egui + a cor de clear p/ alpha 0, pra a
/// transparência do SO realmente aparecer (senão o painel opaco do egui tapa).
#[derive(Clone, Copy)]
pub struct WindowChrome {
    pub transparent: bool,
    pub decorations: bool,
}

impl WindowChrome {
    pub fn from_bits(bits: i64) -> Self {
        WindowChrome {
            transparent: bits & 0b1_0000 != 0,  // bit4
            decorations: bits & 0b10_0000 == 0, // bit5 set => SEM decorations
        }
    }
}

/// Backend de render de UMA janela. wgpu (DX12/Vulkan/Metal — pesado em RAM) ou
/// glow (OpenGL — leve), escolhido por janela via `GpuConfig::use_glow`. O pass do
/// egui (begin/drena/end/tessellate) é AGNÓSTICO; só a pintura/apresentação
/// (`present_wgpu` vs `GlowState::paint`) difere.
pub enum Backend {
    Wgpu(RenderState),
    #[cfg(feature = "glow-backend")]
    Glow(crate::glbackend::GlowState),
}

impl Backend {
    /// Reage a um `WindowEvent::Resized` reconfigurando a surface do backend ativo.
    pub fn resize(&mut self, width: u32, height: u32) {
        match self {
            Backend::Wgpu(r) => r.resize(width, height),
            #[cfg(feature = "glow-backend")]
            Backend::Glow(g) => g.resize(width, height),
        }
    }
}

#[derive(Clone)]
struct SharedGpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

thread_local! {
    static SHARED_GPU: RefCell<Option<SharedGpu>> = const { RefCell::new(None) };
}

/// Retorna (clonando os handles) a GPU compartilhada, criando-a UMA vez. O adapter
/// é pedido sem `compatible_surface` (desktop: pega o GPU default, compatível com
/// as surfaces de janela criadas depois).
fn shared_gpu(cfg: GpuConfig) -> Result<SharedGpu, String> {
    SHARED_GPU.with(|cell| {
        if let Some(g) = cell.borrow().as_ref() {
            return Ok(g.clone());
        }
        // Minimal instance: backends from env (DX12 on Windows), but flags forced
        // to the release build-config (no validation/debug layers, which the DX12
        // debug layer would otherwise reserve memory for).
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        desc.flags = wgpu::InstanceFlags::from_build_config();
        let instance = wgpu::Instance::new(desc);
        // Default `LowPower` prefers the INTEGRATED GPU when present (lighter
        // driver, less RAM for a 2D UI). The user opts into `HighPerformance` via
        // the window config when they need discrete-GPU throughput.
        let power_preference = if cfg.high_perf {
            wgpu::PowerPreference::HighPerformance
        } else {
            wgpu::PowerPreference::LowPower
        };
        let adapter = pollster::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference,
                force_fallback_adapter: false,
                compatible_surface: None,
            },
        ))
        .map_err(|e| format!("request_adapter: {e}"))?;
        // RAM-minimizing device by default: `downlevel_defaults` limits (a 2D UI
        // needs no high-end limits that make the driver reserve large heaps) +
        // `MemoryHints::MemoryUsage` (gpu-allocator favors small blocks vs the
        // Performance default that pre-allocates big chunks). The user opts into
        // the heavier `default` limits / `Performance` hint via the window config.
        let required_limits = if cfg.high_limits {
            wgpu::Limits::default()
        } else {
            wgpu::Limits::downlevel_defaults()
        };
        let memory_hints = if cfg.mem_performance {
            wgpu::MemoryHints::Performance
        } else {
            wgpu::MemoryHints::MemoryUsage
        };
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("rts-egui shared device"),
                required_features: wgpu::Features::empty(),
                required_limits,
                memory_hints,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                trace: wgpu::Trace::Off,
            },
        ))
        .map_err(|e| format!("request_device: {e}"))?;
        let gpu = SharedGpu {
            instance,
            adapter,
            device,
            queue,
        };
        *cell.borrow_mut() = Some(gpu.clone());
        Ok(gpu)
    })
}

/// Backend de render wgpu de uma janela. Tudo `!Send`.
pub struct RenderState {
    /// `'static` porque a surface é dona da janela (via `Arc<Window>`).
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub renderer: egui_wgpu::Renderer,
    /// Janela transparente → clear com alpha 0 (o painel egui também fica
    /// transparente em `end_frame`), pra o SO compor o fundo.
    pub transparent: bool,
    /// Estado do scene pass 3D (`gpu3d`) desta janela — `None` até o 1º
    /// `gpu3d.mesh`. Quando há draws no frame, a cena é gravada ANTES do pass
    /// do egui (que então carrega em vez de limpar). Ver `crate::scene3d`.
    pub scene: Option<crate::scene3d::SceneState>,
}

impl RenderState {
    /// Inicializa o backend wgpu para `window`. Síncrono via `pollster::block_on`
    /// (wgpu 29 retorna futures para `request_adapter`/`request_device`).
    ///
    /// `window` é `Arc<Window>` para que `create_surface` produza
    /// `Surface<'static>` (o alvo owned satisfaz o lifetime `'static`).
    pub fn new(window: Arc<Window>, cfg: GpuConfig, transparent: bool) -> Result<RenderState, String> {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        // Instance/Adapter/Device/Queue COMPARTILHADOS (criados 1×, reusados por
        // todas as janelas) — antes eram por-janela (~190 MB cada), a causa do
        // vazamento. Só a Surface + config + Renderer abaixo são por-janela. `cfg`
        // só tem efeito na 1ª janela (criação do device); depois é reusado.
        let gpu = shared_gpu(cfg)?;

        // `Arc<Window>` (owned) → `Surface<'static>`, da instance compartilhada.
        let surface = gpu
            .instance
            .create_surface(window.clone())
            .map_err(|e| format!("create_surface: {e}"))?;

        let mut config = surface
            .get_default_config(&gpu.adapter, width, height)
            .ok_or_else(|| "surface não suportada pelo adapter".to_string())?;
        // 1 frame em voo (default 2): metade das imagens do swapchain → menos VRAM/
        // RAM reservada. Suficiente para uma UI imediata que não precisa de
        // pipelining profundo de frames.
        config.desired_maximum_frame_latency = 1;
        // vsync EXPLÍCITO: o loop é dirigido pelo TS sem throttle próprio. Ver
        // UI_PRESENT_MODE (e o kill-gate que protege essa escolha).
        config.present_mode = UI_PRESENT_MODE;
        // Janela transparente: escolhe um alpha_mode que componha o alpha (não
        // Opaque). Pega o 1º não-Opaque suportado (PreMultiplied/PostMultiplied/
        // Inherit); se só houver Opaque, segue opaco (transparência indisponível).
        if transparent {
            let caps = surface.get_capabilities(&gpu.adapter);
            if let Some(mode) = caps
                .alpha_modes
                .iter()
                .copied()
                .find(|m| *m != wgpu::CompositeAlphaMode::Opaque)
            {
                config.alpha_mode = mode;
            }
        }
        surface.configure(&gpu.device, &config);

        // egui-wgpu 0.34: `Renderer::new(device, color_format, RendererOptions)`.
        let renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            config.format,
            egui_wgpu::RendererOptions::default(),
        );

        Ok(RenderState {
            surface,
            device: gpu.device,
            queue: gpu.queue,
            config,
            renderer,
            transparent,
            scene: None,
        })
    }

    /// Reconfigura a surface após resize (chamado quando o tamanho muda).
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }
}

/// Render + present de um frame já tesselado (backend wgpu). Separado para manter
/// `endFrame` curto e a regra das 500 linhas. Recebe a `RenderState` + a janela
/// por empréstimos disjuntos (o `endFrame` os separa de `c`).
pub(crate) fn present_wgpu(
    r: &mut RenderState,
    window: &Window,
    paint_jobs: Vec<egui::ClippedPrimitive>,
    textures_delta: egui::TexturesDelta,
    pixels_per_point: f32,
) {
    // ── 0. Sincroniza a surface com o tamanho FÍSICO real da janela ──────────
    // `config.width/height` é o tamanho FÍSICO (px) da surface, e vira o
    // `size_in_pixels` do `ScreenDescriptor` — a base que o shader do egui usa
    // para mapear os vértices (em pontos) para clip-space. Ele NÃO pode divergir
    // do `inner_size()` real da janela.
    //
    // Por que divergia (a causa raiz da "faixa vertical estreita"): o `config` é
    // construído UMA vez em `RenderState::new`, a partir do `inner_size()` do
    // momento da criação. Mas durante o `openWindow` a janela emite eventos
    // `Resized` (tamanho transitório do WM/decoração, p.ex. 1424×714 antes de
    // assentar) que o handler `Builder` IGNORA (`window_event` no-op). Assim a
    // surface fica com um `config` (size_in_pixels) defasado do tamanho final da
    // janela, enquanto o LAYOUT do egui usa o `inner_size()` ATUAL (via
    // `take_egui_input`). Render num espaço físico LARGO + layout num espaço
    // lógico ESTREITO ⇒ o conteúdo, correto em pontos, é comprimido numa faixa no
    // canto. Sincronizar aqui (e reconfigurar quando muda) casa os dois espaços
    // todo frame, independente de quando/se um `Resized` chegou.
    let size = window.inner_size();
    if size.width > 0
        && size.height > 0
        && (size.width != r.config.width || size.height != r.config.height)
    {
        r.config.width = size.width;
        r.config.height = size.height;
        r.surface.configure(&r.device, &r.config);
    }

    // 3. Sobe textures novas/atualizadas.
    for (id, image_delta) in &textures_delta.set {
        r.renderer
            .update_texture(&r.device, &r.queue, *id, image_delta);
    }

    let screen_descriptor = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [r.config.width, r.config.height],
        pixels_per_point,
    };

    // 4. Adquire o frame da surface. wgpu 29: `get_current_texture` retorna o
    // enum `CurrentSurfaceTexture` (não mais `Result`). Tratamos
    // Success/Suboptimal como ok; os demais casos reconfiguram e pulam o frame.
    let frame = match r.surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(f)
        | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
        _ => {
            // Surface perdida/desatualizada/timeout — reconfigura e desiste.
            r.surface.configure(&r.device, &r.config);
            textures_delta
                .free
                .iter()
                .for_each(|id| r.renderer.free_texture(id));
            return;
        }
    };
    let view = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = r
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rts-egui encoder"),
        });

    r.renderer
        .update_buffers(&r.device, &r.queue, &mut encoder, &paint_jobs, &screen_descriptor);

    // ── Scene pass 3D (gpu3d) — ANTES do pass do egui, no MESMO encoder ─────
    // Quando o TS enfileirou `gpu3d.draw` neste frame, a cena limpa cor+depth e
    // desenha as malhas; o pass do egui abaixo então CARREGA (LoadOp::Load) em
    // vez de limpar, compondo a UI por cima. Sem draws: comportamento idêntico
    // ao anterior (egui limpa). Ver docs/specs/gpu3d-scene-pass.md.
    let scene_drawn = crate::scene3d::record_if_active(
        &mut r.scene,
        &r.device,
        &r.queue,
        &mut encoder,
        &view,
        r.config.width,
        r.config.height,
    );

    {
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rts-egui pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    // Cena 3D desenhada → carrega (a UI compõe por cima).
                    // Transparente: clear com alpha 0 p/ o SO compor o fundo.
                    // Opaco: fundo escuro padrão.
                    load: if scene_drawn {
                        wgpu::LoadOp::Load
                    } else if r.transparent {
                        wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 })
                    } else {
                        wgpu::LoadOp::Clear(wgpu::Color { r: 0.02, g: 0.02, b: 0.03, a: 1.0 })
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        // `Renderer::render` exige `RenderPass<'static>`: o pass empresta o
        // `encoder`, mas `forget_lifetime()` descarta esse vínculo de lifetime
        // (passando a checagem de "encoder usado durante o pass" para runtime).
        // É o padrão recomendado pela egui-wgpu 0.34 para casar o `&'static`
        // exigido por `render`.
        let mut pass = pass.forget_lifetime();
        r.renderer.render(&mut pass, &paint_jobs, &screen_descriptor);
        // `pass` é dropado aqui, liberando o `encoder` para `finish()`.
    }

    r.queue.submit(std::iter::once(encoder.finish()));
    frame.present();

    // 5. Libera textures marcadas para free DEPOIS do submit.
    for id in &textures_delta.free {
        r.renderer.free_texture(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kill-gate do vsync: a surface DEVE usar Fifo. Sem isso o loop dirigido pelo
    /// TS gira a milhares de fps e a janela parava sozinha após N frames (bug real
    /// corrigido). Se alguém trocar `UI_PRESENT_MODE`, este teste falha o build —
    /// forçando reintroduzir um throttle antes de abandonar o vsync.
    #[test]
    fn vsync_kill_gate() {
        assert_eq!(
            UI_PRESENT_MODE,
            wgpu::PresentMode::Fifo,
            "a UI precisa de vsync (Fifo); sem throttle no loop, trocar isso traz de volta o bug da janela que para sozinha"
        );
    }
}
