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
        let css_p = dom.computed_style_idx(p);
        let positioned = css_p
            .as_ref()
            .and_then(|c| c.position)
            .map(|pos| pos != crate::style::Position::Static)
            .unwrap_or(false);
        if positioned {
            // O containing block é a PADDING BOX do ancestral (CSS 2.1 §10.1),
            // não a border box guardada em `node_rects` — ver
            // `caixa_contentora.rs` para o achado (a referência de 31 dos 33
            // reftests `flex-align-justify-familia` tem um `border` no
            // ancestral e desviava 1px nos dois eixos sem esta conversão).
            if let (Some(r), Some(css_p)) = (flow_rects.get(&p), css_p) {
                return Some(super::caixa_contentora::padding_box(*r, &css_p));
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
    let left = resolve_inset(css.inset_left, cb.w, &resolve);
    let right = resolve_inset(css.inset_right, cb.w, &resolve);
    let top = resolve_inset(css.inset_top, cb.h, &resolve);
    let bottom = resolve_inset(css.inset_bottom, cb.h, &resolve);
    // STRETCH (CSS Position 3 / CSS 2.1 §10.3.7 e §10.6.4): com os DOIS
    // offsets de um eixo definidos e a dimensão AUTO, o valor usado desse
    // eixo enche o espaço entre eles — não é shrink-to-fit. `forced_outer_w`/
    // `forced_outer_h` já fazem esta conta para o `stretch` do flex (`outer
    // imposto → content = outer − frame`); reusa-se aqui em vez de duplicar a
    // subtracção do frame. `css.height.is_none()` e não `resolve_height(...)
    // .is_none()`: um `height:auto` explícito e a AUSÊNCIA de `height` são o
    // mesmo "auto" para este efeito, e é o `Option` do valor DECLARADO que a
    // spec chama de "auto" no §10.6.4 — testar o resolvido re-abriria a
    // pergunta que o `is_none()` já fecha.
    // Um REPLACED (`<img>`/`<svg>`/`<iframe>`) nunca estica pelos dois insets
    // (CSS 2.1 §10.3.8/§10.6.5, diferente de §10.3.7/§10.6.4 para não-replaced):
    // o tamanho usado é sempre o INTRÍNSECO — `measure_block` já mede por ele
    // quando não há `width`/`height` declarados — e um eixo com os DOIS insets
    // dados fica OVER-CONSTRAINED em vez de esticado: o segundo inset é
    // ignorado (a mesma regra que `(Some(l), _) => cb.x + l` já aplica no
    // eixo horizontal quando SOBRA um `right`). Sem esta guarda, um
    // `<img>{position:absolute}` com `top`/`bottom` dados e `height:auto`
    // encolhia para `(cb.h-top-bottom).max(0.0)` — normalmente 0, porque o
    // containing block de um ancestral só com filhos fora-de-fluxo mede
    // altura 0 no fluxo — em vez de manter os 15px do PNG. Achado na família
    // `absolute-replaced-height` do WPT (`-016`, `-030`: os comentários do
    // próprio teste fazem essa conta e concluem "o bottom dado é IGNORADO").
    // `claude-replaced-inset-nao-estica.html` prova o caso mínimo.
    let is_replaced = matches!(
        &dom.node(id).kind,
        NodeKind::Element { tag } if matches!(tag.as_str(), "img" | "svg" | "iframe" | "video" | "canvas" | "embed" | "object")
    );
    let stretch_w = left.is_some() && right.is_some() && css.width.is_none() && !is_replaced;
    let stretch_h = top.is_some()
        && bottom.is_some()
        && css.height.is_none()
        && css.aspect_ratio.is_none()
        && !is_replaced;
    let forced_outer_w = stretch_w.then(|| (cb.w - left.unwrap() - right.unwrap()).max(0.0));
    let forced_outer_h = stretch_h.then(|| (cb.h - top.unwrap() - bottom.unwrap()).max(0.0));
    // mede (w, h) numa lista descartável para resolver o eixo shrink-to-fit
    // (um só offset definido) — sem efeito no eixo esticado, que os `forced_*`
    // acima já fixam.
    let (w, h) = measure_block(
        dom,
        id,
        cb.w,
        Some(cb.h),
        forced_outer_w,
        forced_outer_h,
        true,
        ctx,
    );
    // Os offsets são RELATIVOS ao container: soma a origem do containing block.
    // Sem NENHUM dos dois insets de um eixo, a posição não é a origem do
    // containing block — é a STATIC POSITION (CSS 2.1 §10.3.7/§10.6.4): onde
    // `id` cairia se estivesse em fluxo normal, no seu PAI de verdade (que
    // pode não ser `cb`, quando o ancestral positioned está mais acima). Só
    // resolve o eixo que falta: o outro já veio de um inset declarado.
    let precisa_estatica = (left.is_none() && right.is_none()) || (top.is_none() && bottom.is_none());
    let estatica = precisa_estatica.then(|| {
        super::posicao_estatica::posicao_estatica(dom, id, &css, flow_rects, ctx)
    });
    // MARGEM AUTO sobre `left`+`width`+`right` TODOS dados (CSS 2.1 §10.3.7,
    // "solve the equation... the two margins get equal values, unless this
    // would make them negative"): o ÚNICO caso em que uma margin-left/right
    // `auto` de um posicionado NÃO vale 0 — `bloco.rs` (o `is_out_of_flow_pos`
    // do bloco "margin: 0 auto") já zera qualquer margem `auto` de um
    // absolute/fixed por default, porque ele só conhece `avail_w` (a largura
    // do containing block inteiro) e não tem `left`/`right`: a decisão de
    // QUANDO essa margem não é 0 fica só aqui, que já tem os dois insets.
    // `left` sempre vence como âncora (`x = cb.x + l`, a regra de cima), e é
    // por isso que só `margin_left` entra em `x` — `margin-right` nunca pinta
    // nada (não há caixa nenhuma do lado do `right` ignorado), só entra na
    // equação para a completar; calculá-lo é honesto (a spec pede) mas o
    // valor não tem efeito visual, então não é lido de volta.
    //
    // `border_box_w` desfaz o que `w` (outer = content+padding+border+margem)
    // já embutiu: a margem RAW que `bloco.rs` teria aplicado (explícita, ou 0
    // se `auto` — a mesma medição da passada de `measure_block` acima, ANTES
    // desta injeção existir).
    let margin_left_extra = (left.is_some() && right.is_some() && !stretch_w)
        .then(|| {
            let ml_auto = css.margin.left.is_auto();
            let mr_auto = css.margin.right.is_auto();
            if !ml_auto {
                return 0.0; // já correto: `bloco.rs` aplica o valor explícito sozinho.
            }
            let mr_raw = if mr_auto { 0.0 } else { css.margin.right.resolve(&resolve).unwrap_or(0.0) };
            // `w` (outer) já saiu de `measure_block` com margin-left `auto` →
            // 0 (garantido acima, `ml_auto` é verdade aqui) — só a margem
            // DIREITA explícita, se houver, precisa de ser descontada para
            // chegar à border-box pura.
            let border_box_w = w - mr_raw;
            let free = cb.w - left.unwrap() - right.unwrap() - border_box_w;
            if mr_auto {
                // Os dois `auto`: split igual, exceto se desse negativo — CSS
                // 2.1 manda zerar `margin-left` em `ltr` (`margin-right` em
                // `rtl`) e resolver o outro para o que sobrar.
                let rtl = css.direction.unwrap_or_default() == crate::style::Direction::Rtl;
                if free < 0.0 {
                    if rtl { free } else { 0.0 }
                } else {
                    free / 2.0
                }
            } else {
                // Só `margin-left` é `auto`: resolve sozinho (pode dar negativo
                // — WPT `absolute-non-replaced-width-009`, direction:rtl,
                // `margin-left:100/right:auto` invertido: aqui é o espelho,
                // `margin-right` fixo e `margin-left` a resolver).
                free - mr_raw
            }
        })
        .unwrap_or(0.0);
    let x = match (left, right) {
        (Some(l), _) => cb.x + l + margin_left_extra,
        (None, Some(r)) => cb.x + cb.w - w - r,
        (None, None) => estatica.unwrap().0,
    };
    let y = match (top, bottom) {
        (Some(t), _) => cb.y + t,
        (None, Some(b)) => cb.y + cb.h - h - b,
        (None, None) => estatica.unwrap().1,
    };
    layout_block(
        dom,
        id,
        x,
        y,
        cb.w,
        Some(cb.h),
        forced_outer_w,
        forced_outer_h,
        false,
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
/// `pub(in crate::layout)`: reusado por `relativo.rs` para os mesmos quatro
/// insets no caminho de `position:relative` — mesma resolução, containing
/// block diferente.
pub(in crate::layout) fn resolve_inset(
    d: Option<crate::style::Dimension>,
    axis: f32,
    ctx: &ResolveCtx,
) -> Option<f32> {
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
