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

use crate::dom::{BoxCacheTarget, Dom, IntrinsicWidthKey, LayoutMeasureKey, LayoutMeasureTarget, NodeIdx, NodeKind};
use crate::inline_box::{AtomicKind, apara_css, e_espaco_css, so_espaco_css};
use crate::style::{ComputedStyle, ResolveCtx};

mod bfc;
mod bfc_evita_float;
pub(crate) mod caixa;
mod caixa_contentora;
mod clearfix;
mod dimensao_indefinida;
mod display;
mod empilhamento;
mod float;
mod inline_fragmentos;
mod input;
mod select;
mod intrinseco_min_max;
mod tamanho_intrinseco;
mod itens;
mod fonte_metricas;
mod medida;
pub mod medidor_ativo;
mod medidor_texto;
mod pintura;
mod margem_escapada;
mod overflow_viewport;
mod posicao_estatica;
mod posicionado;
mod pseudo_bloco;
mod pseudo_caixa;
mod relativo;
mod fundo_imagem;
mod replaced;
mod replaced_transferido;
pub(crate) mod bloco;
mod bloco_caixa;
mod fragmento;
mod rtl_bloco;
mod sequencia;
mod vertical;
mod linha_ib;
mod alinhamento_vertical;
use self::coluna::{align_offset, justify_offsets, layout_children_column};
use self::flex::layout_children_horizontal;
use self::grid::layout_children_grid;
use self::linha::layout_inline_flow;
use self::quebra::wrap_runs;
use self::runs::{InlineRun, collect_runs, pseudo_run};
use self::segmento::{Segment, aplicar_elipse, collapse_ws, elipse_pedida, push_segment};
mod coluna;
mod coluna_rtl;
mod coluna_shrink;
mod coluna_wrap;
mod coluna_wrap_largura;
mod eixos_flex;
mod flex;
mod flex_baseline;
mod flex_basis_content;
mod flex_limites;
mod flex_linhas;
mod flex_margens_auto;
mod flex_pseudo;
mod flex_stretch_replaced;
mod grid;
mod grid_linhas;
mod grid_tracks;
mod hifen;
mod linha;
mod quebra;
mod runs;
mod segmento;
mod texto_solto;
mod tabulacao;
mod transformacao;
pub(crate) use self::bloco::layout_block;
pub use self::fragmento::{ChildRef, Fragment};
pub(crate) use self::fragmento::insert_item;
use self::fragmento::{KeyBase, emit_fragment, layout_block_reusing};
use self::vertical::layout_children_vertical;
use self::linha_ib::layout_inline_block_line;
use self::relativo::aplica_offset_relativo;

pub use self::display::{Corners, DisplayItem, DisplayList, Geometry, Rect, ScrollRegion};
pub use self::medidor_texto::{ApproxMeasurer, TextMeasurer};
pub use self::pintura::{emit_scrollbar, emit_scrollbar_in};
pub use self::transformacao::{Mat2d, TransformList, TransformOp, MAX_TRANSFORM_OPS};
pub(crate) use self::bfc::BlockFormattingContext;
pub(crate) use self::caixa::{font_px, is_non_rendered_tag, used_display};
pub(crate) use self::float::Exclusao;
pub(crate) use self::itens::{record_box_rect, record_node_rect, reserve_box_order, reserve_node_order};
pub(crate) use self::medida::intrinsic_outer_width;
pub(crate) use self::pintura::border_items;
pub(crate) use self::posicionado::is_out_of_flow;
use self::caixa::{css_display, em_contexto_inline, is_block_level, is_inline_block, is_inline_text_container, whitespace_is_inline_separator};
use self::float::{banda_livre, fecha_a_corrida, float_of};
use self::input::{layout_button, layout_input, medida_do_input};
use self::select::layout_select;
use self::itens::{translate_item, walk_items};
use self::medida::{child_outer_height, child_outer_width, collect_text, content_natural_width};
use self::pintura::{apply_opacity, body_background, cor_visivel, decoration_code, deve_suprimir_fundo, is_text_input_tag, italico, tag_de};
use self::posicionado::{collect_out_of_flow, e_display_none, layout_out_of_flow, resolve_height};
use self::replaced::{layout_canvas, layout_image, layout_svg_placeholder};

/// The one-box bridge for legacy callers that still begin at a DOM node.
///
/// A layout path that can name the box must pass it through instead. Refusing a
/// split inline here is intentional: choosing `.first()` would make one half
/// of the element silently stand in for the other.
pub(crate) fn unica_caixa_do_no(dom: &Dom, no: NodeIdx) -> Option<crate::boxes::BoxId> {
    match dom.box_tree().boxes_of(no) {
        [caixa] => Some(*caixa),
        [] => None,
        caixas => panic!(
            "o layout de bloco recebeu o no {no} com {} caixas; o chamador tem de levar o BoxId exacto",
            caixas.len()
        ),
    }
}

/// Endereço estável de uma caixa para caches que sobrevivem à reconstrução da
/// árvore. O `BoxId` é a identidade operacional dentro de uma passada; o par
/// `(nó, ordinal)` é usado somente na fronteira persistente do cache.
pub(crate) fn caixa_cache_target(
    dom: &Dom,
    no: NodeIdx,
    caixa: crate::boxes::BoxId,
) -> BoxCacheTarget {
    let tree = dom.box_tree();
    let ordinal = tree
        .boxes_of(no)
        .iter()
        .position(|&candidate| candidate == caixa)
        .expect("o cache recebeu uma caixa que não pertence ao nó") as u32;
    BoxCacheTarget { node: no, ordinal }
}

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
    caixa: Option<crate::boxes::BoxId>,
    avail_w: f32,
    avail_h: Option<f32>,
    forced_outer_w: Option<f32>,
    forced_outer_h: Option<f32>,
    shrink_to_fit: bool,
    ctx: &LayoutCtx,
) -> (f32, f32) {
    // A lista descartavel tambem carrega a BoxTree. Os chamadores novos passam
    // a identidade exata; os poucos caminhos legados de medida que ainda so
    // sabem o no entram aqui pela caixa unica do espelho, para que a descida
    // interna nunca perca a sequencia da arvore.
    let caixa = caixa.or_else(|| unica_caixa_do_no(dom, id));
    let measurer = ctx.measurer.identity();
    let key = LayoutMeasureKey {
        tree: dom.cache_identity(),
        node_epoch: dom.layout_epoch(id),
        style_epoch: crate::style::props::style_epoch(),
        target: caixa
            .map(|caixa| LayoutMeasureTarget::Caixa(caixa_cache_target(dom, id, caixa)))
            .unwrap_or(LayoutMeasureTarget::No(id)),
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
    let mut scratch = DisplayList::for_dom(dom);
    let size = layout_block(
        dom,
        id,
        caixa,
        0.0,
        0.0,
        avail_w,
        avail_h,
        forced_outer_w,
        forced_outer_h,
        // `measure_block` nunca mede com o main-size DURO de coluna — quem o
        // usa (`layout_children_column`) mede naturalmente aqui e só impõe o
        // resolvido depois, no `layout_block_reusing` final.
        false,
        shrink_to_fit,
        // A MEDIDA de um bloco é a do seu conteúdo, não a da banda onde calha
        // ficar: medir com o float à frente dava uma largura intrínseca que
        // mudava consoante a vizinhança — um BFC novo e vazio, nunca lido
        // depois, é a mesma isolação que o `&[]` de antes dava.
        &BlockFormattingContext::new(),
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
    let mut list = DisplayList::for_dom(dom);
    // A árvore de caixas deste documento, memoizada no `Dom`. Vive na lista
    // para que `record_node_rect`/`reserve_node_order` (em `itens.rs`)
    // traduzam nó→caixa por dentro, sem que nenhum dos chamadores mude.
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
    // A entrada do layout é a árvore de caixas, não a lista de filhos do
    // documento. Isto importa mesmo quando a forma atual coincide: o `BoxId`
    // concreto percorre o despacho inteiro sem a ponte `NodeIdx -> caixa
    // única`, e um nó que não gera caixa simplesmente não aparece aqui.
    let tree = std::rc::Rc::clone(&list.tree);
    for caixa in tree.roots() {
        let Some(child) = tree.node_of(caixa) else {
            // A raiz do documento não deve produzir uma caixa anónima; não
            // inventamos geometria se uma extensão futura o fizer sem antes
            // definir o seu formatting context de topo.
            continue;
        };
        // position:absolute/fixed não participa do fluxo, inclusive quando é filho
        // direto do documento; será layoutado na passada final por z-index.
        if is_out_of_flow(dom, child) {
            continue;
        }
        // o containing block da raiz é a VIEWPORT: `height:100%` no <html> resolve
        // contra a altura da janela (base do `h-100` de páginas reais).
        //
        // O BFC passado aqui não é lido: `child` é o elemento RAIZ do documento
        // (tipicamente `<html>`), que estabelece sempre o seu próprio BFC (CSS
        // 2.1 §9.4.1) — `bloco.rs` cria um novo internamente e ignora este.
        let (_, h) = layout_block(
            dom,
            child,
            Some(caixa),
            0.0,
            cursor_y,
            ctx.viewport_w,
            Some(ctx.viewport_h),
            None,
            None,
            false,
            false,
            &BlockFormattingContext::new(),
            ctx,
            &mut list,
        );
        cursor_y += h;
    }
    list.content_height = cursor_y;
    // ── PASSADA OUT-OF-FLOW: `position:absolute/fixed` saíram do fluxo (não
    // ocuparam espaço). Pintados contra o VIEWPORT com top/right/bottom/left,
    // por Z-INDEX: negativo pinta ANTES do fluxo normal — atrás dele —, e
    // ≥0/auto pinta DEPOIS — por cima —, como o CSS 2.1 Apêndice E pede (ver
    // `empilhamento.rs`; esta linha dizia "sem z-index real" e não diz mais).
    // V1: o containing block é sempre a viewport (o de `absolute` — ancestral
    // positioned — e o "fica fixo ao rolar" do `fixed` são a v2).
    let mut out_of_flow = Vec::new();
    // Só varre se a página PODE ter algum: a varredura pede o estilo computado
    // de cada nó da árvore, e era 78% de um frame de mutação numa página que não
    // tem um único posicionado.
    if dom.may_have_out_of_flow() {
        collect_out_of_flow(dom, &list.tree, dom.root, &mut out_of_flow);
    }
    // STACKING CONTEXTS: ordena pela cadeia de contextos, não pelo `z-index`
    // isolado. Assim `[0, 100]` (filho 100 dentro de um grupo 0) continua
    // atrás de `[1]` (irmão do grupo), como no Apêndice E. Sort ESTÁVEL: chaves
    // iguais preservam a ordem do documento.
    // O rect do containing block de cada abs é lido do `node_rects` JÁ preenchido
    // pelo fluxo normal (o ancestral positioned já foi pintado). Clona antes do
    // empréstimo mutável de `list`.
    // A geometria COMPLETA (com as subárvores reusadas): o containing block de
    // um `absolute` pode ser um ancestral cujo retângulo veio de um fragmento.
    crate::bump!(out_of_flow, out_of_flow.len());
    // Separa os NEGATIVOS: o sort acima já os deixa em ordem ascendente (mais
    // negativo primeiro) e o filtro preserva essa ordem — a mesma que o
    // Apêndice E pede DENTRO do grupo. `resto` (≥0/auto) segue exatamente o
    // caminho de sempre, por cima do fluxo.
    let mut rects_conhecidos = list.geometry_now().rects;
    let mut positioned = Vec::with_capacity(out_of_flow.len());
    for alvo in out_of_flow {
        let mut fragment = DisplayList::for_dom(dom);
        layout_out_of_flow(dom, alvo, ctx, &rects_conhecidos, &mut fragment);
        rects_conhecidos.extend(fragment.geometry_now().rects);
        positioned.push((empilhamento::stacking_key(dom, alvo.node), fragment));
    }
    positioned.sort_by(|(a, _), (b, _)| a.cmp(b));
    let mut negativos = DisplayList::for_dom(dom);
    for (key, fragment) in positioned {
        if key.first().copied().unwrap_or(0) < 0 {
            empilhamento::merge_after(&mut negativos, fragment);
        } else {
            empilhamento::merge_after(&mut list, fragment);
        }
    }
    if !negativos.items.is_empty() || !negativos.children.is_empty() {
        // Numa lista À PARTE: os itens negativos só entram em `list` depois
        // de prontos, PREPENDIDOS — nunca escritos directamente nela, senão
        // sairiam na mesma posição (depois do fluxo) que este lote corrige.
        empilhamento::merge_before(&mut list, negativos);
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
    crate::bump!(node_rects, list.box_rects.len());
    crate::bump!(scroll_regions, list.scroll_regions.len());
    list
}

/// O retângulo (border-box) de um nó, computando o layout do documento na largura
/// dada — a base de `element.getBoundingClientRect()`. `None` se o nó não é
/// renderável (texto/`display:none`/metadata não têm rect próprio).
/// Roda o layout inteiro (O(n)); para várias consultas no mesmo frame, reuse a
/// `DisplayList` de `layout_document` e leia `box_rects`/`rect_of_node` direto.
pub fn bounding_rect(dom: &Dom, node: NodeIdx, ctx: &LayoutCtx) -> Option<Rect> {
    layout_document(dom, ctx).rect_of(node)
}


#[cfg(test)]
mod tests;
