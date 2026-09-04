//! Render do DOM — agora SÓ PAINT. O LAYOUT (geometria x/y/w/h de cada nó) é
//! calculado pelo `rts-dom` (`rts_dom::layout`), que devolve uma `DisplayList`
//! plana; aqui apenas PERCORREMOS essa lista e pintamos via `ui.painter()`.
//!
//! Esta é a virada de 2026-06-27 ("processar tudo no DOM, o egui só lê e exibe").
//! O egui deixou de decidir layout (o antigo `ui.label`/`horizontal`/`Frame` foi
//! removido) — ele é um BACKEND DE PAINT trocável. A única coisa que o `rts-dom`
//! não faz sozinho é MEDIR texto (largura/altura de glifo); isso o egui fornece
//! via [`EguiMeasurer`], que implementa o trait `rts_dom::layout::TextMeasurer`
//! usando o sistema de fontes real do egui (galley) — então a medida é exata, não
//! aproximada, e mesmo assim o DOM continua dono do layout.

use rts_dom::layout::{self, DisplayItem, DisplayList, TextMeasurer};
use std::cell::RefCell;
use std::collections::HashMap;

/// Converte a cor própria do motor de estilo (`u32` RGBA `0xRRGGBBAA`, egui-free)
/// para o `Color32` do egui. A conversão vive AQUI (no backend), nunca no rts-dom.
fn rgba_to_color32(c: u32) -> egui::Color32 {
    let r = ((c >> 24) & 0xFF) as u8;
    let g = ((c >> 16) & 0xFF) as u8;
    let b = ((c >> 8) & 0xFF) as u8;
    let a = (c & 0xFF) as u8;
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Implementa a medição de texto do `rts-dom` usando o sistema de fontes REAL do
/// egui (não a aproximação do `ApproxMeasurer`). Mede largura via galley e usa a
/// altura de linha da fonte — assim o layout calculado no rts-dom bate com o que
/// o egui vai de fato pintar. Guarda o `Context` para consultar `fonts`.
struct EguiMeasurer<'a> {
    ctx: &'a egui::Context,
}

thread_local! {
    /// Métricas persistentes entre relayouts. O contexto faz parte da chave para não
    /// misturar fontes de janelas egui diferentes; o limite evita crescimento infinito.
    static TEXT_WIDTH_CACHE: RefCell<HashMap<(usize, u32, bool, bool, bool), HashMap<String, f32>>> =
        RefCell::new(HashMap::new());
    static LINE_HEIGHT_CACHE: RefCell<HashMap<(usize, u32), f32>> =
        RefCell::new(HashMap::new());
}

mod medida;
mod pintura;
mod scroll;

use pintura::paint_list;
use scroll::process_scroll_regions;


// Os dois caches de DisplayList que viviam aqui foram para o `rts-dom`
// (`layout::layout_cached`): eram a mesma pergunta — "o layout mudou?" — feita
// duas vezes neste crate, e nenhuma das duas servia o caminho HEADLESS, que
// chamava `layout_document` por consulta de geometria.

/// Renderiza um `Dom` inteiro: reutiliza ou calcula o layout (rts-dom) e PINTA a display list.
///
/// A origem do conteúdo é o canto superior-esquerdo da área do `ui`
/// (`ui.max_rect().min`); cada item da lista vem em coordenadas de conteúdo e é
/// transladado por essa origem ao pintar. O `ui` é avançado por `allocate_space`
/// na altura total do conteúdo (para o layout do egui ao redor — scroll, etc —
/// saber o tamanho ocupado).
pub(crate) fn render_dom(ui: &mut egui::Ui, dom: &crate::dom::Dom) {
    // FASE: o frame de render de um DOM, do ponto de vista do backend. O layout
    // tem fase própria (dentro do rts-dom) e o `paint` é o que sobra — é assim
    // que "o frame está lento" vira "o layout é 80% dele" ou "não é".
    let _phase = rts_dom::metrics::phases::scope("render-dom");
    let avail = ui.available_size();
    let viewport_w = avail.x.max(1.0);
    let viewport_h = ui.ctx().screen_rect().height().max(1.0);
    let measurer = EguiMeasurer { ctx: ui.ctx() };
    let ctx = layout::LayoutCtx { viewport_w, viewport_h, measurer: &measurer };
    let list = layout::layout_cached(dom, &ctx);
    // DUMP DO RENDER (`RTS_DOM_PAINT=1`): o que o backend receberia neste frame.
    //
    // Existe porque "a página está branca" tem causas que a tela não distingue:
    // sem geometria, fora da viewport, coberta, ou — o caso real — um frame tão
    // caro que quase nunca termina. Isto responde a pergunta em números.
    if std::env::var_os("RTS_DOM_PAINT").is_some() {
        let (mut itens, mut clips, mut vazios, mut profundidade, mut sobra) = (0usize, 0usize, 0usize, 0i32, 0i32);
        list.walk(|item, _, _| {
            itens += 1;
            match item {
                layout::DisplayItem::BeginClip { rect, .. } => {
                    clips += 1;
                    profundidade += 1;
                    sobra = sobra.max(profundidade);
                    if rect.w <= 0.0 || rect.h <= 0.0 {
                        vazios += 1;
                    }
                }
                layout::DisplayItem::EndClip { .. } => profundidade -= 1,
                _ => {}
            }
        });
        eprintln!(
            "[paint] itens={itens} clips={clips} (vazios={vazios}, aninhamento máx={sobra}, desequilíbrio={profundidade}) content_h={} canvas=#{:08X} viewport={:?}",
            list.content_height,
            list.canvas_background,
            ui.max_rect()
        );
    }
    // O CANVAS primeiro: é a cor que o `<body>`/`<html>` propaga, e branco
    // quando a página não define nenhuma — o que um browser pinta. Sem isto
    // ficava a cor de limpeza do backend (quase preta) por trás, e uma página
    // cujo estilo mora num `<link>` externo saía preto sobre preto.
    let canvas = if std::env::var_os("RTS_DOM_CANVAS_DEBUG").is_some() { 0xFF0000FF } else { list.canvas_background };
    if canvas != 0 {
        let rect = egui::Rect::from_min_size(
            ui.max_rect().min,
            egui::vec2(viewport_w, viewport_h.max(list.content_height)),
        );
        ui.painter().rect_filled(rect, 0.0, rgba_to_color32(canvas));
    }
    paint_list(ui, &list, 0.0, dom);
    // reserva a altura total ocupada (p/ o egui ao redor dimensionar).
    ui.allocate_space(egui::vec2(ui.available_width(), list.content_height));
}

/// Transforma eventos neutros de `rts-input` em eventos crus do DOM. A captura
/// fica no backend activo, mas não invoca callbacks: a fachada TypeScript faz o
/// dispatch e o bubbling junto com os eventos de mouse.
fn emit_keyboard_events(h: u64) {
    let events = rts_input::with_input(|input| input.keyboard_events(h)).unwrap_or_default();
    if events.is_empty() {
        return;
    }
    let _ = rts_dom::store::with_dom_mut(h, |dom| {
        for event in events {
            dom.push_raw_keyboard_event(
                event.key_code,
                event.pressed,
                event.repeat,
                event.modifiers.ctrl,
                event.modifiers.shift,
                event.modifiers.alt,
                event.modifiers.cmd,
            );
        }
    });
}

/// Enfileira os dados de edição capturados pelo backend. O `Text`/`Paste` e o
/// commit IME atravessam a mesma fila; a fachada decide `beforeinput` → mutação
/// → `input`, evitando que um listener seja contornado pelo renderer.
fn emit_input_events(h: u64) {
    let (compositions, text) = rts_input::with_input(|input| {
        (input.composition_events(h), input.text_input(h))
    })
    .unwrap_or_default();
    if compositions.is_empty() && text.is_empty() {
        return;
    }
    let _ = rts_dom::store::with_dom_mut(h, |dom| {
        for event in compositions {
            match event {
                rts_input::CompositionEvent::Start => {
                    dom.push_raw_composition_event(2, String::new())
                }
                rts_input::CompositionEvent::Update(value) => {
                    dom.push_raw_composition_event(3, value)
                }
                rts_input::CompositionEvent::End(value) => {
                    dom.push_raw_composition_event(4, value)
                }
                rts_input::CompositionEvent::Disabled => {
                    dom.push_raw_composition_event(5, String::new())
                }
            }
        }
        if !text.is_empty() {
            dom.push_raw_text_input(text);
        }
    });
}

/// Renderiza o DOM COM SCROLL — o egui burro: mantém só o offset (input do mouse),
/// translada o conteúdo por -offset e pinta. A BARRA (track+thumb) é emitida pelo
/// DOM (`layout::emit_scrollbar`) como `SolidRect` — NÃO usa o ScrollArea do egui,
/// p/ a barra não ficar presa ao backend (visão: egui removível). `h` é o handle do
/// DOM; `sb` o estilo do CSS; `scroll_y` se o eixo Y rola; `force` se a barra é
/// sempre visível (overflow:scroll).
pub(crate) fn render_dom_scrolled(
    ui: &mut egui::Ui,
    h: u64,
    sb: &rts_dom::scrollbar::ScrollbarStyle,
    scroll_y: bool,
    force: bool,
) {
    let _phase = rts_dom::metrics::phases::scope("render-dom");
    emit_keyboard_events(h);
    emit_input_events(h);
    let avail = ui.available_size();
    let viewport_w = avail.x.max(1.0);
    let viewport_h = avail.y.max(1.0);
    // layout (com a barra ainda não — precisa do content_h primeiro).
    // CACHE por revisão: o modo imediato repinta a cada frame/clique, mas o layout
    // (cascade de todas as regras × nós — numa página Bootstrap ~2700 regras) só
    // precisa re-rodar quando o DOM/estilo MUDAM (`render_revision`) ou o viewport
    // muda. Era a "travada" ao clicar: re-layout completo por frame.
    let measurer = EguiMeasurer { ctx: ui.ctx() };
    let lctx = layout::LayoutCtx { viewport_w, viewport_h, measurer: &measurer };
    // A barra e o offset são aplicados SOBRE a lista, então esta cópia é
    // necessária — mas ela agora parte de uma lista cacheada pelo próprio DOM.
    let mut list = rts_dom::store::with_dom(h, |d| (*layout::layout_cached(d, &lctx)).clone())
        .unwrap_or_default();
    let content_h = list.content_height;

    // OFFSET de scroll da PÁGINA: vive no `Dom` (`dom/scroll.rs`), não mais em
    // `ui.ctx().memory()` — finding 3 da auditoria estrutural (o offset era
    // invisível e incontrolável a partir de JS). O egui só ACUMULA o input
    // (roda do rato, arrastar a barra) igual a antes; a diferença é onde lê o
    // valor de partida e para onde escreve o resultado. `id` continua a
    // existir só como identidade de INTERAÇÃO da barra (drag), não mais como
    // chave de armazenamento. Limita a [0, content_h - viewport_h].
    let max_off = (content_h - viewport_h).max(0.0);
    let id = egui::Id::new(("rts_dom_scroll", h));
    let (page_x, mut offset) =
        rts_dom::store::with_dom(h, |d| d.page_scroll()).unwrap_or((0.0, 0.0));
    if scroll_y && (max_off > 0.0 || force) {
        // a roda do mouse só conta quando o ponteiro está sobre a área do DOM.
        let hovered = ui.rect_contains_pointer(ui.max_rect());
        if hovered {
            let dy = ui.input(|i| i.smooth_scroll_delta.y);
            offset -= dy; // roda p/ cima (dy>0) sobe o conteúdo (offset menor)
        }

        // ARRASTAR a barra: a mesma geometria do thumb que o `emit_scrollbar` usa.
        // Clicar/puxar na faixa da barra mapeia a posição do mouse → offset. O input
        // (clique/drag) é legítimo do backend; o resultado vira o nosso `offset`.
        if max_off > 0.0 {
            let bar_w = match sb.width {
                Some(rts_dom::scrollbar::BarWidth::Thin) => 8.0,
                Some(rts_dom::scrollbar::BarWidth::Px(px)) => px,
                _ => 12.0,
            };
            let origin = ui.max_rect().min;
            let bar_rect = egui::Rect::from_min_size(
                origin + egui::vec2(viewport_w - bar_w, 0.0),
                egui::vec2(bar_w, viewport_h),
            );
            // área interativa da barra (resposta a clique/drag).
            let resp = ui.interact(bar_rect, id.with("bar"), egui::Sense::click_and_drag());
            let frac = (viewport_h / content_h).clamp(0.0, 1.0);
            let thumb_h = (viewport_h * frac).max(24.0);
            if let Some(pos) = resp.interact_pointer_pos() {
                if resp.is_pointer_button_down_on() || resp.dragged() {
                    // centraliza o thumb no ponteiro: y do mouse → fração → offset.
                    let local_y = (pos.y - origin.y - thumb_h / 2.0).max(0.0);
                    let track_span = (viewport_h - thumb_h).max(1.0);
                    offset = (local_y / track_span).clamp(0.0, 1.0) * max_off;
                }
            }
        }
    }
    offset = offset.clamp(0.0, max_off);
    // Escreve de volta no `Dom` — só em resposta a input, nunca guardado "para
    // si" (a mesma disciplina de `set_hovered`). `_extent`: este frame já
    // correu `layout_cached` com o medidor REAL para pintar, então o teto
    // (`max_off`) já está em mãos; pedir um segundo layout aqui só para
    // clampar pagaria o documento inteiro a cada tick da roda do rato (ver a
    // nota de topo de `dom/scroll.rs`). `page_x` não é tocado por este
    // backend (só rola Y); passa por igual para não apagar um valor que o
    // bridge (`window.scrollTo`) tenha escrito.
    let _ = rts_dom::store::with_dom_mut(h, |d| d.set_page_scroll_extent(page_x, offset, max_off));

    // BARRA emitida pelo DOM (SolidRect) — fixa na viewport (a função soma o offset).
    if scroll_y {
        layout::emit_scrollbar(&mut list, viewport_w, viewport_h, content_h, offset, sb, force);
    }
    // SCROLL CONTAINERS INTERNOS (#1744): para cada região rolável (div com overflow),
    // o egui lê/escreve o offset dela no `Dom` (`dom/scroll.rs`) e emite as
    // barras dela — não mais injeta o offset na `DisplayList` (`paint_list`
    // volta a perguntar ao `Dom`, ver a nota de topo de `scroll.rs`). O
    // `base_origin` desloca o page-scroll p/ casar com o paint (que usa -offset).
    process_scroll_regions(ui, h, &mut list, sb, -offset);
    // CANVAS da página: a cor vem do `rts-dom` (`DisplayList::canvas_background`),
    // que já resolve a propagação do `<body>`/`<html>` e o branco por omissão.
    // Perguntar aqui de novo era a MESMA regra escrita duas vezes, e as duas
    // discordavam: esta procurava um `<body>` que uma página sem a tag não tem.
    {
        let [r, g, bl, a] = list.canvas_background.to_be_bytes();
        ui.painter().rect_filled(ui.max_rect(), 0.0, egui::Color32::from_rgba_unmultiplied(r, g, bl, a));
    }
    // pinta tudo transladado por -offset (o conteúdo sobe; a barra, somando offset na
    // emissão, fica parada na tela). Recorta na área visível.
    if std::env::var_os("RTS_DOM_PAINT").is_some() {
        let mut n = 0usize;
        list.walk(|_, _, _| n += 1);
        eprintln!("[paint] itens={n} content_h={content_h} offset={offset} max_rect={:?}", ui.max_rect());
    }
    let clip = ui.max_rect();
    let old_clip = ui.clip_rect();
    ui.set_clip_rect(clip);
    // `paint_list` lê o offset AO VIVO de cada `BeginClip` no `Dom` (não do
    // campo gravado no item, que pode vir de um fragmento reusado do cache) —
    // por isso precisa do empréstimo, não só da `list` (já uma cópia própria).
    let _ = rts_dom::store::with_dom(h, |d| paint_list(ui, &list, -offset, d));
    ui.set_clip_rect(old_clip);

    // HIT-TEST de CLIQUE (north-star §3 + handoff #1793 item 6): o egui é só o
    // INPUT — converte tela→conteúdo (origem + offset de scroll) e pergunta ao
    // MOTOR (`DisplayList::hit_test`, menor-área = nó mais profundo) qual nó foi
    // clicado; o resultado vira um evento CRU na fila do Dom, que a fachada TS
    // drena por frame (`pumpEventCallbacks` → bubbling + callbacks). O egui nunca
    // decide semântica de evento nem invoca handler (padrão 1-frame-latency).
    // Input CRU (não `ui.interact` — criaria um widget por cima e roubaria o drag
    // da scrollbar registrada acima). A faixa da barra é excluída do hit-test.
    {
        let origin = ui.max_rect().min;
        let clicked = ui.input(|i| i.pointer.primary_clicked());
        let pos = ui.input(|i| i.pointer.interact_pos());
        // `:hover` VIVO: informa por frame o nó sob o cursor (hit-test do motor).
        // O `set_hovered` só invalida caches quando MUDA e há regra :hover — mover
        // o mouse numa página sem :hover custa zero re-layout.
        {
            let hover_pos = ui.input(|i| i.pointer.hover_pos());
            let hovered_idx = hover_pos
                .filter(|p| ui.max_rect().contains(*p))
                .and_then(|p| list.hit_test(p.x - origin.x, p.y - origin.y + offset));
            let _ = rts_dom::store::with_dom_mut(h, |d| d.set_hovered(hovered_idx));
        }
        if clicked {
            if let Some(pos) = pos {
                if ui.max_rect().contains(pos) {
                    let bar_w = if scroll_y && (max_off > 0.0 || force) {
                        match sb.width {
                            Some(rts_dom::scrollbar::BarWidth::Thin) => 8.0,
                            Some(rts_dom::scrollbar::BarWidth::Px(px)) => px,
                            _ => 12.0,
                        }
                    } else {
                        0.0
                    };
                    let cx = pos.x - origin.x;
                    let cy = pos.y - origin.y + offset; // tela → coords de conteúdo
                    if cx < viewport_w - bar_w {
                        if let Some(idx) = list.hit_test(cx, cy) {
                            let _ = rts_dom::store::with_dom_mut(h, |d| {
                                d.push_raw_event(idx, "click");
                            });
                        }
                    }
                }
            }
        }
    }
    // reserva a área visível (não a altura total — o scroll é nosso, não do egui).
    ui.allocate_space(egui::vec2(viewport_w, viewport_h));
}
