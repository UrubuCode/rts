//! `RenderState` (backend wgpu) + `beginFrame` / `endFrame`.
//!
//! `RenderState` agrega o backend de render wgpu de uma janela: surface,
//! device/queue, config e o `egui_wgpu::Renderer`. A `Surface` é `'static`
//! (criada a partir de um `Arc<Window>` owned — ver `ctx::UiCtx::window`), o que
//! evita um lifetime de empréstimo amarrando a struct à janela.
//!
//! `beginFrame` abre um pass do egui (`begin_pass`) e zera a fila de widgets.
//! `endFrame` drena a fila num `CentralPanel`, encerra o pass (`end_pass`),
//! tesselará as shapes e renderiza/apresenta o frame via wgpu.

use std::cell::RefCell;
use std::sync::Arc;

use winit::window::Window;

use crate::ctx::{self, WidgetCmd};

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
}

impl GpuConfig {
    /// Decode the `openWindow` config bitfield: bit0 = high_perf, bit1 =
    /// mem_performance, bit2 = high_limits. `0` (the common case) = all optimized.
    pub fn from_bits(bits: i64) -> Self {
        GpuConfig {
            high_perf: bits & 0b001 != 0,
            mem_performance: bits & 0b010 != 0,
            high_limits: bits & 0b100 != 0,
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
}

impl RenderState {
    /// Inicializa o backend wgpu para `window`. Síncrono via `pollster::block_on`
    /// (wgpu 29 retorna futures para `request_adapter`/`request_device`).
    ///
    /// `window` é `Arc<Window>` para que `create_surface` produza
    /// `Surface<'static>` (o alvo owned satisfaz o lifetime `'static`).
    pub fn new(window: Arc<Window>, cfg: GpuConfig) -> Result<RenderState, String> {
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

/// Abre um pass do egui: pega o input acumulado e zera a fila de widgets do
/// frame. Os widgets-folha entre aqui e `endFrame` apenas enfileiram em `cmds`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_BEGIN_FRAME(h: u64) {
    ctx::with_ctx(h, |c| {
        if c.frame_active {
            return; // beginFrame duplo sem endFrame — ignora.
        }
        // `take_egui_input` JÁ deriva um `screen_rect` correto a cada chamada,
        // direto do `window.inner_size()` ÷ `pixels_per_point` (não de um evento
        // `Resized` em cache — ver egui_winit::State::take_egui_input). Logo NÃO
        // sobrescrevemos o `screen_rect` aqui: fazê-lo com `window.scale_factor()`
        // (em vez do `pixels_per_point` do contexto, que inclui o zoom_factor)
        // arriscaria descasar do `pixels_per_point` usado no tessellate/render.
        // A consistência físico×lógico é garantida no `present` (size_in_pixels =
        // inner_size físico; ppp = full_output.pixels_per_point), de modo que
        // size_in_pixels / ppp == screen_rect lógico em qualquer DPI.
        let raw_input = c.egui_state.take_egui_input(&c.window);

        c.egui_ctx.begin_pass(raw_input);
        c.frame_active = true;
        c.cmds.clear();
        c.button_cursor = 0;
        c.slider_cursor = 0;
    });
}

/// Encerra o pass do egui e apresenta o frame.
///
/// Passos (espelham o pipeline egui-wgpu padrão):
/// 1. drena a fila de widgets dentro de um `CentralPanel`, gravando os
///    resultados de interação deste frame em `button_results`/`slider_results`
///    (lidos pelo próximo frame);
/// 2. `end_pass` → `FullOutput`; `tessellate(shapes, ppp)` → paint jobs;
/// 3. sobe as textures deltas para o `Renderer`;
/// 4. adquire o frame da surface, grava o render pass (clear escuro + egui) e
///    apresenta;
/// 5. libera as textures marcadas como free.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_END_FRAME(h: u64) {
    ctx::with_ctx(h, |c| {
        if !c.frame_active {
            return;
        }
        c.frame_active = false;

        // ── 1. Drena a fila num CentralPanel, coletando interações ───────────
        // Toma a fila por valor (evita emprestar `c` dentro do closure). O DOM
        // retido (`c.dom`) é só LIDO pelo render — saca por valor com `take` e
        // devolve depois, mantendo `c` livre para o `egui_ctx` no `show`.
        let cmds = std::mem::take(&mut c.cmds);
        let dom = c.dom.take();
        let mut new_buttons: Vec<bool> = Vec::new();
        let mut new_sliders: Vec<f64> = Vec::new();

        // `CentralPanel::show(&Context, ...)` está marcado `#[deprecated]` na
        // 0.34 com a nota "use show_inside()", mas `show_inside` exige um `&mut
        // Ui` pai — que NÃO temos aqui (estamos no nível do `Context`, dentro do
        // pass aberto por `begin_pass`). Para um painel raiz a partir do
        // `Context`, `show` continua sendo a API correta; o allow é localizado.
        #[allow(deprecated)]
        egui::CentralPanel::default().show(&c.egui_ctx, |ui| {
            // A drenagem é RECURSIVA para tratar os escopos horizontais (ver
            // `drenar`). O `idx` percorre a fila linearmente e é compartilhado
            // por todos os níveis de recursão, então a ordem de emissão (e logo
            // a ordem em que `new_buttons`/`new_sliders` são preenchidos) casa
            // exatamente com a ordem de enfileiramento em `widgets.rs`.
            let mut idx = 0usize;
            drenar(ui, &cmds, dom.as_ref(), &mut idx, &mut new_buttons, &mut new_sliders);
            // Se há DOM retido e nenhum `html()` foi chamado NESTE frame (a fila
            // não tem o marcador Html), renderiza o DOM mesmo assim — é o caso do
            // fluxo "parseia uma vez, depois muta via JS e só re-renderiza".
            let has_html_marker = cmds.iter().any(|c| matches!(c, WidgetCmd::Html));
            if !has_html_marker {
                if let Some(dom) = dom.as_ref() {
                    render_dom(ui, dom);
                }
            }
        });
        // Devolve o DOM retido ao UiCtx (persiste para o próximo frame / mutação).
        c.dom = dom;

        c.button_results = new_buttons;
        c.slider_results = new_sliders;

        // ── 2. Encerra o pass e tesselará ────────────────────────────────────
        let full_output = c.egui_ctx.end_pass();
        let ppp = full_output.pixels_per_point;
        let paint_jobs = c.egui_ctx.tessellate(full_output.shapes, ppp);

        // Repassa cliques de janela do egui (cursor etc) — opcional no P1.
        c.egui_state
            .handle_platform_output(&c.window, full_output.platform_output);

        present(c, paint_jobs, full_output.textures_delta, ppp);
    });
}

/// Drena a fila de comandos `cmds` a partir de `*idx`, emitindo cada widget no
/// `ui` ATUAL. É RECURSIVA para tratar os escopos horizontais sem precisar
/// guardar um `egui::Ui` entre chamadas FFI (o `ui.horizontal(...)` exige um
/// closure, que esta recursão fornece).
///
/// Contrato do `idx` (chave do nesting):
/// - `*idx` é um cursor ÚNICO sobre a fila, COMPARTILHADO por todos os níveis de
///   recursão. Cada comando consumido o incrementa exatamente uma vez. Assim a
///   fila inteira é percorrida em ordem de inserção, independentemente da
///   profundidade do aninhamento.
/// - Em `HorizontalBegin`: consome o `Begin`, abre um `ui.horizontal(|hui| ...)`
///   e CHAMA a si mesma com o `hui` (o Ui horizontal). A chamada interna
///   continua do mesmo `idx`, então os widgets seguintes saem LADO A LADO no
///   `hui` até ela encontrar o `HorizontalEnd` pareado, quando retorna.
/// - Em `HorizontalEnd`: consome o `End` e RETORNA, fechando o nível atual
///   (o closure do `ui.horizontal` termina e o layout horizontal é aplicado).
/// - No nível raiz, o laço só termina quando a fila acaba (um `End` órfão sem
///   `Begin` simplesmente fecha o laço raiz cedo — defensivo, sem panicar).
///
/// **Ordem dos resultados button/slider:** como `*idx` avança linearmente sobre
/// a MESMA fila que `widgets.rs` preencheu em ordem, e os `push` em `new_buttons`
/// /`new_sliders` acontecem na ordem em que os comandos são consumidos (mesmo
/// dentro dos horizontais), a N-ésima posição aqui corresponde ao N-ésimo
/// `button`/`slider` enfileirado — exatamente o que o cursor por posição em
/// `widgets.rs` espera.
fn drenar(
    ui: &mut egui::Ui,
    cmds: &[WidgetCmd],
    dom: Option<&crate::dom::Dom>,
    idx: &mut usize,
    new_buttons: &mut Vec<bool>,
    new_sliders: &mut Vec<f64>,
) {
    while *idx < cmds.len() {
        // Lê o comando ANTES de incrementar, para `HorizontalBegin` poder
        // recursar a partir do PRÓXIMO comando já com o cursor avançado.
        let cmd = &cmds[*idx];
        *idx += 1;
        match cmd {
            WidgetCmd::Label(text) => {
                ui.label(text);
            }
            WidgetCmd::Button(label) => {
                let clicked = ui.button(label).clicked();
                new_buttons.push(clicked);
            }
            WidgetCmd::Slider { value, min, max } => {
                let mut v = *value;
                ui.add(egui::Slider::new(&mut v, *min..=*max));
                new_sliders.push(v);
            }
            WidgetCmd::HorizontalBegin => {
                // Abre o escopo horizontal e continua a drenar DENTRO dele, do
                // mesmo `idx` — os widgets seguintes ficam lado a lado no `hui`
                // até o `HorizontalEnd` pareado fazer a recursão retornar.
                ui.horizontal(|hui| {
                    drenar(hui, cmds, dom, idx, new_buttons, new_sliders);
                });
            }
            WidgetCmd::HorizontalEnd => {
                // Fecha o nível horizontal atual: retorna ao chamador (o closure
                // do `ui.horizontal` do `Begin` pareado), encerrando o escopo.
                return;
            }
            WidgetCmd::Html => {
                // Conteúdo HTML: o render PERCORRE a árvore de DOM RETIDA em
                // `UiCtx::dom` (não uma fila plana). Self-contained — não consome
                // `idx` extra. Sem árvore (nenhum `html` ainda), não faz nada.
                if let Some(dom) = dom {
                    render_dom(ui, dom);
                }
            }
        }
    }
}

/// Estilo inline herdado (das tags inline registradas) ao descer na árvore.
#[derive(Clone, Copy, Default)]
struct InlineStyle {
    bold: bool,
    italic: bool,
    mono: bool,
}

/// Renderiza um `Dom` inteiro no `ui`: cada filho do `#document` é um bloco.
///
/// "Render em cima da árvore": a fonte da verdade é a hierarquia de nós, e o COMO
/// de cada tag vem do mapa `block::lookup`/`lookup_inline` (definido pelo TS),
/// não de nomes hardcodados. O Rust só aplica primitivos de layout.
fn render_dom(ui: &mut egui::Ui, dom: &crate::dom::Dom) {
    let root = dom.node(dom.root);
    let mut index = 0usize;
    for &child in &root.children {
        render_block(ui, dom, child, &mut index);
    }
}

/// Renderiza um nó em contexto de BLOCO. `index` é a posição entre irmãos de
/// bloco (usada para numerar itens de lista com `PREFIX_NUMBER`).
fn render_block(
    ui: &mut egui::Ui,
    dom: &crate::dom::Dom,
    id: crate::dom::NodeId,
    index: &mut usize,
) {
    use crate::dom::NodeKind;
    let tag = match &dom.node(id).kind {
        NodeKind::Element { tag } => tag.clone(),
        // Texto solto / não-elemento no nível de bloco: emite inline direto.
        _ => return render_inline(ui, dom, id, InlineStyle::default()),
    };

    // Tag sem layout de bloco registrado ⇒ inline transparente (default seguro,
    // igual a uma tag desconhecida): preserva o texto dos filhos.
    let Some(def) = crate::block::lookup(&tag) else {
        return render_inline(ui, dom, id, InlineStyle::default());
    };

    let this_index = *index;
    *index += 1;

    // Heading: texto concatenado; `indent` é reusado como TAMANHO de fonte.
    if def.has(crate::block::FLAG_HEADING) {
        let text = collect_text(dom, id);
        let size = if def.indent > 0.0 { def.indent } else { 20.0 };
        ui.heading(egui::RichText::new(text).strong().size(size));
        return;
    }

    // Recuo à esquerda (lista/blockquote) via `ui.indent`; senão renderiza direto.
    if def.indent > 0.0 {
        ui.indent(("blk", id), |ui| render_block_body(ui, dom, id, def, this_index));
    } else {
        render_block_body(ui, dom, id, def, this_index);
    }
}

/// Corpo de um bloco (já dentro do recuo): aplica o eixo (`display`) + o
/// marcador (`prefix`) e desce nos filhos.
fn render_block_body(
    ui: &mut egui::Ui,
    dom: &crate::dom::Dom,
    id: crate::dom::NodeId,
    def: crate::block::BlockDef,
    this_index: usize,
) {
    use crate::block::*;

    let prefix = match def.prefix {
        x if x == PREFIX_BULLET => Some("•  ".to_string()),
        x if x == PREFIX_NUMBER => Some(format!("{}.  ", this_index + 1)),
        _ => None,
    };
    let mono = def.has(FLAG_MONO);

    match def.display {
        // GRID: cada filho-elemento é uma linha; os netos são as células.
        x if x == DISPLAY_GRID => {
            egui::Grid::new(("grid", id)).striped(true).show(ui, |ui| {
                for &row in &dom.node(id).children {
                    if !matches!(dom.node(row).kind, crate::dom::NodeKind::Element { .. }) {
                        continue; // ignora texto solto entre linhas
                    }
                    for &cell in &dom.node(row).children {
                        render_block(ui, dom, cell, &mut 0);
                    }
                    ui.end_row();
                }
            });
        }
        // HORIZONTAL: filhos lado a lado, sem quebra (linha de tabela / flex-row).
        x if x == DISPLAY_HORIZONTAL => {
            ui.horizontal(|ui| {
                let mut i = 0usize;
                for &child in &dom.node(id).children {
                    render_block(ui, dom, child, &mut i);
                }
            });
        }
        // WRAP: flui inline (CSS inline-flow) — o parágrafo clássico.
        x if x == DISPLAY_WRAP => {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if let Some(p) = &prefix {
                    ui.label(egui::RichText::new(p).strong());
                }
                let st = InlineStyle { mono, ..Default::default() };
                for &child in &dom.node(id).children {
                    render_inline(ui, dom, child, st);
                }
            });
        }
        // VERTICAL (default block): empilha os filhos.
        _ => {
            ui.vertical(|ui| {
                if let Some(p) = &prefix {
                    ui.label(egui::RichText::new(p).strong());
                }
                let mut i = 0usize;
                for &child in &dom.node(id).children {
                    render_block(ui, dom, child, &mut i);
                }
            });
        }
    }
}

/// Renderiza um nó em contexto INLINE, herdando `style`. As tags inline e seu
/// estilo vêm do mapa `block::lookup_inline` (definido pelo TS) — o Rust não
/// nomeia nenhuma tag. Tag inline ausente do mapa é transparente (sem estilo).
fn render_inline(
    ui: &mut egui::Ui,
    dom: &crate::dom::Dom,
    id: crate::dom::NodeId,
    style: InlineStyle,
) {
    use crate::dom::NodeKind;
    match &dom.node(id).kind {
        NodeKind::Text(text) => {
            let mut rt = egui::RichText::new(text);
            if style.bold {
                rt = rt.strong();
            }
            if style.italic {
                rt = rt.italics();
            }
            if style.mono {
                rt = rt.monospace();
            }
            ui.label(rt);
        }
        NodeKind::Element { tag } => {
            // Liga os bits de estilo registrados para esta tag inline e desce.
            let flags = crate::block::lookup_inline(tag);
            let mut st = style;
            st.bold |= flags & crate::block::FLAG_BOLD != 0;
            st.italic |= flags & crate::block::FLAG_ITALIC != 0;
            st.mono |= flags & crate::block::FLAG_MONO != 0;
            for &child in &dom.node(id).children {
                render_inline(ui, dom, child, st);
            }
        }
        NodeKind::Document => {}
    }
}

/// Concatena o texto de todos os descendentes de `id` (em ordem de documento).
fn collect_text(dom: &crate::dom::Dom, id: crate::dom::NodeId) -> String {
    use crate::dom::NodeKind;
    let mut out = String::new();
    collect_text_into(dom, id, &mut out);
    return out;

    fn collect_text_into(dom: &crate::dom::Dom, id: crate::dom::NodeId, out: &mut String) {
        match &dom.node(id).kind {
            NodeKind::Text(t) => out.push_str(t),
            _ => {
                for &child in &dom.node(id).children {
                    collect_text_into(dom, child, out);
                }
            }
        }
    }
}

/// Render + present de um frame já tesselado. Separado para manter `endFrame`
/// curto e a regra das 500 linhas.
fn present(
    c: &mut ctx::UiCtx,
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
    let size = c.window.inner_size();
    let r = &mut c.render;
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

    {
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rts-egui pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.02,
                        g: 0.02,
                        b: 0.03,
                        a: 1.0,
                    }),
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
