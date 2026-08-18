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
    static TEXT_WIDTH_CACHE: RefCell<HashMap<(usize, u32, bool, bool), HashMap<String, f32>>> =
        RefCell::new(HashMap::new());
    static LINE_HEIGHT_CACHE: RefCell<HashMap<(usize, u32), f32>> =
        RefCell::new(HashMap::new());
}

impl<'a> EguiMeasurer<'a> {
    /// A família egui p/ (mono, bold): bold vence (família nomeada "bold"); senão
    /// mono → Monospace; senão Proportional. Casa medição com pintura.
    fn family(mono: bool, bold: bool) -> egui::FontFamily {
        if bold {
            egui::FontFamily::Name("bold".into())
        } else if mono {
            egui::FontFamily::Monospace
        } else {
            egui::FontFamily::Proportional
        }
    }
    fn font_id(size: f32, mono: bool, bold: bool) -> egui::FontId {
        egui::FontId::new(size, Self::family(mono, bold))
    }
}

impl<'a> TextMeasurer for EguiMeasurer<'a> {
    /// O `Context` do egui identifica as fontes; o `pixels_per_point` identifica
    /// a escala, e mudar o zoom MUDA a largura do texto. Este medidor é
    /// construído na pilha a cada frame, então o endereço dele não serve — ver a
    /// nota em `TextMeasurer::identity`.
    fn identity(&self) -> u64 {
        let context = self.ctx as *const egui::Context as usize as u64;
        context ^ ((self.ctx.pixels_per_point().to_bits() as u64) << 32)
    }

    fn text_width(&self, text: &str, size: f32, mono: bool, bold: bool) -> f32 {
        let context_key = self.ctx as *const egui::Context as usize;
        let font_key = (context_key, size.to_bits(), mono, bold);
        if let Some(width) = TEXT_WIDTH_CACHE.with(|cache| {
            cache
                .borrow()
                .get(&font_key)
                .and_then(|bucket| bucket.get(text))
                .copied()
        }) {
            return width;
        }
        let font = Self::font_id(size, mono, bold);
        // `fonts_mut` dá um `&mut FontsView` (glyph_width exige `&mut`).
        let width = self.ctx.fonts_mut(|f| text.chars().map(|c| f.glyph_width(&font, c)).sum());
        TEXT_WIDTH_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            let bucket = cache.entry(font_key).or_default();
            if bucket.len() >= 8192 && !bucket.contains_key(text) {
                if let Some(old_text) = bucket.keys().next().cloned() {
                    bucket.remove(&old_text);
                }
            }
            bucket.insert(text.to_owned(), width);
        });
        width
    }
    fn line_height(&self, size: f32) -> f32 {
        let key = (self.ctx as *const egui::Context as usize, size.to_bits());
        if let Some(height) = LINE_HEIGHT_CACHE.with(|cache| cache.borrow().get(&key).copied()) {
            return height;
        }
        let font = Self::font_id(size, false, false);
        let height = self.ctx.fonts_mut(|f| f.row_height(&font));
        LINE_HEIGHT_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() >= 256 && !cache.contains_key(&key) {
                if let Some(old_key) = cache.keys().next().copied() {
                    cache.remove(&old_key);
                }
            }
            cache.insert(key, height);
        });
        height
    }
}


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
    paint_list(ui, &list, 0.0);
    // reserva a altura total ocupada (p/ o egui ao redor dimensionar).
    ui.allocate_space(egui::vec2(ui.available_width(), list.content_height));
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

    // OFFSET de scroll: estado por-handle no egui (input é do backend). Acumula a roda
    // do mouse; limita a [0, content_h - viewport_h].
    let max_off = (content_h - viewport_h).max(0.0);
    let id = egui::Id::new(("rts_dom_scroll", h));
    let mut offset = ui.ctx().memory(|m| m.data.get_temp::<f32>(id).unwrap_or(0.0));
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
    ui.ctx().memory_mut(|m| m.data.insert_temp(id, offset));

    // BARRA emitida pelo DOM (SolidRect) — fixa na viewport (a função soma o offset).
    if scroll_y {
        layout::emit_scrollbar(&mut list, viewport_w, viewport_h, content_h, offset, sb, force);
    }
    // SCROLL CONTAINERS INTERNOS (#1744): para cada região rolável (div com overflow),
    // o egui gerencia seu offset (input), injeta no BeginClip e emite as barras dela.
    // O `base_origin` desloca o page-scroll p/ casar com o paint (que usa -offset).
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
    paint_list(ui, &list, -offset);
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

/// Processa os SCROLL CONTAINERS internos (#1744): para cada `ScrollRegion`, lê/
/// atualiza o offset (roda do mouse quando o ponteiro está sobre a div), injeta esse
/// offset no `BeginClip` correspondente (p/ o paint transladar os filhos) e emite as
/// barras (x/y) DENTRO da região via `emit_scrollbar_in`. `page_dy` é a translação do
/// scroll da página (p/ posicionar a região na tela). Egui burro: só input + dados.
fn process_scroll_regions(
    ui: &mut egui::Ui,
    h: u64,
    list: &mut layout::DisplayList,
    sb: &rts_dom::scrollbar::ScrollbarStyle,
    page_dy: f32,
) {
    // As regiões roláveis podem ter vindo de uma subárvore REUSADA: a lista
    // guarda as próprias, e a `geometry()` junta as das subárvores.
    let geometria = list.geometry();
    if geometria.scroll_regions.is_empty() {
        return;
    }
    let base = ui.max_rect().min;
    let regions = geometria.scroll_regions.clone();
    for region in &regions {
        let max_x = (region.content_w - region.visible.w).max(0.0);
        let max_y = (region.content_h - region.visible.h).max(0.0);
        let can_x = region.overflow_x.scrollable() && max_x > 0.0;
        let can_y = region.overflow_y.scrollable() && max_y > 0.0;
        if !can_x && !can_y {
            continue;
        }
        // offset por-nó em memory.
        let oid = egui::Id::new(("rts_dom_region", h, region.node_idx));
        let mut off = ui.ctx().memory(|m| m.data.get_temp::<egui::Vec2>(oid).unwrap_or_default());
        // rect da região na TELA (visible + page scroll).
        let screen = egui::Rect::from_min_size(
            base + egui::vec2(region.visible.x, region.visible.y + page_dy),
            egui::vec2(region.visible.w, region.visible.h),
        );
        if ui.rect_contains_pointer(screen) {
            let d = ui.input(|i| i.smooth_scroll_delta);
            // se rola Y, a roda move Y; se SÓ rola X, a roda (Y) move X (UX comum).
            if can_y {
                off.y -= d.y;
            }
            if can_x {
                off.x -= if can_y { d.x } else { d.y };
            }
        }

        // ARRASTAR as barras da DIV (clicar e puxar). Geometria igual à emit_scrollbar_in:
        // barra-Y na borda direita, barra-X na borda inferior. A posição do mouse na
        // faixa da barra → fração → offset.
        let bar_w = match sb.width {
            Some(rts_dom::scrollbar::BarWidth::Thin) => 8.0,
            Some(rts_dom::scrollbar::BarWidth::Px(px)) => px,
            _ => 12.0,
        };
        let v = region.visible;
        let sx = base.x + v.x;
        let sy = base.y + v.y + page_dy;
        if can_y {
            let track_h = if can_x { v.h - bar_w } else { v.h };
            let bar_rect = egui::Rect::from_min_size(
                egui::pos2(sx + v.w - bar_w, sy),
                egui::vec2(bar_w, track_h),
            );
            let resp = ui.interact(bar_rect, oid.with("bar_y"), egui::Sense::click_and_drag());
            if let Some(p) = resp.interact_pointer_pos() {
                if resp.is_pointer_button_down_on() || resp.dragged() {
                    let frac = (track_h / region.content_h).clamp(0.0, 1.0);
                    let thumb_h = (track_h * frac).max(24.0);
                    let local = (p.y - sy - thumb_h / 2.0).max(0.0);
                    off.y = (local / (track_h - thumb_h).max(1.0)).clamp(0.0, 1.0) * max_y;
                }
            }
        }
        if can_x {
            let track_w = if can_y { v.w - bar_w } else { v.w };
            let bar_rect = egui::Rect::from_min_size(
                egui::pos2(sx, sy + v.h - bar_w),
                egui::vec2(track_w, bar_w),
            );
            let resp = ui.interact(bar_rect, oid.with("bar_x"), egui::Sense::click_and_drag());
            if let Some(p) = resp.interact_pointer_pos() {
                if resp.is_pointer_button_down_on() || resp.dragged() {
                    let frac = (track_w / region.content_w).clamp(0.0, 1.0);
                    let thumb_w = (track_w * frac).max(24.0);
                    let local = (p.x - sx - thumb_w / 2.0).max(0.0);
                    off.x = (local / (track_w - thumb_w).max(1.0)).clamp(0.0, 1.0) * max_x;
                }
            }
        }
        off.x = off.x.clamp(0.0, max_x);
        off.y = off.y.clamp(0.0, max_y);
        ui.ctx().memory_mut(|m| m.data.insert_temp(oid, off));

        // injeta o offset no BeginClip desta região (acha pelo node).
        // Mutar um item exige a lista PLANA: um `BeginClip` pode estar dentro de
        // uma subárvore compartilhada, e escrever nele afetaria todos os nós que
        // a reusam.
        list.materialize();
        for it in list.items.iter_mut() {
            if let layout::DisplayItem::BeginClip { node, offset_x, offset_y, .. } = it {
                if *node == region.node_idx {
                    *offset_x = off.x;
                    *offset_y = off.y;
                    break;
                }
            }
        }
        // barras DENTRO da região (coords de conteúdo; o paint soma o page scroll).
        layout::emit_scrollbar_in(list, region, off.x, off.y, sb);
    }
}

/// Percorre a [`DisplayList`] e pinta cada item via `ui.painter()`, em coordenadas
/// absolutas (conteúdo + origem do `ui`). A ordem da lista É o z-order (o que vem
/// depois pinta por cima). Reserva o espaço da altura total para o `ui` pai.
fn paint_list(ui: &mut egui::Ui, list: &DisplayList, offset_y: f32) {
    let _phase = rts_dom::metrics::phases::scope("paint");
    // origem do conteúdo + a translação de scroll da PÁGINA (offset_y negativo sobe).
    let base_origin = ui.max_rect().min + egui::vec2(0.0, offset_y);
    // PILHA para o scroll container interno (#1744): cada BeginClip empilha (painter
    // recortado, offset extra da região); EndClip desempilha. O item é pintado com o
    // painter do topo e a SOMA dos offsets extra (a região rolada). Base = ui.
    let base = ui.painter().clone();
    let mut stack: Vec<(egui::Painter, egui::Vec2)> = Vec::new();
    // `walk` anda a ÁRVORE de fragmentos: os itens de uma subárvore reusada
    // chegam aqui sem nunca terem sido copiados, com o deslocamento a somar — e
    // somar uma origem já era o que este laço fazia.
    // CULLING: a área realmente visível. Um item inteiramente fora dela não é
    // pintado — o egui pagaria a construção do galley de cada texto, e uma
    // página real tem duas ordens de grandeza mais texto do que cabe na tela.
    //
    // Sem isto a Wikipédia mandava 30 093 textos por frame para pintar os ~52
    // que se veem, e o frame demorava tanto que a janela ficava BRANCA: não é
    // que não pintasse, é que quase nunca chegava ao fim. Redimensionar, que
    // repinta a cada evento, ficava impraticável pela mesma razão.
    let visivel = ui.clip_rect().intersect(ui.max_rect());
    let diagnostico = std::env::var_os("RTS_DOM_PAINT").is_some();
    let (mut vistos, mut cortados) = (0usize, 0usize);
    let mut idx = 0usize;
    list.walk(|item, dx, dy| {
        idx += 1;
        let idx = idx - 1;
        let (painter, extra) = stack
            .last()
            .map(|(p, o)| (p.clone(), *o))
            .unwrap_or_else(|| (base.clone(), egui::Vec2::ZERO));
        // origem da página + translação da região + deslocamento do fragmento
        let origin = base_origin + extra + egui::vec2(dx, dy);
        // O que está fora da tela não é pintado — menos o `BeginClip`/`EndClip`,
        // que são ESTADO da pilha e têm de continuar a casar (saltar um abre um
        // clip que nunca fecha, e aí desaparece o que vinha depois).
        if let Some(caixa) = caixa_do_item(item, origin) {
            if !caixa.intersects(visivel) {
                cortados += 1;
                return;
            }
            vistos += 1;
            if diagnostico && vistos <= 16 {
                let tipo = match item {
                    DisplayItem::Text { text, color, .. } =>
                        format!("txt cor=#{color:08X} {:?}", text.chars().take(18).collect::<String>()),
                    DisplayItem::SolidRect { color, .. } => format!("rect cor=#{color:08X}"),
                    DisplayItem::Border { color, .. } => format!("borda cor=#{color:08X}"),
                    _ => "?".to_owned(),
                };
                eprintln!("  [{vistos}] {tipo} caixa={caixa:?} clip_painter={:?}", painter.clip_rect());
            }
        }
        match item {
            DisplayItem::SolidRect { rect, color, radius } => {
                let r = egui::Rect::from_min_size(
                    origin + egui::vec2(rect.x, rect.y),
                    egui::vec2(rect.w, rect.h),
                );
                painter.rect_filled(r, egui::CornerRadius::same(*radius as u8), rgba_to_color32(*color));
            }
            DisplayItem::Border { rect, width, color, radius } => {
                let r = egui::Rect::from_min_size(
                    origin + egui::vec2(rect.x, rect.y),
                    egui::vec2(rect.w, rect.h),
                );
                painter.rect_stroke(
                    r,
                    egui::CornerRadius::same(*radius as u8),
                    egui::Stroke::new(*width, rgba_to_color32(*color)),
                    egui::StrokeKind::Inside,
                );
            }
            DisplayItem::Text { x, y, text, color, size, mono, bold, letter_spacing, decoration } => {
                // bold vence (família "bold"); senão mono → Monospace; senão Proportional.
                let family = if *bold {
                    egui::FontFamily::Name("bold".into())
                } else if *mono {
                    egui::FontFamily::Monospace
                } else {
                    egui::FontFamily::Proportional
                };
                let font = egui::FontId::new(*size, family);
                let col = rgba_to_color32(*color);
                let base = origin + egui::vec2(*x, *y);
                let total_w = if *letter_spacing != 0.0 {
                    // letter-spacing: pinta char a char, avançando pela largura do glifo
                    // + o espaçamento. Devolve a largura total (p/ a linha de decoração).
                    let mut cx = base.x;
                    for ch in text.chars() {
                        let s = ch.to_string();
                        let gw = painter
                            .ctx()
                            .fonts_mut(|f| f.glyph_width(&font, ch))
                            .max(0.0);
                        painter.text(
                            egui::pos2(cx, base.y),
                            egui::Align2::LEFT_TOP,
                            &s,
                            font.clone(),
                            col,
                        );
                        cx += gw + *letter_spacing;
                    }
                    cx - base.x
                } else {
                    let g = painter.text(base, egui::Align2::LEFT_TOP, text, font.clone(), col);
                    g.width()
                };
                // decoração: linha sob/sobre/cortando o texto (1=under, 2=through, 3=over).
                if *decoration != 0 {
                    let ly = match decoration {
                        2 => base.y + *size * 0.5,  // line-through (meio)
                        3 => base.y + *size * 0.05, // overline (topo)
                        _ => base.y + *size * 0.92, // underline (base)
                    };
                    let thick = (*size * 0.06).max(1.0);
                    painter.line_segment(
                        [egui::pos2(base.x, ly), egui::pos2(base.x + total_w, ly)],
                        egui::Stroke::new(thick, col),
                    );
                }
            }
            DisplayItem::Shadow { rect, dx, dy, blur, spread, color, radius } => {
                // box-shadow: um retângulo deslocado (dx,dy), crescido pelo spread, com
                // borda amaciada pelo blur (feathering do egui). Pintado ANTES da caixa.
                let r = egui::Rect::from_min_size(
                    origin + egui::vec2(rect.x + *dx - *spread, rect.y + *dy - *spread),
                    egui::vec2(rect.w + 2.0 * *spread, rect.h + 2.0 * *spread),
                );
                let shadow = egui::epaint::Shadow {
                    offset: [0, 0],
                    blur: blur.max(0.0) as u8,
                    spread: 0,
                    color: rgba_to_color32(*color),
                };
                let shape = shadow.as_shape(r, egui::CornerRadius::same(*radius as u8));
                painter.add(shape);
            }
            DisplayItem::GradientRect { rect, c0, c1, angle_deg, .. } => {
                // gradiente linear: mesh de 4 vértices, cada canto com a cor interpolada
                // conforme sua projeção no eixo do ângulo (convenção CSS: 0=cima,
                // 90=direita). Aproxima o linear-gradient de 2 cores.
                let r = egui::Rect::from_min_size(
                    origin + egui::vec2(rect.x, rect.y),
                    egui::vec2(rect.w, rect.h),
                );
                paint_linear_gradient(&painter, r, *c0, *c1, *angle_deg);
            }
            DisplayItem::Image { rect, pixels_handle, pixels_off, img_w, img_h } => {
                // lê os RGBA8 do Buffer no HandleTable, sobe como textura efêmera e
                // pinta no rect (escalando). O decode/download já aconteceram no .ts.
                let need = (*img_w as usize) * (*img_h as usize) * 4;
                let rgba: Option<Vec<u8>> =
                    crate::pixels::fetch(*pixels_handle, u64::from(*pixels_off), need);
                if let Some(bytes) = rgba {
                    let ci = egui::ColorImage::from_rgba_unmultiplied(
                        [*img_w as usize, *img_h as usize],
                        &bytes,
                    );
                    let tex = painter.ctx().load_texture(
                        format!("__rts_domimg_{}_{}", pixels_handle, idx),
                        ci,
                        egui::TextureOptions::LINEAR,
                    );
                    let r = egui::Rect::from_min_size(
                        origin + egui::vec2(rect.x, rect.y),
                        egui::vec2(rect.w, rect.h),
                    );
                    painter.image(
                        tex.id(),
                        r,
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
            }
            DisplayItem::Pixels { rect, data, w, h } => {
                // Os bytes vêm DENTRO da lista (um `<canvas>` que o programa
                // pintou), então não há handle a resolver — sobe direto como
                // textura efêmera. O nome da textura inclui o índice do item
                // para que dois canvas no mesmo frame não disputem a mesma.
                let ci = egui::ColorImage::from_rgba_unmultiplied(
                    [*w as usize, *h as usize],
                    data,
                );
                let tex = painter.ctx().load_texture(
                    format!("__rts_canvas_{idx}"),
                    ci,
                    egui::TextureOptions::LINEAR,
                );
                let r = egui::Rect::from_min_size(
                    origin + egui::vec2(rect.x, rect.y),
                    egui::vec2(rect.w, rect.h),
                );
                painter.image(
                    tex.id(),
                    r,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            DisplayItem::BeginClip { rect, offset_x, offset_y, .. } => {
                // o RECT do container é FIXO (não rola) — posiciona com `origin` (que
                // já inclui o extra do pai, mas não o desta região). Os FILHOS dentro
                // rolam: empilha o offset (-offset) somado ao extra herdado.
                let r = egui::Rect::from_min_size(
                    origin + egui::vec2(rect.x, rect.y),
                    egui::vec2(rect.w, rect.h),
                );
                let clipped = painter.with_clip_rect(r.intersect(painter.clip_rect()));
                let new_extra = extra + egui::vec2(-*offset_x, -*offset_y);
                stack.push((clipped, new_extra));
            }
            DisplayItem::EndClip { .. } => {
                stack.pop();
            }
        }
    });
    if diagnostico {
        eprintln!("[paint] pintados={vistos} cortados={cortados} visivel={visivel:?} base_origin={base_origin:?}");
    }
}

/// O retângulo que um item ocupa na tela, ou `None` quando ele não é pintura
/// (os marcadores de clip) — nesses o culling não se aplica.
fn caixa_do_item(item: &DisplayItem, origin: egui::Pos2) -> Option<egui::Rect> {
    let de = |x: f32, y: f32, w: f32, h: f32| {
        Some(egui::Rect::from_min_size(origin + egui::vec2(x, y), egui::vec2(w.max(1.0), h.max(1.0))))
    };
    match item {
        DisplayItem::SolidRect { rect, .. }
        | DisplayItem::Border { rect, .. } => de(rect.x, rect.y, rect.w, rect.h),
        DisplayItem::Text { x, y, size, text, .. } => {
            // largura estimada por cima: o culling só precisa de não cortar o que
            // é visível, e medir cada texto aqui pagaria o custo que ele evita.
            de(*x, *y, text.chars().count() as f32 * *size, *size * 2.0)
        }
        _ => None,
    }
}

/// Pinta um GRADIENTE LINEAR de 2 cores num retângulo, como mesh de 4 vértices. A cor
/// de cada canto é a interpolação `c0`→`c1` conforme a projeção do canto no EIXO do
/// gradiente (definido por `angle_deg`, convenção CSS: 0°=de baixo p/ cima, 90°=p/ a
/// direita). Aproxima o `linear-gradient` de 2 pontos (paradas intermediárias já
/// foram descartadas no parse).
fn paint_linear_gradient(
    painter: &egui::Painter,
    rect: egui::Rect,
    c0: u32,
    c1: u32,
    angle_deg: f32,
) {
    // Vetor de direção do gradiente. CSS: 0°=para cima (0,-1); cresce no sentido
    // horário → 90°=(1,0), 180°=(0,1). rad = angle; dir = (sin, -cos).
    let rad = angle_deg.to_radians();
    let (dx, dy) = (rad.sin(), -rad.cos());
    let corners = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    // projeção de cada canto no eixo; normaliza para [0,1] entre min e max.
    let proj: Vec<f32> = corners.iter().map(|p| p.x * dx + p.y * dy).collect();
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for &p in &proj {
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let span = (hi - lo).max(1e-3);
    let ca = rgba_to_color32(c0);
    let cb = rgba_to_color32(c1);
    let mut mesh = egui::epaint::Mesh::default();
    for (i, corner) in corners.iter().enumerate() {
        let t = ((proj[i] - lo) / span).clamp(0.0, 1.0);
        let color = lerp_color32(ca, cb, t);
        mesh.colored_vertex(*corner, color);
    }
    // dois triângulos (0,1,2) e (0,2,3).
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// Interpola dois `Color32` no parâmetro `t` ∈ [0,1] (por canal, sem premultiply).
fn lerp_color32(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round().clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgba_unmultiplied(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
        l(a.a(), b.a()),
    )
}
