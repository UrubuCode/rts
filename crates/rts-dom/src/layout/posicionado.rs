//! Fora do fluxo: `absolute`/`fixed`, o containing block, e a altura
//! resolvida contra ele.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução destes pedaços é byte a byte a do original.

use super::*;
/// O rect do CONTAINING BLOCK de um `position:absolute` = o ancestral mais próximo
/// com `position != static` (relative/absolute/fixed), lido do `node_rects` do
/// fluxo. `None` = nenhum ancestral positioned → o containing block é a viewport
/// (a raiz inicial). Um `fixed` sempre usa a viewport (tratado no caller).
fn containing_block_rect(
    dom: &Dom,
    id: NodeIdx,
    flow_rects: &crate::fasthash::FastMap<NodeIdx, Rect>,
) -> Option<Rect> {
    let mut cur = dom.node(id).parent;
    while let Some(p) = cur {
        let positioned = dom
            .computed_style_idx(p)
            .and_then(|c| c.position)
            .map(|pos| pos != crate::style::Position::Static)
            .unwrap_or(false);
        if positioned {
            if let Some(r) = flow_rects.get(&p) {
                return Some(*r);
            }
            // Um ancestral posicionado SEM caixa (não foi layoutado) não serve de
            // containing block, e continuar a subir escolhe um contentor que o
            // browser nunca escolheria — foi assim que um elemento de um ramo
            // escondido se ancorou num contentor com a altura do documento.
            //
            // Devolver `None` faz o chamador cair na viewport, que é o
            // containing block inicial. É uma aproximação, e a alternativa
            // (reconstruir a caixa do ancestral) não tem caso desde que o ramo
            // escondido deixou de ser layoutado.
            return None;
        }
        cur = dom.node(p).parent;
    }
    None
}

/// DFS que coleta os nós `position:absolute/fixed`. Não desce DENTRO de um
/// out-of-flow (os filhos dele pertencem ao layout dele; abs-dentro-de-abs = v2).
pub(in crate::layout) fn collect_out_of_flow(dom: &Dom, id: NodeIdx, out: &mut Vec<NodeIdx>) {
    for &child in &dom.node(id).children {
        // `display:none` num ANCESTRAL remove a subárvore inteira do layout, e o
        // fora de fluxo não é exceção: um `position:absolute` dentro de um ramo
        // escondido não gera caixa nenhuma no browser.
        //
        // Sem isto ele era medido e pintado, e — por o pai escondido não ter
        // caixa — a procura do containing block saltava-o e ia parar a um
        // ancestral posicionado muito acima: na Wikipédia, um
        // `<input type=checkbox height:100%>` de um menu escondido resolvia
        // contra um contentor com a altura do DOCUMENTO e vinha com 96 665px.
        if e_display_none(dom, child) {
            continue;
        }
        if is_out_of_flow(dom, child) {
            out.push(child);
        } else {
            collect_out_of_flow(dom, child, out);
        }
    }
}

/// `true` se este nó declara `display:none` — a pergunta que tira uma subárvore
/// inteira do layout. Só o próprio nó: quem varre a árvore de cima para baixo já
/// não desce nele, e é isso que a torna hereditária na prática.
pub(in crate::layout) fn e_display_none(dom: &Dom, id: NodeIdx) -> bool {
    matches!(&dom.node(id).kind, NodeKind::Element { .. })
        && dom
            .computed_style_idx(id)
            .and_then(|c| c.effective_display())
            == Some(crate::style::DisplayKind::None)
}

/// Layouta UM nó fora do fluxo contra o viewport: mede shrink-to-fit e posiciona
/// pelos offsets (`left` OU `right`−largura; `top` OU `bottom`−altura; sem nenhum
/// dos dois no eixo → 0).
pub(in crate::layout) fn layout_out_of_flow(
    dom: &Dom,
    id: NodeIdx,
    ctx: &LayoutCtx,
    flow_rects: &crate::fasthash::FastMap<NodeIdx, Rect>,
    list: &mut DisplayList,
) {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    // CONTAINING BLOCK: `absolute` posiciona contra o ancestral positioned mais
    // próximo (o Google ancora os ícones no canto direito da CAIXA DE BUSCA, não
    // da tela); `fixed` sempre contra a viewport. Sem ancestral positioned →
    // viewport. `cb` = (origem_x, origem_y, largura, altura) do container.
    let is_fixed = matches!(css.position, Some(crate::style::Position::Fixed));
    let cb = if is_fixed {
        Rect::new(0.0, 0.0, ctx.viewport_w, ctx.viewport_h)
    } else {
        containing_block_rect(dom, id, flow_rects)
            .unwrap_or_else(|| Rect::new(0.0, 0.0, ctx.viewport_w, ctx.viewport_h))
    };
    let resolve = ResolveCtx {
        parent_content_w: cb.w,
        node_font_size: font_px(&css, DEFAULT_FONT_SIZE),
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // mede (w, h) numa lista descartável para resolver right/bottom.
    let (w, h) = measure_block(dom, id, cb.w, Some(cb.h), None, None, true, ctx);
    let left = resolve_inset(css.inset_left, cb.w, &resolve);
    let right = resolve_inset(css.inset_right, cb.w, &resolve);
    let top = resolve_inset(css.inset_top, cb.h, &resolve);
    let bottom = resolve_inset(css.inset_bottom, cb.h, &resolve);
    // Os offsets são RELATIVOS ao container: soma a origem do containing block.
    let x = match (left, right) {
        (Some(l), _) => cb.x + l,
        (None, Some(r)) => cb.x + cb.w - w - r,
        (None, None) => cb.x,
    };
    let y = match (top, bottom) {
        (Some(t), _) => cb.y + t,
        (None, Some(b)) => cb.y + cb.h - h - b,
        (None, None) => cb.y,
    };
    layout_block(
        dom,
        id,
        x,
        y,
        cb.w,
        Some(cb.h),
        None,
        None,
        true,
        // `position:absolute/fixed` estabelece o seu próprio BFC (CSS 2.1
        // §9.4.1); `bloco.rs` cria um novo internamente de qualquer forma.
        &BlockFormattingContext::new(),
        ctx,
        list,
    );
}

/// Resolve um offset de posicionamento (`top`/`left`/…): px SEM clamp (negativo
/// desloca para fora — badges/tooltips); `%` contra o eixo do viewport dado.
fn resolve_inset(d: Option<crate::style::Dimension>, axis: f32, ctx: &ResolveCtx) -> Option<f32> {
    match d? {
        crate::style::Dimension::Px(v) => Some(v),
        crate::style::Dimension::Percent(p) => Some(axis * p / 100.0),
        other => other.resolve(ctx),
    }
}

/// `true` se o nó SAI do fluxo (`position: absolute/fixed`) — não ocupa espaço
/// entre os irmãos; pintado na passada out-of-flow de [`layout_document`].
pub(crate) fn is_out_of_flow(dom: &Dom, id: NodeIdx) -> bool {
    matches!(&dom.node(id).kind, NodeKind::Element { .. })
        && dom
            .computed_style_idx(id)
            .and_then(|c| c.position)
            .map(|p| p.out_of_flow())
            .unwrap_or(false)
}

/// Resolve uma dimensão do EIXO VERTICAL (`height`/`min-height`/`max-height`):
/// `%` resolve contra a ALTURA do containing block (não a largura — era o bug que
/// fazia `height:100%` virar 100% da largura do pai); as demais unidades usam o
/// ctx normal. `avail_h = None` (pai com altura auto) → `%` vira auto (`None`),
/// fiel ao browser.
pub(in crate::layout) fn resolve_height(
    d: Option<crate::style::Dimension>,
    avail_h: Option<f32>,
    ctx: &ResolveCtx,
) -> Option<f32> {
    match d? {
        crate::style::Dimension::Percent(p) => avail_h.map(|h| (h * p / 100.0).max(0.0)),
        // `calc(...)` num contexto de ALTURA: o componente `%` resolve contra a
        // ALTURA do containing block (avail_h), NÃO a largura — o `resolve`
        // genérico usa parent_content_w e daria `calc(100% - 560px)` = 1000-560
        // (largura) em vez de 800-560 (altura). Reconstrói a soma no eixo certo.
        crate::style::Dimension::Calc(c) => {
            let h = avail_h?;
            let v = c.px
                + h * c.pct / 100.0
                + ctx.node_font_size * c.em
                + ctx.root_font_size * c.rem
                + ctx.viewport_w * c.vw / 100.0
                + ctx.viewport_h * c.vh / 100.0;
            Some(v.max(0.0))
        }
        other => other.resolve(ctx),
    }
}
