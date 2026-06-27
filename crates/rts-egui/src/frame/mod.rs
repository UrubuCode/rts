//! Ciclo de frame (`beginFrame`/`endFrame`/`snapshot`) + a drenagem da fila de
//! widgets. O backend de GPU (wgpu/glow) vive em [`gpu`]; o render do DOM retido
//! em [`render`].
//!
//! `beginFrame` abre um pass do egui (`begin_pass`) e zera a fila de widgets.
//! `endFrame` drena a fila num `CentralPanel`, encerra o pass (`end_pass`),
//! tesselará as shapes e renderiza/apresenta o frame via o backend ativo.

mod gpu;
mod render;

// API pública do módulo `frame` (mantém o `pub use frame::*` do lib.rs): os tipos
// de GPU/janela (usados por app.rs/ctx.rs) e o render do DOM (usado abaixo).
pub use gpu::{Backend, GpuConfig, RenderState, WindowChrome};
use gpu::present_wgpu;
use render::render_dom;

use crate::ctx::{self, WidgetCmd};

/// Converte a cor própria do canvas (`u32` RGBA `0xRRGGBBAA`) para `Color32`.
/// (A mesma conversão que `render::rgba_to_color32` faz para o estilo do DOM.)
fn rgba32(c: u32) -> egui::Color32 {
    let r = ((c >> 24) & 0xFF) as u8;
    let g = ((c >> 16) & 0xFF) as u8;
    let b = ((c >> 8) & 0xFF) as u8;
    let a = (c & 0xFF) as u8;
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
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
        // Margem do painel ZERADA: TODO o espaçamento da borda deve vir do CSS
        // (`body { margin/padding }`), não do egui — senão há espaçamento duplo e
        // o DOM não controla o layout (a "borda à esquerda" que aparecia era a
        // `inner_margin` padrão do tema). O DOM/CSS é o dono do espaçamento.
        // Transparente → sem fundo (Frame::NONE); opaco → fundo do tema mas margem 0.
        let panel = if c.transparent {
            egui::CentralPanel::default().frame(egui::Frame::NONE)
        } else {
            let bg = c.egui_ctx.style().visuals.panel_fill;
            egui::CentralPanel::default().frame(egui::Frame::NONE.fill(bg))
        };
        #[allow(deprecated)]
        panel.show(&c.egui_ctx, |ui| {
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

        // Despacha a pintura pro backend ativo. `window` clonado (Arc, barato) p/
        // emprestar `c.backend` mutável sem colidir com `c.window`.
        let window = c.window.clone();
        match &mut c.backend {
            Backend::Wgpu(r) => present_wgpu(r, &window, paint_jobs, full_output.textures_delta, ppp),
            #[cfg(feature = "glow-backend")]
            Backend::Glow(g) => g.paint(&window, paint_jobs, full_output.textures_delta, ppp),
        }
    });
}

/// Agenda um snapshot do PRÓXIMO `endFrame` num arquivo PPM (teste visual
/// headless). Chame entre `beginFrame` e `endFrame`. Só o backend glow suporta
/// (lê o back buffer via `glReadPixels`); no wgpu é no-op com aviso (a janela
/// wgpu já é confirmada visualmente). O PPM serve p/ asserir que o frame não está
/// em branco — que o texto/widgets realmente pintaram.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_SNAPSHOT(h: u64, path_ptr: *const u8, path_len: i64) {
    let path = match unsafe { rts_engine::abi::str_abi::from_abi(path_ptr, path_len) } {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => return,
    };
    ctx::with_ctx(h, |c| match &mut c.backend {
        #[cfg(feature = "glow-backend")]
        Backend::Glow(g) => g.request_snapshot(path),
        _ => eprintln!("rts-egui snapshot: suportado só no backend glow (render: \"glow\")"),
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
            WidgetCmd::HtmlHandle(h) => {
                // Headless-puro: o DOM vive no `store` do rts-dom; o egui SÓ o lê e
                // pinta (empresta `&Dom` via `with_dom`, sem copiar nem deter). DOM
                // inválido (handle morto) → não pinta nada.
                rts_dom::store::with_dom(*h, |d| render_dom(ui, d));
            }
            // ── CANVAS BURRO: pinta em coords ABSOLUTAS via Painter ──────────────
            // Não participam do layout-flow (o TS já calculou a posição). O
            // `painter()` desenha no espaço da tela do `ui`.
            WidgetCmd::DrawRect { x, y, w, h, fill, stroke_w, stroke, radius } => {
                let rect = egui::Rect::from_min_size(
                    egui::pos2(*x, *y),
                    egui::vec2(*w, *h),
                );
                let stroke = if *stroke_w > 0.0 {
                    egui::Stroke::new(*stroke_w, rgba32(*stroke))
                } else {
                    egui::Stroke::NONE
                };
                ui.painter().rect(
                    rect,
                    egui::CornerRadius::same(*radius as u8),
                    rgba32(*fill),
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
            WidgetCmd::DrawText { x, y, text, color, size, flags } => {
                let family = if flags & crate::block::FLAG_MONO != 0 {
                    egui::FontFamily::Monospace
                } else {
                    egui::FontFamily::Proportional
                };
                ui.painter().text(
                    egui::pos2(*x, *y),
                    egui::Align2::LEFT_TOP,
                    text,
                    egui::FontId::new(*size, family),
                    rgba32(*color),
                );
            }
            WidgetCmd::DrawLine { x1, y1, x2, y2, w, color } => {
                ui.painter().line_segment(
                    [egui::pos2(*x1, *y1), egui::pos2(*x2, *y2)],
                    egui::Stroke::new(*w, rgba32(*color)),
                );
            }
            WidgetCmd::DrawImage { x, y, w, h, tex } => {
                let rect = egui::Rect::from_min_size(egui::pos2(*x, *y), egui::vec2(*w, *h));
                // `image` mapeia a textura inteira (uv 0..1) escalada no retângulo.
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter().image(tex.id(), rect, uv, egui::Color32::WHITE);
            }
        }
    }
}
