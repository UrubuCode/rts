//! Motor de LAYOUT — calcula a geometria (x, y, largura, altura) de cada nó e
//! emite uma DISPLAY LIST plana que o backend de render só PINTA. EGUI-FREE.
//!
//! Esta é a virada arquitetural decidida em 2026-06-27 ("processar tudo no DOM e
//! o egui só lê e exibe"): o `rts-dom` deixa de só guardar a árvore/estilo e passa
//! a CALCULAR onde cada caixa fica, seguindo a lógica do CSS (fluxo normal, box
//! model content-box). O `rts-egui` (ou qualquer backend futuro: web/png/canvas)
//! recebe a [`DisplayList`] pronta — uma lista de "pinte retângulo/texto em
//! (x,y,w,h)" — e só desenha. **O backend nunca decide layout.**
//!
//! ## Modelo (fluxo normal block, fase 1)
//!
//! - **Block empilha vertical**, cada caixa ocupando a largura do container por
//!   padrão (MDN CSS Flow Layout). `width` explícito (px/%) encolhe; `%` resolve
//!   contra o content-box do PAI (containing block), TARDE, aqui no layout.
//! - **Box model content-box** (MDN): `outer_w = margin + border + padding +
//!   content_w`. O `width` do CSS é a largura do CONTENT; padding/border/margin
//!   somam por fora.
//! - **Texto** é medido por um [`TextMeasurer`] (a largura/altura do glifo é o
//!   único dado que o `rts-dom` não tem sozinho — o backend mede; ver o trait).
//!   Fase 1 usa uma medida aproximada ([`ApproxMeasurer`]); o egui pluga a real.
//!
//! Cortes da fase 1 (aditivos depois): inline-flow rico multi-run, margin-collapse
//! pai-filho, `display:grid`, float/position. O objetivo da fatia é provar a
//! TUBULAÇÃO DOM→layout→display-list→paint com box model block.
//!
//! ## Flexbox (gap/justify-content/align-items) — cortes CONSCIENTES
//!
//! Implementado: `display:flex` (row) + `flex-wrap`, `gap`/`row-gap`/`column-gap`,
//! `justify-content` (todas as formas, fiel à CSS Box Alignment L3 incl. fallback
//! de overflow), `align-items` (flex-start/center/flex-end). Cortes documentados:
//! - **`align-items:stretch` NÃO estica de fato** — trata como flex-start (cada
//!   item mantém sua altura natural). Stretch é o DEFAULT do flex, então um card
//!   sem `align-items` explícito não preenche a altura da linha (o browser
//!   esticaria). Esticar real exige passar altura imposta ao `layout_block`
//!   (fase futura — ver `align_offset`).
//! - **`flex-direction` só Row** — `column`/`row-reverse`/`column-reverse` são
//!   parseados e guardados (cascade pronta) mas o layout SEMPRE dispõe em row. Uma
//!   fatia futura generaliza `layout_children_horizontal` por eixo (`column` =
//!   main vertical, justify no Y). `flex-grow`/`shrink`/`basis` também fora.

use crate::dom::{Dom, IntrinsicWidthKey, LayoutMeasureKey, NodeIdx, NodeKind};
use crate::inline_box::{AtomicKind, apara_css, e_espaco_css, so_espaco_css};
use crate::style::{ComputedStyle, ResolveCtx};

mod caixa;
mod display;
mod float;
mod input;
mod itens;
mod medida;
mod pintura;
mod posicionado;
mod replaced;

pub use self::display::{Corners, DisplayItem, DisplayList, Geometry, Rect, ScrollRegion};
pub use self::medida::{ApproxMeasurer, TextMeasurer};
pub use self::pintura::{emit_scrollbar, emit_scrollbar_in};
pub(crate) use self::caixa::{font_px, is_non_rendered_tag, used_display};
pub(crate) use self::float::Exclusao;
pub(crate) use self::itens::{record_node_rect, reserve_node_order};
pub(crate) use self::medida::intrinsic_outer_width;
pub(crate) use self::pintura::border_items;
pub(crate) use self::posicionado::is_out_of_flow;
use self::caixa::{css_display, em_contexto_inline, is_block_level, is_inline_block, is_inline_text_container, ua_list_indent, whitespace_is_inline_separator};
use self::float::{banda_livre, float_of, fundo_dos_floats};
use self::input::{layout_button, layout_input, medida_do_input};
use self::itens::{apply_transform_to_item, translate_item, walk_items};
use self::medida::{child_outer_height, child_outer_width, collect_text, content_natural_width, intrinsic_content_width};
use self::pintura::{apply_opacity, body_background, cor_visivel, decoration_code, deve_suprimir_fundo, is_text_input_tag, italico, tag_de};
use self::posicionado::{collect_out_of_flow, e_display_none, layout_out_of_flow, resolve_height};
use self::replaced::{layout_canvas, layout_image, layout_svg_placeholder};

/// Tamanho de fonte default (pontos) quando o estilo não especifica — base de
/// `em`/`rem` e do texto sem `font-size`. **16px, o default de todo browser**
/// (era 20, o que inflava cada `em`/`rem` em 25% — `max-width:42em` dava 840 em
/// vez dos 672 do Chrome; validado número-a-número no cover).
pub const DEFAULT_FONT_SIZE: f32 = 16.0;

/// O contexto de uma passada de layout: o viewport (para `vw`/`vh` e largura
/// inicial) e o medidor de texto. Imutável durante a passada.
pub struct LayoutCtx<'a> {
    pub viewport_w: f32,
    pub viewport_h: f32,
    pub measurer: &'a dyn TextMeasurer,
}

/// Mede um bloco sem emitir pintura. É usado apenas pelos pré-passos de flex/grid/
/// inline-block e pelo posicionamento out-of-flow. O resultado depende das constraints
/// e do estilo vigente, mas não da posição absoluta; por isso o cache não guarda uma
/// DisplayList e a chamada final continua responsável por pintar tudo no z-order certo.
#[allow(clippy::too_many_arguments)]
pub(crate) fn measure_block(
    dom: &Dom,
    id: NodeIdx,
    avail_w: f32,
    avail_h: Option<f32>,
    forced_outer_w: Option<f32>,
    forced_outer_h: Option<f32>,
    shrink_to_fit: bool,
    ctx: &LayoutCtx,
) -> (f32, f32) {
    let measurer = ctx.measurer.identity();
    let key = LayoutMeasureKey {
        tree: dom.cache_identity(),
        node_epoch: dom.layout_epoch(id),
        style_epoch: crate::style::props::style_epoch(),
        node: id,
        avail_w: avail_w.to_bits(),
        avail_h: avail_h.map(f32::to_bits),
        forced_outer_w: forced_outer_w.map(f32::to_bits),
        forced_outer_h: forced_outer_h.map(f32::to_bits),
        shrink_to_fit,
        viewport_w: ctx.viewport_w.to_bits(),
        viewport_h: ctx.viewport_h.to_bits(),
        measurer,
    };
    crate::bump!(measure_calls);
    if let Some(size) = dom.layout_measure_get(key) {
        crate::bump!(measure_hits);
        return size;
    }
    let mut scratch = DisplayList::default();
    let size = layout_block(
        dom,
        id,
        0.0,
        0.0,
        avail_w,
        avail_h,
        forced_outer_w,
        forced_outer_h,
        shrink_to_fit,
        // A MEDIDA de um bloco é a do seu conteúdo, não a da banda onde calha
        // ficar: medir com o float à frente dava uma largura intrínseca que
        // mudava consoante a vizinhança.
        &[],
        ctx,
        &mut scratch,
    );
    dom.layout_measure_put(key, size);
    size
}

/// O layout de um `Dom`, REUSADO enquanto nada que o afete mudar.
///
/// Um browser não recalcula layout quando nada mudou, e o caminho headless
/// (`rts:dom` a partir do TS) chamava [`layout_document`] por consulta de
/// geometria — uma passada completa por `getBoundingClientRect`. O `rts-egui`
/// já tinha um cache assim, por frame, dentro dele: dois caches para a mesma
/// pergunta, e só um dos consumidores servido.
///
/// A chave é `(revisão de render, viewport, medidor)`. A revisão cobre árvore,
/// estilo e animação (todo mutador a incrementa); o viewport porque o layout
/// depende dele; e o MEDIDOR porque a mesma árvore no mesmo viewport se dispõe
/// diferente com uma fonte diferente — é o mesmo componente que já entra nas
/// chaves dos caches de medição.
///
/// Devolve `Rc` e não valor: uma `DisplayList` de página grande são 15 000
/// itens e milhares de `String`, e clonar isso por consulta desfaria o ganho.
pub fn layout_cached(dom: &Dom, ctx: &LayoutCtx) -> std::rc::Rc<DisplayList> {
    let key = (
        dom.render_revision(),
        ctx.viewport_w.to_bits(),
        ctx.viewport_h.to_bits(),
        ctx.measurer.identity(),
    );
    if let Some(hit) = dom.display_cache_get(key) {
        crate::bump!(display_cache_hits);
        return hit;
    }
    let fresh = std::rc::Rc::new(layout_document(dom, ctx));
    dom.display_cache_put(key, &fresh);
    fresh
}

/// Calcula o layout de um `Dom` inteiro e devolve a [`DisplayList`]. Ponto de
/// entrada do motor: percorre os filhos de `#document` como blocos empilhados na
/// largura do viewport, resolvendo box model e emitindo os itens de pintura.
pub fn layout_document(dom: &Dom, ctx: &LayoutCtx) -> DisplayList {
    crate::bump!(documents);
    let _phase = crate::metrics::phases::scope("layout");
    // informa o viewport à CASCADE (base de vw/vh no font-size fluido/calc; o
    // memo de estilo do Dom invalida sozinho se mudou).
    dom.set_viewport(ctx.viewport_w, ctx.viewport_h);
    let mut list = DisplayList::default();
    // PROPAGAÇÃO DO FUNDO do <body>/<html> (regra especial do CSS): o background
    // desses dois elementos "vaza" para o VIEWPORT inteiro, não só a caixa deles.
    // Pintamos PRIMEIRO (atrás de tudo) um retângulo do tamanho do viewport com a cor
    // do body. (Reserva uma altura generosa; o egui faz clip na sua área.)
    //
    // E BRANCO quando nenhum dos dois define fundo: é o que um browser pinta no
    // canvas de uma página sem `background`. Sem isto o que aparecia era a cor
    // de limpeza do backend (quase preta), e uma página real cujo estilo mora
    // num `<link>` externo ficava texto preto sobre preto — o sintoma parecia
    // "a cascata falhou" quando a cascata estava certa e o canvas é que não
    // tinha dono.
    // Vai no CAMPO e não como item da lista: quem pinta o canvas é o backend
    // (é a cor de limpeza dele), e um item a mais deslocaria todos os índices
    // que os testes de layout usam para nomear o que estão a verificar.
    list.canvas_background = body_background(dom).unwrap_or(0xFFFF_FFFF);
    let mut cursor_y = 0.0f32;
    let root = dom.node(dom.root);
    for &child in &root.children {
        // position:absolute/fixed não participa do fluxo, inclusive quando é filho
        // direto do documento; será layoutado na passada final por z-index.
        if is_out_of_flow(dom, child) {
            continue;
        }
        // o containing block da raiz é a VIEWPORT: `height:100%` no <html> resolve
        // contra a altura da janela (base do `h-100` de páginas reais).
        let (_, h) = layout_block(
            dom,
            child,
            0.0,
            cursor_y,
            ctx.viewport_w,
            Some(ctx.viewport_h),
            None,
            None,
            false,
            &[],
            ctx,
            &mut list,
        );
        cursor_y += h;
    }
    list.content_height = cursor_y;
    // ── PASSADA OUT-OF-FLOW: `position:absolute/fixed` saíram do fluxo (não
    // ocuparam espaço); pinta cada um contra o VIEWPORT com top/right/bottom/left,
    // por cima do fluxo (apêndice da lista = z maior; sem z-index real). V1: o
    // containing block é sempre a viewport (o de `absolute` — ancestral positioned
    // — e o "fica fixo ao rolar" do `fixed` são a v2).
    let mut out_of_flow = Vec::new();
    // Só varre se a página PODE ter algum: a varredura pede o estilo computado
    // de cada nó da árvore, e era 78% de um frame de mutação numa página que não
    // tem um único posicionado.
    if dom.may_have_out_of_flow() {
        collect_out_of_flow(dom, dom.root, &mut out_of_flow);
    }
    // Z-INDEX: ordena por z-index (menor pinta primeiro = fica atrás). Sort ESTÁVEL:
    // z-index igual (ou ambos auto=0) preserva a ordem do documento. Cobre o caso
    // comum (modais/dropdowns/overlays posicionados que se sobrepõem).
    out_of_flow.sort_by_key(|&id| {
        dom.computed_style_idx(id)
            .and_then(|c| c.z_index)
            .unwrap_or(0)
    });
    // O rect do containing block de cada abs é lido do `node_rects` JÁ preenchido
    // pelo fluxo normal (o ancestral positioned já foi pintado). Clona antes do
    // empréstimo mutável de `list`.
    // A geometria COMPLETA (com as subárvores reusadas): o containing block de
    // um `absolute` pode ser um ancestral cujo retângulo veio de um fragmento.
    let flow_rects = list.geometry_now().rects;
    crate::bump!(out_of_flow, out_of_flow.len());
    for id in &out_of_flow {
        layout_out_of_flow(dom, *id, ctx, &flow_rects, &mut list);
    }
    // A HashMap não carrega ordem de pintura. Materializamos uma ordem explícita
    // para o hit-test: fluxo normal em pré-ordem e, depois, posicionados em ordem
    // crescente de z-index (o último pintado fica no topo).
    // A ordem de pintura já foi registrada durante as inserções de retângulos:
    // fluxo normal durante a descida e out-of-flow na ordem de z-index acima.
    crate::bump!(display_items, list.total_items());
    // As marcas de sujeira são POR PASSADA: quem as consome é este layout, e
    // acumulá-las entre frames faria a lista de filhos sujos de um container
    // crescer até o teto — e aí a costura desistiria sempre.
    dom.clear_dirty();
    crate::bump!(node_rects, list.node_rects.len());
    crate::bump!(scroll_regions, list.scroll_regions.len());
    list
}

/// O retângulo (border-box) de um nó, computando o layout do documento na largura
/// dada — a base de `element.getBoundingClientRect()`. `None` se o nó não é
/// renderável (texto/`display:none`/metadata não têm rect próprio).
/// Roda o layout inteiro (O(n)); para várias consultas no mesmo frame, reuse a
/// `DisplayList` de `layout_document` e leia `node_rects` direto.
pub fn bounding_rect(dom: &Dom, node: NodeIdx, ctx: &LayoutCtx) -> Option<Rect> {
    layout_document(dom, ctx).rect_of(node)
}

/// Faz o layout de UM nó-bloco a partir de `(x, y)`, com `avail_w` de largura
/// disponível (a do container). Emite os itens (fundo/borda/texto/filhos) na
/// `list` e devolve o TAMANHO EXTERNO `(outer_w, outer_h)` da caixa (incluindo
/// padding/border/margin) — o pai usa a altura (empilhamento vertical) ou a
/// largura (horizontal) para posicionar o irmão seguinte. Texto solto e nós inline
/// são desenhados como linhas dentro do content-box.
pub(crate) fn layout_block(
    dom: &Dom,
    id: NodeIdx,
    x: f32,
    y: f32,
    avail_w: f32,
    // Altura do CONTENT do containing block, quando DEFINIDA (height explícito no
    // pai / viewport na raiz): a base de `height: %` — que resolve contra a ALTURA
    // do pai (antes resolvia errado contra a largura; `h-100` não funcionava).
    // `None` = pai com altura auto → `height: %` vira auto (fiel ao browser).
    avail_h: Option<f32>,
    // Largura OUTER IMPOSTA (com margem) — o flex resolveu grow/shrink e DITA o
    // main size do item; vence width/min-max/shrink-to-fit (v1: clamp min/max no
    // resolve flex fica como corte documentado). `None` = fluxo normal.
    forced_outer_w: Option<f32>,
    // Altura OUTER IMPOSTA (com margem) — o `align-items/self: stretch` do flex.
    // O caller só passa para item SEM height explícito. `None` = altura natural.
    forced_outer_h: Option<f32>,
    // `shrink_to_fit`: quando true, um bloco SEM `width` explícito dimensiona pela
    // largura do CONTEÚDO (como `inline-block`/item flex), não ocupa a largura
    // disponível. É o que faz badges num container horizontal não esticarem para a
    // linha toda. No fluxo vertical normal é false (block ocupa a largura — MDN).
    shrink_to_fit: bool,
    // Os floats ABERTOS do contexto que envolve este bloco, em coordenadas
    // absolutas. Pelo CSS um float estorva o conteúdo de todo o bloco de
    // formatação, não só o do container onde foi declarado — é por isso que
    // atravessa a fronteira em vez de ficar em `layout_children_vertical`.
    exclusoes: &[Exclusao],
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> (f32, f32) {
    crate::bump!(block_calls);
    // Nós não-elemento no nível de bloco (texto solto, comentário): trata o texto
    // como uma linha; comentário não pinta.
    let css = match &dom.node(id).kind {
        NodeKind::Element { tag } => {
            // Metadata não-renderável (`<head>` e seu conteúdo, `<style>`,
            // `<script>`): pula a subárvore inteira — não pinta nada. Permite
            // carregar um HTML COMPLETO (com <head><title><meta>) e renderizar só
            // o que é visível (o <body> e seus filhos).
            if is_non_rendered_tag(tag) {
                return (0.0, 0.0);
            }
            let css = dom.computed_style_idx(id).unwrap_or_default();
            // `display:none` — não renderiza nem ocupa espaço (some da árvore visual).
            if e_display_none(dom, id) {
                return (0.0, 0.0);
            }
            // `<input>`/`<textarea>` editável (mini-browser): void, sem filhos — o
            // "conteúdo" é o texto do value/placeholder + cursor. Caminho próprio,
            // fora do fluxo de bloco genérico (que desceria em filhos inexistentes).
            if is_text_input_tag(tag) {
                let itype = dom
                    .node(id)
                    .attr("type")
                    .map(|t| t.to_ascii_lowercase())
                    .unwrap_or_default();
                // `type=hidden`: invisível e sem espaço (o form legado do google
                // tem 5 — viravam caixas de texto fantasmas).
                if itype == "hidden" {
                    return (0.0, 0.0);
                }
                // `type=submit/button/reset`: BOTÃO — caixa cinza UA com o value
                // como rótulo (não editável). O suficiente p/ o "Pesquisa Google".
                if matches!(itype.as_str(), "submit" | "button" | "reset") {
                    return layout_button(dom, id, &css, x, y, ctx, list);
                }
                return layout_input(
                    dom,
                    id,
                    &css,
                    x,
                    y,
                    avail_w,
                    avail_h,
                    forced_outer_w,
                    ctx,
                    list,
                );
            }
            // `<img>` com pixels decodificados: emite a imagem no rect (tamanho do CSS
            // width/height, senão o natural da imagem). Void — sem filhos.
            // `<canvas>`: elemento REPLACED cujo conteúdo é uma superfície de
            // pixels. A caixa vem dos atributos `width`/`height` (ou do CSS), e
            // o desenho aparece quando o programa pinta — antes disso a caixa
            // existe e fica vazia, que é o que o browser também faz.
            if tag == "canvas" {
                if let Some(r) = layout_canvas(dom, id, &css, x, y, avail_w, ctx, list) {
                    return r;
                }
            }
            if tag == "img" {
                if let Some(img) = layout_image(dom, id, &css, x, y, avail_w, ctx, list) {
                    return img;
                }
                // sem pixels ainda (não baixou/decodificou): ocupa 0 (não pinta nada).
            }
            // `<svg>` é um REPLACED element: não desenhamos o vetor, mas RESERVAMOS
            // a caixa (dimensões do CSS width/height, dos atributos, ou da razão do
            // `viewBox`) e pintamos um placeholder cinza — assim a estrutura da
            // página fica correta mesmo sem o SVG (logo/ícones do google ocupam o
            // espaço certo em vez de colapsar pra 0×0).
            if tag == "svg" {
                if let Some(r) = layout_svg_placeholder(dom, id, &css, x, y, avail_w, ctx, list) {
                    return r;
                }
            }
            css
        }
        NodeKind::Text(t) => {
            // Whitespace estrutural é preservado no DOM, mas não cria uma linha
            // visual quando chega sozinho ao fluxo de blocos/root. Em contexto
            // inline, ele é tratado por `wrap_runs` e continua separando palavras.
            if t.trim().is_empty() {
                return (0.0, 0.0);
            }
            let size = DEFAULT_FONT_SIZE;
            let lh = ctx.measurer.line_height(size);
            let tw = ctx.measurer.text_width(t, size, false, false, false);
            list.items.push(DisplayItem::Text {
                x,
                y,
                text: t.as_str().into(),
                color: 0x000000FF,
                size,
                mono: false,
                bold: false,
                italic: false,
                letter_spacing: 0.0,
                decoration: 0,
            });
            return (tw, lh);
        }
        _ => return (0.0, 0.0), // Comment / Document aninhado: não pinta.
    };

    // ── Box model (content-box): resolve as bordas/espaços absolutos ─────────────
    // O contexto de RESOLUÇÃO tardia primeiro (margens/paddings agora aceitam
    // unidades relativas — `p-3` = 1rem do Bootstrap — e resolvem AQUI, como width).
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font_px(&css, DEFAULT_FONT_SIZE),
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // Margin/padding POR LADO (Edges). O `margin_v` (UA-stylesheet, só vertical) é
    // somado ao top/bottom. Margens são SIGNED (negativa puxa — gutters `.row`);
    // padding é clampado ≥ 0 (padding negativo não existe no CSS).
    let m = &css.margin;
    let p = &css.padding;
    let mut margin_left = m.left.resolve(&resolve).unwrap_or(0.0);
    let mut margin_right = m.right.resolve(&resolve).unwrap_or(0.0);
    // margin_v (UA-stylesheet) só vale no lado que o AUTOR NÃO declarou — um
    // `margin-top: 0` explícito ANULA o default da UA naquele lado (era o brand
    // do cover descendo 16px apesar do `h3 { margin-top: 0 }` do Bootstrap).
    let margin_v_extra = css.margin_v.unwrap_or(0.0);
    let mv_top = if m.top == crate::style::Side::Unset {
        margin_v_extra
    } else {
        0.0
    };
    let mv_bottom = if m.bottom == crate::style::Side::Unset {
        margin_v_extra
    } else {
        0.0
    };
    let margin_top = m.top.resolve(&resolve).unwrap_or(0.0) + mv_top;
    let margin_bottom = m.bottom.resolve(&resolve).unwrap_or(0.0) + mv_bottom;
    let pad_left = p.left.resolve(&resolve).unwrap_or(0.0).max(0.0);
    // RECUO DA LISTA (UA-stylesheet): `<ul>`/`<ol>` trazem `padding-inline-start:
    // 40px` em todo o browser, e é esse recuo que aloja o marcador do `<li>`.
    // Entra como PADDING e não como uma variável à parte porque é o que ele é:
    // assim conta na caixa de borda, no `content_x` e na largura disponível dos
    // filhos sem que nenhum desses três sítios precise de saber que existem
    // listas. Um `padding-left` do autor anula-o — é a camada mais fraca da
    // cascade, e o `list-style:none;padding-left:0` de um menu tem de vencer.
    let pad_left = pad_left + ua_list_indent(dom, id, p);
    let pad_right = p.right.resolve(&resolve).unwrap_or(0.0).max(0.0);
    let pad_top = p.top.resolve(&resolve).unwrap_or(0.0).max(0.0);
    let pad_bottom = p.bottom.resolve(&resolve).unwrap_or(0.0).max(0.0);
    // BORDA POR LADO: as larguras USADAS (um lado com `border-style: none` vale
    // zero, por mais que declare largura — ver `style::borders::used_widths`).
    // Era um ESCALAR `css.border_width` aplicado aos quatro lados, e isso não é
    // uma simplificação: `border-bottom: 5px` alargava a caixa nos quatro lados
    // ou em nenhum. Medido no corpus, era o maior desvio de uma fixture só —
    // `claude-border-lados`, 15 de 82.
    let [border_top, border_right, border_bottom, border_left] =
        crate::style::borders::used_widths(&css);
    // Atalhos para o eixo (horizontal = left+right): a maioria do box model usa o
    // total por eixo. (`margin_h`/`padding_h` = soma do eixo horizontal.)
    let border_h = border_left + border_right;
    let border_v = border_top + border_bottom;
    let margin_h = margin_left + margin_right;
    let padding_h = pad_left + pad_right;
    // `frame` horizontal = o que cerca o content no eixo X (margin+border+padding
    // dos DOIS lados); cada termo já é a soma do seu eixo.
    let frame = margin_h + border_h + padding_h;
    let font_for_content = font_px(&css, DEFAULT_FONT_SIZE);
    let border_box = css.border_box.unwrap_or(false);
    // O PAPEL da caixa (item de lista, parte de tabela) — decidido já aqui e não
    // junto do eixo dos filhos porque a `<table>` muda a resolução da SUA PRÓPRIA
    // largura, três linhas abaixo.
    let used = used_display(dom, id);
    // Uma `<table>` sem `width` é SHRINK-TO-FIT: encolhe ao conteúdo em vez de
    // ocupar o pai. É a diferença mais visível entre uma tabela e um `<div>`, e
    // sem ela cada tabela da página nasce com a largura da coluna inteira.
    let shrink_to_fit = shrink_to_fit || used == Some(crate::style::DisplayKind::Table);
    let content_w = if let Some(fw) = forced_outer_w {
        // main size do FLEX (grow/shrink já resolvidos): outer imposto → content =
        // outer - frame (o frame já soma margem+borda+padding dos dois lados).
        // Vence width/min-max (o clamp no resolve flex é corte documentado).
        (fw - frame).max(0.0)
    } else {
        // `width: max-content` — a largura que o conteúdo PEDE, e nada a limita.
        //
        // É essa a diferença face ao shrink-to-fit logo abaixo, que é a mesma
        // medição com `.min(disponível)` por cima: `max-content` transborda de
        // propósito, o shrink-to-fit cede. Usar o ramo do shrink-to-fit seria
        // dar a resposta certa por acaso sempre que coubesse, e a errada quando
        // é precisamente o caso que a palavra-chave existe para exprimir.
        //
        // Sem esta linha, `width:max-content` chegava aqui como `None` (o parse
        // descartava-o) e o elemento tomava a largura DO PAI: o painel do menu da
        // Wikipédia media 56,2 onde o Chrome dá 198,6, e tudo lá dentro herdava
        // o estrangulamento — 135 `<li>` a quebrar em linhas a mais.
        //
        // O `box-sizing` não entra: o valor medido JÁ é o conteúdo, ao contrário
        // de um `width` declarado, onde `border-box` manda descontar o frame.
        // Descontá-lo aqui tirava padding a um número que nunca o incluiu.
        //
        // O clamp de `min`/`max-width` abaixo continua a morder por cima, como
        // manda a spec — e neste elemento o `max-width:200px` da mesma regra NÃO
        // morde: o conteúdo pede 166,6. Se um dia bater nos 200, é sinal de que
        // esta medição passou a calcular a mais.
        let base = if css.width == Some(crate::style::Dimension::MaxContent) {
            content_natural_width(dom, id, font_for_content, ctx)
        } else {
            match css.width.and_then(|d| d.resolve(&resolve)) {
            // `width` explícito. Em `border-box`, o `width` INCLUI padding+border —
            // então o content é `width - (padding_h + 2*border)`. Em content-box
            // (default), o `width` JÁ é o content.
            Some(w) if border_box => (w - (padding_h + border_h)).max(0.0),
            Some(w) => w,
            // Sem width: shrink-to-fit → largura do conteúdo (limitada ao disponível);
            // senão (fluxo block normal) → ocupa a largura disponível.
            None if shrink_to_fit => content_natural_width(dom, id, font_for_content, ctx)
                .min((avail_w - frame).max(0.0)),
            None => (avail_w - frame).max(0.0),
            }
        };
        // CLAMP min/max-width (#1751): `used = clamp(min, width, max)`. min/max são
        // sobre a CAIXA (border-box) na spec — descontamos o frame p/ aplicar ao
        // content quando border-box; em content-box já são do content.
        let mnw = css.min_width.and_then(|d| d.resolve(&resolve)).map(|v| {
            if border_box {
                (v - (padding_h + border_h)).max(0.0)
            } else {
                v
            }
        });
        let mxw = css.max_width.and_then(|d| d.resolve(&resolve)).map(|v| {
            if border_box {
                (v - (padding_h + border_h)).max(0.0)
            } else {
                v
            }
        });
        crate::style::clamp_size(base, mnw, mxw)
    };

    // `margin: 0 auto` (#1745): se o margin-left/right é `auto` E o bloco tem largura
    // definida (não ocupa o pai inteiro), o espaço livre se distribui pelos lados
    // auto — centralizando (ambos auto) ou empurrando (um só auto). Resolvido AQUI,
    // depois de saber o content_w. Só quando há largura explícita (senão o bloco já
    // ocupa avail_w e não há espaço a distribuir).
    let has_width = css.width.is_some() || css.max_width.is_some();
    if has_width {
        let box_outer = content_w + padding_h + border_h; // sem a margin
        let free = (avail_w - box_outer).max(0.0);
        match (m.left.is_auto(), m.right.is_auto()) {
            (true, true) => {
                margin_left = free / 2.0;
                margin_right = free / 2.0;
            }
            (true, false) => margin_left = (free - margin_right).max(0.0),
            (false, true) => margin_right = (free - margin_left).max(0.0),
            (false, false) => {}
        }
    }

    // Posição do content-box (canto sup-esq): deslocado pelo lado ESQUERDO/TOPO
    // (margin+border+padding daquele lado), não a soma do eixo.
    let content_x = x + margin_left + border_left + pad_left;
    let content_y = y + margin_top + border_top + pad_top;

    // Z-ORDER: o fundo/borda da caixa precisam ficar ATRÁS dos filhos. Como a
    // display list é pintada em ordem, reservamos AGORA o índice onde a caixa será
    // inserida (antes de qualquer filho), descemos nos filhos (que dão append no
    // fim), e só DEPOIS — conhecendo a altura — inserimos o fundo nesse índice.
    let box_index = list.items.len();
    // Quantas subárvores já existiam ANTES desta caixa começar. Só as que
    // vierem a seguir é que são empurradas quando o fundo for inserido — ver
    // [`insert_item`].
    let filhos_antes_da_caixa = list.children.len();
    // Reserva a posição do pai antes dos filhos; a geometria final é preenchida
    // depois que a altura natural do conteúdo for conhecida.
    reserve_node_order(list, id);

    // ── Filhos: o EIXO depende do `display` do bloco ─────────────────────────────
    // vertical (default): cada filho ABAIXO do anterior, ocupando a largura.
    // horizontal (`display:horizontal`/flex-row): cada filho À DIREITA do anterior,
    // a altura do content = a do filho mais alto (MDN flow: inline-axis stacking).
    let display = css_display(dom, id);
    let font_size = font_px(&css, DEFAULT_FONT_SIZE);

    // SCROLL CONTAINER (#1744): uma div com `overflow-x:auto/scroll` NÃO comprime os
    // filhos — eles transbordam e a div rola. Nesse caso layoutamos os filhos com a
    // largura NATURAL do conteúdo (intrinsic), não a do container. (overflow-y já não
    // comprime: o vertical empilha e a altura é a soma — só precisamos do clip+barra.)
    let ov_x = css
        .overflow_x
        .unwrap_or(crate::scrollbar::Overflow::Visible);
    let ov_y = css
        .overflow_y
        .unwrap_or(crate::scrollbar::Overflow::Visible);
    let scrolls_x = ov_x.scrollable() || ov_x == crate::scrollbar::Overflow::Hidden;
    // A inflação vale para o eixo do FLUXO HORIZONTAL, que é onde a compressão
    // aconteceria (o flex encolhe os itens até caberem). Nos demais layouts ela
    // vira base de PORCENTAGEM dos filhos, e aí está errada: `width:100%` dentro
    // de um container que rola é 100% da CAIXA, não do conteúdo transbordado.
    //
    // Medido na página real do WhatsApp Web, que aninha vários containers com
    // `overflow-y:auto`: cada nível multiplicava a largura do seguinte, e o
    // conteúdo terminava em x = 2300 numa janela de 1100 — a tela abria vazia
    // com tudo desenhado fora dela.
    let scroll_children_w = if scrolls_x {
        // largura que o conteúdo QUER (sem comprimir) — pode exceder content_w.
        intrinsic_content_width(dom, id, font_size, ctx).max(content_w)
    } else {
        content_w
    };
    let children_w = content_w;

    // `height` EXPLÍCITO resolve ANTES dos filhos (não depende deles): eles o
    // recebem como containing-block height (base do `height:%` deles), e o flex
    // COLUMN o usa como referência do eixo principal (justify/margin-auto).
    let frame_v = pad_top + pad_bottom + border_v;
    let explicit_content_h = resolve_height(css.height, avail_h, &resolve)
        .map(|h| {
            if border_box {
                (h - frame_v).max(0.0)
            } else {
                h
            }
        })
        // `aspect-ratio`: sem height explícito, a altura vem da largura / razão. Só
        // quando há largura resolvida (content_w) e uma razão > 0.
        .or_else(|| {
            css.aspect_ratio
                .filter(|r| *r > 0.0)
                .map(|r| (content_w / r).max(0.0))
        })
        // ALTURA IMPOSTA pelo flex (grow/stretch): o `forced_outer_h` é a altura
        // OUTER do item — o content-box é ela menos margem-v/frame. Vira o
        // containing block dos filhos (um filho `height:100%` resolve contra ela),
        // resolvendo o logo/caixa do google que crescem via flex-grow vertical.
        .or_else(|| {
            forced_outer_h.map(|oh| {
                let mv = margin_top + margin_bottom;
                (oh - mv - frame_v).max(0.0)
            })
        });

    // Altura que serve de CONTAINING BLOCK aos filhos (`height:%`): o height
    // explícito, senão um `max-height` conhecido (o Google dá ao container do
    // logo `height:calc(100% - 560px); max-height:290px` — o max é a altura
    // efetiva; sem isso o filho `height:100%` resolvia contra o conteúdo e
    // inflava). Calculado ANTES de layoutar os filhos (a resolução do `%` do
    // filho é top-down; a spec exige o CB conhecido).
    let mnh_pre = resolve_height(css.min_height, avail_h, &resolve).map(|v| {
        if border_box {
            (v - frame_v).max(0.0)
        } else {
            v
        }
    });
    let mxh_pre = resolve_height(css.max_height, avail_h, &resolve).map(|v| {
        if border_box {
            (v - frame_v).max(0.0)
        } else {
            v
        }
    });
    let avail_children = explicit_content_h.or(mxh_pre);

    // `flex-direction: column` — o eixo PRINCIPAL do flex vira o vertical: os itens
    // empilham (sem margin-collapse, que flex não tem), gap/justify/margin-auto
    // atuam no Y e align-items no X (stretch = ocupar a largura, o default).
    let is_column = css.flex_direction.map(|f| f.is_column()).unwrap_or(false);
    let is_flex =
        display == crate::block::DISPLAY_HORIZONTAL || display == crate::block::DISPLAY_WRAP;
    let content_h = match display {
        // flex column (com ou sem wrap — multi-coluna do wrap é corte documentado).
        _ if is_flex && is_column => layout_children_column(
            dom,
            id,
            content_x,
            content_y,
            children_w,
            avail_children,
            &css,
            font_size,
            ctx,
            list,
        ),
        // horizontal (flex-row sem wrap): lado a lado, encolhe pra caber, não quebra.
        d if d == crate::block::DISPLAY_HORIZONTAL => layout_children_horizontal(
            dom,
            id,
            content_x,
            content_y,
            scroll_children_w,
            avail_children,
            &css,
            font_size,
            false,
            None,
            ctx,
            list,
        ),
        // GRID REAL: track-sizing (px/fr/auto/%) + auto-placement row-by-row +
        // alinhamento de célula (align-items/justify-items). Só quando é
        // `display:grid` de fato; senão o wrap horizontal (inline-block flow).
        // TABELA: a grade inteira é construída antes de posicionar o que quer que
        // seja (a largura de uma célula vem da COLUNA, não dela). Fica antes do
        // grid porque uma `<table>` que o autor não tocou tem eixo vertical e
        // cairia no empilhamento de blocos, descendo por `<tr>` como se fossem
        // `<div>` — que é exatamente o que a página real mostrava.
        _ if used == Some(crate::style::DisplayKind::Table) => crate::table::layout_table(
            dom, id, content_x, content_y, children_w, &css, font_size, ctx, list,
        ),
        _ if css.effective_display() == Some(crate::style::DisplayKind::Grid) => {
            layout_children_grid(
                dom,
                id,
                content_x,
                content_y,
                children_w,
                avail_children,
                &css,
                font_size,
                ctx,
                list,
            )
        }
        // wrap (inline-block flow): lado a lado E QUEBRA linha quando enche.
        d if d == crate::block::DISPLAY_WRAP => layout_children_horizontal(
            dom,
            id,
            content_x,
            content_y,
            scroll_children_w,
            avail_children,
            &css,
            font_size,
            true,
            None,
            ctx,
            list,
        ),
        // vertical (block): empilha.
        _ => layout_children_vertical(
            dom,
            id,
            content_x,
            content_y,
            children_w,
            avail_children,
            &css,
            font_size,
            exclusoes,
            ctx,
            list,
        ),
    };
    // MARCADOR do item de lista. Emitido DEPOIS dos filhos e com o content-box já
    // conhecido, e não desloca coisa nenhuma: `list-style-position: outside` (o
    // default, e o único que este motor desenha) põe o marcador FORA da caixa de
    // conteúdo, dentro do recuo que o `<ul>` já reservou.
    if used == Some(crate::style::DisplayKind::ListItem) {
        crate::listitem::emit_marker(dom, id, &css, content_x, content_y, font_size, ctx, list);
    }

    // a altura REAL do conteúdo (antes de `height` explícito a cortar) — p/ o scroll-Y.
    let content_h_natural = content_h;

    // CAIXA INLINE: um elemento cujo display USADO é `inline` mas que tem caixa
    // (fundo, padding, borda) NÃO tira a altura do fluxo dos filhos — tira-a da
    // FONTE, como qualquer inline. É a mesma regra da content area que o fluxo
    // inline já aplica, e faltava aqui: um `<a>` com padding num parágrafo de
    // `line-height:1.6` respondia 22,4 (a altura da LINHA) onde o browser
    // responde 16, e são milhares deles numa página real.
    //
    // Só quando o autor não declara `display` nem `height`: um `display:inline-block`
    // de facto é um contentor de blocos e a altura dele vem mesmo do conteúdo.
    // `used.is_none()`: um papel USADO (célula, linha, item de lista, tabela) já
    // não é uma caixa inline — foi o `<td>` que o mostrou, porque tem padding da
    // UA e por isso passava no teste de "inline com caixa".
    let caixa_inline = used.is_none()
        && css.effective_display().is_none()
        && css.height.is_none()
        && is_inline_block(dom, id);
    let content_h = if caixa_inline {
        crate::inline_box::altura_do_conteudo(font_size, ctx.measurer)
    } else {
        content_h
    };
    // `height` explícito SOBRESCREVE a altura do conteúdo (a caixa tem essa altura,
    // mesmo que o conteúdo seja menor) — já resolvido antes dos filhos.
    let content_h = explicit_content_h.unwrap_or(content_h);
    // CLAMP min/max-height (#1751): used = clamp(min, height, max) — eixo vertical
    // (`%` contra a ALTURA do containing block, como o height).
    let content_h = crate::style::clamp_size(content_h, mnh_pre, mxh_pre);
    // STRETCH do flex: altura OUTER imposta pelo container (align-items/self:
    // stretch) → content = outer - margens - frame_v; nunca ENCOLHE o conteúdo
    // (max com o natural — um item mais alto que a linha não é cortado).
    let content_h = match forced_outer_h {
        Some(fh) => (fh - margin_top - margin_bottom - frame_v).max(content_h),
        None => content_h,
    };

    // ── Insere a CAIXA (fundo + borda) no índice reservado, ATRÁS dos filhos ─────
    // O BORDER-BOX do nó: content + padding + border (NÃO a margin — esta é espaço
    // externo). É o retângulo que `getBoundingClientRect()` reporta.
    let box_rect = Rect::new(
        x + margin_left,
        y + margin_top,
        content_w + padding_h + border_h,
        content_h + pad_top + pad_bottom + border_v,
    );
    // Registra a geometria deste nó (base do getBoundingClientRect/offsetWidth).
    record_node_rect(list, id, box_rect);

    // Pinta a CAIXA (fundo/borda) ATRÁS dos filhos. `insert` no `box_index` põe o
    // fundo antes dos itens dos filhos (z-order).
    if css.has_box() {
        let radius = css.corner_radius.unwrap_or(0.0);
        // O FUNDO pinta por canto; a borda e a sombra continuam a ler o campo
        // único, e é isso que as deixa responder hoje o que respondiam ontem.
        let cantos = Corners::from_style(&css, 0.0);
        // `opacity` do elemento: multiplica o ALPHA das cores próprias (fundo/borda).
        // Cobre o caso comum (card/botão/overlay com fade) sem grupo de compositing.
        // `visibility:hidden` zera o alpha de tudo o que ESTE elemento pinta. Não
        // salta o layout: o elemento continua a ocupar o espaço dele, que é
        // exatamente o que o distingue de `display:none` — e como a propriedade
        // é herdada, os descendentes chegam aqui já com ela.
        let op = if css.visibility == Some(crate::style::values::Visibility::Hidden) {
            0.0
        } else {
            css.opacity.unwrap_or(1.0)
        };
        // `filter`: a cadeia inteira reduzida a UMA matriz de cor, uma vez por
        // elemento. Ver `painteffects` para o que é exprimível — em resumo, as
        // funções que são aritmética de canal são exatas e o `blur`/`drop-shadow`
        // recusam a cadeia toda, deixando esta matriz na identidade.
        //
        // Aplicada ANTES do `opacity` porque é essa a ordem do CSS: o filtro
        // atua sobre o elemento renderizado e a opacidade compõe o resultado.
        //
        // LIMITE, e é o mesmo que o `opacity` acima já tem: alcança as cores
        // PRÓPRIAS desta caixa (sombra, fundo, gradiente, borda) e não os
        // descendentes, que são pintados pelos seus próprios layouts. Um
        // `filter: invert(1)` numa div com texto inverte aqui o fundo e não o
        // texto. Não há grupo de compositing nesta display list onde a subárvore
        // pudesse ser filtrada como uma unidade — quando houver, é ele que passa
        // a carregar isto, e não este sítio.
        let fx = crate::painteffects::filtro(css.filter.as_deref().unwrap_or(""));
        // Compõe as duas numa função só, para que nenhum dos pontos de emissão
        // abaixo possa aplicar uma e esquecer a outra.
        let cor = |c: u32| apply_opacity(fx.aplicar(c), op);
        // Insere na ordem: primeiro o fundo, depois a borda por cima dele (ambos
        // atrás dos filhos). `insert` desloca os filhos para a frente.
        let mut at = box_index;
        // SOMBRA primeiro (atrás de tudo): box-shadow.
        if let Some(sh) = css.box_shadow {
            insert_item(
                list,
                at,
                filhos_antes_da_caixa,
                DisplayItem::Shadow {
                    rect: box_rect,
                    dx: sh.dx,
                    dy: sh.dy,
                    blur: sh.blur,
                    spread: sh.spread,
                    color: cor(sh.color),
                    radius,
                },
            );
            at += 1;
        }
        // FUNDO: gradiente (se houver) OU cor sólida — a menos que uma MÁSCARA
        // dê a forma da caixa (ver `deve_suprimir_fundo`).
        let fundo = !deve_suprimir_fundo(&css);
        if let Some(g) = css.gradient.filter(|_| fundo) {
            insert_item(
                list,
                at,
                filhos_antes_da_caixa,
                DisplayItem::GradientRect {
                    rect: box_rect,
                    c0: cor(g.c0),
                    c1: cor(g.c1),
                    angle_deg: g.angle_deg,
                    radius,
                },
            );
            at += 1;
        } else if let Some(color) = css.bg.filter(|_| fundo) {
            let color = cor(color);
            insert_item(
                list,
                at,
                filhos_antes_da_caixa,
                DisplayItem::SolidRect {
                    rect: box_rect,
                    color,
                    radius: cantos,
                },
            );
            at += 1;
        }
        for item in border_items(&css, box_rect, radius, op, fx) {
            insert_item(list, at, filhos_antes_da_caixa, item);
            at += 1;
        }
    }

    // ── SCROLL CONTAINER interno (#1744): se a div rola (overflow-x/y) e o conteúdo
    // excede a caixa, (1) RECORTA os itens dos filhos ao content-box (BeginClip já
    // emitido depois da caixa, EndClip no fim), (2) registra a ScrollRegion p/ o
    // backend gerenciar o offset + pintar as barras. `hidden` também recorta (corta o
    // excesso, sem barra). `visible` não faz nada (transborda, como hoje).
    let clips =
        ov_x != crate::scrollbar::Overflow::Visible || ov_y != crate::scrollbar::Overflow::Visible;
    if clips {
        let content_rect = Rect::new(content_x, content_y, content_w, content_h);
        // BeginClip no índice onde os FILHOS começam (logo após os itens de caixa que
        // foram inseridos em `box_index`); EndClip no fim. Quantos itens de caixa:
        // fundo (se bg) + borda (se visível).
        let box_items = if css.has_box() {
            // MESMA contagem da emissão acima: sombra + (gradiente OU bg) + as
            // barras de borda/outline. Estas últimas vêm de `border_items`, a mesma
            // função que as emitiu — contar por outra regra é o que dessincroniza
            // o índice do clip quando uma borda por lado entra em jogo.
            css.box_shadow.is_some() as usize
                + (css.gradient.is_some() || css.bg.is_some()) as usize
                + border_items(
                    &css,
                    box_rect,
                    css.corner_radius.unwrap_or(0.0),
                    1.0,
                    crate::painteffects::FilterMatriz::IDENTIDADE,
                )
                .len()
        } else {
            0
        };
        let children_start = box_index + box_items;
        // offset 0 aqui; o backend injeta o offset rolado por região antes de pintar.
        insert_item(
            list,
            children_start,
            filhos_antes_da_caixa,
            DisplayItem::BeginClip {
                rect: content_rect,
                node: id,
                offset_x: 0.0,
                offset_y: 0.0,
                filhos_antes: list.children.len(),
            },
        );
        list.items.push(DisplayItem::EndClip {
            filhos_dentro: list.children.len(),
        });
        if std::env::var_os("RTS_CLIP_DEBUG").is_some() && content_w <= 2.0 {
            let filhos: Vec<(usize, f32)> = list.children.iter().map(|c| (c.at, c.dy)).collect();
            eprintln!(
                "[clip] no={id:?} box_index={box_index} children_start={children_start} end_at={} children={:?}",
                list.items.len() - 1,
                &filhos[filhos.len().saturating_sub(6)..]
            );
        }
        // só registra como rolável (com barra) se de fato rola (auto/scroll), não hidden.
        if ov_x.scrollable() || ov_y.scrollable() {
            list.scroll_regions.push(ScrollRegion {
                node_idx: id,
                visible: content_rect,
                content_w: scroll_children_w.max(content_w),
                content_h: content_h_natural,
                overflow_x: ov_x,
                overflow_y: ov_y,
            });
        }
    }

    // ── CLIP-PATH: recorta o elemento a um retângulo. SÓ `inset()` sem `round`
    // chega aqui com um rect — as outras formas devolvem `None` e não recortam
    // nada, porque recortar um `polygon()` pela caixa envolvente desenharia um
    // quadrado onde devia estar um losango (ver `painteffects`).
    //
    // Emitido DEPOIS do bloco de overflow acima, e de propósito: inserir em
    // `box_index` empurra tudo o que vem a partir dali, e fazê-lo antes
    // desalinharia por um o `children_start` que aquele bloco calcula. Como o
    // `EndClip` deste é empilhado no fim, o aninhamento sai certo — este abre
    // primeiro e fecha por último, portanto envolve o clip de scroll.
    //
    // A diferença para o clip de overflow é onde ABRE: aquele recorta só os
    // FILHOS (abre depois dos itens de caixa), este recorta o elemento INTEIRO,
    // fundo e borda incluídos, que é o que o `clip-path` do CSS faz. Daí abrir
    // em `box_index`.
    if let Some(cp) = css.clip_path.as_deref() {
        if let Some(rect) = crate::painteffects::clip_retangulo(cp, box_rect) {
            insert_item(
                list,
                box_index,
                filhos_antes_da_caixa,
                DisplayItem::BeginClip {
                    rect,
                    node: id,
                    offset_x: 0.0,
                    offset_y: 0.0,
                    // Os fragmentos-filhos que existiam quando a CAIXA foi
                    // reservada — não `list.children.len()` de agora, que já
                    // conta os desta subárvore. O clip abre conceptualmente
                    // antes deles, mesmo sendo inserido depois de existirem.
                    filhos_antes: filhos_antes_da_caixa,
                },
            );
            list.items.push(DisplayItem::EndClip {
                filhos_dentro: list.children.len(),
            });
        }
    }

    // ── TRANSFORM (translate/scale/rotate): pós-processa os itens DESTE elemento e
    // seus descendentes (o range `[box_index..]`), em torno do CENTRO do border-box.
    // Aplicado por último (não afeta o fluxo/tamanho — como no CSS, transform é visual).
    if let Some(tf) = css.transform {
        if !tf.is_identity() {
            let cx = box_rect.x + box_rect.w / 2.0;
            let cy = box_rect.y + box_rect.h / 2.0;
            // translate em px + fração do tamanho do elemento (translate(-50%,-50%)).
            let tx = tf.tx + tf.tx_pct * box_rect.w;
            let ty = tf.ty + tf.ty_pct * box_rect.h;
            let (sin, cos) = tf.rot_deg.to_radians().sin_cos();
            // Um transform MUTA itens, e um item de subárvore reusada é
            // COMPARTILHADO — mutá-lo no lugar mudaria o desenho de todo mundo
            // que aponta para ele.
            //
            // Um TRANSLATE puro não precisa de achatar nada: a subárvore é
            // desenhada com um deslocamento que já existe no `ChildRef`, e somar
            // ao `dx`/`dy` dele é a mesma conta sem tocar no que é partilhado.
            //
            // Achatar aqui era um defeito com alcance muito além do elemento:
            // `materialize` reescreve `items` INTEIRO, e todos os índices que os
            // ancestrais reservaram para as caixas deles passam a apontar para
            // outro item. Um `position:absolute` com `transform:translateY(-50%)`
            // — uma regra de ícone, na folha do MediaWiki — punha a página
            // inteira da Wikipédia a zero: 16 813 elementos sem geometria porque
            // uma regra de 40 bytes casou com um `<span>`.
            let so_translate = tf.sx == 1.0 && tf.sy == 1.0 && tf.rot_deg == 0.0;
            if so_translate {
                for it in list.items[box_index..].iter_mut() {
                    translate_item(it, tx, ty);
                }
                for child in list.children.iter_mut().filter(|c| c.at >= box_index) {
                    child.dx += tx;
                    child.dy += ty;
                }
            } else {
                // Escala e rotação continuam a exigir os itens em mãos. Vale a
                // mesma ressalva de índices — por isso só quando não há
                // subárvore por referência para achatar.
                list.materialize();
                for it in list.items[box_index..].iter_mut() {
                    apply_transform_to_item(it, cx, cy, tx, ty, tf.sx, tf.sy, sin, cos);
                }
            }
        }
    }

    // Tamanho EXTERNO da caixa (outer = content + padding + border + margin) — cada
    // componente já é a SOMA do seu eixo (padding_h = left+right; margin_h idem;
    // border conta 2× pelos dois lados). Não multiplicar margin/padding por 2.
    let outer_w = content_w + padding_h + border_h + margin_h;
    let outer_h = content_h + pad_top + pad_bottom + border_v + margin_top + margin_bottom;
    (outer_w, outer_h)
}

/// Põe um filho-bloco do fluxo normal, REUSANDO o desenho dele quando nada que
/// o afete mudou.
///
/// É o layout incremental: `layout_epochs[nó]` sobe quando a subárvore muda (e
/// nos ancestrais dela), então um irmão intacto casa a chave e só precisa ser
/// deslocado. Numa lista de mil cartões em que um texto mudou, 999 cartões são
/// uma cópia de itens em vez de cascade + medição de texto + box model.
///
/// Só o fluxo VERTICAL normal entra aqui — sem `forced_outer_*` (flex) e sem
/// `shrink_to_fit`. Os outros caminhos dependem de negociação com os irmãos, e
/// um fragmento que ignorasse isso responderia errado.
#[allow(clippy::too_many_arguments)]
/// A chave do fragmento de um nó com certas constraints. Extraída porque o laço
/// do fluxo vertical CONSULTA o cache antes de classificar o filho: um fragmento
/// só existe para bloco-normal, então encontrá-lo já responde o que a
/// classificação responderia — e a classificação custa estilo computado,
/// `block::lookup` e a margem resolvida, mil vezes por frame.
fn fragment_key(
    dom: &Dom,
    id: NodeIdx,
    avail_w: f32,
    avail_h: Option<f32>,
    ctx: &LayoutCtx,
) -> crate::dom::FragmentKey {
    KeyBase::new(dom, avail_w, avail_h, ctx).key(dom, id)
}

/// A parte da chave de fragmento que NÃO varia entre os filhos de um container:
/// identidade da árvore, epochs globais, viewport, medidor e as constraints.
///
/// Montar a chave inteira por filho relia um `thread_local` (o epoch de estilo)
/// e refazia as conversões mil vezes por container — o laço do fluxo vertical
/// pergunta o mesmo a cada iteração e só o nó muda.
#[derive(Clone, Copy)]
struct KeyBase {
    tree: u64,
    style_epoch: u64,
    anim_epoch: u64,
    avail_w: u32,
    avail_h: Option<u32>,
    viewport_w: u32,
    viewport_h: u32,
    measurer: u64,
}

impl KeyBase {
    fn new(dom: &Dom, avail_w: f32, avail_h: Option<f32>, ctx: &LayoutCtx) -> KeyBase {
        KeyBase {
            tree: dom.cache_identity(),
            style_epoch: crate::style::props::style_epoch(),
            anim_epoch: dom.anim_epoch(),
            avail_w: avail_w.to_bits(),
            avail_h: avail_h.map(f32::to_bits),
            viewport_w: ctx.viewport_w.to_bits(),
            viewport_h: ctx.viewport_h.to_bits(),
            measurer: ctx.measurer.identity(),
        }
    }

    fn key(&self, dom: &Dom, id: NodeIdx) -> crate::dom::FragmentKey {
        crate::dom::FragmentKey {
            tree: self.tree,
            node_epoch: dom.layout_epoch(id),
            style_epoch: self.style_epoch,
            anim_epoch: self.anim_epoch,
            node: id,
            avail_w: self.avail_w,
            avail_h: self.avail_h,
            viewport_w: self.viewport_w,
            viewport_h: self.viewport_h,
            measurer: self.measurer,
        }
    }
}

/// Insere um item numa posição, corrigindo o ponto de entrada das SUBÁRVORES.
///
/// O box model emite os filhos primeiro e insere o fundo e a borda atrás deles;
/// as subárvores reusadas guardam o índice antes do qual entram, e sem esta
/// correção elas passariam a ser pintadas na frente do próprio fundo. Foi o que
/// um teste de altura percentual acusou, ao ver os retângulos na ordem trocada.
/// Insere um item em `at` e corrige o `at` das subárvores que ficam depois dele.
///
/// `filhos_antes` é quantas subárvores já existiam quando `at` foi RESERVADO, e
/// é o que distingue "esta subárvore é minha, empurra-a" de "esta subárvore já
/// cá estava, não lhe toques". Sem essa fronteira, um `at >= at` sozinho não
/// consegue separar os dois casos quando os índices coincidem — e coincidem
/// exatamente no caso que interessa: um `position:fixed`, pintado no fim do
/// documento, reserva o índice 0 da lista de topo, que é também o `at` do
/// fragmento onde vive a página inteira. O fixed empurrava a página para
/// depois de si e ficava ATRÁS dela; numa página real é o dropdown a
/// desaparecer por trás do conteúdo.
///
/// É a mesma distinção que o `BeginClip { filhos_antes }` já fazia, e pela mesma
/// razão: o índice sozinho não carrega a ordem de criação.
pub(crate) fn insert_item(
    list: &mut DisplayList,
    at: usize,
    filhos_antes: usize,
    item: DisplayItem,
) {
    list.items.insert(at, item);
    for child in list.children.iter_mut().skip(filhos_antes) {
        if child.at >= at {
            child.at += 1;
        }
    }
}

/// Reconstrói o fragmento de um container trocando SÓ as subárvores sujas.
///
/// Devolve `None` — e o chamador refaz tudo — quando alguma premissa não vale:
/// o próprio nó foi alvo da invalidação (o estilo DELE pode ter mudado); não há
/// desenho anterior ou ele não tinha subárvores; a sujeira não tem alvo ou está
/// espalhada demais; a lista de filhos mudou; ou a subárvore refeita mudou de
/// ALTURA ou de margem, e aí tudo abaixo dela desloca.
fn costurar(
    dom: &Dom,
    id: NodeIdx,
    key: crate::dom::FragmentKey,
    ctx: &LayoutCtx,
) -> Option<std::rc::Rc<Fragment>> {
    if dom.is_self_dirty(id) {
        return None;
    }
    let (antiga, anterior) = dom.last_fragment_of(id)?;
    // Só o epoch do nó pode diferir: viewport, constraints, estilo global e
    // animação mudam o desenho inteiro, não uma parte dele.
    if (
        antiga.tree,
        antiga.avail_w,
        antiga.avail_h,
        antiga.viewport_w,
        antiga.viewport_h,
    ) != (
        key.tree,
        key.avail_w,
        key.avail_h,
        key.viewport_w,
        key.viewport_h,
    ) || (antiga.style_epoch, antiga.anim_epoch, antiga.measurer)
        != (key.style_epoch, key.anim_epoch, key.measurer)
    {
        return None;
    }
    if anterior.children.is_empty() {
        return None;
    }
    let sujos = dom.dirty_children_of(id)?;
    // A SEQUÊNCIA de filhos precisa ser a mesma, não só o tamanho: inserção,
    // remoção e reordenação mudam quem desenha o quê, e trocar uma referência
    // não daria conta. Comparar índice a índice é uma passada de leitura.
    if !mesma_sequencia_de_filhos(dom, id, &anterior.children) {
        return None;
    }
    let _phase = crate::metrics::phases::scope("fragment-patch");

    let mut children = anterior.children.clone();
    let mut trocou = false;
    for child in &mut children {
        if !sujos.contains(&child.node) {
            continue;
        }
        let mut own = DisplayList::default();
        // Onde o filho FOI POSTO: a origem em que o fragmento dele foi calculado
        // mais o deslocamento com que entrou aqui. Somar à origem do PAI daria
        // uma posição sem sentido — foi o que o teste de equivalência mostrou,
        // com o texto reaparecendo em (0,16) em vez de (12, 67.4).
        let origem = (
            child.fragment.origin.0 + child.dx,
            child.fragment.origin.1 + child.dy,
        );
        let margem = child.margin_top;
        let ((_, altura), nova_margem) = layout_block_reusing(
            dom,
            child.node,
            origem.0,
            origem.1,
            child.avail_w,
            child.avail_h,
            || margem,
            // A costura só alcança o que virou fragmento, e um bloco estorvado
            // por float nunca vira (ver o guard em `layout_block_reusing`).
            &[],
            ctx,
            &mut own,
        );
        if (altura - child.height).abs() > 0.001 || (nova_margem - child.margin_top).abs() > 0.001 {
            return None;
        }
        // O `layout_block_reusing` emitiu numa lista própria; o que interessa é a
        // referência que ele acabou de registrar para este nó.
        let novo = own.children.first()?.fragment.clone();
        child.fragment = novo;
        trocou = true;
    }
    if !trocou {
        return None;
    }
    let fragment = std::rc::Rc::new(Fragment {
        node: id,
        // Compartilha o que NÃO mudou — só a lista de subárvores é nova.
        items: std::rc::Rc::clone(&anterior.items),
        children,
        rects: std::rc::Rc::clone(&anterior.rects),
        hit_order: std::rc::Rc::clone(&anterior.hit_order),
        scroll_regions: anterior.scroll_regions.clone(),
        origin: anterior.origin,
        size: anterior.size,
        margin_top: anterior.margin_top,
    });
    dom.fragment_put(key, std::rc::Rc::clone(&fragment));
    Some(fragment)
}

/// `true` se os filhos-elemento do nó são exatamente os que o desenho anterior
/// referencia, na mesma ordem. Uma passada de leitura; o que não é barato é o
/// layout deles.
fn mesma_sequencia_de_filhos(dom: &Dom, id: NodeIdx, children: &[ChildRef]) -> bool {
    let mut esperados = children.iter().map(|c| c.node);
    let mut atuais = dom
        .node(id)
        .children
        .iter()
        .copied()
        .filter(|&c| matches!(dom.node(c).kind, NodeKind::Element { .. }));
    loop {
        match (esperados.next(), atuais.next()) {
            (None, None) => return true,
            (Some(a), Some(b)) if a == b => continue,
            _ => return false,
        }
    }
}

fn emit_fragment(
    fragment: &std::rc::Rc<Fragment>,
    list: &mut DisplayList,
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
) {
    let _phase = crate::metrics::phases::scope("fragment-emit");
    fragment.emit_at(list, x, y, avail_w, avail_h);
}

fn layout_block_reusing(
    dom: &Dom,
    id: NodeIdx,
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
    margem_de_topo: impl FnOnce() -> f32,
    exclusoes: &[Exclusao],
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> ((f32, f32), f32) {
    // Um bloco ESTORVADO por um float não entra no cache de fragmentos, nem sai
    // dele: a chave é feita das constraints (largura, altura, viewport) e a
    // banda livre não é nenhuma delas. Sem esta recusa, o parágrafo ao lado da
    // figura seria servido pela versão de largura cheia guardada antes — e o
    // contrário também, a versão estreita reusada longe do float. Acrescentar a
    // banda à chave era a outra saída; recusar custa só nos blocos que têm
    // float ao lado, que são poucos, e não põe um campo novo em todas as
    // chaves da página.
    if !exclusoes.is_empty() {
        let size = layout_block(
            dom, id, x, y, avail_w, avail_h, None, None, false, exclusoes, ctx, list,
        );
        return (size, margem_de_topo());
    }
    let key = fragment_key(dom, id, avail_w, avail_h, ctx);
    if let Some(fragment) = dom.fragment_get(key) {
        crate::bump!(fragment_hits);
        emit_fragment(&fragment, list, x, y, avail_w, avail_h);
        return (fragment.size, fragment.margin_top);
    }
    // COSTURA: trocar no desenho anterior só a subárvore que ficou suja. Agora
    // que a saída é uma ÁRVORE, costurar é substituir uma REFERÊNCIA num vetor
    // de mil entradas de 48 bytes — a primeira versão disto (revertida) copiava
    // 3000 itens com String e por isso não ganhava nada.
    if let Some(fragment) = costurar(dom, id, key, ctx) {
        crate::bump!(fragment_patches);
        emit_fragment(&fragment, list, x, y, avail_w, avail_h);
        return (fragment.size, fragment.margin_top);
    }
    crate::bump!(fragment_misses);
    let _phase = crate::metrics::phases::scope("fragment-build");
    // Lista PRÓPRIA: o fragmento precisa saber exatamente quais itens são dele,
    // e a única forma de saber isso é não misturá-los com os dos irmãos.
    let mut own = DisplayList::default();
    let size = layout_block(
        dom,
        id,
        x,
        y,
        avail_w,
        avail_h,
        None,
        None,
        false,
        &[],
        ctx,
        &mut own,
    );
    let fragment = std::rc::Rc::new(Fragment {
        node: id,
        rects: std::rc::Rc::new(
            own.node_rects
                .iter()
                .map(|(idx, rect)| (*idx, *rect))
                .collect(),
        ),
        hit_order: std::rc::Rc::new(std::mem::take(&mut own.hit_order)),
        scroll_regions: std::mem::take(&mut own.scroll_regions),
        items: std::rc::Rc::new(std::mem::take(&mut own.items)),
        children: std::mem::take(&mut own.children),
        origin: (x, y),
        size,
        margin_top: margem_de_topo(),
    });
    dom.fragment_put(key, std::rc::Rc::clone(&fragment));
    fragment.emit_at(list, x, y, avail_w, avail_h);
    (fragment.size, fragment.margin_top)
}

/// Uma subárvore emitida por referência dentro de uma lista ou de outro
/// fragmento.
#[derive(Clone, Debug)]
pub struct ChildRef {
    /// O nó que esta subárvore desenha — a costura precisa saber quem é.
    pub node: NodeIdx,
    /// Altura externa que ele ocupou e a margem de topo resolvida: se qualquer
    /// uma mudar ao refazê-lo, tudo abaixo desloca e a costura não serve.
    pub height: f32,
    pub margin_top: f32,
    /// As CONSTRAINTS com que ele foi layoutado — as do CONTEÚDO do pai, não as
    /// do pai. Refazer um filho com a largura do container em vez da do conteúdo
    /// dá uma caixa larga demais pela soma do padding e da margem.
    pub avail_w: f32,
    pub avail_h: Option<f32>,
    /// Posição em `items` ANTES da qual esta subárvore é pintada.
    pub at: usize,
    /// Posição em `hit_order` antes da qual a ordem de hit-test dela entra.
    ///
    /// Separada do `at` porque as duas sequências crescem por motivos
    /// diferentes: nem todo item de pintura registra um nó, e nem todo nó
    /// registrado pinta um item. Montar a ordem de hit-test com os próprios
    /// primeiro e os das subárvores depois inverte o z-order — foi o que o teste
    /// de `z-index` acusou.
    pub hit_at: usize,
    pub fragment: std::rc::Rc<Fragment>,
    pub dx: f32,
    pub dy: f32,
}

impl PartialEq for ChildRef {
    /// Compara CONTEÚDO — duas listas equivalentes podem ter chegado ao mesmo
    /// desenho por caminhos diferentes.
    fn eq(&self, other: &Self) -> bool {
        self.at == other.at
            && self.dx == other.dx
            && self.dy == other.dy
            && self.fragment.items == other.fragment.items
            && self.fragment.children == other.fragment.children
    }
}

/// O DESENHO de uma subárvore posta com certas constraints, guardado para ser
/// reusado numa posição diferente.
///
/// Coordenadas ABSOLUTAS, como saíram do layout: a origem em que foi calculado
/// fica registrada em `origin`, e reusar é somar a diferença. Guardar já
/// relativo daria na mesma e custaria uma passada extra na hora de gravar — o
/// caso comum é justamente reusar na MESMA posição (nada acima dele mudou de
/// altura), e aí a soma é zero e nem se percorre.
#[derive(Clone, Debug, Default)]
pub struct Fragment {
    /// O nó que este fragmento desenha.
    pub node: NodeIdx,
    /// Itens de pintura PRÓPRIOS desta subárvore.
    ///
    /// Os três vetores grandes são `Rc`: quando um container é COSTURADO, só a
    /// lista de subárvores muda, e clonar retângulos e ordem de hit-test de um
    /// container de mil filhos custaria mais do que a costura economiza.
    pub items: std::rc::Rc<Vec<DisplayItem>>,
    /// As subárvores que ela reusou, por referência — o desenho é uma árvore.
    pub children: Vec<ChildRef>,
    /// Geometria por nó (o que alimenta `getBoundingClientRect`).
    pub rects: std::rc::Rc<Vec<(NodeIdx, Rect)>>,
    /// Ordem de pintura para o hit-test (ancestral antes de descendente).
    pub hit_order: std::rc::Rc<Vec<NodeIdx>>,
    /// Regiões roláveis internas descobertas dentro da subárvore.
    pub scroll_regions: Vec<ScrollRegion>,
    /// Onde este fragmento foi calculado.
    pub origin: (f32, f32),
    /// Tamanho externo devolvido pelo `layout_block` (o que o chamador usa para
    /// avançar o cursor).
    pub size: (f32, f32),
    /// A MARGEM DE TOPO resolvida deste bloco, para o colapso com o irmão
    /// anterior.
    ///
    /// Guardada junto porque o laço a calculava ANTES de descobrir que o
    /// fragmento servia: resolver a margem pede o estilo computado, o
    /// `font-size` do contexto e um `ResolveCtx` — por filho, mil vezes por
    /// frame, para um valor que não muda enquanto o epoch do nó não muda.
    pub margin_top: f32,
}

impl Fragment {
    /// Emite este fragmento numa `DisplayList`, deslocado para `(x, y)`.
    pub fn emit_at(
        self: &std::rc::Rc<Self>,
        list: &mut DisplayList,
        x: f32,
        y: f32,
        avail_w: f32,
        avail_h: Option<f32>,
    ) {
        let (dx, dy) = (x - self.origin.0, y - self.origin.1);
        // APONTA, não copia: os itens desta subárvore já existem e não mudaram.
        // Os RETÂNGULOS abaixo continuam sendo materializados, porque a consulta
        // de geometria é por nó e precisa do valor pronto — são 16 bytes contra
        // os 48 de um item, numa quantidade menor.
        list.children.push(ChildRef {
            node: self.node,
            height: self.size.1,
            margin_top: self.margin_top,
            avail_w,
            avail_h,
            at: list.items.len(),
            hit_at: list.hit_order.len(),
            fragment: std::rc::Rc::clone(self),
            dx,
            dy,
        });
        // A GEOMETRIA da subárvore (retângulos, ordem de hit-test, regiões
        // roláveis) também fica na referência: materializá-la aqui era metade do
        // custo de um frame parado — três inserções em mapa por fragmento, mil
        // fragmentos. Quem precisa dela chama `geometry()`, que percorre a
        // árvore uma vez e guarda o resultado.
    }
}

impl Fragment {
    /// Quantos itens este fragmento pinta, contando as subárvores que ele reusa.
    pub fn total_items(&self) -> usize {
        self.items.len()
            + self
                .children
                .iter()
                .map(|c| c.fragment.total_items())
                .sum::<usize>()
    }
}

/// Empilha os filhos VERTICAL (cada um abaixo do anterior), ocupando a largura do
/// content. Devolve a altura TOTAL do content (soma das alturas dos filhos).
/// `avail_h` = altura do content DESTE container quando explícita (containing
/// block dos filhos p/ `height:%`).
// as macros de estado (close_floats!/flush_inline!) escrevem no cursor a cada
// fechamento — a ÚLTIMA atribuição (no flush final) é estruturalmente morta, o
// que dispara unused_assignments sem haver bug.
/// As duas margens adjacentes colapsadas numa só, pela regra do CSS 2.1 §8.3.1.
///
/// Não é `max(a, b)`: essa é a regra apenas quando as DUAS são positivas.
/// - as duas ≥ 0 → a maior;
/// - as duas < 0 → a mais negativa (a que puxa mais);
/// - uma de cada sinal → a SOMA, e é por isso que uma margem negativa cancela
///   uma positiva em vez de ser ignorada por ela.
fn colapso_de_margens(a: f32, b: f32) -> f32 {
    if a >= 0.0 && b >= 0.0 {
        a.max(b)
    } else if a < 0.0 && b < 0.0 {
        a.min(b)
    } else {
        a + b
    }
}

/// Quanto é que a soma das duas margens excede a colapsada — o que há a
/// descontar ao cursor, já que cada bloco traz a sua margem dentro da altura.
///
/// A forma antiga era `min(a, b)`, que dá o mesmo resultado enquanto as duas
/// forem positivas e o resultado ERRADO assim que uma é negativa: com `a = 0` e
/// `b = -10px` descontava −10, ou seja SOMAVA 10 ao cursor, e a margem negativa
/// que devia puxar o bloco para cima empurrava-o para baixo. É o `margin-top`
/// negativo dos gutters `.row` do Bootstrap, e a razão de um teste que o pinava
/// ter começado a falhar.
fn excesso_de_margens(a: f32, b: f32) -> f32 {
    a + b - colapso_de_margens(a, b)
}

#[allow(unused_assignments)]
fn layout_children_vertical(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    avail_h: Option<f32>,
    css: &ComputedStyle,
    font_size: f32,
    // Os floats abertos HERDADOS do contexto de cima (ver `layout_block`).
    herdadas: &[Exclusao],
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let mut child_y = content_y;
    // A base da chave de fragmento é a mesma para todos os filhos deste
    // container — só o nó e o epoch dele mudam.
    let key_base = KeyBase::new(dom, content_w, avail_h, ctx);
    // MARGIN-COLLAPSE: as margens verticais de blocos ADJACENTES colapsam numa
    // só, não somam. Como o `outer_h` de cada bloco já inclui a sua margem dos
    // dois lados, ao empilhar dois blocos a soma conta
    // `margin_bottom_anterior + margin_top_atual`; subtrai-se o excesso para
    // ficar a colapsada ([`colapso_de_margens`]). `prev_margin` guarda a margem
    // do último bloco posto.
    let mut prev_margin = 0.0f32;
    // ── FLOATS COMO EXCLUSÕES: cada float colocado deixa uma faixa vertical
    // ocupada de um dos lados, e o conteúdo seguinte CONTORNA-A em vez de descer
    // abaixo dela. Ver [`Exclusao`] para a medição no Chrome que fixa o modelo.
    // Os herdados vêm primeiro; os deste container são acrescentados depois, e
    // `proprios` é a fronteira entre uns e outros. A distinção importa no fecho:
    // `clear` desce abaixo de QUALQUER float que o estorve, incluindo os de
    // cima, mas este container só cresce para conter os SEUS — crescer para
    // conter um float do pai punha altura no sítio errado.
    let mut floats: Vec<Exclusao> = herdadas.to_vec();
    let proprios = floats.len();
    // Desce o cursor para BAIXO dos floats. Já não é "fechar a linha": é o que o
    // `clear` pede. Um irmão sem `clear` NÃO chama isto: passa ao lado do float.
    macro_rules! close_floats {
        ($y:expr) => {
            if let Some(fundo) = fundo_dos_floats(&floats) {
                $y = $y.max(fundo);
            }
        };
    }
    // ── CONTEXTO INLINE (P4): irmãos inline CONSECUTIVOS (texto + <a>/<b>/<span>)
    // fluem JUNTOS numa sequência de linhas — acumulados aqui e descarregados por
    // `flush_inline!` quando um bloco/float/fim interrompe o fluxo.
    let mut inline_group: Vec<NodeIdx> = Vec::new();
    // Corrida de INLINE-BLOCKS consecutivos (botões/pills lado a lado). Pintada
    // por `flush_ib` — mede cada um (shrink), põe lado a lado quebrando linha ao
    // encher, e alinha a linha pelo text-align do pai (center do google).
    let mut ib_run: Vec<NodeIdx> = Vec::new();
    macro_rules! flush_ib {
        ($y:expr) => {
            if !ib_run.is_empty() {
                $y = layout_inline_block_line(
                    dom, &ib_run, content_x, $y, content_w, avail_h, css, ctx, list,
                );
                ib_run.clear();
                prev_margin = 0.0;
            }
        };
    }
    macro_rules! flush_inline {
        ($y:expr) => {
            if !ib_run.is_empty() {
                flush_ib!($y);
            }
            if !inline_group.is_empty() {
                // NÃO desce abaixo dos floats: as linhas CONTORNAM-NOS. As
                // exclusões vão com o grupo — é a travessia de camada que o
                // comentário de `layout_inline_flow` justifica.
                $y = layout_inline_flow(
                    dom,
                    id,
                    &inline_group,
                    content_x,
                    $y,
                    content_w,
                    css,
                    font_size,
                    &floats,
                    ctx,
                    list,
                );
                inline_group.clear();
                prev_margin = 0.0; // texto quebra a sequência de margin-collapse
            }
        };
    }
    for &child in &dom.node(id).children {
        // CAMINHO RÁPIDO: se existe fragmento para este filho com estas
        // constraints, ele já foi classificado como BLOCO NORMAL quando foi
        // criado — é o único caminho que produz fragmento. Encontrá-lo responde
        // a classificação inteira, que custaria estilo computado,
        // `block::lookup` e a margem resolvida por filho: mil vezes por frame
        // numa lista, para redescobrir o que não mudou.
        // `floats.is_empty()`: um bloco com float ao lado não pode ser servido
        // pelo fragmento guardado — ele foi medido com a linha inteira e a banda
        // livre não faz parte da chave. É a mesma recusa de
        // `layout_block_reusing`, no caminho rápido que a antecede.
        if floats.is_empty() && matches!(dom.node(child).kind, NodeKind::Element { .. }) {
            let key = key_base.key(dom, child);
            if let Some(fragment) = dom.fragment_get(key) {
                crate::bump!(fragment_hits);
                flush_inline!(child_y);
                child_y -= excesso_de_margens(prev_margin, fragment.margin_top);
                emit_fragment(&fragment, list, content_x, child_y, content_w, avail_h);
                child_y += fragment.size.1;
                prev_margin = fragment.margin_top;
                continue;
            }
        }
        let child_css = match &dom.node(child).kind {
            NodeKind::Element { .. } => Some(dom.computed_style_idx(child).unwrap_or_default()),
            _ => None,
        };
        let child_out = child_css
            .as_ref()
            .and_then(|c| c.position)
            .map(|p| p.out_of_flow())
            .unwrap_or(false);
        let child_float = child_css
            .as_ref()
            .and_then(|c| c.float_side)
            .unwrap_or(crate::style::FloatSide::None);
        // `clear` — o par do `float`: este filho começa ABAIXO dos floats
        // correntes. Fica ANTES do dispatch por tipo de caixa porque vale para
        // qualquer um deles: o caminho de bloco já fechava a linha de floats
        // sempre, mas um inline-block ou um texto com `clear` não fechava nada e
        // acabava por cima do float. Os três valores agem como `both` (ver
        // `style::text::Clear` para porquê).
        if child_css
            .as_ref()
            .and_then(|c| c.clear)
            .map(|c| c.clears())
            .unwrap_or(false)
        {
            flush_inline!(child_y);
            close_floats!(child_y);
        }
        let (child_block, child_inline_block) = match &dom.node(child).kind {
            NodeKind::Element { tag } => {
                let replaced = (tag == "img" && dom.image_of(child).is_some())
                    || tag == "svg"
                    || tag == "canvas";
                let effective = child_css.as_ref().and_then(|c| c.effective_display());
                // "é de bloco?" e NÃO "não é inline?" — e o `InlineBlock` é o
                // valor que as duas leituras separam. Por `d != Inline` um
                // `display:inline-block` contava como bloco: o elemento saía do
                // fluxo da linha, empilhava-se em vez de fluir e tomava a largura
                // do contentor. Um `<span style="display:inline-block">` entre
                // duas palavras descia para a linha seguinte, e a caixa que o
                // browser põe ao lado do texto ficava sozinha numa linha só.
                //
                // Esta é a QUINTA aparição da mesma pergunta mal posta, e as
                // outras quatro estão em `is_block_level`, `is_inline_block` e
                // duas decisões de fluxo. A causa é esta cópia: o laço reescreve
                // à mão o que `is_inline_block` já responde, em vez de lhe
                // perguntar. Substituir a cópia pela chamada é a correção de
                // fundo e muda mais do que o inline-block — fica para um lote
                // próprio, medido à parte, para que o efeito seja atribuível.
                let explicit_block = effective
                    .map(|d| {
                        d != crate::style::DisplayKind::Inline
                            && d != crate::style::DisplayKind::InlineBlock
                    })
                    .unwrap_or(false);
                // `display:inline` DECLARADO vence a tag e a UA-stylesheet: um
                // `<h3 style="display:inline">` — a forma dos cabeçalhos
                // colapsáveis do MediaWiki — é conteúdo de linha e mede o seu
                // texto, não os 752px do contentor. `effective.is_some()`
                // respondia "há display declarado", não "é de bloco", e por ela
                // entrava também o inline.
                // A pergunta que resta é `cria_caixa_apesar_de_inline` e não
                // `has_box()`: esta última conta a margem e a `height` que o
                // próprio `display:inline` torna inoperantes, e devolvia o
                // elemento ao caminho de bloco de onde a declaração o tirou.
                let inline_declarado = effective == Some(crate::style::DisplayKind::Inline);
                let block = if inline_declarado {
                    replaced
                        || child_css
                            .as_ref()
                            .map(|c| crate::inline_box::cria_caixa_apesar_de_inline(c))
                            .unwrap_or(false)
                } else {
                    replaced
                        || effective.is_some()
                        || crate::block::lookup(tag).is_some()
                        || child_css
                            .as_ref()
                            .map(|c| c.has_box() || c.height.is_some())
                            .unwrap_or(false)
                };
                let inline_block =
                    // Um `display:inline-block` DECLARADO responde antes da TAG:
                    // `.mw-list-item{display:inline-block}` sobre um `<li>` batia
                    // no `block::lookup("li")` e voltava ao caminho de bloco, com
                    // os itens do menu empilhados e cada um com a largura do
                    // contentor. São 27 dos 55 inline-blocks desta página.
                    if effective == Some(crate::style::DisplayKind::InlineBlock) {
                        true
                    } else if matches!(tag.as_str(), "input" | "button" | "select" | "textarea") {
                        !explicit_block
                    } else if crate::block::lookup(tag).is_some() || explicit_block {
                        false
                    } else {
                        child_css
                            .as_ref()
                            .map(|c| c.has_box() || c.height.is_some())
                            .unwrap_or(false)
                    };
                (block, inline_block)
            }
            _ => (false, false),
        };
        match &dom.node(child).kind {
            // Metadata não-renderável (`<head>`/`<title>`/`<style>`/`<script>`):
            // pula — NÃO coleta seu texto como inline (senão o título e o CSS cru
            // vazam pra tela). Checado ANTES do caminho inline.
            NodeKind::Element { tag } if is_non_rendered_tag(tag) => {}
            // Fora do fluxo (`position:absolute/fixed`): não ocupa espaço aqui —
            // pintado na passada out-of-flow de layout_document.
            NodeKind::Element { .. } if child_out => {}
            // FLOAT left/right: encosta ao lado pedido, na primeira faixa a
            // partir do cursor onde CAIBA ao lado dos floats já postos.
            NodeKind::Element { .. } if child_float != crate::style::FloatSide::None => {
                flush_inline!(child_y);
                let side = child_float;
                let w = child_outer_width(dom, child, content_w, font_size, ctx);
                let h = child_outer_height(dom, child, content_w, avail_h, css, font_size, ctx);
                // Onde cabe: tenta o cursor; se a banda livre aí é estreita
                // demais, desce para o fundo de cada float que a estorva, pela
                // ordem em que eles acabam. Dois floats do mesmo lado que cabem
                // lado a lado continuam lado a lado — é o header brand+nav do
                // Bootstrap, e é o que a primeira tentativa já responde.
                let mut top = child_y;
                let mut fundos: Vec<f32> = floats.iter().map(|e| e.bottom).collect();
                fundos.sort_by(f32::total_cmp);
                let (mut bx, mut bw) = banda_livre(&floats, top, h, content_x, content_w);
                for f in fundos {
                    if bw >= w || f <= top {
                        continue;
                    }
                    top = f;
                    (bx, bw) = banda_livre(&floats, top, h, content_x, content_w);
                }
                let x = if side == crate::style::FloatSide::Left {
                    bx
                } else {
                    bx + bw - w
                };
                layout_block(
                    dom,
                    child,
                    x,
                    top,
                    content_w,
                    avail_h,
                    None,
                    None,
                    true,
                    &[],
                    ctx,
                    list,
                );
                floats.push(Exclusao {
                    top,
                    bottom: top + h,
                    side,
                    edge: if side == crate::style::FloatSide::Left {
                        x + w
                    } else {
                        x
                    },
                });
                prev_margin = 0.0; // float quebra a sequência de collapse
            }
            NodeKind::Element { .. } if child_block && !child_inline_block => {
                flush_inline!(child_y);
                // Sem `close_floats!`: pelo CSS a caixa de bloco ao lado de um
                // float NÃO desce nem encolhe — mantém a largura e sobrepõe-se
                // ao float; quem encolhe são as linhas lá dentro. Ver
                // [`Exclusao`] para os números do Chrome que o fixam.
                // margin VERTICAL TOP do filho (para o collapse com o anterior):
                // margin.top + margin_v da UA.
                let m = child_css
                    .as_ref()
                    .map(|c| {
                        // margem TOP do filho p/ o collapse (unidades relativas
                        // resolvem contra o content deste container).
                        let r = ResolveCtx {
                            parent_content_w: content_w,
                            node_font_size: font_px(&c, font_size),
                            root_font_size: crate::style::root_font_size(),
                            viewport_w: ctx.viewport_w,
                            viewport_h: ctx.viewport_h,
                        };
                        let mv = if c.margin.top == crate::style::Side::Unset {
                            c.margin_v.unwrap_or(0.0)
                        } else {
                            0.0
                        };
                        c.margin.top.resolve(&r).unwrap_or(0.0) + mv
                    })
                    .unwrap_or(0.0);
                // Colapsa com o bloco anterior: recua o overlap antes de posicionar.
                child_y -= excesso_de_margens(prev_margin, m);
                let ((_, h), _) = layout_block_reusing(
                    dom,
                    child,
                    content_x,
                    child_y,
                    content_w,
                    avail_h,
                    || m,
                    &floats,
                    ctx,
                    list,
                );
                child_y += h;
                prev_margin = m;
            }
            // INLINE-BLOCK (pill/botão solto): NÃO pinta agora — acumula na
            // "linha de inline-blocks" corrente (irmãos consecutivos fluem LADO A
            // LADO, quebrando quando enche). Os botões 'Pesquisa Google'/'Estou
            // com sorte' do google são 2 inline-block irmãos que compartilham a
            // linha. Um texto/inline entre eles fecha a corrida (flush_inline).
            // Um inline-block RODEADO DE TEXTO é conteúdo de linha, não uma
            // corrida própria: entra no grupo inline e o `wrap_runs` trata-o como
            // palavra inquebrável. A corrida (`ib_run`) fica para o que ela
            // existe — inline-blocks IRMÃOS sem texto à volta, os botões do
            // google. Sem esta distinção um `<span>` com fundo no meio de um
            // parágrafo fechava o fluxo e abria linha nova.
            NodeKind::Element { .. }
                if child_inline_block && em_contexto_inline(dom, id, child) =>
            {
                flush_ib!(child_y);
                inline_group.push(child);
            }
            NodeKind::Element { .. } if child_inline_block => {
                // descarrega só o TEXTO inline pendente (não o ib_run — este b
                // continua a acumular os inline-blocks IRMÃOS na mesma corrida).
                if !inline_group.is_empty() {
                    child_y = layout_inline_flow(
                        dom,
                        id,
                        &inline_group,
                        content_x,
                        child_y,
                        content_w,
                        css,
                        font_size,
                        &floats,
                        ctx,
                        list,
                    );
                    inline_group.clear();
                }
                ib_run.push(child);
                prev_margin = 0.0;
            }
            // Whitespace estrutural continua no DOM, mas não cria uma linha entre
            // blocos/floats. Quando está perto de texto/inline, entra no grupo e o
            // `wrap_runs` o colapsa como um espaço normal.
            NodeKind::Text(t)
                if t.trim().is_empty() && !whitespace_is_inline_separator(dom, id, child) => {}
            // Texto / elemento inline: entra no CONTEXTO INLINE corrente — flui
            // com os irmãos inline adjacentes (o flush pinta o grupo inteiro).
            _ => {
                flush_ib!(child_y); // fecha a corrida de inline-blocks
                inline_group.push(child);
            }
        }
    }
    // descarrega o fluxo inline pendente e cresce para conter os floats DESTE
    // container. ⚠️ DIVERGÊNCIA CONHECIDA, não é um bug à espera de correção:
    // pelo CSS um float só faz o pai crescer se o pai for um BFC (`overflow`,
    // `flow-root`, flex, tabela) ou houver clearfix; aqui cresce sempre. Foi
    // decidido manter — mexer nisso muda a altura de TODO o contentor com float
    // e é um segundo eixo de regressão por cima deste lote. O BFC é outro lote,
    // com medição própria. Ver `float_left_right_dividem_a_linha`.
    flush_inline!(child_y);
    if let Some(fundo) = fundo_dos_floats(&floats[proprios..]) {
        child_y = child_y.max(fundo);
    }
    (child_y - content_y).max(0.0)
}

/// Pinta uma CORRIDA de inline-blocks consecutivos (botões/pills irmãos) numa
/// sequência de linhas horizontais: mede cada um (shrink, numa lista descartável),
/// põe lado a lado enquanto cabe na `content_w`, quebra linha quando enche, e
/// alinha CADA linha pelo `text-align` do pai (center do google centra os botões).
/// Devolve o novo `y` (abaixo da última linha). Vazio → devolve `y`.
#[allow(clippy::too_many_arguments)]
fn layout_inline_block_line(
    dom: &Dom,
    run: &[NodeIdx],
    content_x: f32,
    y: f32,
    content_w: f32,
    avail_h: Option<f32>,
    parent_css: &ComputedStyle,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    // 1) mede a largura+altura desejada (shrink) de cada item numa lista descartável.
    let mut sizes: Vec<(NodeIdx, f32, f32)> = Vec::with_capacity(run.len());
    for &child in run {
        let (w, h) = measure_block(dom, child, content_w, avail_h, None, None, true, ctx);
        sizes.push((child, w, h));
    }
    // 2) agrupa em LINHAS (soma das larguras ≤ content_w). Cada linha guarda os
    //    itens + a largura total (p/ o alinhamento).
    let mut lines: Vec<(Vec<(NodeIdx, f32, f32)>, f32)> = Vec::new();
    let mut cur: Vec<(NodeIdx, f32, f32)> = Vec::new();
    let mut cur_w = 0.0f32;
    for (child, w, h) in sizes {
        if !cur.is_empty() && cur_w + w > content_w {
            lines.push((std::mem::take(&mut cur), cur_w));
            cur_w = 0.0;
        }
        cur_w += w;
        cur.push((child, w, h));
    }
    if !cur.is_empty() {
        lines.push((cur, cur_w));
    }
    // 3) pinta cada linha: x inicial pelo text-align do pai, itens lado a lado;
    //    y avança pela ALTURA da linha (o item mais alto).
    let mut cy = y;
    for (items, line_w) in &lines {
        let free = (content_w - line_w).max(0.0);
        let mut x = match parent_css.text_align {
            Some(crate::style::TextAlign::Center) => content_x + free / 2.0,
            Some(crate::style::TextAlign::Right) => content_x + free,
            _ => content_x,
        };
        // `line_h` tem de ser conhecida ANTES de posicionar, porque é contra ela
        // que o `vertical-align` alinha — daí a passada de altura separada.
        let line_h = items.iter().fold(0.0f32, |acc, &(_, _, h)| acc.max(h));
        for &(child, w, h) in items {
            // `vertical-align`: a caixa desce dentro da altura da linha. O default
            // (`baseline`, e o não-declarado) mantém o topo, que é o que este
            // motor sempre fez — ver o corte em `style::text::VerticalAlign`.
            let dy = match dom.computed_style_idx(child).and_then(|c| c.vertical_align) {
                Some(crate::style::VerticalAlign::Middle) => (line_h - h) / 2.0,
                Some(crate::style::VerticalAlign::Bottom) => line_h - h,
                _ => 0.0,
            };
            layout_block(
                dom,
                child,
                x,
                cy + dy,
                content_w,
                avail_h,
                None,
                None,
                true,
                &[],
                ctx,
                list,
            );
            x += w;
        }
        cy += line_h;
    }
    cy
}

/// Um item do flex (pré-pass), com a BASE no eixo principal (flex-basis/width/
/// conteúdo, outer com margem), o MAIN size final (após grow/shrink) e os
/// fatores de flexibilidade lidos do estilo.
struct FlexItem {
    node: NodeIdx,
    /// tamanho BASE outer no eixo principal (antes de grow/shrink).
    base: f32,
    /// main size FINAL outer (após grow/shrink) — começa igual à base.
    main: f32,
    /// altura outer (cross) — re-medida com o main final quando ele muda.
    h: f32,
    /// `true` se é um nó de texto solto (pintado direto, não via layout_block).
    is_text: bool,
    /// `flex-grow` (0 = não cresce).
    grow: f32,
    /// `flex-shrink` (1 = default do CSS; texto solto não encolhe).
    shrink: f32,
    /// `align-self` do item (None = usa o align-items do container).
    align_self: Option<crate::style::AlignItems>,
    /// `order` (menor primeiro; empate = ordem do documento — sort estável).
    order: i32,
    /// o item PODE ser esticado pelo stretch (sem `height` explícito).
    can_stretch: bool,
}

/// A BASE outer de um item flex no eixo principal: `flex-basis` explícita
/// (resolvida como o width — respeita box-sizing) + margens; `auto`/ausente →
/// width/conteúdo ([`child_outer_width`]). O `.col` do Bootstrap tem basis `0%`
/// → a base é só o frame (e o grow distribui o espaço).
fn flex_base_outer(
    dom: &Dom,
    id: NodeIdx,
    container_w: f32,
    parent_font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    let font = font_px(&css, parent_font);
    let resolve = ResolveCtx {
        parent_content_w: container_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let basis = css.flex_basis.and_then(|d| match d {
        crate::style::Dimension::Auto => None,
        other => other.resolve(&resolve),
    });
    let Some(basis) = basis else {
        return child_outer_width(dom, id, container_w, parent_font, ctx);
    };
    let margin_h = css.margin.resolve_h(&resolve);
    if css.border_box.unwrap_or(false) {
        basis + margin_h // border-box: a basis JÁ é a caixa (pad+borda inclusos)
    } else {
        basis + margin_h + 2.0 * css.border_width.unwrap_or(0.0) + css.padding.resolve_h(&resolve)
    }
}

/// Dispõe os filhos HORIZONTAL (flex-row). Implementa gap, justify-content (eixo
/// principal) e align-items (eixo cruzado). Devolve a altura total do content.
///
/// - `wrap = false` (flex sem wrap): tudo numa linha; justify distribui o espaço
///   livre; em overflow, cai para flex-start (transborda no fim).
/// - `wrap = true` (inline-block/flex-wrap): quebra para a próxima linha quando não
///   cabe; justify/align aplicam POR LINHA.
fn layout_children_horizontal(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    // altura do CONTENT do container quando explícita (já resolvida pelo caller,
    // no eixo certo) — referência do cross-axis p/ align-items e containing block
    // dos filhos.
    container_content_h: Option<f32>,
    css: &ComputedStyle,
    font_size: f32,
    wrap: bool,
    // `Some(N)` quando `display:grid`: cada item vira uma coluna de largura fixa
    // `(content_w - (N-1)*gap)/N` e a linha quebra a cada N. `None` = flex/wrap normal.
    grid_cols: Option<i32>,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    // gap/row-gap resolvidos do CSS (px/%/… contra o content do container).
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_size,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let gap = css
        .gap
        .and_then(|d| d.resolve(&resolve))
        .unwrap_or(0.0)
        .max(0.0);
    let row_gap = css
        .row_gap
        .and_then(|d| d.resolve(&resolve))
        .unwrap_or(0.0)
        .max(0.0);
    let justify = css
        .justify
        .unwrap_or(crate::style::JustifyContent::FlexStart);
    let align = css.align_items.unwrap_or(crate::style::AlignItems::Stretch);
    // `0` = sem height explícito (o cross-size da linha usa o max dos itens).
    let container_cross_h = container_content_h.unwrap_or(0.0);

    // ── PRÉ-PASS: coleta cada filho renderável com a BASE flex + fatores ─────────
    let mut items: Vec<FlexItem> = Vec::new();
    for &child in &dom.node(id).children {
        if let NodeKind::Element { tag } = &dom.node(child).kind {
            if is_non_rendered_tag(tag) {
                continue;
            }
        }
        // fora do fluxo: não é item flex (pintado na passada out-of-flow).
        if is_out_of_flow(dom, child) {
            continue;
        }
        // `display:none` não é item de flex: não conta para o wrap, não come um
        // `gap` e não recebe main size. `layout_block` já lhe dava caixa zero, o
        // que escondia o defeito — a caixa era invisível mas o LUGAR dela não.
        if e_display_none(dom, child) {
            continue;
        }
        // BLOCKIFICAÇÃO: um filho de flex é um item de nível BLOCO, mesmo sendo
        // um `<span>` (a spec blockifica os itens de flex; o Chrome reporta
        // `display:block` neles). Só um NÓ DE TEXTO é item anónimo.
        //
        // A condição era `!is_block_level`, e por isso um `<span>` filho de flex
        // caía no ramo de texto: era achatado para uma string, pintado com o
        // estilo do CONTAINER, e não registava caixa nenhuma — 345 dos 351
        // elementos `display:block` sem caixa da Wikipédia eram exatamente isto.
        if matches!(dom.node(child).kind, NodeKind::Text(_)) {
            // texto solto: largura medida; vazio é ignorado. Não cresce nem encolhe.
            let text = collect_text(dom, child);
            if text.trim().is_empty() {
                continue;
            }
            let w = ctx
                .measurer
                .text_width(&text, font_size, false, false, false);
            let h = crate::inline_box::altura_da_linha(css, font_size, ctx.measurer);
            items.push(FlexItem {
                node: child,
                base: w,
                main: w,
                h,
                is_text: true,
                grow: 0.0,
                shrink: 0.0,
                align_self: None,
                order: 0,
                can_stretch: false,
            });
            continue;
        }
        let ccss = dom.computed_style_idx(child).unwrap_or_default();
        let base = flex_base_outer(dom, child, content_w, font_size, ctx);
        let h = child_outer_height(
            dom,
            child,
            content_w,
            container_content_h,
            css,
            font_size,
            ctx,
        );
        items.push(FlexItem {
            node: child,
            base,
            main: base,
            h,
            is_text: false,
            grow: ccss.flex_grow.unwrap_or(0.0),
            shrink: ccss.flex_shrink.unwrap_or(1.0), // 1 é o default do CSS
            align_self: ccss.align_self,
            order: ccss.order.unwrap_or(0),
            can_stretch: ccss.height.is_none(),
        });
    }
    // `order` reordena ANTES do wrap (sort estável: empate = ordem do documento).
    items.sort_by_key(|it| it.order);

    // GRID: cada item (não-texto) vira uma coluna de largura fixa. Fixa base=main=col_w
    // e zera grow/shrink (a coluna não flui) → o wrap abaixo quebra a cada N colunas.
    if let Some(n) = grid_cols {
        let n = n.max(1) as f32;
        let col_w = ((content_w - (n - 1.0) * gap) / n).max(0.0);
        for it in items.iter_mut() {
            if it.is_text {
                continue;
            }
            it.base = col_w;
            it.main = col_w;
            it.grow = 0.0;
            it.shrink = 0.0;
        }
    }

    // agrupa em LINHAS pela BASE (o wrap decide pelas bases; grow/shrink POR linha).
    let mut lines: Vec<Vec<FlexItem>> = vec![Vec::new()];
    let mut line_w = 0.0f32;
    for it in items {
        let cur = lines.last_mut().unwrap();
        let with_gap = if cur.is_empty() { 0.0 } else { gap };
        if wrap && !cur.is_empty() && line_w + with_gap + it.base > content_w {
            lines.push(Vec::new());
            line_w = it.base;
        } else {
            line_w += with_gap + it.base;
        }
        lines.last_mut().unwrap().push(it);
    }

    // ── RESOLVE + POSICIONA por linha: grow/shrink (main), justify, align ────────
    let mut line_y = content_y;
    for line in &mut lines {
        if line.is_empty() {
            continue;
        }
        let n = line.len();
        let total_gap = (n.saturating_sub(1)) as f32 * gap;

        // GROW/SHRINK (spec flexbox §9.7 simplificada): espaço livre positivo
        // distribui ∝ flex-grow (o `.col { flex:1 0 0% }` divide igual); negativo
        // encolhe ∝ shrink × base (itens maiores cedem mais), clamp ≥ 0.
        let sum_base: f32 = line.iter().map(|it| it.base).sum();
        let free_pre = content_w - sum_base - total_gap;
        let sum_grow: f32 = line.iter().map(|it| it.grow).sum();
        if free_pre > 0.0 && sum_grow > 0.0 {
            for it in line.iter_mut() {
                it.main = it.base + free_pre * it.grow / sum_grow;
            }
        } else if free_pre < 0.0 {
            let weighted: f32 = line.iter().map(|it| it.shrink * it.base).sum();
            if weighted > 0.0 {
                for it in line.iter_mut() {
                    it.main = (it.base + free_pre * (it.shrink * it.base) / weighted).max(0.0);
                }
            }
        }
        // re-mede a ALTURA com o main final (mais largura → menos linhas de texto);
        // só quando o main mudou (senão a medição do pré-pass vale).
        for it in line.iter_mut() {
            if !it.is_text && (it.main - it.base).abs() > 0.5 {
                let (_, h) = measure_block(
                    dom,
                    it.node,
                    content_w,
                    container_content_h,
                    Some(it.main),
                    None,
                    true,
                    ctx,
                );
                it.h = h;
            }
        }

        // Cross-size de referência da linha = max das alturas dos itens, MAS se o
        // container tem `height` explícito e a linha é única (no-wrap), o cross-size
        // é a ALTURA DO CONTENT do container (fiel ao Chrome). Em wrap, cada linha
        // usa seu próprio max (repartir o height entre linhas — corte documentado).
        let items_h = line.iter().fold(0.0f32, |a, it| a.max(it.h));
        let line_h = if !wrap && container_cross_h > items_h {
            container_cross_h
        } else {
            items_h
        };

        // justify-content sobre o espaço restante PÓS-grow (com grow>0 o free é 0
        // e o justify é neutro — correto). Em overflow, ver justify_offsets.
        let sum_main: f32 = line.iter().map(|it| it.main).sum();
        let free = content_w - sum_main - total_gap;
        let (leading, between) = justify_offsets(justify, free, n);

        let mut x = content_x + leading;
        for (j, it) in line.iter().enumerate() {
            if j > 0 {
                x += gap + between;
            }
            // align por item: `align-self` vence o `align-items` do container;
            // STRETCH real: item sem height explícito ganha a ALTURA DA LINHA
            // (forced_outer_h) — os cards `.col` preenchem a linha.
            let item_align = it.align_self.unwrap_or(align);
            let stretches = item_align == crate::style::AlignItems::Stretch
                && it.can_stretch
                && !it.is_text
                && line_h > it.h;
            let off_cross = if stretches {
                0.0
            } else {
                align_offset(item_align, line_h, it.h)
            };
            let item_y = line_y + off_cross;
            if it.is_text {
                let text = collect_text(dom, it.node);
                let color = cor_visivel(&css, css.color.unwrap_or(0x000000FF));
                list.items.push(DisplayItem::Text {
                    x,
                    y: item_y,
                    text: text.into(),
                    color,
                    size: font_size,
                    mono: false,
                    bold: css.bold.unwrap_or(false),
                    italic: italico(Some(&css), tag_de(dom, it.node), false),
                    letter_spacing: css.letter_spacing.unwrap_or(0.0),
                    decoration: decoration_code(css),
                });
            } else {
                // o main resolvido é IMPOSTO ao item (grow/shrink venceram o
                // width); stretch impõe a altura da linha.
                let forced_h = if stretches { Some(line_h) } else { None };
                layout_block(
                    dom,
                    it.node,
                    x,
                    item_y,
                    content_w,
                    container_content_h,
                    Some(it.main),
                    forced_h,
                    true,
                    &[],
                    ctx,
                    list,
                );
            }
            x += it.main;
        }
        line_y += line_h + row_gap;
    }
    // desconta o último row_gap (só ENTRE linhas, não após a última).
    let total_h = (line_y - row_gap - content_y).max(0.0);
    total_h
}

/// Dispõe os filhos como FLEX COLUMN (`display:flex; flex-direction:column`): o
/// eixo PRINCIPAL é o vertical. Diferenças do block vertical: SEM margin-collapse
/// (flex não colapsa margens), `gap` entre itens (em column o espaçamento main é o
/// `row-gap`; o shorthand `gap:` seta ambos), `justify-content` distribui o espaço
/// livre VERTICAL (só quando o container tem altura explícita), `margin-top/bottom:
/// auto` de um item ABSORVE o espaço livre (spec flexbox §8.1 — é o `mb-auto`/
/// `mt-auto` do Bootstrap empurrando header/footer para as pontas), e `align-items`
/// atua no X: `stretch` (default) = item ocupa a largura; start/center/end = item
/// shrink-to-fit deslocado. Devolve a altura natural do content.
/// ⚠️ Cortes: `column-reverse` dispõe como `column` (sem inverter); `flex-wrap` em
/// column (multi-coluna) trata como coluna única; `flex-grow/shrink/basis` ainda
/// fora (fatia própria).
/// GRID real (css-grid track-sizing simplificado): resolve as trilhas de coluna
/// (px/%/fr/auto) e de linha, faz auto-placement dos itens célula-a-célula
/// (row-by-row), e posiciona cada item na sua célula com `justify-items`
/// (horizontal) / `align-items` (vertical). Suporta o subset do MDN:
/// grid-template-columns/rows, grid-auto-rows, gap, repeat(N,...), minmax(→max),
/// fr. NÃO suporta: grid-column/row-span explícito, areas, auto-fill/fit reais,
/// dense. Um item sem placement explícito preenche a próxima célula livre.
#[allow(clippy::too_many_arguments)]
fn layout_children_grid(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    container_content_h: Option<f32>,
    css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_size,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let col_gap = css
        .gap
        .or(css.row_gap)
        .and_then(|d| d.resolve(&resolve))
        .unwrap_or(0.0)
        .max(0.0);
    let row_gap = css
        .row_gap
        .or(css.gap)
        .and_then(|d| d.resolve(&resolve))
        .unwrap_or(0.0)
        .max(0.0);

    // ── COLUNAS: resolve as trilhas ──────────────────────────────────────────────
    // Sem grid-template-columns explícito → 1 coluna 1fr (o container-do-logo do
    // google: single-column grid). Com N colunas do grid_columns legado (repeat) →
    // N trilhas 1fr.
    let areas = css.grid_template_areas.clone();
    let col_tracks: Vec<crate::style::GridTrack> = match &css.grid_template_columns {
        Some(t) => (**t).clone(),
        // Sem trilhas declaradas mas COM áreas, é a matriz que diz quantas colunas
        // existem — cair no default de 1 coluna empilharia lado e conteúdo, que é
        // exatamente o sintoma que as áreas existem para resolver.
        None => {
            let n = match &areas {
                Some(a) => a.cols,
                None => css.grid_columns.unwrap_or(1).max(1) as usize,
            };
            vec![crate::style::GridTrack::Fr(1.0); n]
        }
    };
    // O número de colunas vem da LISTA de trilhas e não dos tamanhos: os
    // tamanhos ainda não estão decididos, porque uma trilha intrínseca precisa de
    // saber que itens lhe calham — e para isso é preciso ter colocado os itens.
    // A ordem é: quantas colunas → colocar os itens → medir → dimensionar.
    let ncols = col_tracks.len().max(1);

    // ── ITENS: os filhos renderizáveis (auto-placement row-by-row) ───────────────
    let mut children: Vec<NodeIdx> = Vec::new();
    for &child in &dom.node(id).children {
        if let NodeKind::Element { tag } = &dom.node(child).kind {
            if is_non_rendered_tag(tag) {
                continue;
            }
        }
        if is_out_of_flow(dom, child) {
            continue;
        }
        if !is_block_level(dom, child) && collect_text(dom, child).trim().is_empty() {
            continue;
        }
        children.push(child);
    }
    if children.is_empty() {
        return 0.0;
    }
    let cells = place_grid_items(dom, &children, areas.as_deref(), ncols);

    // A largura INTRÍNSECA por coluna — só medida quando alguma trilha é
    // intrínseca, porque medir custa uma travessia por item e a esmagadora
    // maioria das grades é só `fr` e px.
    let precisa_medir = col_tracks
        .iter()
        .any(|t| matches!(t, crate::style::GridTrack::Auto));
    let conteudo: Option<Vec<f32>> = precisa_medir.then(|| {
        let mut w = vec![0.0f32; ncols];
        for c in &cells {
            // Um item que ATRAVESSA colunas não dita nenhuma delas sozinho: a
            // repartição do que ele pede pelas colunas que ocupa é a mesma
            // pergunta da tabela com `colspan`, e aqui não vale a complicação —
            // o que uma grade real tem em trilha intrínseca é a barra lateral,
            // que ocupa uma coluna só.
            if c.c1 - c.c0 != 1 || c.c0 >= ncols {
                continue;
            }
            w[c.c0] = w[c.c0].max(intrinsic_outer_width(dom, c.child, font_size, ctx));
        }
        w
    });
    let col_sizes = resolve_tracks(
        &col_tracks,
        content_w,
        col_gap,
        conteudo.as_deref(),
        &resolve,
    );
    // Uma linha DECLARADA pela matriz existe mesmo sem item nela (ela ainda empurra
    // as linhas seguintes pelo gap), daí o max com `areas.rows`.
    let nrows = cells
        .iter()
        .map(|c| c.r1)
        .max()
        .unwrap_or(1)
        .max(areas.as_ref().map(|a| a.rows).unwrap_or(0))
        .max(1);

    // ── LINHAS: altura de cada linha ─────────────────────────────────────────────
    // grid-template-rows explícito (px/%/fr/auto), senão grid-auto-rows, senão a
    // altura do conteúdo mais alto da linha. `fr`/`%` de linha precisam da altura
    // do container (container_content_h).
    let explicit_rows: Vec<crate::style::GridTrack> = css
        .grid_template_rows
        .as_ref()
        .map(|t| (**t).clone())
        .unwrap_or_default();
    // mede a altura de conteúdo de cada linha (o item mais alto medido em shrink).
    // Um item que ATRAVESSA linhas reparte a sua altura IGUALMENTE pelas linhas do
    // span. O algoritmo da spec (§12.5) distribui pela contribuição de cada trilha;
    // a repartição igual foi escolhida por não precisar de uma segunda medição e por
    // errar sempre para MAIS espaço, nunca para item cortado.
    let mut content_row_h = vec![0.0f32; nrows];
    for cell in &cells {
        let cw = span_size(&col_sizes, cell.c0, cell.c1, col_gap);
        let (_, h) = measure_block(
            dom,
            cell.child,
            cw,
            container_content_h,
            None,
            None,
            true,
            ctx,
        );
        let each = h / cell.rows() as f32;
        for r in cell.r0..cell.r1.min(nrows) {
            content_row_h[r] = content_row_h[r].max(each);
        }
    }
    let auto_row = css.grid_auto_rows;
    let has_explicit_row_track = |r: usize| explicit_rows.get(r).is_some() || auto_row.is_some();
    let mut row_sizes: Vec<f32> = (0..nrows)
        .map(|r| {
            let track = explicit_rows.get(r).copied().or(auto_row);
            match track {
                Some(crate::style::GridTrack::Fixed(d)) => {
                    resolve_height(Some(d), container_content_h, &resolve)
                        .unwrap_or(content_row_h[r])
                }
                _ => content_row_h[r], // Auto/None/Fr → conteúdo por ora (ajuste abaixo)
            }
        })
        .collect();
    // Se o container tem ALTURA definida e as linhas NÃO têm track explícita (auto),
    // as linhas DIVIDEM a altura do container (uma row auto num grid de altura fixa
    // preenche o espaço — é o que dá a track de 240 pro logo centrar). Distribui o
    // espaço livre igualmente entre as linhas auto (aproximação; fr real seria por
    // peso — mas grid sem template-rows usa 1fr implícito quando há altura).
    if let Some(ch) = container_content_h {
        let auto_rows: Vec<usize> = (0..nrows).filter(|&r| !has_explicit_row_track(r)).collect();
        if !auto_rows.is_empty() {
            let fixed: f32 = (0..nrows)
                .filter(|r| has_explicit_row_track(*r))
                .map(|r| row_sizes[r])
                .sum();
            let total_gap = (nrows.saturating_sub(1)) as f32 * row_gap;
            let free = (ch - fixed - total_gap).max(0.0);
            let each = free / auto_rows.len() as f32;
            for r in auto_rows {
                row_sizes[r] = row_sizes[r].max(each);
            }
        }
    }

    // ── POSICIONA cada item na sua célula ────────────────────────────────────────
    let justify = css
        .grid_justify_items
        .unwrap_or(crate::style::AlignItems::Stretch);
    let align = css.align_items.unwrap_or(crate::style::AlignItems::Stretch);
    // x acumulado de cada coluna, y de cada linha.
    let mut col_x = vec![content_x; ncols + 1];
    for c in 0..ncols {
        col_x[c + 1] = col_x[c] + col_sizes[c.min(col_sizes.len() - 1)] + col_gap;
    }
    let mut row_y = vec![content_y; nrows + 1];
    for r in 0..nrows {
        row_y[r + 1] = row_y[r] + row_sizes[r] + row_gap;
    }
    for cell in &cells {
        let child = cell.child;
        let cell_x = col_x[cell.c0];
        let cell_y = row_y[cell.r0];
        let cell_w = span_size(&col_sizes, cell.c0, cell.c1, col_gap);
        let cell_h = span_size(&row_sizes, cell.r0, cell.r1.min(nrows), row_gap);
        // mede o tamanho natural do item (shrink) p/ o alinhamento não-stretch.
        let stretch_x = justify == crate::style::AlignItems::Stretch;
        let stretch_y = align == crate::style::AlignItems::Stretch;
        let (nat_w, nat_h) = measure_block(dom, child, cell_w, Some(cell_h), None, None, true, ctx);
        let iw = if stretch_x { cell_w } else { nat_w.min(cell_w) };
        let ih = if stretch_y { cell_h } else { nat_h.min(cell_h) };
        let x = cell_x + cell_align_offset(justify, cell_w, iw);
        let y = cell_y + cell_align_offset(align, cell_h, ih);
        // pinta o item: stretch no eixo → forced size; senão shrink-to-fit.
        let forced_w = if stretch_x { None } else { Some(iw) };
        let forced_h = if stretch_y { Some(cell_h) } else { None };
        layout_block(
            dom,
            child,
            x,
            y,
            cell_w,
            Some(cell_h),
            forced_w,
            forced_h,
            !stretch_x,
            &[],
            ctx,
            list,
        );
    }
    // altura total = soma das linhas + gaps.
    let total_h: f32 = row_sizes.iter().sum::<f32>() + (nrows.saturating_sub(1)) as f32 * row_gap;
    total_h.max(0.0)
}

/// Onde UM item do grid vive: a célula inicial e o span, em índices de trilha com
/// o fim exclusivo. É o resultado da colocação — nomeada ou automática — e o único
/// que o resto do layout de grid consome, o que é o que permite às duas colocações
/// coexistirem sem um segundo caminho de posicionamento.
struct GridCell {
    child: NodeIdx,
    r0: usize,
    c0: usize,
    r1: usize,
    c1: usize,
}

impl GridCell {
    fn rows(&self) -> usize {
        (self.r1 - self.r0).max(1)
    }
}

/// Coloca os filhos: quem tem `grid-area: <nome>` presente na matriz do container
/// vai para o retângulo daquele nome; o resto preenche a próxima célula LIVRE em
/// row-major.
///
/// Os nomeados são colocados ANTES (spec §8.5 passo 1) por uma razão concreta e não
/// por fidelidade: se os automáticos fossem primeiro, um item nomeado para a coluna
/// da direita encontraria a célula já ocupada e ou sobrepunha ou empurrava — que é o
/// empilhamento que as áreas existem para evitar.
fn place_grid_items(
    dom: &Dom,
    children: &[NodeIdx],
    areas: Option<&crate::style::GridAreas>,
    ncols: usize,
) -> Vec<GridCell> {
    let mut cells: Vec<GridCell> = Vec::with_capacity(children.len());
    // ocupação row-major, crescida sob demanda (o nº de linhas não é conhecido antes
    // de saber quantos itens sobram para a colocação automática).
    let mut taken: Vec<bool> = Vec::new();
    let mut mark = |taken: &mut Vec<bool>, r0: usize, c0: usize, r1: usize, c1: usize| {
        let need = r1 * ncols;
        if taken.len() < need {
            taken.resize(need, false);
        }
        for r in r0..r1 {
            for c in c0..c1.min(ncols) {
                taken[r * ncols + c] = true;
            }
        }
    };

    let mut auto: Vec<NodeIdx> = Vec::new();
    for &child in children {
        let name = dom
            .computed_style_idx(child)
            .and_then(|s| s.grid_area.clone());
        match name.and_then(|n| areas.and_then(|a| a.area(&n))) {
            Some(a) => {
                mark(&mut taken, a.r0, a.c0, a.r1, a.c1);
                cells.push(GridCell {
                    child,
                    r0: a.r0,
                    c0: a.c0,
                    r1: a.r1,
                    c1: a.c1.min(ncols),
                });
            }
            None => auto.push(child),
        }
    }

    // As linhas declaradas pela matriz contam como existentes mesmo sem item: um
    // automático não deve cair numa célula vazia RESERVADA (o `.` da matriz) antes
    // das linhas implícitas... mas cair nela é o comportamento da spec, então só as
    // células realmente ocupadas bloqueiam.
    let mut cursor = 0usize;
    for &child in &auto {
        while taken.get(cursor).copied().unwrap_or(false) {
            cursor += 1;
        }
        let (r, c) = (cursor / ncols, cursor % ncols);
        mark(&mut taken, r, c, r + 1, c + 1);
        cells.push(GridCell {
            child,
            r0: r,
            c0: c,
            r1: r + 1,
            c1: c + 1,
        });
        cursor += 1;
    }
    cells
}

/// Soma os tamanhos das trilhas `start..end` mais os gaps entre elas — o tamanho de
/// uma célula, que para span 1 é a trilha e para span N inclui os gaps que o span
/// atravessa (um span de 2 colunas cobre o gap do meio, não o perde).
fn span_size(sizes: &[f32], start: usize, end: usize, gap: f32) -> f32 {
    if sizes.is_empty() {
        return 0.0;
    }
    let end = end.max(start + 1).min(sizes.len());
    let start = start.min(sizes.len() - 1);
    let n = end.saturating_sub(start);
    sizes[start..end].iter().sum::<f32>() + (n.saturating_sub(1)) as f32 * gap
}
/// A LARGURA (ou altura) de cada trilha de uma grade.
///
/// A ordem das três passadas é a regra, e não um detalhe de implementação: uma
/// trilha intrínseca (`auto`/`min-content`) é dimensionada pelo CONTEÚDO antes
/// de qualquer espaço livre ser repartido, porque o espaço livre só existe
/// depois de se saber o que o conteúdo pede. Inverter as duas é o que fazia a
/// grade do `<main>` da Wikipédia dar 948px à coluna de conteúdo e empurrar a
/// barra lateral para fora da janela.
///
/// `conteudo[i]` é a largura intrínseca dos itens da trilha `i` — `None` quando
/// quem chama não a mediu (nenhuma trilha intrínseca na lista, e aí ela não é
/// precisa).
fn resolve_tracks(
    tracks: &[crate::style::GridTrack],
    container: f32,
    gap: f32,
    conteudo: Option<&[f32]>,
    ctx: &ResolveCtx,
) -> Vec<f32> {
    use crate::style::GridTrack as T;
    let n = tracks.len().max(1);
    let total_gap = (n.saturating_sub(1)) as f32 * gap;
    let dim = |d: &crate::style::Dimension| -> f32 {
        match d {
            // % de trilha resolve contra o container (largura p/ colunas).
            crate::style::Dimension::Percent(p) => container * p / 100.0,
            other => other.resolve(ctx).unwrap_or(0.0),
        }
        .max(0.0)
    };

    // 1ª passada: a BASE de cada trilha — o que ela pede antes de haver sobra.
    let mut sizes = vec![0.0f32; tracks.len()];
    let mut sum_fr = 0.0f32;
    for (i, t) in tracks.iter().enumerate() {
        sizes[i] = match t {
            T::Fixed(d) => dim(d),
            T::Bounded { min, .. } => dim(min),
            T::Auto => conteudo
                .and_then(|c| c.get(i))
                .copied()
                .unwrap_or(0.0)
                .max(0.0),
            T::Fr(f) => {
                sum_fr += f.max(0.0);
                0.0
            }
        };
    }
    let free = (container - sizes.iter().sum::<f32>() - total_gap).max(0.0);

    // 2ª passada: o espaço livre. `fr` come-o todo quando existe — é o que a
    // unidade significa —, e nesse caso uma trilha limitada ou intrínseca fica
    // pela sua base.
    if sum_fr > 0.0 {
        for (i, t) in tracks.iter().enumerate() {
            if let T::Fr(f) = t {
                sizes[i] = free * f.max(0.0) / sum_fr;
            }
        }
        return sizes;
    }

    // 3ª passada, sem `fr`: primeiro as trilhas LIMITADAS crescem até ao seu
    // máximo (é o que `minmax` pede), e só o que sobrar depois disso é que
    // estica as intrínsecas — `align-content: stretch`, o default.
    let mut sobra = free;
    let limitadas: Vec<usize> = tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t, T::Bounded { .. }))
        .map(|(i, _)| i)
        .collect();
    if !limitadas.is_empty() && sobra > 0.0 {
        // Reparte por igual e não em proporção: a proporção seria contra as
        // bases, que num `minmax(0, x)` são todas zero.
        let quota = sobra / limitadas.len() as f32;
        for i in limitadas {
            if let T::Bounded { max, .. } = &tracks[i] {
                let teto = dim(max);
                let novo = (sizes[i] + quota).min(teto);
                sobra -= novo - sizes[i];
                sizes[i] = novo;
            }
        }
    }
    let autos: Vec<usize> = tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t, T::Auto))
        .map(|(i, _)| i)
        .collect();
    if !autos.is_empty() && sobra > 0.0 {
        let cada = sobra / autos.len() as f32;
        for i in autos {
            sizes[i] += cada;
        }
    }
    sizes
}

/// Offset de alinhamento de um item de tamanho `item` dentro de uma célula de
/// tamanho `cell` (start=0, center=(cell-item)/2, end=cell-item; stretch=0).
fn cell_align_offset(a: crate::style::AlignItems, cell: f32, item: f32) -> f32 {
    match a {
        crate::style::AlignItems::Center => ((cell - item) / 2.0).max(0.0),
        crate::style::AlignItems::FlexEnd => (cell - item).max(0.0),
        _ => 0.0, // FlexStart / Stretch
    }
}

fn layout_children_column(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    // altura do CONTENT do container quando explícita — a referência do eixo
    // principal (justify/margin-auto) e o containing block dos filhos (height:%).
    container_content_h: Option<f32>,
    css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_size,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // Em column, o espaço entre itens no eixo principal é o ROW-gap; o shorthand
    // `gap: X` seta os dois, então row_gap cobre o caso comum. (Fallback ao `gap`
    // — column-gap — só quando row_gap não veio, cobrindo `column-gap` usado
    // "errado" sem quebrar o shorthand.)
    let main_gap = css
        .row_gap
        .or(css.gap)
        .and_then(|d| d.resolve(&resolve))
        .unwrap_or(0.0)
        .max(0.0);
    let justify = css
        .justify
        .unwrap_or(crate::style::JustifyContent::FlexStart);
    let align = css.align_items.unwrap_or(crate::style::AlignItems::Stretch);

    // ── PASSO 1: mede a altura outer desejada de cada filho + margens auto ───────
    struct ColItem {
        node: NodeIdx,
        h: f32,
        is_text: bool,
        mt_auto: bool,
        mb_auto: bool,
        grow: f32,
    }
    let mut items: Vec<ColItem> = Vec::new();
    for &child in &dom.node(id).children {
        if let NodeKind::Element { tag } = &dom.node(child).kind {
            if is_non_rendered_tag(tag) {
                continue;
            }
        }
        // fora do fluxo: não é item flex (pintado na passada out-of-flow).
        if is_out_of_flow(dom, child) {
            continue;
        }
        // `display:none` não é item — mesmo motivo do eixo horizontal; aqui o
        // que ele roubava era altura e um `gap` vertical.
        if e_display_none(dom, child) {
            continue;
        }
        // Blockificação, como no eixo horizontal — ver o comentário lá.
        if matches!(dom.node(child).kind, NodeKind::Text(_)) {
            let text = collect_text(dom, child);
            if text.trim().is_empty() {
                continue;
            }
            items.push(ColItem {
                node: child,
                h: crate::inline_box::altura_da_linha(css, font_size, ctx.measurer),
                is_text: true,
                mt_auto: false,
                mb_auto: false,
                grow: 0.0,
            });
            continue;
        }
        let h = child_outer_height(
            dom,
            child,
            content_w,
            container_content_h,
            css,
            font_size,
            ctx,
        );
        let (mt_auto, mb_auto, grow) = dom
            .computed_style_idx(child)
            .map(|c| {
                (
                    c.margin.top.is_auto(),
                    c.margin.bottom.is_auto(),
                    c.flex_grow.unwrap_or(0.0),
                )
            })
            .unwrap_or((false, false, 0.0));
        items.push(ColItem {
            node: child,
            h,
            is_text: false,
            mt_auto,
            mb_auto,
            grow,
        });
    }
    if items.is_empty() {
        return 0.0;
    }

    // ── PASSO 2: distribui o espaço livre do eixo principal (Y) ──────────────────
    let n = items.len();
    let sum_h: f32 = items.iter().map(|it| it.h).sum();
    let total_gap = (n.saturating_sub(1)) as f32 * main_gap;
    let free = container_content_h
        .map(|ch| ch - sum_h - total_gap)
        .unwrap_or(0.0);
    // FLEX-GROW no eixo principal (css-flexbox §9.7): quando há espaço livre
    // positivo e algum item tem flex-grow, cada um cresce em proporção
    // `grow / soma_dos_grows * free` — dando ALTURA aos containers que os filhos
    // com `height:100%` resolvem (o logo/caixa do google centram assim). Consome
    // o `free` (o justify/margin-auto abaixo vê 0). margin:auto tem prioridade.
    let sum_grow: f32 = items.iter().map(|it| it.grow).sum();
    let any_auto = items.iter().any(|it| it.mt_auto || it.mb_auto);
    if free > 0.0 && sum_grow > 0.0 && !any_auto {
        for it in &mut items {
            if it.grow > 0.0 {
                it.h += it.grow / sum_grow * free;
            }
        }
    }
    let sum_h: f32 = items.iter().map(|it| it.h).sum();
    let free = container_content_h
        .map(|ch| ch - sum_h - total_gap)
        .unwrap_or(0.0);
    let auto_count: usize = items
        .iter()
        .map(|it| it.mt_auto as usize + it.mb_auto as usize)
        .sum();
    // margin:auto no eixo main absorve TODO o espaço livre (o justify vira no-op) —
    // spec css-flexbox §8.1. Sem autos, o justify distribui.
    let auto_size = if free > 0.0 && auto_count > 0 {
        free / auto_count as f32
    } else {
        0.0
    };
    let (leading, between) = if auto_count > 0 {
        (0.0, 0.0)
    } else {
        justify_offsets(justify, free, n)
    };

    // ── PASSO 3: posiciona e pinta ────────────────────────────────────────────────
    let mut y = content_y + leading;
    for (j, it) in items.iter().enumerate() {
        if j > 0 {
            y += main_gap + between;
        }
        if it.mt_auto {
            y += auto_size;
        }
        if it.is_text {
            let text = collect_text(dom, it.node);
            list.items.push(DisplayItem::Text {
                x: content_x,
                y,
                text: text.into(),
                color: cor_visivel(&css, css.color.unwrap_or(0x000000FF)),
                size: font_size,
                mono: false,
                bold: css.bold.unwrap_or(false),
                italic: italico(Some(&css), tag_de(dom, it.node), false),
                letter_spacing: css.letter_spacing.unwrap_or(0.0),
                decoration: decoration_code(css),
            });
        } else {
            // CROSS (X): stretch (default) → o item ocupa a largura do container
            // (layout normal de bloco); start/center/end → shrink-to-fit + offset.
            let stretch = align == crate::style::AlignItems::Stretch;
            let child_x = if stretch {
                content_x
            } else {
                let (w, _) = measure_block(
                    dom,
                    it.node,
                    content_w,
                    container_content_h,
                    None,
                    None,
                    true,
                    ctx,
                );
                let free_x = (content_w - w).max(0.0);
                content_x + align_offset(align, content_w, content_w - free_x)
            };
            // Um item que CRESCEU por flex-grow tem altura MAIOR que o conteúdo —
            // passa essa altura como containing block (avail_h) E como outer forçada
            // (forced_outer_h) para os filhos com `height:100%` resolverem contra ela.
            let (avail, forced_h) = if it.grow > 0.0 {
                (Some(it.h), Some(it.h))
            } else {
                (container_content_h, None)
            };
            layout_block(
                dom,
                it.node,
                child_x,
                y,
                content_w,
                avail,
                None,
                forced_h,
                !stretch,
                &[],
                ctx,
                list,
            );
        }
        y += it.h;
        if it.mb_auto {
            y += auto_size;
        }
    }
    (y - content_y).max(0.0)
}

/// Calcula (leading, between) do justify-content dado o espaço livre `free` e o nº
/// de itens `n`. `leading` = offset inicial; `between` = espaço EXTRA entre itens
/// (além do gap).
///
/// OVERFLOW (free<=0): VALIDADO contra o Chrome (com `flex-shrink:0` para forçar
/// overflow real — sem isso o flex-shrink encolhe os itens e não há overflow). Os
/// três distribuidores `space-*` caem para FLEX-START ([0,100,200] no teste), e só
/// `center`/`flex-end` mantêm o leading (negativo = transborda dos dois lados/start).
/// NB: a verificação adversarial sugeriu around/evenly→center, mas o Chrome real os
/// trata como flex-start — a medição no browser desempatou.
fn justify_offsets(j: crate::style::JustifyContent, free: f32, n: usize) -> (f32, f32) {
    use crate::style::JustifyContent as J;
    if free <= 0.0 {
        return match j {
            J::Center => (free / 2.0, 0.0), // leading negativo = transbordo centrado
            J::FlexEnd => (free, 0.0),      // todo o overflow no start
            // flex-start E os space-* → flush no start (fiel ao Chrome em overflow).
            J::FlexStart | J::SpaceBetween | J::SpaceAround | J::SpaceEvenly => (0.0, 0.0),
        };
    }
    match j {
        J::FlexStart => (0.0, 0.0),
        J::FlexEnd => (free, 0.0),
        J::Center => (free / 2.0, 0.0),
        J::SpaceBetween => {
            if n > 1 {
                (0.0, free / (n - 1) as f32)
            } else {
                (0.0, 0.0)
            }
        }
        J::SpaceAround => {
            if n >= 1 {
                (free / (2 * n) as f32, free / n as f32)
            } else {
                (0.0, 0.0)
            }
        }
        J::SpaceEvenly => (free / (n + 1) as f32, free / (n + 1) as f32),
    }
}

/// Offset no eixo cruzado de um item, dado o align-items, a altura da linha `line_h`
/// e a altura outer do item `item_h`. (stretch é tratado como flex-start aqui — o
/// esticar real exige passar altura imposta ao layout_block, fase futura.)
fn align_offset(a: crate::style::AlignItems, line_h: f32, item_h: f32) -> f32 {
    use crate::style::AlignItems as A;
    let free = line_h - item_h;
    match a {
        A::Stretch | A::FlexStart => 0.0,
        A::FlexEnd => free,
        A::Center => free / 2.0,
    }
}

/// Desenha um nó como linha(s) de texto (texto solto ou inline simples), herdando
/// cor/tamanho do bloco pai, e devolve o `y` abaixo. Caso de UM nó do fluxo
/// inline — o caminho geral (irmãos inline fluindo juntos) é
/// [`layout_inline_flow`].
fn layout_inline_line(
    dom: &Dom,
    id: NodeIdx,
    x: f32,
    y: f32,
    content_w: f32,
    parent_css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    layout_inline_flow(
        dom,
        id,
        &[id],
        x,
        y,
        content_w,
        parent_css,
        font_size,
        &[],
        ctx,
        list,
    )
}

/// O FLUXO INLINE RICO (P4): um GRUPO de irmãos inline consecutivos (nós de texto
/// + elementos inline como `<a>`/`<b>`/`<span>`) flui como UM contexto — os runs
/// de todos concatenam, quebram por palavra na largura, e cada pedaço pinta com a
/// SUA cor/peso. É o que faz `<p>texto <a>link</a>, fim</p>` virar UMA linha
/// (antes cada filho virava uma linha própria — o footer do Bootstrap cover saía
/// em 5 linhas).
fn layout_inline_flow(
    dom: &Dom,
    // O elemento DONO deste fluxo — de quem são as caixas geradas
    // (`::before`/`::after`) que envolvem o grupo. Ver `pseudo_run`.
    dono: NodeIdx,
    group: &[NodeIdx],
    x: f32,
    y: f32,
    content_w: f32,
    parent_css: &ComputedStyle,
    font_size: f32,
    // Os floats abertos que atravessam este fluxo. É a razão de a exclusão
    // atravessar DUAS camadas em vez de ficar no empilhamento de blocos: pelo
    // CSS a caixa de bloco ao lado de um float não desce nem encolhe — mantém a
    // largura e sobrepõe-se ao float —, e quem encolhe são as CAIXAS DE LINHA
    // lá dentro. Parar de empurrar o bloco sem encurtar as linhas trocava um
    // erro de posição por texto pintado por baixo da figura. Ver [`Exclusao`].
    exclusoes: &[Exclusao],
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let _phase = crate::metrics::phases::scope("layout-inline");
    // coleta os RUNS (cada pedaço de texto com a SUA cor/bold herdada do span que
    // o contém) de TODOS os nós do grupo, em ordem de documento.
    let mut runs = Vec::new();
    // A caixa gerada do DONO envolve todo o conteúdo dele — e só existe como run
    // aqui quando este grupo É todo o conteúdo. Com filhos de bloco pelo meio, o
    // conteúdo do dono parte-se em vários grupos e a caixa gerada teria de virar
    // um bloco anónimo, que é maquinaria de árvore de caixas que este layout não
    // tem; nesse caso não se gera nada, que é o estado anterior, em vez de a pôr
    // num pedaço arbitrário do conteúdo.
    // "este grupo é TODO o conteúdo do dono?" — contado sobre os filhos que
    // geram conteúdo. Os nós de texto só com espaços não contam: um HTML
    // indentado põe um antes e outro depois de cada elemento, e compará-los
    // fazia um `<div>` com o `<span>` numa linha indentada parecer conteúdo
    // partido, e perdia a caixa gerada em quase toda a página real.
    let filhos_com_conteudo = dom
        .node(dono)
        .children
        .iter()
        .filter(|&&c| !matches!(&dom.node(c).kind, NodeKind::Text(t) if t.trim().is_empty()))
        .count();
    let dono_inteiro = group
        .iter()
        .filter(|&&c| !matches!(&dom.node(c).kind, NodeKind::Text(t) if t.trim().is_empty()))
        .count()
        == filhos_com_conteudo;
    let cor_base = cor_visivel(parent_css, parent_css.color.unwrap_or(0x000000FF));
    if dono_inteiro {
        runs.extend(pseudo_run(
            dom,
            dono,
            &[dono],
            crate::style::PseudoElement::Before,
            cor_base,
            parent_css.italic.unwrap_or(false),
        ));
    }
    for &id in group {
        runs.extend(collect_runs(dom, id, parent_css, content_w, ctx));
    }
    if dono_inteiro {
        runs.extend(pseudo_run(
            dom,
            dono,
            &[dono],
            crate::style::PseudoElement::After,
            cor_base,
            parent_css.italic.unwrap_or(false),
        ));
    }
    // Um MARKER (elemento inline vazio) não conta como conteúdo: um `<span></span>`
    // sozinho num bloco não cria linha nenhuma no browser, e criá-la aqui mudaria
    // a altura do bloco — o oposto de "acrescenta geometria, não muda a pintura".
    if runs.iter().all(|r| {
        r.text.trim().is_empty()
            && !matches!(
                r.atomic,
                Some((
                    _,
                    AtomicKind::Widget
                        | AtomicKind::Replaced
                        | AtomicKind::Block
                        | AtomicKind::Break
                ))
            )
    }) {
        return y;
    }
    let mono = parent_css
        .font_family
        .as_deref()
        .map(crate::style::is_mono_family)
        .unwrap_or(false);
    // line-height: do CSS (multiplicador ou px), senão o default do measurer —
    // #1749. O medidor é também quem responde por `line-height: normal`, porque
    // esse valor sai das MÉTRICAS DA FONTE e não de uma constante: sem isto, o
    // elemento sem declaração e o que declara `normal` — a spec diz que são o
    // mesmo valor — davam alturas diferentes.
    let lh = crate::inline_box::altura_da_linha(parent_css, font_size, ctx.measurer);
    let nowrap = matches!(
        parent_css.white_space,
        Some(crate::style::WhiteSpace::Nowrap | crate::style::WhiteSpace::Pre)
    );
    // A LARGURA DE QUEBRA, linha a linha: onde um float estorva, a linha é
    // curta; onde ele acaba, volta a ser a do content.
    //
    // ⚠️ APROXIMAÇÃO DECLARADA: a banda de cada linha é prevista pelo ÍNDICE
    // dela, assumindo que todas medem `lh`. Uma linha com um widget mais alto
    // desloca as seguintes e a previsão fica uma fração de linha acima do
    // sítio real. É uma decisão, não um esquecimento: a alternativa é quebrar e
    // posicionar na mesma passagem, o que obriga a intercalar `wrap_runs` com o
    // avanço do cursor. A PINTURA não usa esta previsão — usa o `cy` verdadeiro
    // (ver a banda recalculada no laço), portanto o erro fica no ponto de
    // quebra e nunca em texto pintado por cima de um float.
    let mut largura_da_linha = |i: usize| -> f32 {
        if nowrap {
            return f32::INFINITY;
        }
        if exclusoes.is_empty() {
            return content_w;
        }
        banda_livre(exclusoes, y + i as f32 * lh, lh, x, content_w).1
    };
    // quebra os runs em LINHAS, cada linha = sequência de pedaços coloridos (word).
    let lines = wrap_runs(
        &runs,
        &mut largura_da_linha,
        font_size,
        mono,
        crate::inline_box::quebra_dentro(parent_css),
        ctx.measurer,
    );
    // `text-overflow: ellipsis` — depois da quebra e antes da colocação, porque
    // o que se corta é uma LINHA já formada. Ver [`aplicar_elipse`].
    let lines = match elipse_pedida(parent_css, nowrap) {
        true => aplicar_elipse(lines, content_w, font_size, mono, ctx.measurer),
        false => lines,
    };
    // `text-indent`: recuo da PRIMEIRA linha (MDN). ⚠️ CORTE: recua o início da
    // linha mas NÃO encurta a largura de quebra dela — a quebra já foi calculada
    // acima, e refazê-la só para a primeira linha exigia partir o `wrap_runs` em
    // duas passadas. O erro fica no ponto de quebra da 1ª linha; o recuo, que é o
    // efeito que a página pede, está certo. Negativo é aceite (o truque de
    // esconder texto atrás da margem).
    let indent = parent_css
        .text_indent
        .and_then(|d| {
            d.resolve_signed(&ResolveCtx {
                parent_content_w: content_w,
                node_font_size: font_size,
                root_font_size: crate::style::root_font_size(),
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            })
        })
        .unwrap_or(0.0);
    let mut first_line = true;
    let mut cy = y;
    // CONSUMINDO as linhas: o texto de cada segmento vai direto para o
    // `DisplayItem`, em vez de ser clonado. Eram milhares de `String` alocadas
    // por passada de layout, uma por segmento, para copiar algo que ninguém mais
    // usaria depois.
    for line in lines {
        // largura total da linha (texto no SEU peso + widgets) p/ text-align.
        let line_w: f32 = line
            .iter()
            .map(|seg| {
                seg.lead_w
                    + match seg.atomic {
                        Some(_) => seg.ww,
                        None => seg.text_width,
                    }
            })
            .sum();
        // altura da linha: o texto (lh) ou o widget mais alto nela.
        let line_h = line
            .iter()
            .filter(|s| {
                matches!(
                    s.atomic,
                    Some((
                        _,
                        AtomicKind::Widget
                            | AtomicKind::Replaced
                            | AtomicKind::Block
                            | AtomicKind::Break
                    ))
                )
            })
            .map(|s| s.wh)
            .fold(lh, f32::max);
        // A CAIXA de cada inline desta linha: a content area da fonte, centrada na
        // linha pela meia-entrelinha. A linha continua a avançar `line_h` — quem
        // decide o espaçamento é o `line-height`, quem decide a caixa é a fonte.
        let conteudo = crate::inline_box::altura_do_conteudo(font_size, ctx.measurer);
        let meia = crate::inline_box::meia_entrelinha(line_h, conteudo);
        // A banda desta linha, no `cy` VERDADEIRO — é aqui que o texto passa a
        // correr ao lado do float em vez de por baixo dele.
        let (linha_x, linha_w) = if exclusoes.is_empty() {
            (x, content_w)
        } else {
            banda_livre(exclusoes, cy, line_h, x, content_w)
        };
        let free = (linha_w - line_w).max(0.0);
        let mut seg_x = match parent_css.text_align {
            Some(crate::style::TextAlign::Right) => linha_x + free,
            Some(crate::style::TextAlign::Center) => linha_x + free / 2.0,
            _ => linha_x, // left/justify
        };
        if first_line {
            seg_x += indent;
            first_line = false;
        }
        // pinta cada pedaço NA SUA COR e PESO, avançando o x.
        for seg in line {
            let seg: Segment = seg;
            // O vão que precede o segmento ocupa lugar na linha mas não pertence
            // a nada: avança o cursor antes de qualquer caixa ser calculada.
            seg_x += seg.lead_w;
            if let Some((a_idx, kind)) = seg.atomic {
                match kind {
                    AtomicKind::Widget => {
                        // WIDGET inline: pinta a caixa no lugar (botão via layout_button;
                        // campo de texto via layout_input com o avail da linha).
                        let wcss = dom.computed_style_idx(a_idx).unwrap_or_default();
                        let itype = dom
                            .node(a_idx)
                            .attr("type")
                            .map(|t| t.to_ascii_lowercase())
                            .unwrap_or_default();
                        if matches!(itype.as_str(), "submit" | "button" | "reset") {
                            layout_button(dom, a_idx, &wcss, seg_x, cy, ctx, list);
                        } else {
                            // `None` de altura disponível: uma caixa atómica numa
                            // linha não tem containing block de altura definida, e
                            // é isso que faz `height:%` valer `auto` — como no
                            // browser.
                            layout_input(
                                dom, a_idx, &wcss, seg_x, cy, seg.ww, None, None, ctx, list,
                            );
                        }
                    }
                    AtomicKind::Replaced => {
                        // REPLACED inline (um `<img>` no meio do texto): a caixa é o
                        // tamanho já medido. Só se pinta quando há pixels — e aí é
                        // `layout_image` que o faz, o mesmo caminho do fluxo de bloco,
                        // em vez de um segundo emissor de imagem só para o inline.
                        if dom.image_of(a_idx).is_some() {
                            let icss = dom.computed_style_idx(a_idx).unwrap_or_default();
                            layout_image(dom, a_idx, &icss, seg_x, cy, seg.ww.max(1.0), ctx, list);
                        }
                    }
                    AtomicKind::Block => {
                        // Um inline-block PINTA-SE como bloco (fundo, borda,
                        // padding) mas na posição que a linha lhe deu. É o mesmo
                        // `layout_block` da corrida de inline-blocks irmãos —
                        // não um segundo emissor — só que o x/y vem do fluxo.
                        layout_block(
                            dom,
                            a_idx,
                            seg_x,
                            cy,
                            seg.ww.max(1.0),
                            None,
                            None,
                            None,
                            true,
                            &[],
                            ctx,
                            list,
                        );
                    }
                    AtomicKind::Marker | AtomicKind::Break => {}
                }
                // A CAIXA DO PRÓPRIO: a de uma caixa atómica é o seu tamanho; a
                // de um vazio/quebra é a fatia de linha que ele ocupa.
                let propria = match kind {
                    AtomicKind::Marker | AtomicKind::Break => {
                        Rect::new(seg_x, cy + meia, 0.0, conteudo)
                    }
                    _ => Rect::new(seg_x, cy, seg.ww, seg.wh),
                };
                crate::inline_box::union_rect(list, a_idx, propria);
                // A CAIXA DOS ANCESTRAIS inline: a largura que esta caixa ocupa na
                // linha, com a altura da FONTE — um `<a>` à volta de uma imagem de
                // 528px de altura mede 17px no browser, não 528. É a mesma regra
                // que já vale para o texto, aplicada ao que não é texto.
                for &owner in &seg.owners {
                    crate::inline_box::union_rect(
                        list,
                        owner,
                        fragmento_do_dono(dom, owner, seg_x, cy + meia, seg.ww, conteudo, ctx),
                    );
                }
                // A CAIXA DOS ANCESTRAIS inline: a largura que esta caixa ocupa na
                // linha, com a altura da FONTE — um `<a>` à volta de uma imagem de
                // 528px de altura mede 17px no browser, não 528. É a mesma regra
                // que já vale para o texto, aplicada ao que não é texto.

                seg_x += seg.ww;
                continue;
            }
            let ls = parent_css.letter_spacing.unwrap_or(0.0);
            let w = seg.text_width + ls * seg.text.chars().count() as f32;
            list.items.push(DisplayItem::Text {
                x: seg_x,
                y: cy + meia,
                text: seg.text.into(),
                color: seg.color,
                size: font_size,
                mono,
                bold: seg.bold,
                italic: seg.italic,
                letter_spacing: ls,
                decoration: seg.deco,
            });
            for &owner in &seg.owners {
                crate::inline_box::union_rect(
                    list,
                    owner,
                    fragmento_do_dono(dom, owner, seg_x, cy + meia, w.max(0.0), conteudo, ctx),
                );
            }
            seg_x += w;
        }
        cy += line_h;
    }
    cy
}

/// O fragmento que ESTE dono recebe desta fatia de linha.
///
/// A altura é a content area da fonte DELE, não a do bloco que conduz o fluxo:
/// um `<span>` de 14px dentro de um título de 17,5px mede 15,75 e não 19,7. Sem
/// isto, 1 172 dos 1 257 `<span>` da Wikipédia com altura errada tinham
/// exatamente `1.125 x a fonte de um ANCESTRAL` — quase sempre o bloco quatro
/// níveis acima.
///
/// Fica CENTRADO na content area da linha, que é a mesma aproximação da
/// meia-entrelinha (o browser alinha pela linha de base; centrar acertou dentro
/// de 1px no caso medido do `<a>` à volta de uma imagem).
#[allow(clippy::too_many_arguments)]
fn fragmento_do_dono(
    dom: &Dom,
    dono: NodeIdx,
    x: f32,
    y: f32,
    w: f32,
    conteudo_da_linha: f32,
    ctx: &LayoutCtx,
) -> Rect {
    let Some(css) = dom.computed_style_idx(dono) else {
        return Rect::new(x, y, w, conteudo_da_linha);
    };
    let Some(crate::style::Dimension::Px(fonte)) = css.font_size else {
        return Rect::new(x, y, w, conteudo_da_linha);
    };
    let conteudo = crate::inline_box::altura_do_conteudo(fonte, ctx.measurer);
    Rect::new(x, y + (conteudo_da_linha - conteudo) / 2.0, w, conteudo)
}

/// Um pedaço de texto inline com seu estilo resolvido (cor/peso herdados do span pai).
/// `atomic: Some((idx, kind))` = uma CAIXA em vez de texto — um widget de
/// formulário, um replaced element (`<img>`), ou o marcador de um inline vazio.
/// As duas primeiras fluem como uma "palavra" inquebrável de `ww × wh` pontos
/// (item 8 do handoff #1793; os botões 'Pesquisa Google' do google legado vivem
/// em span>span>input); o marcador não ocupa nada.
struct InlineRun {
    text: String,
    color: u32,
    bold: bool,
    /// `font-style: italic` do span que contém este texto. Eixo INDEPENDENTE do
    /// `bold` — `<em><strong>` é bold-italic e um único bit não o exprimiria.
    italic: bool,
    /// decoração do RUN (0=none 1=underline 2=line-through) — vem do <a>/<span>
    /// que contém o texto, não do bloco pai (um <a> sublinha só o seu texto).
    deco: u8,
    /// Elementos inline ancestrais deste run. Cada um recebe a união dos fragmentos.
    owners: Vec<NodeIdx>,
    atomic: Option<(NodeIdx, AtomicKind)>,
    ww: f32,
    wh: f32,
}

/// O run de texto de uma caixa gerada (`::before`/`::after`) de `id`, ou vazio
/// se a cascata não manda gerar nenhuma.
///
/// Entregar conteúdo gerado como um `InlineRun` é o que faz esta funcionalidade
/// caber sem reescrever o fluxo: um run é "texto com um estilo, pertencente a
/// estes elementos inline", e é exatamente o que um `::before` de texto é. Em
/// particular ele quebra linha, herda e é medido pelo mesmo caminho do resto —
/// nada disto precisou de um segundo caminho.
///
/// `donos` é a CADEIA inline inteira terminada no elemento originante, e não só
/// ele. No browser a caixa gerada está dentro da caixa do elemento e um clique
/// nela atinge o elemento — mas também está dentro de cada inline que o
/// envolve, exatamente como o texto normal está.
///
/// Isto já esteve errado, e o sintoma era invisível até o resto ficar certo:
/// com `owners: vec![id]` um `<span><a></a></span>` em que todo o conteúdo do
/// `<a>` vem de `a::before` deixava o `<span>` sem geometria NENHUMA, porque
/// nada lhe chamava `union_rect`. Na Wikipédia eram os 397 retrolinks da lista
/// de referências. Um fragmento gerado é um fragmento: conta para a união dos
/// ancestrais como qualquer outro, e é `uniontests.rs` que o fixa.
///
/// CORTE DECLARADO: só o texto e as propriedades que um run carrega (cor, peso,
/// decoração) chegam à pintura. `background`, `padding`, `border` e `width` do
/// pseudo são ignorados, e `display:block`/`inline-block`/`position:absolute`
/// nele são tratados como o inline que a maioria é. Medido na folha da
/// Wikipédia: 88 das 100 regras com pseudo-elemento são inline por omissão.
fn pseudo_run(
    dom: &Dom,
    id: NodeIdx,
    // A cadeia inline que envolve o originante, ele incluído e por último.
    donos: &[NodeIdx],
    pe: crate::style::PseudoElement,
    // A cor já resolvida do contexto — a caixa gerada herda-a quando não
    // declara `color`.
    cor_herdada: u32,
    // idem para o itálico: a caixa gerada herda o estilo do elemento.
    herdado_italico: bool,
) -> Option<InlineRun> {
    let caixa = dom.pseudo_box(id, pe)?;
    crate::bump!(inline_runs);
    Some(InlineRun {
        text: caixa.texto,
        color: cor_visivel(&caixa.css, caixa.css.color.unwrap_or(cor_herdada)),
        bold: caixa.css.bold.unwrap_or(false),
        // a caixa gerada é do PRÓPRIO elemento: nenhuma tag nova entra, por isso
        // a UA não tem aqui nada a dizer — só o CSS do pseudo e o que herdou.
        italic: caixa.css.italic.unwrap_or(herdado_italico),
        deco: decoration_code(&caixa.css),
        owners: donos.to_vec(),
        atomic: None,
        ww: 0.0,
        wh: 0.0,
    })
}

/// Coleta os RUNS de texto de `id` em ordem de documento, cada um com a COR efetiva
/// do elemento inline que o contém (um `<span style=color:x>` muda a cor do seu
/// texto). Aplica text-transform por run. A cor vem do `computed_style_idx` do nó
/// inline (que já herda do pai via a cascade) — é por isso que o style do span passa
/// a valer no texto.
fn collect_runs(
    dom: &Dom,
    id: NodeIdx,
    parent_css: &ComputedStyle,
    avail_w: f32,
    ctx: &LayoutCtx,
) -> Vec<InlineRun> {
    let _phase = crate::metrics::phases::scope("collect-runs");
    let mut runs = Vec::new();
    walk(
        dom,
        ctx,
        avail_w,
        id,
        cor_visivel(parent_css, parent_css.color.unwrap_or(0x000000FF)),
        decoration_code(parent_css),
        parent_css.text_transform,
        parent_css.bold.unwrap_or(false),
        parent_css.italic.unwrap_or(false),
        &[],
        &mut runs,
    );
    return runs;

    fn walk(
        dom: &Dom,
        ctx: &LayoutCtx,
        avail_w: f32,
        id: NodeIdx,
        inherited_color: u32,
        inherited_deco: u8,
        inherited_tt: Option<crate::style::TextTransform>,
        inherited_bold: bool,
        inherited_italic: bool,
        inherited_owners: &[NodeIdx],
        out: &mut Vec<InlineRun>,
    ) {
        match &dom.node(id).kind {
            NodeKind::Text(t) => {
                let text = match inherited_tt {
                    Some(tt) => tt.apply(t),
                    None => t.clone(),
                };
                crate::bump!(inline_runs);
                out.push(InlineRun {
                    text,
                    color: inherited_color,
                    bold: inherited_bold,
                    italic: inherited_italic,
                    deco: inherited_deco,
                    owners: inherited_owners.to_vec(),
                    atomic: None,
                    ww: 0.0,
                    wh: 0.0,
                });
            }
            NodeKind::Element { tag } => {
                // `<script>`/`<style>`/head-etc DENTRO de um contexto inline (um
                // script dentro de <td>/<center> — google.com faz isso): o texto
                // cru NÃO é conteúdo renderável — sem este skip, o código JS era
                // PINTADO na página.
                if is_non_rendered_tag(tag) {
                    return;
                }
                // `display:none` DENTRO de uma linha. O comentário de
                // `e_display_none` diz que a herança vem de "quem varre já não
                // desce nele" — e este varredor descia: um
                // `<span><span style=display:none>Z39.88…</span></span>` (o
                // COinS de cada citação da Wikipédia, ~280 na página) era
                // medido e PINTADO na linha, dando ao pai a largura do texto
                // oculto em vez da caixa de largura zero que o Chrome lhe dá.
                //
                // Saltar aqui é também o que devolve a caixa ao pai: sem filho
                // que gere run, ele cai no `Marker` lá abaixo, que é a resposta
                // que já existia para o inline vazio. A alternativa — um caminho
                // novo para "inline cujo conteúdo todo é invisível" — era pôr a
                // mesma resposta num segundo sítio.
                if e_display_none(dom, id) {
                    return;
                }
                // WIDGET inline: um `<input>` no meio do fluxo (botão/campo) vira
                // um run-widget com o tamanho pré-medido — o wrap o trata como
                // palavra inquebrável e a emissão pinta a caixa no lugar.
                if is_text_input_tag(tag) {
                    let itype = dom
                        .node(id)
                        .attr("type")
                        .map(|t| t.to_ascii_lowercase())
                        .unwrap_or_default();
                    if itype == "hidden" {
                        return;
                    }
                    let (ww, wh) = inline_widget_size(dom, id, &itype, avail_w, ctx);
                    // Os ANCESTRAIS inline não engolem a caixa deste widget: no
                    // browser a caixa de um inline tem a largura do que ele
                    // contém e a altura da FONTE. Quem recebe `ww × wh` é só o
                    // próprio elemento, na emissão.
                    let owners = inherited_owners.to_vec();
                    crate::bump!(inline_runs);
                    out.push(InlineRun {
                        text: String::new(),
                        color: inherited_color,
                        bold: false,
                        italic: false,
                        deco: 0,
                        owners,
                        atomic: Some((id, AtomicKind::Widget)),
                        ww,
                        wh,
                    });
                    return;
                }
                // `<br>`: uma QUEBRA no meio do fluxo. Não é texto nem caixa — é o
                // fim da linha corrente, e o browser dá-lhe na mesma posição e
                // altura de linha. Sem isto as duas linhas que ele separa saíam
                // como uma só, e tudo o que vinha abaixo subia uma linha.
                if tag == "br" {
                    let mut owners = inherited_owners.to_vec();
                    owners.push(id);
                    crate::bump!(inline_runs);
                    out.push(InlineRun {
                        text: String::new(),
                        color: inherited_color,
                        bold: false,
                        italic: false,
                        deco: 0,
                        owners,
                        atomic: Some((id, AtomicKind::Break)),
                        ww: 0.0,
                        wh: 0.0,
                    });
                    return;
                }
                // REPLACED inline (`<img>` dentro de um `<a>`, `<video>`, …): não é
                // texto e não tem filhos que o descrevam, por isso não produzia run
                // nenhum e ficava sem caixa. Flui como palavra inquebrável.
                let rcss = dom.computed_style_idx(id).unwrap_or_default();
                if let Some((ww, wh)) =
                    crate::inline_box::replaced_inline_size(dom, id, &rcss, avail_w, ctx)
                {
                    // Como no widget: a caixa do replaced é dele; os ancestrais
                    // inline recebem só a linha que ele ocupa.
                    let owners = inherited_owners.to_vec();
                    crate::bump!(inline_runs);
                    out.push(InlineRun {
                        text: String::new(),
                        color: inherited_color,
                        bold: false,
                        italic: false,
                        deco: 0,
                        owners,
                        atomic: Some((id, AtomicKind::Replaced)),
                        ww,
                        wh,
                    });
                    return;
                }
                // INLINE COM CAIXA: mede-se como bloco shrink-to-fit e entra na
                // linha como palavra inquebrável. Antes fechava o fluxo inline e
                // abria linha própria — um `<p>texto <span com fundo>x</span>
                // texto</p>` saía em TRÊS linhas em vez de uma, e numa página
                // real isso multiplicava a altura do documento por ~2,7.
                if is_inline_block(dom, id) {
                    let (bw, bh) = measure_block(dom, id, avail_w, None, None, None, true, ctx);
                    let mut owners = inherited_owners.to_vec();
                    crate::bump!(inline_runs);
                    out.push(InlineRun {
                        text: String::new(),
                        color: inherited_color,
                        bold: false,
                        italic: false,
                        deco: 0,
                        owners: std::mem::take(&mut owners),
                        atomic: Some((id, AtomicKind::Block)),
                        ww: bw,
                        wh: bh,
                    });
                    return;
                }
                // a cor/text-transform/peso/decoração DESTE inline (se declarar)
                // vence p/ os filhos (o <a> sublinha só o próprio texto).
                let css = dom.computed_style_idx(id);
                let color = css
                    .as_ref()
                    .and_then(|c| c.color)
                    .unwrap_or(inherited_color);
                let tt = css.as_ref().and_then(|c| c.text_transform).or(inherited_tt);
                let bold = css.as_ref().and_then(|c| c.bold).unwrap_or(inherited_bold);
                let italic = italico(css.as_deref(), Some(tag), inherited_italic);
                let deco = match css.as_deref().map(decoration_code) {
                    Some(d) if d != 0 => d,
                    _ => inherited_deco,
                };
                let mut owners = inherited_owners.to_vec();
                // Um `display:inline` DECLARADO é dono dos seus fragmentos,
                // mesmo quando `is_block_level` o marcou para pintura de caixa.
                //
                // `is_inline_text_container` pergunta `!is_block_level`, e essa
                // responde `true` a um inline que declare padding — porque
                // alguém tem de pintar esse padding. Só que "precisa de ser
                // pintado como caixa" não é "não é conteúdo de linha": o
                // elemento continua a fluir, os filhos continuam a receber as
                // suas caixas (é o que se mede: 223 descendentes certos), e o
                // único que ficava de fora era ele.
                //
                // É a hlist do MediaWiki, e bastava `padding:0` para a disparar:
                // `.hlist ul{padding:0}` faz `padding.any_set()` responder
                // "declarado" — que não é "cria caixa". 28 `<ul>` da página
                // ficavam sem retângulo à volta de conteúdo já desenhado.
                //
                // A alternativa era ensinar `any_set()` a ignorar o zero. Está
                // errada aqui por duas razões: o segundo seletor que atinge
                // estes mesmos `<ul>` declara `padding:0.125em 0`, que não é
                // zero e continuaria a perdê-los; e `any_set()` é lida por quem
                // decide pintura, onde "declarado" é a pergunta certa.
                let is_container = is_inline_text_container(dom, id)
                    || css.as_ref().and_then(|c| c.effective_display())
                        == Some(crate::style::DisplayKind::Inline);
                if is_container {
                    owners.push(id);
                }
                // As caixas geradas de um elemento INLINE (`a::after`) entram
                // aqui, à volta do conteúdo próprio dele. O dono de um fluxo
                // inteiro é tratado em `layout_inline_flow`, que é onde ele se
                // sabe dono; os dois casos não se sobrepõem.
                let before = out.len();
                // A cadeia que o fragmento gerado herda. `owners` só contém
                // `id` quando ele é container inline; um `inline-block` com
                // `::before` continua a ser dono da sua própria caixa gerada.
                let donos_do_pseudo = if owners.last() == Some(&id) {
                    owners.clone()
                } else {
                    let mut v = owners.clone();
                    v.push(id);
                    v
                };
                out.extend(pseudo_run(
                    dom,
                    id,
                    &donos_do_pseudo,
                    crate::style::PseudoElement::Before,
                    color,
                    italic,
                ));
                for &c in &dom.node(id).children {
                    walk(dom, ctx, avail_w, c, color, deco, tt, bold, italic, &owners, out);
                }
                out.extend(pseudo_run(
                    dom,
                    id,
                    &donos_do_pseudo,
                    crate::style::PseudoElement::After,
                    color,
                    italic,
                ));
                // Um inline VAZIO (`<source>`, `<br>`, `<span></span>`) não gerou run
                // e ficaria sem caixa. O marker dá-lhe a posição na linha sem lhe dar
                // largura nem altura próprias — que é a caixa que o browser reporta.
                if is_container && out.len() == before {
                    crate::bump!(inline_runs);
                    out.push(InlineRun {
                        text: String::new(),
                        color: inherited_color,
                        bold: false,
                        italic: false,
                        deco: 0,
                        owners,
                        atomic: Some((id, AtomicKind::Marker)),
                        ww: 0.0,
                        wh: 0.0,
                    });
                }
            }
            _ => {}
        }
    }
}

/// Tamanho OUTER de um widget inline (`<input>`): o MESMO cálculo que a emissão
/// usa (layout_button / layout_input), para o wrap reservar a largura exata.
fn inline_widget_size(
    dom: &Dom,
    id: NodeIdx,
    itype: &str,
    avail_w: f32,
    ctx: &LayoutCtx,
) -> (f32, f32) {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    if matches!(itype, "submit" | "button" | "reset") {
        let font = font_px(&css, DEFAULT_FONT_SIZE - 3.0);
        let label = dom.node(id).attr("value").unwrap_or("").to_string();
        let tw = ctx.measurer.text_width(&label, font, false, false, false);
        let lh = ctx.measurer.line_height(font);
        return (tw + 24.0 + 6.0, lh + 10.0 + 4.0); // espelha layout_button
    }
    // Campo de texto ou marca: a MESMA medida que a emissão vai usar, pedida à
    // mesma função. Estava aqui uma cópia com números à mão (190 x lh+8) que
    // dizia espelhar o `layout_input` e não espelhava — um `checkbox` reservava
    // um campo de texto e pintava um quadrado.
    //
    // `None` de altura disponível: uma caixa numa linha não tem containing block
    // de altura definida, logo `height:%` vale `auto`, como no browser.
    medida_do_input(dom, id, &css, avail_w, None, None, ctx).outer()
}

/// Um segmento de texto colorido/pesado posicionado numa linha (após o wrap).
/// `atomic: Some((idx, kind))` = uma caixa de `ww × wh` (pintada pela emissão),
/// ou um marcador de largura zero que só existe para receber a sua geometria.
struct Segment {
    text: String,
    text_width: f32,
    color: u32,
    bold: bool,
    italic: bool,
    deco: u8,
    owners: Vec<NodeIdx>,
    atomic: Option<(NodeIdx, AtomicKind)>,
    ww: f32,
    wh: f32,
    /// A largura do espaço que precede este segmento e NÃO lhe pertence: o que
    /// veio do run ANTERIOR, em `antes <a>alvo</a>`.
    ///
    /// É um vão antes do segmento, nunca parte do `text`/`text_width`/`ww` — e é
    /// por isso que existe em vez de se somar à largura. O espaço ocupa lugar na
    /// linha (o `<a>` começa depois dele) mas é conteúdo do texto anónimo que vem
    /// antes, portanto a CAIXA do `<a>` não o contém: o Chrome responde `x=48,
    /// w=32` onde somá-lo dava `x=40, w=40`. Quando o segmento é FUNDIDO no
    /// anterior o vão passa a ser interior e vive dentro do `text` — é o mesmo
    /// espaço no mesmo sítio, visto do lado de dentro.
    lead_w: f32,
}

/// Quebra uma sequência de RUNS coloridos em LINHAS por palavra (word-wrap), juntando
/// runs adjacentes na mesma linha. Cada linha é um vetor de [`Segment`] (pedaços
/// coloridos contíguos). Uma palavra que não cabe começa nova linha. FIEL AOS
/// ESPAÇOS do fonte: um espaço só entra entre duas palavras quando o texto
/// ORIGINAL tinha whitespace ali (colapsado p/ 1) — inclusive ATRAVÉS de runs
/// (`<a>Bootstrap</a>, by` NÃO ganha espaço antes da vírgula; antes toda palavra
/// ganhava espaço e a pontuação descolava).
/// Colapsa o whitespace de um run como o fluxo inline faz: sequências viram um
/// espaço só, o do fim some (o separador seguinte o recria) e o do início só
/// entra quando havia palavra antes na linha. É a normalização que o scanner
/// palavra-a-palavra produz implicitamente — o fast path precisa dela explícita
/// para que os dois caminhos gerem o MESMO texto.
///
/// `leading_space` é a resposta JÁ TOMADA pelo chamador à pergunta "havia
/// whitespace desde a última palavra?", e é a única coisa que decide o espaço da
/// frente. Perguntá-la outra vez aqui — exigindo além disso que o texto DESTE
/// run comece por whitespace — apagava o espaço em toda fronteira de elemento
/// inline: em `antes <a>alvo</a>`, o espaço está no fim do run anterior e o run
/// do `<a>` começa por 'a', portanto nenhum dos dois o emitia. A página saía
/// pintada com `antesalvo`, e cada fronteira encurtava a linha em um espaço, o
/// que mudava o ponto de quebra. O scanner palavra-a-palavra decide por
/// `pending_space && !at_line_start` e só por isso — é a mesma pergunta e passa
/// a ter a mesma resposta.
fn collapse_ws(text: &str, leading_space: bool) -> std::borrow::Cow<'_, str> {
    // O caso comum é o texto JÁ normalizado (uma palavra, ou palavras separadas
    // por um espaço só, sem borda) — devolver emprestado evita uma alocação por
    // run, e um relayout de página grande são milhares deles.
    let needs_work = leading_space
        || text.starts_with(e_espaco_css)
        || text.ends_with(e_espaco_css)
        || text.contains("  ")
        || text.chars().any(|c| e_espaco_css(c) && c != ' ');
    if !needs_work {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + 1);
    if leading_space {
        out.push(' ');
    }
    let mut first = true;
    for word in crate::inline_box::palavras_css(text) {
        if !first {
            out.push(' ');
        }
        out.push_str(word);
        first = false;
    }
    std::borrow::Cow::Owned(out)
}

/// Acrescenta texto à linha, juntando ao último segmento quando o estilo é o
/// mesmo (é o que evita um segmento por palavra na hora de pintar).
/// `lead` é a largura do espaço com que `text` começa quando esse espaço veio do
/// run ANTERIOR (zero se não há espaço ou se ele é deste run). Ao abrir segmento
/// novo o espaço sai do texto e vira vão, para ficar de fora da caixa dos donos;
/// ao FUNDIR fica onde está, porque aí é interior ao segmento que o recebe.
fn push_segment(cur: &mut Vec<Segment>, run: &InlineRun, text: &str, width: f32, lead: f32) {
    if let Some(last) = cur.last_mut() {
        if last.atomic.is_none()
            && last.color == run.color
            && last.bold == run.bold
            && last.italic == run.italic
            && last.deco == run.deco
            && last.owners == run.owners
        {
            last.text.push_str(text);
            last.text_width += width;
            return;
        }
    }
    // Só sai do texto se sobrar texto: um segmento que fosse SÓ o vão não tem
    // dono a quem servir e ainda perderia o espaço.
    let separa = lead > 0.0 && text.starts_with(' ') && text.len() > 1;
    let (text, width, lead) = match separa {
        true => (&text[1..], width - lead, lead),
        false => (text, width, 0.0),
    };
    cur.push(Segment {
        text: text.to_string(),
        text_width: width,
        color: run.color,
        bold: run.bold,
        italic: run.italic,
        deco: run.deco,
        owners: run.owners.clone(),
        atomic: None,
        ww: 0.0,
        wh: 0.0,
        lead_w: lead,
    });
}

/// As TRÊS condições de `text-overflow: ellipsis` — e são três porque com
/// qualquer uma em falta o Chrome não põe reticências nenhumas.
///
/// 1. a propriedade pedida; 2. o transbordo ESCONDIDO (`visible` deixa o texto
/// sair da caixa e não há nada a cortar); 3. a linha a NÃO quebrar — com quebra,
/// o texto desce em vez de transbordar e a elipse nunca chega a ser devida.
///
/// ⚠️ CORTE declarado: o Chrome aplica-a também no eixo do bloco e em conteúdo
/// que transborda por outras razões. Aqui é só a linha única horizontal, que é
/// o que as 29 declarações `ellipsis` do corpus escrevem — todas num container
/// com `overflow:hidden` e `white-space:nowrap`.
fn elipse_pedida(css: &ComputedStyle, nowrap: bool) -> bool {
    css.text_overflow == Some(crate::style::vocab::TextOverflow::Ellipsis)
        && matches!(
            css.overflow_x,
            Some(crate::scrollbar::Overflow::Hidden | crate::scrollbar::Overflow::Auto)
                | Some(crate::scrollbar::Overflow::Scroll)
        )
        && nowrap
}

/// Corta cada linha que transborda `content_w` e acrescenta-lhe `…`.
///
/// O orçamento é `content_w` MENOS a largura da própria elipse: o browser
/// garante que as reticências ficam DENTRO da caixa, e cortar em `content_w` e
/// depois somar o `…` punha-as de fora — o mesmo transbordo que isto existe
/// para esconder, um carácter mais estreito.
///
/// Uma caixa atómica no ponto de corte é DESCARTADA em vez de encolhida: um
/// `<img>` não tem prefixo, e escalá-lo para caber inventaria uma geometria que
/// o Chrome não produz.
fn aplicar_elipse(
    lines: Vec<Vec<Segment>>,
    content_w: f32,
    font_size: f32,
    mono: bool,
    m: &dyn TextMeasurer,
) -> Vec<Vec<Segment>> {
    const ELIPSE: &str = "…";
    lines
        .into_iter()
        .map(|line| {
            let total: f32 = line
                .iter()
                .map(|s| s.lead_w + if s.atomic.is_some() { s.ww } else { s.text_width })
                .sum();
            if total <= content_w {
                return line;
            }
            let w_elipse = m.text_width(ELIPSE, font_size, mono, false, false);
            let orcamento = content_w - w_elipse;
            let mut out: Vec<Segment> = Vec::with_capacity(line.len());
            let mut acc = 0.0f32;
            for mut seg in line {
                let largura = if seg.atomic.is_some() {
                    seg.ww
                } else {
                    seg.text_width
                };
                if acc + seg.lead_w + largura <= orcamento {
                    acc += seg.lead_w + largura;
                    out.push(seg);
                    continue;
                }
                if seg.atomic.is_none() {
                    let disp = orcamento - acc - seg.lead_w;
                    let (n, w) = crate::inline_box::prefixo_que_cabe(
                        &seg.text,
                        disp,
                        font_size,
                        mono,
                        seg.bold,
                        seg.italic,
                        m,
                    );
                    seg.text.truncate(n);
                    seg.text.push_str(ELIPSE);
                    seg.text_width = w + w_elipse;
                    out.push(seg);
                    return out;
                }
                // atómica a transbordar: cai fora, e a elipse vai para o texto
                // que ficou — ou abre segmento próprio se a linha começa por ela.
                break;
            }
            match out.last_mut() {
                Some(last) if last.atomic.is_none() => {
                    last.text.push_str(ELIPSE);
                    last.text_width += w_elipse;
                }
                _ => out.push(Segment {
                    text: ELIPSE.to_string(),
                    text_width: w_elipse,
                    color: 0,
                    bold: false,
                    italic: false,
                    deco: 0,
                    owners: Vec::new(),
                    atomic: None,
                    ww: 0.0,
                    wh: 0.0,
                    lead_w: 0.0,
                }),
            }
            out
        })
        .collect()
}

fn wrap_runs(
    runs: &[InlineRun],
    // A largura disponível DA LINHA `i` — não uma largura só para todas. Um
    // float encurta uma linha e deixa a seguinte inteira, e a diferença entre as
    // duas é o que faz o texto contornar a figura em vez de descer abaixo dela.
    max_w: &mut dyn FnMut(usize) -> f32,
    font_size: f32,
    mono: bool,
    // Pode partir-se DENTRO de um aglomerado? Vem do elemento que possui o
    // fluxo, e não de cada run: `word-break`/`overflow-wrap` são herdadas e o
    // corpus real escreve-as sempre no container (13 folhas, zero excepções).
    // Guardá-las por run era a alternativa e custava um campo em cada `InlineRun`
    // para responder o mesmo valor em todos eles.
    quebra: crate::inline_box::QuebraDentro,
    m: &dyn TextMeasurer,
) -> Vec<Vec<Segment>> {
    let _phase = crate::metrics::phases::scope("wrap-runs");
    // A largura do espaço só interessa ao caminho palavra-a-palavra. Medida
    // sempre, era metade de todas as medições de texto de um relayout — uma por
    // chamada, mesmo quando o fast path respondia sozinho.
    let mut space_w_memo: Option<f32> = None;
    let mut space_w = |m: &dyn TextMeasurer| -> f32 {
        *space_w_memo.get_or_insert_with(|| m.text_width(" ", font_size, mono, false, false))
    };
    let mut lines: Vec<Vec<Segment>> = Vec::new();
    let mut cur: Vec<Segment> = Vec::new();
    let mut cur_w = 0.0f32;
    let mut at_line_start = true;
    // havia whitespace no ORIGINAL desde a última palavra? (carrega entre runs)
    let mut pending_space = false;

    // -- O CLUSTER: a unidade que a linha move.
    //
    // Uma linha so pode quebrar numa OPORTUNIDADE DE QUEBRA, e no texto essa
    // oportunidade e o whitespace. Entre dois runs colados -- `<span>[</span>`
    // seguido de `<span>135</span>`, a marcacao de referencia do MediaWiki --
    // nao existe nenhuma, e o Chrome desce o `[135]` inteiro para a linha
    // seguinte. Decidir peca a peca partia-o ao meio: medido na Wikipedia, um
    // `<a>` com fragmentos de 8px no canto direito da linha e 24px no inicio da
    // seguinte, e a caixa dele passava a ser a uniao dos dois -- 752 de largura
    // onde o Chrome da 21.
    //
    // Por isso as pecas sem oportunidade entre elas sao acumuladas aqui e a
    // pergunta "cabe?" e feita ao conjunto, uma vez. Nao e uma regra nova: e a
    // regra do CSS aplicada a unidade certa. Uma peca sozinha, que e o caso
    // esmagador, comporta-se exatamente como antes.
    struct Peca {
        run: usize,
        texto: String,
        largura: f32,
        atomico: Option<(NodeIdx, AtomicKind, f32, f32)>,
    }
    let mut cluster: Vec<Peca> = Vec::new();
    let mut cluster_w = 0.0f32;
    // havia whitespace ANTES do cluster? e esse whitespace veio de FORA do run
    // que abre o cluster? (a segunda pergunta decide de quem e o vao -- ver o
    // `lead_w` do `Segment`.)
    let mut cluster_espaco = false;
    let mut cluster_de_fora = false;
    // o whitespace pendente veio de um run ANTERIOR (e nao de dentro deste)?
    let mut espaco_de_fora = false;

    macro_rules! fechar_cluster {
        () => {
            if !cluster.is_empty() {
                let sep = cluster_espaco && !at_line_start;
                let need = if sep {
                    space_w(m) + cluster_w
                } else {
                    cluster_w
                };
                // `break-all` ENCHE a linha corrente antes de descer, e por isso
                // salta a quebra prévia: descer primeiro e partir depois deixava
                // à direita um vazio do tamanho da palavra, que é exatamente o
                // que `break-all` existe para não deixar. Só vale para um
                // aglomerado todo de texto — uma caixa atómica (um `<img>`, um
                // widget) é inquebrável e continua a descer inteira.
                let so_texto = cluster.iter().all(|p| p.atomico.is_none());
                let enche_a_linha =
                    quebra == crate::inline_box::QuebraDentro::Sempre && so_texto;
                if !at_line_start && !enche_a_linha && cur_w + need > max_w(lines.len()) {
                    lines.push(std::mem::take(&mut cur));
                    cur_w = 0.0;
                    at_line_start = true;
                }
                // recalculado DEPOIS da quebra: um cluster que abre linha nao
                // leva o espaco com ele.
                let sep = cluster_espaco && !at_line_start;
                let mut primeiro = true;
                for peca in cluster.drain(..) {
                    let run = &runs[peca.run];
                    let com_espaco = primeiro && sep;
                    let espaco = if com_espaco { space_w(m) } else { 0.0 };
                    match peca.atomico {
                        Some((a_idx, kind, ww, wh)) => {
                            cur.push(Segment {
                                text: String::new(),
                                text_width: 0.0,
                                color: run.color,
                                bold: false,
                                italic: false,
                                deco: 0,
                                owners: run.owners.clone(),
                                atomic: Some((a_idx, kind)),
                                ww,
                                wh,
                                lead_w: espaco,
                            });
                            cur_w += ww + espaco;
                        }
                        None => {
                            let vao = if com_espaco && cluster_de_fora {
                                espaco
                            } else {
                                0.0
                            };
                            let mut texto = String::with_capacity(peca.texto.len() + 1);
                            if com_espaco {
                                texto.push(' ');
                            }
                            texto.push_str(&peca.texto);
                            let largura = peca.largura + espaco;
                            // PARTIR DENTRO DA PALAVRA — o que `overflow-wrap` e
                            // `word-break` ligam. A pergunta faz-se aqui, na
                            // emissão de uma peça, porque é aqui que já se sabe
                            // quanto resta da linha; fazê-la antes, sobre o
                            // aglomerado inteiro, obrigava a uma segunda regra de
                            // quebra ao lado da que já existe.
                            let disponivel = max_w(lines.len());
                            let partir = match quebra {
                                crate::inline_box::QuebraDentro::Nao => false,
                                // `break-word`: só quando a palavra não cabe NEM
                                // numa linha vazia. Se cabe, ela já desceu inteira
                                // na quebra prévia e parti-la seria errado.
                                crate::inline_box::QuebraDentro::SePreciso => {
                                    peca.largura > disponivel
                                }
                                crate::inline_box::QuebraDentro::Sempre => {
                                    cur_w + largura > disponivel
                                }
                            };
                            if partir {
                                let mut resto = texto.as_str();
                                let mut lead = vao;
                                while !resto.is_empty() {
                                    let disp = max_w(lines.len()) - cur_w;
                                    let (mut n, mut w) = crate::inline_box::prefixo_que_cabe(
                                        resto,
                                        disp,
                                        font_size,
                                        mono,
                                        run.bold,
                                        run.italic,
                                        m,
                                    );
                                    if n == 0 && at_line_start {
                                        // Numa caixa mais estreita que um glifo,
                                        // nada cabe e descer de linha não muda
                                        // isso: sem um carácter forçado o laço
                                        // não termina. Transbordar um carácter é
                                        // o que o browser também faz.
                                        n = resto.chars().next().map_or(0, char::len_utf8);
                                        w = m.text_width(
                                            &resto[..n],
                                            font_size,
                                            mono,
                                            run.bold,
                                            run.italic,
                                        );
                                    }
                                    if n == 0 {
                                        lines.push(std::mem::take(&mut cur));
                                        cur_w = 0.0;
                                        at_line_start = true;
                                        continue;
                                    }
                                    push_segment(&mut cur, run, &resto[..n], w, lead);
                                    lead = 0.0;
                                    cur_w += w;
                                    at_line_start = false;
                                    resto = &resto[n..];
                                    if !resto.is_empty() {
                                        lines.push(std::mem::take(&mut cur));
                                        cur_w = 0.0;
                                        at_line_start = true;
                                    }
                                }
                            } else {
                                push_segment(&mut cur, run, &texto, largura, vao);
                                cur_w += largura;
                            }
                        }
                    }
                    primeiro = false;
                    at_line_start = false;
                }
                cluster_w = 0.0;
                cluster_espaco = false;
                cluster_de_fora = false;
            }
        };
    }
    // acrescenta uma peca ao cluster corrente, abrindo-o se estiver vazio.
    macro_rules! juntar {
        ($peca:expr, $w:expr) => {{
            if cluster.is_empty() {
                cluster_espaco = pending_space;
                cluster_de_fora = espaco_de_fora;
            }
            cluster.push($peca);
            cluster_w += $w;
            pending_space = false;
            espaco_de_fora = false;
        }};
    }

    for (i, run) in runs.iter().enumerate() {
        // WIDGET: uma "palavra" inquebravel de run.ww pontos, segmento proprio.
        if let Some((a_idx, kind)) = run.atomic {
            // BREAK: entra na linha (para receber a sua caixa) e FECHA-A.
            if kind == AtomicKind::Break {
                fechar_cluster!();
                cur.push(Segment {
                    text: String::new(),
                    text_width: 0.0,
                    color: run.color,
                    bold: false,
                    italic: false,
                    deco: 0,
                    owners: run.owners.clone(),
                    atomic: Some((a_idx, AtomicKind::Break)),
                    ww: 0.0,
                    wh: 0.0,
                    lead_w: 0.0,
                });
                lines.push(std::mem::take(&mut cur));
                cur_w = 0.0;
                at_line_start = true;
                pending_space = false;
                espaco_de_fora = false;
                continue;
            }
            // MARKER: largura zero, nao quebra a linha, nao consome o espaco
            // pendente -- so marca uma posicao para quem lhe quiser a caixa.
            if kind == AtomicKind::Marker {
                fechar_cluster!();
                cur.push(Segment {
                    text: String::new(),
                    text_width: 0.0,
                    color: run.color,
                    bold: false,
                    italic: false,
                    deco: 0,
                    owners: run.owners.clone(),
                    atomic: Some((a_idx, AtomicKind::Marker)),
                    ww: 0.0,
                    wh: 0.0,
                    lead_w: 0.0,
                });
                continue;
            }
            juntar!(
                Peca {
                    run: i,
                    texto: String::new(),
                    largura: run.ww,
                    atomico: Some((a_idx, kind, run.ww, run.wh)),
                },
                run.ww
            );
            continue;
        }
        // so whitespace: vira separador pendente e nao abre peca. Decidido ANTES
        // de normalizar, porque um separador pendente faz a normalizacao
        // devolver " " -- nao-vazio -- e o run deixaria de ser reconhecido como
        // o separador que e.
        if !run.text.is_empty() && so_espaco_css(&run.text) {
            fechar_cluster!();
            pending_space = true;
            espaco_de_fora = true;
            continue;
        }
        if run.text.is_empty() {
            continue;
        }
        // O espaco da frente e devido quando havia whitespace desde a ultima
        // palavra, esteja ele no fim do run ANTERIOR ou no inicio deste.
        if run.text.starts_with(e_espaco_css) {
            fechar_cluster!();
            pending_space = true;
            // NAO e vao: este espaco esta no texto DESTE run, logo pertence aos
            // donos dele e vive dentro do segmento. So o espaco que vem de um
            // run ANTERIOR e um vao. E a diferenca entre `<a> alvo</a>` e
            // `antes <a>alvo</a>` -- o `::after` com `content:" (…)"` e o
            // primeiro caso, e o espaco tem de sobreviver no texto.
            espaco_de_fora = false;
        }
        // FAST PATH: o run inteiro e UMA peca quando nao tem whitespace dentro.
        //
        // Medir a string inteira e o que um browser faz, e e o que evita uma
        // medicao por palavra: `wrap-runs` era 38% de um relayout de pagina
        // grande, com 11 000 `text_width` por frame.
        let miolo = apara_css(&run.text);
        if !miolo.contains(e_espaco_css) {
            let w = m.text_width(miolo, font_size, mono, run.bold, run.italic);
            let terminava_em_espaco = run.text.ends_with(e_espaco_css);
            juntar!(
                Peca {
                    run: i,
                    texto: miolo.to_string(),
                    largura: w,
                    atomico: None
                },
                w
            );
            if terminava_em_espaco {
                fechar_cluster!();
                pending_space = true;
                espaco_de_fora = true;
            }
            continue;
        }
        // FAST PATH 2 — o run INTEIRO cabe na linha corrente.
        //
        // Medir palavra a palavra custa uma medicao por palavra, e medir texto e
        // a unica coisa que o layout pede ao backend: `wrap-runs` era 38% de um
        // relayout de pagina grande, com 11 000 `text_width` por frame. Quando o
        // run cabe todo, uma medicao responde por todas.
        //
        // So e seguro sob duas condicoes, e as duas sao sobre CLUSTERS: o run
        // tem de ABRIR um (senao a sua primeira palavra pertence ao aglomerado
        // que vem de tras e nao pode ser commitada sozinha) e tem de FECHAR um
        // (senao a sua ultima palavra pode ainda vir a ter de descer com o run
        // seguinte). Sem as duas, o caminho lento e o que responde certo.
        let abre_cluster = cluster.is_empty();
        let fecha_cluster = run.text.ends_with(e_espaco_css);
        if abre_cluster && fecha_cluster {
            let normalizado = collapse_ws(&run.text, pending_space && !at_line_start);
            if !normalizado.is_empty() {
                let w = m.text_width(&normalizado, font_size, mono, run.bold, run.italic);
                if !at_line_start && cur_w + w <= max_w(lines.len()) {
                    let vao = if pending_space && espaco_de_fora {
                        space_w(m)
                    } else {
                        0.0
                    };
                    push_segment(&mut cur, run, &normalizado, w, vao);
                    cur_w += w;
                    at_line_start = false;
                    pending_space = true;
                    espaco_de_fora = true;
                    continue;
                }
            }
        }
        // scanner ws/palavra: cada whitespace FECHA o cluster (e uma
        // oportunidade de quebra) e cada palavra abre o seguinte.
        let mut rest = run.text.as_str();
        while !rest.is_empty() {
            if rest.starts_with(e_espaco_css) {
                fechar_cluster!();
                pending_space = true;
                espaco_de_fora = false;
                rest = rest.trim_start_matches(e_espaco_css);
                continue;
            }
            let end = rest.find(e_espaco_css).unwrap_or(rest.len());
            let word = &rest[..end];
            rest = &rest[end..];
            let ww = m.text_width(word, font_size, mono, run.bold, run.italic);
            juntar!(
                Peca {
                    run: i,
                    texto: word.to_string(),
                    largura: ww,
                    atomico: None
                },
                ww
            );
        }
        if run.text.ends_with(e_espaco_css) {
            fechar_cluster!();
            pending_space = true;
            espaco_de_fora = true;
        }
    }
    fechar_cluster!();
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(vec![Segment {
            text: String::new(),
            text_width: 0.0,
            color: 0,
            bold: false,
            italic: false,
            deco: 0,
            owners: Vec::new(),
            atomic: None,
            ww: 0.0,
            wh: 0.0,
            lead_w: 0.0,
        }]);
    }
    lines
}

/// Quebra `text` em LINHAS que cabem em `max_w` (word-wrap do CSS `white-space:
/// normal`): acumula palavras separadas por espaço; quando a próxima não cabe,
/// fecha a linha e começa outra. Uma palavra maior que `max_w` fica sozinha na
/// linha (não quebra no meio da palavra — `overflow-wrap:normal`).
fn wrap_text(
    text: &str,
    max_w: f32,
    font_size: f32,
    mono: bool,
    m: &dyn TextMeasurer,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_w = 0.0f32;
    let space_w = m.text_width(" ", font_size, mono, false, false);
    for word in text.split_whitespace() {
        let word_w = m.text_width(word, font_size, mono, false, false);
        if current.is_empty() {
            current.push_str(word);
            current_w = word_w;
        } else if current_w + space_w + word_w <= max_w {
            current.push(' ');
            current.push_str(word);
            current_w += space_w + word_w;
        } else {
            // não cabe: fecha a linha atual e começa nova com a palavra.
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
            current_w = word_w;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(test)]
mod tests;
